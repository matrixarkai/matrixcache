// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::{cache_error_for_callback, CacheBuffer, CacheError, CacheInstance, StorageEngineApi};
use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::task::{Context, Poll};
#[path = "storage_completion.rs"]
mod completion;
use completion::{CompletionReceiver, CompletionSender};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

type WriteResult = Result<CacheBuffer, CacheError>;
type WriteCallback = Box<dyn FnOnce(WriteResult) + Send>;

/// A write that was not admitted. No write or callback ran; the original
/// buffer is returned so the caller can retry without copying its payload.
#[derive(Debug)]
pub struct StorageWriteRejected {
    pub error: CacheError,
    pub buffer: CacheBuffer,
}

impl std::fmt::Display for StorageWriteRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for StorageWriteRejected {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

trait StorageWorkerBackend: Send + 'static {
    fn start(&mut self) -> Result<(), CacheError>;
    fn put(&mut self, buffer: CacheBuffer) -> WriteResult;
    fn delete(&mut self, key: &str) -> Result<(), CacheError>;
    fn stop(&mut self) -> bool;
}

struct EngineBackend<E>(E);
impl<E: StorageEngineApi + Send + 'static> StorageWorkerBackend for EngineBackend<E> {
    fn start(&mut self) -> Result<(), CacheError> {
        if self.0.start() {
            Ok(())
        } else {
            Err(CacheError::Stopped)
        }
    }
    fn put(&mut self, buffer: CacheBuffer) -> WriteResult {
        self.0.async_put(buffer, |_| {})
    }
    fn delete(&mut self, key: &str) -> Result<(), CacheError> {
        self.0.delete(key)
    }
    fn stop(&mut self) -> bool {
        self.0.stop()
    }
}

struct InstanceBackend(CacheInstance);
impl StorageWorkerBackend for InstanceBackend {
    fn start(&mut self) -> Result<(), CacheError> {
        self.0.start()
    }
    fn put(&mut self, buffer: CacheBuffer) -> WriteResult {
        self.0.put_cache_buffer(buffer)
    }
    fn delete(&mut self, key: &str) -> Result<(), CacheError> {
        self.0.delete(key)
    }
    fn stop(&mut self) -> bool {
        self.0.stop().is_ok()
    }
}

// Writes remain inline to avoid a separate heap allocation for every submission.
#[allow(clippy::large_enum_variant)]
enum StorageCommand {
    Put(StorageWrite),
    Delete {
        key: String,
        callback: Box<dyn FnOnce(Result<(), CacheError>) + Send>,
        completed: CompletionSender<()>,
    },
}

struct StorageWrite {
    buffer: CacheBuffer,
    callback: WriteCallback,
    completed: CompletionSender<CacheBuffer>,
    reservation: Option<WriteReservation>,
}

struct WriteReservation {
    pending: Arc<AtomicUsize>,
    bytes: usize,
}

impl Drop for WriteReservation {
    fn drop(&mut self) {
        self.pending.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// Completion of an accepted write. Dropping it does not cancel the write.
pub struct StorageWriteCompletion(CompletionReceiver<CacheBuffer>);

impl StorageWriteCompletion {
    pub fn wait(self) -> WriteResult {
        self.0.wait()
    }
}

impl Future for StorageWriteCompletion {
    type Output = WriteResult;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}

/// Completion of an accepted delete. Dropping it does not cancel deletion.
pub struct StorageDeleteCompletion(CompletionReceiver<()>);

impl StorageDeleteCompletion {
    pub fn wait(self) -> Result<(), CacheError> {
        self.0.wait()
    }
}

impl Future for StorageDeleteCompletion {
    type Output = Result<(), CacheError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}

/// Bounded, ordered storage writes on a dedicated worker thread.
///
/// Rejected submissions are not written and do not invoke the callback.
/// Accepted submissions invoke it once, on the worker, including storage errors.
/// Callbacks must not wait for another write on this same worker. Shutdown drains
/// accepted writes before stopping the engine; dropping a completion is not cancellation.
///
/// ```
/// use matrixcache::{AsyncStorageRuntime, CacheBuffer, StorageEngineSimple};
/// let mut runtime = AsyncStorageRuntime::start(StorageEngineSimple::new(), 16)?;
/// let mut buffer = CacheBuffer::new(b"payload".to_vec());
/// buffer.set_key("key");
/// let completion = runtime.try_put(buffer, |_| {})?;
/// assert_eq!(completion.wait()?.value(), b"payload");
/// assert!(runtime.shutdown()?);
/// # Ok::<(), matrixcache::CacheError>(())
/// ```
pub struct AsyncStorageRuntime {
    sender: Option<SyncSender<StorageCommand>>,
    worker: Option<JoinHandle<bool>>,
    pending_bytes: Arc<AtomicUsize>,
    max_pending_bytes: usize,
}

impl AsyncStorageRuntime {
    pub fn start<E: StorageEngineApi + Send + 'static>(
        engine: E,
        queue_capacity: usize,
    ) -> Result<Self, CacheError> {
        Self::start_with_byte_limit(engine, queue_capacity, usize::MAX)
    }

    /// Bound submitted payload bytes across queued writes, the active write and
    /// its callback. Completed buffers held by callers and engine-resident data
    /// are not part of this budget. A zero limit accepts only empty payloads.
    pub fn start_with_byte_limit<E: StorageEngineApi + Send + 'static>(
        engine: E,
        queue_capacity: usize,
        max_pending_bytes: usize,
    ) -> Result<Self, CacheError> {
        Self::start_backend(EngineBackend(engine), queue_capacity, max_pending_bytes)
    }

