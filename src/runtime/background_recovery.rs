// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

#[derive(Debug)]
struct CacheSsdRecoveryWorker {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<CacheSsdRecoveryStatus>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl MultiLayerCache {
    fn install_recovered_ssd_batch(
        &self,
        batch: &[(String, CacheKey, u64)],
        report: &mut CacheRecoverReport,
    ) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        for (store_key, key, block_len) in batch {
            let shared = inner.shared_key(key);
            if inner.disk_index.contains_key(&shared) {
                report.skipped_files = report.skipped_files.saturating_add(1);
                continue;
            }
            if !inner.ssd_store.peek(store_key) {
                report.skipped_files = report.skipped_files.saturating_add(1);
                continue;
            }
            inner.disk_index.insert(shared.clone(), *block_len);
            inner
                .disk_order
                .push_back_if_absent_shared(Arc::clone(&shared));
            inner.ssd_bytes = inner.ssd_bytes.saturating_add(*block_len);
            inner.set_metadata_if_absent(
                shared,
                CacheEntryMeta::new(
                    infer_block_kind(key),
                    extract_routing_slot(key),
                    0,
                    0,
                    0,
                    CacheAdmissionReason::MemoryOnly,
                ),
            );
            report.recovered_files = report.recovered_files.saturating_add(1);
            report.recovered_bytes = report.recovered_bytes.saturating_add(*block_len);
        }
        inner.evict_ssd_to_capacity();
        Ok(())
    }

    pub fn start_ssd_recovery_worker(&self, batch_size: usize) -> Result<bool, CacheError> {
        let mut worker_slot = self
            .ssd_recovery_worker
            .lock()
            .expect("ssd recovery worker lock poisoned");
        if worker_slot.as_ref().is_some_and(|worker| {
            worker
                .status
                .lock()
                .expect("ssd recovery status lock poisoned")
                .running
        }) {
            return Ok(false);
        }
        if let Some(mut worker) = worker_slot.take() {
            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
            }
        }

        let store = {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            if !inner.ssd_store.holds_blocks() {
                return Ok(false);
            }
            inner.ssd_store.clone()
        };

        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(CacheSsdRecoveryStatus {
            running: true,
            ..CacheSsdRecoveryStatus::default()
        }));
        let worker_stop = Arc::clone(&stop);
        let worker_status = Arc::clone(&status);
        let cache = self.clone();
        let batch_size = batch_size.max(1);
        let handle = std::thread::Builder::new()
            .name("matrixcache-ssd-recovery".to_string())
            .spawn(move || {
                let mut report = CacheRecoverReport::default();
                let mut batch = Vec::<(String, CacheKey, u64)>::with_capacity(batch_size);
                let mut failure = None;
                let scan_result = store.visit_recovery_view_data(&mut |store_key, view| {
                    if failure.is_some() || worker_stop.load(Ordering::Acquire) {
                        return false;
                    }
                    report.scanned_files = report.scanned_files.saturating_add(1);
                    let Some(record) = CacheManifestRecord::decode_line(store_key) else {
                        report.skipped_files = report.skipped_files.saturating_add(1);
                        return true;
                    };
                    batch.push((store_key.to_string(), record.key(), view.size() as u64));
                    if batch.len() >= batch_size {
                        if let Err(err) = cache.install_recovered_ssd_batch(&batch, &mut report) {
                            failure = Some(err.to_string());
                        }
                        batch.clear();
                    }
                    failure.is_none() && !worker_stop.load(Ordering::Acquire)
                });
                if failure.is_none() && !worker_stop.load(Ordering::Acquire) && !batch.is_empty() {
                    if let Err(err) = cache.install_recovered_ssd_batch(&batch, &mut report) {
                        failure = Some(err.to_string());
                    }
                }
                if failure.is_none() {
                    if let Err(err) = scan_result {
                        failure = Some(err.to_string());
                    }
                }
                let cancelled = worker_stop.load(Ordering::Acquire);
                let mut status = worker_status
                    .lock()
                    .expect("ssd recovery status lock poisoned");
                status.running = false;
                status.completed = !cancelled && failure.is_none();
                status.cancelled = cancelled;
                status.report = report;
                status.error = failure;
            })
            .map_err(CacheError::Io)?;
        *worker_slot = Some(CacheSsdRecoveryWorker {
            stop,
            status,
            handle: Some(handle),
        });
        Ok(true)
    }

    fn start_configured_ssd_recovery_worker(&self) -> Result<bool, CacheError> {
        self.start_ssd_recovery_worker(self.ssd_recovery_batch_size())
    }

    pub fn ssd_recovery_status(&self) -> Option<CacheSsdRecoveryStatus> {
        self.ssd_recovery_worker
            .lock()
            .expect("ssd recovery worker lock poisoned")
            .as_ref()
            .map(|worker| {
                worker
                    .status
                    .lock()
                    .expect("ssd recovery status lock poisoned")
                    .clone()
            })
    }

    pub fn ssd_recovery_worker_running(&self) -> bool {
        self.ssd_recovery_status()
            .is_some_and(|status| status.running)
    }

    pub fn wait_for_ssd_recovery(&self) -> Option<CacheSsdRecoveryStatus> {
        let mut worker = self
            .ssd_recovery_worker
            .lock()
            .expect("ssd recovery worker lock poisoned");
        let worker = worker.as_mut()?;
        if let Some(handle) = worker.handle.take() {
            let _ = handle.join();
        }
        let status = worker
            .status
            .lock()
            .expect("ssd recovery status lock poisoned")
            .clone();
        Some(status)
    }

    pub fn stop_ssd_recovery_worker(&self) -> bool {
        let worker = self
            .ssd_recovery_worker
            .lock()
            .expect("ssd recovery worker lock poisoned")
            .take();
        let Some(mut worker) = worker else {
            return false;
        };
        worker.stop.store(true, Ordering::Release);
        if let Some(handle) = worker.handle.take() {
            let _ = handle.join();
        }
        true
    }
}

impl ShardedMultiLayerCache {
    pub fn start_ssd_recovery_workers(&self, batch_size: usize) -> Result<usize, CacheError> {
        let mut started = 0usize;
        for shard in self.shards.iter() {
            if shard.start_ssd_recovery_worker(batch_size)? {
                started = started.saturating_add(1);
            }
        }
        Ok(started)
    }

    pub fn ssd_recovery_statuses(&self) -> Vec<Option<CacheSsdRecoveryStatus>> {
        self.shards
            .iter()
            .map(MultiLayerCache::ssd_recovery_status)
            .collect()
    }

    pub fn wait_for_ssd_recovery_workers(&self) -> Vec<Option<CacheSsdRecoveryStatus>> {
        self.shards
            .iter()
            .map(MultiLayerCache::wait_for_ssd_recovery)
            .collect()
    }

    pub fn stop_ssd_recovery_workers(&self) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.stop_ssd_recovery_worker())
            .count()
    }
}
