// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

/// Result of one bounded cold-tier migration pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheColdTierMigrationReport {
    pub requested: usize,
    pub selected: usize,
    pub migrated: usize,
    pub already_persistent: usize,
    pub changed_before_write: usize,
    pub ssd_disabled: bool,
}

#[derive(Debug)]
struct CacheColdTierMigrationWorker {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    thread: std::thread::Thread,
    handle: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug, Default)]
struct CacheColdTierScanCursor {
    memory_last_key: Option<CacheKey>,
    pmem_last_key: Option<CacheKey>,
    pmem_first: bool,
}

impl MultiLayerCache {
    /// Configure bounded automatic migration for this cache and future starts.
    /// Zero entries disables it. Reconfiguration replaces the running worker.
    pub fn configure_cold_tier_migration_worker(
        &self,
        max_entries_per_round: usize,
        interval: Duration,
    ) {
        self.stop_cold_tier_migration_worker();
        self.cold_tier_migration_entries_per_round
            .store(max_entries_per_round as u64, Ordering::Release);
        self.cold_tier_migration_interval_millis.store(
            interval.as_millis().clamp(1, u64::MAX as u128) as u64,
            Ordering::Release,
        );
        self.start_configured_cold_tier_migration_worker();
    }

    fn start_configured_cold_tier_migration_worker(&self) -> bool {
        let max_entries = self
            .cold_tier_migration_entries_per_round
            .load(Ordering::Acquire);
        if max_entries == 0 {
            return false;
        }
        {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started || inner.ssd_capacity_bytes == 0 {
                return false;
            }
        }
        let interval = Duration::from_millis(
            self.cold_tier_migration_interval_millis
                .load(Ordering::Acquire)
                .max(1),
        );
        self.start_cold_tier_migration_worker(
            usize::try_from(max_entries).unwrap_or(usize::MAX),
            interval,
        )
    }

    pub fn cold_tier_migration_worker_configuration(&self) -> Option<(usize, Duration)> {
        let max_entries = self
            .cold_tier_migration_entries_per_round
            .load(Ordering::Acquire);
        (max_entries > 0).then(|| {
            (
                usize::try_from(max_entries).unwrap_or(usize::MAX),
                Duration::from_millis(
                    self.cold_tier_migration_interval_millis
                        .load(Ordering::Acquire)
                        .max(1),
                ),
            )
        })
    }

    /// Copies the coldest resident entries into the SSD tier without removing
    /// their upper-tier copies.
    ///
    /// This is useful when SSD write-through is disabled: a small bounded pass
    /// can preposition cold values before memory pressure forces eviction. A
    /// candidate is revalidated immediately before it is written, so a value
    /// changed after selection is never persisted as the current value.
    pub fn migrate_cold_entries_to_ssd(
        &self,
        max_entries: usize,
    ) -> Result<CacheColdTierMigrationReport, CacheError> {
        let mut report = CacheColdTierMigrationReport {
            requested: max_entries,
            ..CacheColdTierMigrationReport::default()
        };
        if max_entries == 0 {
            return Ok(report);
        }

        let (candidates, block_options) = {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            if inner.ssd_capacity_bytes == 0 {
                report.ssd_disabled = true;
                return Ok(report);
            }

            let mut selected = Vec::with_capacity(max_entries);
            let mut seen = AHashSet::with_capacity(max_entries);
            let mut cursor = self
                .cold_tier_scan_cursor
                .lock()
                .expect("cold-tier scan cursor lock poisoned");
            let scan_limit = max_entries.saturating_mul(8).clamp(64, 4096);
            let first_pmem = cursor.pmem_first;
            cursor.pmem_first = !first_pmem;
            for pmem in [first_pmem, !first_pmem] {
                let order = if pmem {
                    &inner.pmem_order
                } else {
                    &inner.memory_order
                };
                let last_key = if pmem {
                    &mut cursor.pmem_last_key
                } else {
                    &mut cursor.memory_last_key
                };
                for _ in 0..scan_limit / 2 {
                    if selected.len() >= max_entries {
                        break;
                    }
                    let Some(key) = order.iter_access_after(last_key.as_ref()).next() else {
                        *last_key = None;
                        break;
                    };
                    *last_key = Some(key.clone());
                    if inner.disk_index.contains_key(key) || !seen.insert(key.clone()) {
                        continue;
                    }
                    let value = inner.resident_value(key).or_else(|| inner.pmem_value(key));
                    if let Some(value) = value {
                        selected.push((key.clone(), Arc::clone(value)));
                    }
                }
            }
            (selected, inner.block_options)
        };
        report.selected = candidates.len();

        for (key, selected_value) in candidates {
            // Compression and framing can dominate the migration cost for a
            // large value. Prepare the immutable block before taking the
            // cache-wide write lock, then revalidate the resident allocation
            // below before publishing it.
            let prepared_block = encode_cache_block(selected_value.as_ref(), block_options)?;
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            if inner.disk_index.contains_key(&key) {
                report.already_persistent = report.already_persistent.saturating_add(1);
                continue;
            }
            let current = inner
                .resident_value(&key)
                .or_else(|| inner.pmem_value(&key));
            if !current.is_some_and(|value| Arc::ptr_eq(value, &selected_value)) {
                report.changed_before_write = report.changed_before_write.saturating_add(1);
                continue;
            }
            let block_kind = infer_block_kind(&key);
            let routing_slot = extract_routing_slot(&key);
            if inner.put_bypass_ssd_preencoded(
                &key,
                selected_value.as_ref(),
                prepared_block,
                block_kind,
                routing_slot,
            )? {
                report.migrated = report.migrated.saturating_add(1);
            }
        }
        Ok(report)
    }

