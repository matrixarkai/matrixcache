// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use std::thread::{self, JoinHandle};

struct L2PolicyInbox {
    access: VecDeque<(AccessRecordKind, String)>,
    evictions: VecDeque<CacheBuffer>,
    access_capacity: usize,
    eviction_capacity: usize,
}

/// Runs lower-tier migration without making callers wait for SSD writes.
pub struct L2CachePolicyRuntime {
    policy: Arc<Mutex<L2CachePolicy>>,
    inbox: Arc<Mutex<L2PolicyInbox>>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    eviction_enabled: bool,
    worker: Option<JoinHandle<Result<(), CacheError>>>,
}

impl L2CachePolicyRuntime {
    pub fn start(mut policy: L2CachePolicy) -> Result<Self, CacheError> {
        policy.start();
        policy.resume();
        // The runtime inbox already makes access recording asynchronous.
        policy.set_async_on_access(false);
        let eviction_enabled = policy.use_eviction_handler();
        let inbox = Arc::new(Mutex::new(L2PolicyInbox {
            access: VecDeque::new(),
            evictions: VecDeque::new(),
            access_capacity: policy.access_buffer_capacity,
            eviction_capacity: policy.write_buffer_capacity,
        }));
        let policy = Arc::new(Mutex::new(policy));
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let worker_policy = Arc::clone(&policy);
        let worker_inbox = Arc::clone(&inbox);
        let worker_stop = Arc::clone(&stop);
        let worker_paused = Arc::clone(&paused);
        let worker = thread::Builder::new()
            .name("matrixcache-l2-policy".to_string())
            .spawn(move || {
                loop {
                    let mut policy = worker_policy.lock().expect("L2 policy lock poisoned");
                    if worker_paused.load(Ordering::Acquire)
                        && !worker_stop.load(Ordering::Acquire)
                    {
                        drop(policy);
                        thread::park_timeout(Duration::from_millis(100));
                        continue;
                    }
                    let (access, evictions, more) = {
                        let mut inbox = worker_inbox.lock().expect("L2 inbox lock poisoned");
                        let access_count = inbox.access.len().min(256);
                        let eviction_count = inbox.evictions.len().min(256);
                        let access = inbox.access.drain(..access_count).collect::<Vec<_>>();
                        let evictions = inbox.evictions.drain(..eviction_count).collect::<Vec<_>>();
                        let more = !inbox.access.is_empty() || !inbox.evictions.is_empty();
                        (access, evictions, more)
                    };
                    let had_work = !access.is_empty() || !evictions.is_empty();
                    for (kind, key) in access {
                        policy.on_access(kind, &key);
                    }
                    for buffer in evictions {
                        if policy.write_buffer_capacity != 0
                            && policy.write_buffer_size() >= policy.write_buffer_capacity
                        {
                            policy.write_task_internal()?;
                        }
                        policy.on_evict(buffer);
                    }
                    if worker_stop.load(Ordering::Acquire) {
                        if more {
                            continue;
                        }
                        policy.resume();
                        let result = policy.flush_once().map(|_| ());
                        policy.stop();
                        return result;
                    }
                    if let Err(error) = policy.poll() {
                        policy.stop();
                        return Err(error);
                    }
                    let interval_ms = if policy.access_buffer_size() > 0 {
                        policy.access_interval_ms()
                            .min(policy.tail_interval_ms())
                            .min(policy.write_interval_ms())
                    } else {
                        policy.tail_interval_ms().min(policy.write_interval_ms())
                    };
                    drop(policy);
                    if !had_work && !more {
                        thread::park_timeout(Duration::from_millis(interval_ms.clamp(1, 100)));
                    }
                }
            })?;
        Ok(Self {
            policy,
            inbox,
            stop,
            paused,
            eviction_enabled,
            worker: Some(worker),
        })
    }

    /// Returns false if stopped or the bounded access inbox is full.
    pub fn on_access(&self, kind: AccessRecordKind, key: &str) -> bool {
        if self.stop.load(Ordering::Acquire) {
            return false;
        }
        let mut inbox = self.inbox.lock().expect("L2 inbox lock poisoned");
        if inbox.access_capacity != 0 && inbox.access.len() >= inbox.access_capacity {
            return false;
        }
        inbox.access.push_back((kind, key.to_owned()));
        drop(inbox);
        if let Some(worker) = self.worker.as_ref() {
            worker.thread().unpark();
        }
        true
    }

    /// Returns the buffer to the caller if migration cannot accept it.
    pub fn on_evict(&self, buffer: CacheBuffer) -> Result<(), Box<CacheBuffer>> {
        if self.stop.load(Ordering::Acquire) || !self.eviction_enabled {
            return Err(Box::new(buffer));
        }
        let mut inbox = self.inbox.lock().expect("L2 inbox lock poisoned");
        if inbox.eviction_capacity != 0 && inbox.evictions.len() >= inbox.eviction_capacity {
            return Err(Box::new(buffer));
        }
        inbox.evictions.push_back(buffer);
        drop(inbox);
        if let Some(worker) = self.worker.as_ref() {
            worker.thread().unpark();
        }
        Ok(())
    }

    pub fn pause(&self) {
        let _policy = self.policy.lock().expect("L2 policy lock poisoned");
        self.paused.store(true, Ordering::Release);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        if let Some(worker) = self.worker.as_ref() {
            worker.thread().unpark();
        }
    }

    pub fn inspect<R>(&self, f: impl FnOnce(&L2CachePolicy) -> R) -> R {
        let policy = self.policy.lock().expect("L2 policy lock poisoned");
        f(&policy)
    }

    /// Joins the worker and drains accepted migration events before returning.
    pub fn stop(&mut self) -> Result<(), CacheError> {
        self.stop.store(true, Ordering::Release);
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.thread().unpark();
        worker.join().map_err(|_| {
            CacheError::Io(std::io::Error::other("L2 policy worker panicked"))
        })?
    }
}

impl Drop for L2CachePolicyRuntime {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