    /// Run cache-instance writes and deletes on the bounded worker. This runtime
    /// owns start/stop for the instance, including its shared clones. Other
    /// owners must not stop the instance while the runtime is active.
    pub fn start_cache_instance(
        instance: CacheInstance,
        queue_capacity: usize,
        max_pending_bytes: usize,
    ) -> Result<Self, CacheError> {
        Self::start_backend(InstanceBackend(instance), queue_capacity, max_pending_bytes)
    }

    fn start_backend<B: StorageWorkerBackend>(
        mut engine: B,
        queue_capacity: usize,
        max_pending_bytes: usize,
    ) -> Result<Self, CacheError> {
        engine.start()?;
        let (sender, receiver) = mpsc::sync_channel::<StorageCommand>(queue_capacity);
        let pending_bytes = Arc::new(AtomicUsize::new(0));
        let worker = thread::Builder::new()
            .name("matrixcache-storage".into())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        StorageCommand::Put(write) => {
                            let result = engine.put(write.buffer);
                            let callback_result = match &result {
                                Ok(buffer) => Ok(buffer.clone_for_callback()),
                                Err(error) => Err(cache_error_for_callback(error)),
                            };
                            // A caller callback must not strand the rest of the queue.
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                (write.callback)(callback_result);
                            }));
                            drop(write.reservation);
                            write.completed.send(result);
                        }
                        StorageCommand::Delete {
                            key,
                            callback,
                            completed,
                        } => {
                            let result = engine.delete(&key);
                            let callback_result = match &result {
                                Ok(()) => Ok(()),
                                Err(error) => Err(cache_error_for_callback(error)),
                            };
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                callback(callback_result)
                            }));
                            completed.send(result);
                        }
                    }
                }
                engine.stop()
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            pending_bytes,
            max_pending_bytes,
        })
    }

    pub fn try_put<F>(
        &self,
        buffer: CacheBuffer,
        callback: F,
    ) -> Result<StorageWriteCompletion, CacheError>
    where
        F: FnOnce(WriteResult) + Send + 'static,
    {
        self.try_put_recoverable(buffer, callback)
            .map_err(|rejected| rejected.error)
    }

    /// Submit with ownership recovery on queue, byte-budget or stopped rejection.
    /// The callback is discarded without invocation when submission fails.
    #[allow(clippy::result_large_err)] // Preserve ownership inline without a rejection-path allocation.
    pub fn try_put_recoverable<F>(
        &self,
        buffer: CacheBuffer,
        callback: F,
    ) -> Result<StorageWriteCompletion, StorageWriteRejected>
    where
        F: FnOnce(WriteResult) + Send + 'static,
    {
        let Some(sender) = self.sender.as_ref() else {
            return Err(StorageWriteRejected {
                error: CacheError::Stopped,
                buffer,
            });
        };
        let reservation = if self.max_pending_bytes == usize::MAX || buffer.value().is_empty() {
            None
        } else {
            let bytes = buffer.value().len();
            if self
                .pending_bytes
                .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |pending| {
                    pending
                        .checked_add(bytes)
                        .filter(|next| *next <= self.max_pending_bytes)
                })
                .is_err()
            {
                return Err(StorageWriteRejected {
                    error: CacheError::CapacityExceeded,
                    buffer,
                });
            }
            Some(WriteReservation {
                pending: Arc::clone(&self.pending_bytes),
                bytes,
            })
        };
        let (completed, receiver) = completion::channel();
        let write = StorageWrite {
            buffer,
            callback: Box::new(callback),
            completed,
            reservation,
        };
        match sender.try_send(StorageCommand::Put(write)) {
            Ok(()) => Ok(StorageWriteCompletion(receiver)),
            Err(rejected) => {
                let (error, command) = match rejected {
                    TrySendError::Full(command) => (CacheError::CapacityExceeded, command),
                    TrySendError::Disconnected(command) => (CacheError::Stopped, command),
                };
                let StorageCommand::Put(write) = command else {
                    unreachable!("only write submitted");
                };
                Err(StorageWriteRejected {
                    error,
                    buffer: write.buffer,
                })
            }
        }
    }

    /// Queue a delete after earlier accepted writes/deletes. Queue admission is
    /// bounded by job count; deletes do not reserve payload bytes.
    pub fn try_delete<F>(
        &self,
        key: impl Into<String>,
        callback: F,
    ) -> Result<StorageDeleteCompletion, CacheError>
    where
        F: FnOnce(Result<(), CacheError>) + Send + 'static,
    {
        let sender = self.sender.as_ref().ok_or(CacheError::Stopped)?;
        let (completed, receiver) = completion::channel();
        let command = StorageCommand::Delete {
            key: key.into(),
            callback: Box::new(callback),
            completed,
        };
        match sender.try_send(command) {
            Ok(()) => Ok(StorageDeleteCompletion(receiver)),
            Err(TrySendError::Full(_)) => Err(CacheError::CapacityExceeded),
            Err(TrySendError::Disconnected(_)) => Err(CacheError::Stopped),
        }
    }

    pub fn shutdown(&mut self) -> Result<bool, CacheError> {
        self.sender.take();
        let Some(worker) = self.worker.take() else {
            return Ok(true);
        };
        if worker.thread().id() == thread::current().id() {
            // The final owner can be dropped from a callback. Closing the inbox
            // still drains and stops the engine after that callback returns.
            return Ok(false);
        }
        worker.join().map_err(|_| CacheError::Stopped)
    }
}

impl Drop for AsyncStorageRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