    /// Starts a lifecycle-managed worker that runs bounded cold-tier passes.
    /// Returns `false` when a worker is already running.
    pub fn start_cold_tier_migration_worker(
        &self,
        max_entries_per_round: usize,
        interval: Duration,
    ) -> bool {
        let max_entries_per_round = max_entries_per_round.max(1);
        let mut worker = self
            .cold_tier_migration_worker
            .lock()
            .expect("cold-tier migration worker lock poisoned");
        if worker.is_some() {
            return false;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let paused = Arc::new(AtomicBool::new(false));
        let worker_paused = Arc::clone(&paused);
        let cache = self.clone();
        let handle = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                if !worker_paused.load(Ordering::Acquire) {
                    let _ = cache.migrate_cold_entries_to_ssd(max_entries_per_round);
                }
                std::thread::park_timeout(interval);
            }
        });
        let thread = handle.thread().clone();
        *worker = Some(CacheColdTierMigrationWorker {
            stop,
            paused,
            thread,
            handle: Some(handle),
        });
        true
    }

    /// Suspends migration without tearing down the worker. Returns `false`
    /// when no worker is running or it was already paused.
    pub fn pause_cold_tier_migration_worker(&self) -> bool {
        let worker = self
            .cold_tier_migration_worker
            .lock()
            .expect("cold-tier migration worker lock poisoned");
        let Some(worker) = worker.as_ref() else {
            return false;
        };
        !worker.paused.swap(true, Ordering::AcqRel)
    }

    /// Resumes a paused worker and wakes it for an immediate bounded pass.
    /// Returns `false` when no worker is running or it was already active.
    pub fn resume_cold_tier_migration_worker(&self) -> bool {
        let worker = self
            .cold_tier_migration_worker
            .lock()
            .expect("cold-tier migration worker lock poisoned");
        let Some(worker) = worker.as_ref() else {
            return false;
        };
        if !worker.paused.swap(false, Ordering::AcqRel) {
            return false;
        }
        worker.thread.unpark();
        true
    }

    pub fn cold_tier_migration_worker_paused(&self) -> bool {
        self.cold_tier_migration_worker
            .lock()
            .expect("cold-tier migration worker lock poisoned")
            .as_ref()
            .is_some_and(|worker| worker.paused.load(Ordering::Acquire))
    }

    /// Stops the cold-tier worker and waits for it to exit.
    pub fn stop_cold_tier_migration_worker(&self) -> bool {
        let worker = self
            .cold_tier_migration_worker
            .lock()
            .expect("cold-tier migration worker lock poisoned")
            .take();
        let Some(mut worker) = worker else {
            return false;
        };
        worker.stop.store(true, Ordering::Relaxed);
        worker.thread.unpark();
        if let Some(handle) = worker.handle.take() {
            let _ = handle.join();
        }
        true
    }

    pub fn cold_tier_migration_worker_running(&self) -> bool {
        self.cold_tier_migration_worker
            .lock()
            .expect("cold-tier migration worker lock poisoned")
            .is_some()
    }
}

impl ShardedMultiLayerCache {
    pub fn migrate_cold_entries_to_ssd(
        &self,
        max_entries_per_shard: usize,
    ) -> Result<CacheColdTierMigrationReport, CacheError> {
        let mut total = CacheColdTierMigrationReport::default();
        for shard in self.shards.iter() {
            let report = shard.migrate_cold_entries_to_ssd(max_entries_per_shard)?;
            total.requested = total.requested.saturating_add(report.requested);
            total.selected = total.selected.saturating_add(report.selected);
            total.migrated = total.migrated.saturating_add(report.migrated);
            total.already_persistent = total
                .already_persistent
                .saturating_add(report.already_persistent);
            total.changed_before_write = total
                .changed_before_write
                .saturating_add(report.changed_before_write);
            total.ssd_disabled |= report.ssd_disabled;
        }
        Ok(total)
    }

