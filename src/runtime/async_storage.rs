// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use super::{cache_error_for_callback, CacheBuffer, CacheError, StorageEngineApi};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

type WriteResult = Result<CacheBuffer, CacheError>;
type WriteCallback = Box<dyn FnOnce(WriteResult) + Send>;

struct StorageWrite {
    buffer: CacheBuffer,
    callback: WriteCallback,
    completed: mpsc::Sender<WriteResult>,
    reservation: WriteReservation,
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
pub struct StorageWriteCompletion(Receiver<WriteResult>);

impl StorageWriteCompletion {
    pub fn wait(self) -> WriteResult {
        self.0.recv().map_err(|_| CacheError::Stopped)?
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
    sender: Option<SyncSender<StorageWrite>>,
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
        mut engine: E,
        queue_capacity: usize,
        max_pending_bytes: usize,
    ) -> Result<Self, CacheError> {
        if !engine.start() {
            return Err(CacheError::Stopped);
        }
        let (sender, receiver) = mpsc::sync_channel::<StorageWrite>(queue_capacity);
        let pending_bytes = Arc::new(AtomicUsize::new(0));
        let worker = thread::Builder::new()
            .name("matrixcache-storage".into())
            .spawn(move || {
                while let Ok(write) = receiver.recv() {
                    let result = engine.async_put(write.buffer, |_| {});
                    let callback_result = match &result {
                        Ok(buffer) => Ok(buffer.clone_detached()),
                        Err(error) => Err(cache_error_for_callback(error)),
                    };
                    // A caller callback must not strand the rest of the queue.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (write.callback)(callback_result);
                    }));
                    drop(write.reservation);
                    let _ = write.completed.send(result);
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
        let sender = self.sender.as_ref().ok_or(CacheError::Stopped)?;
        let bytes = buffer.value().len();
        self.pending_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |pending| {
                pending
                    .checked_add(bytes)
                    .filter(|next| *next <= self.max_pending_bytes)
            })
            .map_err(|_| CacheError::CapacityExceeded)?;
        let reservation = WriteReservation {
            pending: Arc::clone(&self.pending_bytes),
            bytes,
        };
        let (completed, receiver) = mpsc::channel();
        let write = StorageWrite {
            buffer,
            callback: Box::new(callback),
            completed,
            reservation,
        };
        match sender.try_send(write) {
            Ok(()) => Ok(StorageWriteCompletion(receiver)),
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
