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
    fn put_batch(&mut self, buffers: Vec<CacheBuffer>) -> Vec<WriteResult> {
        buffers.into_iter().map(|buffer| self.put(buffer)).collect()
    }
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
    fn put_batch(&mut self, buffers: Vec<CacheBuffer>) -> Vec<WriteResult> {
        self.0.put_buffers_batch(buffers)
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

impl StorageWrite {
    fn run<B: StorageWorkerBackend>(self, engine: &mut B) {
        let result = engine.put(self.buffer);
        finish_write(self.callback, self.reservation, self.completed, result);
    }
}

fn finish_write(
    callback: WriteCallback,
    reservation: Option<WriteReservation>,
    completed: CompletionSender<CacheBuffer>,
    result: WriteResult,
) {
    let callback_result = match &result {
        Ok(buffer) => Ok(buffer.clone_for_callback()),
        Err(error) => Err(cache_error_for_callback(error)),
    };
    // A caller callback must not strand the rest of the queue.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(callback_result)));
    drop(reservation);
    completed.send(result);
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
        Self::start_backend(
            EngineBackend(engine),
            queue_capacity,
            max_pending_bytes,
            1,
            usize::MAX,
        )
    }

    /// Coalesce already-queued consecutive writes without waiting to fill a
    /// batch. Deletes are ordering barriers. Callbacks run in submission order
    /// after all writes in their group, so they may observe later group writes.
    /// A group is not a portable atomic transaction across storage engines.
    ///
    /// Limits must be nonzero. A single payload larger than `max_batch_bytes`
    /// runs alone if admitted by `max_pending_bytes`. The worker holds at most
    /// `max_batch_writes` active writes and one deferred command in addition to
    /// the bounded inbox. Payload reservations cover all of them and callbacks.
    pub fn start_batched<E: StorageEngineApi + Send + 'static>(
        engine: E,
        queue_capacity: usize,
        max_pending_bytes: usize,
        max_batch_writes: usize,
        max_batch_bytes: usize,
    ) -> Result<Self, CacheError> {
        if max_batch_writes == 0 || max_batch_bytes == 0 {
            return Err(CacheError::CapacityExceeded);
        }
        Self::start_backend(
            EngineBackend(engine),
            queue_capacity,
            max_pending_bytes,
            max_batch_writes,
            max_batch_bytes,
        )
    }

    /// Run cache-instance writes and deletes on the bounded worker. This runtime
    /// owns start/stop for the instance, including its shared clones. Other
    /// owners must not stop the instance while the runtime is active.
    pub fn start_cache_instance(
        instance: CacheInstance,
        queue_capacity: usize,
        max_pending_bytes: usize,
    ) -> Result<Self, CacheError> {
        Self::start_backend(
            InstanceBackend(instance),
            queue_capacity,
            max_pending_bytes,
            1,
            usize::MAX,
        )
    }

    fn start_backend<B: StorageWorkerBackend>(
        mut engine: B,
        queue_capacity: usize,
        max_pending_bytes: usize,
        max_batch_writes: usize,
        max_batch_bytes: usize,
    ) -> Result<Self, CacheError> {
        engine.start()?;
        let (sender, receiver) = mpsc::sync_channel::<StorageCommand>(queue_capacity);
        let pending_bytes = Arc::new(AtomicUsize::new(0));
        let worker = thread::Builder::new()
            .name("matrixcache-storage".into())
            .spawn(move || {
                let mut deferred = None;
                while let Some(command) = deferred.take().or_else(|| receiver.recv().ok()) {
                    match command {
                        StorageCommand::Put(write) if max_batch_writes > 1 => {
                            let mut bytes = write.buffer.value().len();
                            let next = if bytes < max_batch_bytes {
                                receiver.try_recv().ok()
                            } else {
                                None
                            };
                            let second = match next {
                                Some(StorageCommand::Put(second))
                                    if bytes
                                        .checked_add(second.buffer.value().len())
                                        .is_some_and(|total| total <= max_batch_bytes) =>
                                {
                                    second
                                }
                                next => {
                                    deferred = next;
                                    write.run(&mut engine);
                                    continue;
                                }
                            };
                            bytes += second.buffer.value().len();
                            let mut writes = vec![write, second];
                            while writes.len() < max_batch_writes && bytes < max_batch_bytes {
                                let Ok(command) = receiver.try_recv() else {
                                    break;
                                };
                                match command {
                                    StorageCommand::Put(write)
                                        if bytes
                                            .checked_add(write.buffer.value().len())
                                            .is_some_and(|total| total <= max_batch_bytes) =>
                                    {
                                        bytes += write.buffer.value().len();
                                        writes.push(write);
                                    }
                                    command => {
                                        deferred = Some(command);
                                        break;
                                    }
                                }
                            }
                            let (buffers, completions): (Vec<_>, Vec<_>) = writes
                                .into_iter()
                                .map(|write| {
                                    (
                                        write.buffer,
                                        (write.callback, write.reservation, write.completed),
                                    )
                                })
                                .unzip();
                            let results = engine.put_batch(buffers);
                            for ((callback, reservation, completed), result) in
                                completions.into_iter().zip(results)
                            {
                                finish_write(callback, reservation, completed, result);
                            }
                        }
                        StorageCommand::Put(write) => {
                            write.run(&mut engine);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    struct BatchBackend(Arc<Mutex<Vec<Vec<String>>>>);
    impl StorageWorkerBackend for BatchBackend {
        fn start(&mut self) -> Result<(), CacheError> {
            Ok(())
        }
        fn stop(&mut self) -> bool {
            true
        }
        fn put(&mut self, buffer: CacheBuffer) -> WriteResult {
            self.0.lock().unwrap().push(vec![buffer.key().to_string()]);
            Ok(buffer)
        }
        fn put_batch(&mut self, buffers: Vec<CacheBuffer>) -> Vec<WriteResult> {
            assert!(
                buffers.len() > 1,
                "single writes must bypass batch allocation"
            );
            self.0
                .lock()
                .unwrap()
                .push(buffers.iter().map(|b| b.key().to_string()).collect());
            buffers.into_iter().map(Ok).collect()
        }
        fn delete(&mut self, key: &str) -> Result<(), CacheError> {
            self.0.lock().unwrap().push(vec![format!("delete:{key}")]);
            Ok(())
        }
    }

    #[test]
    fn bounded_coalescing_preserves_delete_barriers_callbacks_and_reservations() {
        for (count, bytes, group_size) in [(2, 100, 2), (8, 6, 2), (8, 2, 1)] {
            let history = Arc::new(Mutex::new(Vec::new()));
            let mut runtime = AsyncStorageRuntime::start_backend(
                BatchBackend(Arc::clone(&history)),
                8,
                12,
                count,
                bytes,
            )
            .unwrap();
            let make = |key| {
                let mut b = CacheBuffer::new(vec![7; 3]);
                b.set_key(key);
                b
            };
            let (entered, ready) = mpsc::channel();
            let (release, gate) = mpsc::channel();
            let first = runtime
                .try_put(make("first"), move |_| {
                    entered.send(()).unwrap();
                    gate.recv_timeout(Duration::from_secs(5)).unwrap();
                })
                .unwrap();
            ready.recv_timeout(Duration::from_secs(5)).unwrap();
            let callbacks = Arc::new(Mutex::new(Vec::new()));
            let mut completions = Vec::new();
            for key in ["a", "b", "c"] {
                let observed = Arc::clone(&callbacks);
                completions.push(
                    runtime
                        .try_put(make(key), move |result| {
                            observed
                                .lock()
                                .unwrap()
                                .push(result.unwrap().key().to_string());
                            if key == "b" {
                                panic!("callback isolation");
                            }
                        })
                        .unwrap(),
                );
            }
            assert!(matches!(
                runtime.try_put(make("overflow"), |_| {}),
                Err(CacheError::CapacityExceeded)
            ));
            let delete = runtime.try_delete("a", |_| {}).unwrap();
            release.send(()).unwrap();
            first.wait().unwrap();
            for (completion, key) in completions.into_iter().zip(["a", "b", "c"]) {
                assert_eq!(completion.wait().unwrap().key(), key);
            }
            delete.wait().unwrap();
            assert_eq!(runtime.pending_bytes.load(Ordering::Acquire), 0);
            runtime
                .try_put(make("last"), |_| {})
                .unwrap()
                .wait()
                .unwrap();
            assert!(runtime.shutdown().unwrap());
            assert_eq!(*callbacks.lock().unwrap(), ["a", "b", "c"]);
            let mut expected = vec![vec!["first".to_string()]];
            expected.extend(
                ["a", "b", "c"]
                    .chunks(group_size)
                    .map(|group| group.iter().map(|s| s.to_string()).collect()),
            );
            expected.push(vec!["delete:a".to_string()]);
            expected.push(vec!["last".to_string()]);
            assert_eq!(*history.lock().unwrap(), expected);
        }
    }
}