    pub fn start_cold_tier_migration_workers(
        &self,
        max_entries_per_round: usize,
        interval: Duration,
    ) -> usize {
        self.shards
            .iter()
            .filter(|shard| {
                shard.start_cold_tier_migration_worker(max_entries_per_round, interval)
            })
            .count()
    }

    pub fn stop_cold_tier_migration_workers(&self) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.stop_cold_tier_migration_worker())
            .count()
    }

    pub fn pause_cold_tier_migration_workers(&self) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.pause_cold_tier_migration_worker())
            .count()
    }

    pub fn resume_cold_tier_migration_workers(&self) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.resume_cold_tier_migration_worker())
            .count()
    }

    pub fn cold_tier_migration_workers_paused(&self) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.cold_tier_migration_worker_paused())
            .count()
    }

    pub fn cold_tier_migration_workers_running(&self) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.cold_tier_migration_worker_running())
            .count()
    }
}

#[cfg(test)]
mod cold_tier_migration_tests {
    use super::*;

    fn cache_with_ssd(dir: &Path) -> MultiLayerCache {
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 4096,
            pmem_capacity_bytes: 4096,
            ssd_capacity_bytes: 1 << 20,
            ssd_write_through: false,
            ..CacheTieringPolicy::default()
        };
        MultiLayerCache::with_tiering_policy(dir, policy, CacheBlockOptions::default())
    }

    #[test]
    fn bounded_pass_prepositions_oldest_resident_value() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_with_ssd(dir.path());
        let oldest = CacheKey::string(1, "oldest");
        let newest = CacheKey::string(1, "newest");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, oldest.clone(), b"cold".to_vec())
            .unwrap();
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, newest.clone(), b"hot".to_vec())
            .unwrap();

        let report = cache.migrate_cold_entries_to_ssd(1).unwrap();
        assert_eq!(report.selected, 1);
        assert_eq!(report.migrated, 1);
        let inner = cache.inner.read().unwrap();
        assert!(inner.disk_index.contains_key(&oldest));
        assert!(!inner.disk_index.contains_key(&newest));
    }

    #[test]
    fn migration_scans_a_bounded_prefix_and_resumes_at_the_next_key() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 1 << 20,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 1 << 20,
            ..CacheTieringPolicy::default()
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions::default(),
        );
        let keys = (0..200)
            .map(|index| CacheKey::string(7, &format!("scan-{index}")))
            .collect::<Vec<_>>();
        for key in &keys {
            cache
                .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), vec![7; 16])
                .unwrap();
        }
        let (expected_first, final_key) = {
            let mut inner = cache.inner.write().unwrap();
            let expected = inner.memory_order.iter_access().nth(31).unwrap().clone();
            let final_key = inner.memory_order.iter_access().last().unwrap().clone();
            for key in &keys {
                if key != &final_key {
                    inner.disk_index.insert(Arc::new(key.clone()), 1);
                }
            }
            (expected, final_key)
        };

        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().selected, 0);
        assert_eq!(
            cache.cold_tier_scan_cursor.lock().unwrap().memory_last_key,
            Some(expected_first)
        );
        for _ in 0..5 {
            assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().selected, 0);
        }
        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 1);
        assert!(cache.inner.read().unwrap().disk_index.contains_key(&final_key));
    }

    #[test]
    fn migration_does_not_starve_pmem_behind_a_large_memory_tier() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 1 << 20,
            pmem_capacity_bytes: 4096,
            ssd_capacity_bytes: 1 << 20,
            ..CacheTieringPolicy::default()
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions::default(),
        );
        for index in 0..200 {
            let key = CacheKey::string(7, &format!("resident-{index}"));
            cache
                .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), vec![7; 16])
                .unwrap();
            cache.inner.write().unwrap().disk_index.insert(Arc::new(key), 1);
        }
        let pmem_key = CacheKey::string(7, "pmem-candidate");
        cache
            .put_bypass_storage_for_tier(CacheTier::Pmem, pmem_key.clone(), b"cold".to_vec())
            .unwrap();

        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 1);
        assert!(cache.inner.read().unwrap().disk_index.contains_key(&pmem_key));
    }

    #[test]
    fn migrated_value_survives_upper_tier_removal_and_refills() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_with_ssd(dir.path());
        let key = CacheKey::string(2, "cold-read");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), b"value".to_vec())
            .unwrap();
        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 1);
        cache.invalidate_memory_only(&key);
        assert_eq!(cache.get(&key).unwrap(), Some(b"value".to_vec()));
        assert_eq!(cache.get_memory(&key), Some(b"value".to_vec()));
    }

    #[test]
    fn migration_respects_ssd_write_budget() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_with_ssd(dir.path());
        let key = CacheKey::string(2, "budgeted");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), b"value".to_vec())
            .unwrap();
        cache.inner.write().unwrap().ssd_write_budget.disable_admission();

        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 0);
        assert!(!cache.inner.read().unwrap().disk_index.contains_key(&key));
    }

    #[test]
    fn migration_rejects_oversized_values_without_evicting_ssd() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 4096,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 1 << 20,
            max_ssd_block_bytes: 16,
            ..CacheTieringPolicy::default()
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions::default(),
        );
        let key = CacheKey::string(2, "oversized");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), vec![7; 32])
            .unwrap();

        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 0);
        assert!(!cache.inner.read().unwrap().disk_index.contains_key(&key));
    }

    #[test]
    fn cold_migration_does_not_displace_a_hot_ssd_resident() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 4096,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 100,
            max_ssd_block_bytes: 100,
            ..CacheTieringPolicy::default()
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions::default(),
        );
        let hot = CacheKey::string(2, "hot");
        let cold = CacheKey::string(2, "cold");
        let hot_value = (0..48).collect::<Vec<u8>>();
        let cold_value = (48..96).collect::<Vec<u8>>();
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, hot.clone(), hot_value.clone())
            .unwrap();
        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 1);
        assert_eq!(cache.get(&hot).unwrap(), Some(hot_value));
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, cold.clone(), cold_value)
            .unwrap();

        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 0);
        let inner = cache.inner.read().unwrap();
        assert!(inner.disk_index.contains_key(&hot));
        assert!(!inner.disk_index.contains_key(&cold));
        assert_eq!(inner.stats.ssd_evictions, 0);
    }

    #[test]
    fn background_retries_do_not_inflate_ssd_admission_frequency() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CacheTieringPolicy {
            memory_capacity_bytes: 4096,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 100,
            max_ssd_block_bytes: 100,
            ..CacheTieringPolicy::default()
        };
        let cache = MultiLayerCache::with_tiering_policy(
            dir.path(),
            policy,
            CacheBlockOptions::default(),
        );
        cache.set_ssd_admission_filter_enabled(true);
        let hot = CacheKey::string(2, "hot-frequency");
        let cold = CacheKey::string(2, "cold-frequency");
        let hot_value = (0..48).collect::<Vec<u8>>();
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, hot.clone(), hot_value.clone())
            .unwrap();
        assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 1);
        cache.invalidate_memory_only(&hot);
        for _ in 0..128 {
            assert_eq!(cache.get(&hot).unwrap(), Some(hot_value.clone()));
        }
        assert!(cache.inner.read().unwrap().ssd_access_frequency
            .as_ref().unwrap().estimate(&hot) > 0);
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, cold.clone(), (48..96).collect())
            .unwrap();

        let before = cache.inner.read().unwrap().ssd_access_frequency
            .as_ref().unwrap().estimate(&cold);
        for _ in 0..8 {
            assert_eq!(cache.migrate_cold_entries_to_ssd(1).unwrap().migrated, 0);
        }
        let inner = cache.inner.read().unwrap();
        assert_eq!(inner.ssd_access_frequency.as_ref().unwrap().estimate(&cold), before);
        assert!(inner.disk_index.contains_key(&hot));
        assert!(!inner.disk_index.contains_key(&cold));
    }

    #[test]
    fn exact_tier_shared_write_keeps_the_resident_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_with_ssd(dir.path());
        let key = CacheKey::string(2, "shared-resident");
        let value: Arc<[u8]> = Arc::from(vec![7u8; 4096]);

        {
            let mut inner = cache.inner.write().unwrap();
            assert!(inner.put_memory_shared(&key, Arc::clone(&value)));
        }

        let inner = cache.inner.read().unwrap();
        let resident = inner.resident_value(&key).unwrap();
        assert!(Arc::ptr_eq(&value, resident));
    }

    #[test]
    fn worker_migrates_and_stops_promptly() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_with_ssd(dir.path());
        let key = CacheKey::string(3, "worker");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), b"value".to_vec())
            .unwrap();
        assert!(cache.start_cold_tier_migration_worker(8, Duration::from_secs(30)));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !cache.inner.read().unwrap().disk_index.contains_key(&key) {
            assert!(Instant::now() < deadline, "worker did not migrate the value");
            std::thread::sleep(Duration::from_millis(10));
        }
        let started = Instant::now();
        assert!(cache.stop_cold_tier_migration_worker());
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(!cache.cold_tier_migration_worker_running());
    }

    #[test]
    fn configured_worker_migrates_after_start_and_restarts_with_cache() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions::new(4096, 0, 1 << 20)
            .with_ssd_paths([dir.path().to_path_buf()])
            .with_cold_tier_migration_worker(1, 10);
        let cache = MultiLayerCache::try_with_options(options).unwrap();
        assert!(cache.cold_tier_migration_worker_running());
        assert_eq!(
            cache.cold_tier_migration_worker_configuration(),
            Some((1, Duration::from_millis(10)))
        );

        let first = CacheKey::string(3, "first");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, first.clone(), b"one".to_vec())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !cache.inner.read().unwrap().disk_index.contains_key(&first) {
            assert!(Instant::now() < deadline, "configured worker did not migrate");
            std::thread::sleep(Duration::from_millis(10));
        }
        cache.invalidate_memory_only(&first);
        assert_eq!(cache.get(&first).unwrap(), Some(b"one".to_vec()));
        assert_eq!(cache.get_memory(&first), Some(b"one".to_vec()));

        cache.stop();
        assert!(!cache.cold_tier_migration_worker_running());
        cache.start().unwrap();
        assert!(cache.cold_tier_migration_worker_running());
        let second = CacheKey::string(3, "second");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, second.clone(), b"two".to_vec())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !cache.inner.read().unwrap().disk_index.contains_key(&second) {
            assert!(Instant::now() < deadline, "restarted worker did not migrate");
            std::thread::sleep(Duration::from_millis(10));
        }
        cache.configure_cold_tier_migration_worker(0, Duration::from_millis(10));
        assert!(!cache.cold_tier_migration_worker_running());
        cache.stop();
    }

    #[test]
    fn configured_worker_respects_disabled_ssd_and_can_be_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions::new(4096, 0, 0)
            .with_ssd_paths([dir.path().to_path_buf()])
            .with_cold_tier_migration_worker(1, 10);
        let cache = MultiLayerCache::try_with_options(options).unwrap();
        assert!(!cache.cold_tier_migration_worker_running());
        cache.configure_cold_tier_migration_worker(0, Duration::from_millis(10));
        assert_eq!(cache.cold_tier_migration_worker_configuration(), None);
        cache.stop();
    }

    #[test]
    fn paused_worker_keeps_state_and_resume_runs_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache_with_ssd(dir.path());
        assert!(cache.start_cold_tier_migration_worker(8, Duration::from_secs(30)));
        assert!(cache.pause_cold_tier_migration_worker());
        assert!(!cache.pause_cold_tier_migration_worker());
        assert!(cache.cold_tier_migration_worker_paused());

        let key = CacheKey::string(4, "paused-worker");
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), b"value".to_vec())
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(!cache.inner.read().unwrap().disk_index.contains_key(&key));

        assert!(cache.resume_cold_tier_migration_worker());
        assert!(!cache.resume_cold_tier_migration_worker());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !cache.inner.read().unwrap().disk_index.contains_key(&key) {
            assert!(Instant::now() < deadline, "resumed worker did not run");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!cache.cold_tier_migration_worker_paused());
        assert!(cache.stop_cold_tier_migration_worker());
    }

    #[test]
    fn sharded_worker_lifecycle_reaches_every_shard() {
        let dir = tempfile::tempdir().unwrap();
        let options = CacheOptions::new(4096, 4096, 1 << 20)
            .with_ssd_paths([dir.path().to_path_buf()]);
        let cache = ShardedMultiLayerCache::with_options(options, 4);
        assert_eq!(
            cache.start_cold_tier_migration_workers(4, Duration::from_secs(30)),
            4
        );
        assert_eq!(cache.cold_tier_migration_workers_running(), 4);
        assert_eq!(cache.pause_cold_tier_migration_workers(), 4);
        assert_eq!(cache.cold_tier_migration_workers_paused(), 4);
        assert_eq!(cache.resume_cold_tier_migration_workers(), 4);
        assert_eq!(cache.cold_tier_migration_workers_paused(), 0);
        assert_eq!(cache.stop_cold_tier_migration_workers(), 4);
        assert_eq!(cache.cold_tier_migration_workers_running(), 0);
    }
}
