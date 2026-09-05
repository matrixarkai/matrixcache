// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

#[derive(Debug, Default, Clone)]
pub struct BlockCache {
    inner: Arc<RwLock<BlockCacheInner>>,
}

#[derive(Debug, Default, Clone)]
struct BlockCacheInner {
    initialized: bool,
    cache: Option<MultiLayerCache>,
    ssd_paths: Vec<PathBuf>,
    blockcache_clear_ssd_folder: bool,
}

impl BlockCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: CacheOptions) -> Self {
        let cache = Self::new();
        cache.init(options);
        cache
    }

    pub fn init(&self, options: CacheOptions) {
        let mut inner = self.inner.write().expect("blockcache lock poisoned");
        inner.cache = Some(MultiLayerCache::with_options(options.clone()));
        inner.ssd_paths = options.ssd_paths;
        inner.blockcache_clear_ssd_folder = options.blockcache_clear_ssd_folder;
        inner.initialized = false;
    }

    pub fn start(&self) -> Result<(), CacheError> {
        let (cache, ssd_paths, clear_ssd_folder) = {
            let inner = self.inner.read().expect("blockcache lock poisoned");
            (
                inner.cache.clone().ok_or_else(|| {
                    CacheError::InvalidConfig("BlockCacheImpl not initialized".to_string())
                })?,
                inner.ssd_paths.clone(),
                inner.blockcache_clear_ssd_folder,
            )
        };
        if clear_ssd_folder {
            Self::clear_ssd_paths(&ssd_paths)?;
        }
        cache.start()?;
        self.inner
            .write()
            .expect("blockcache lock poisoned")
            .initialized = true;
        Ok(())
    }

    pub fn stop(&self) -> Result<(), CacheError> {
        let (cache, ssd_paths, clear_ssd_folder) = {
            let inner = self.inner.read().expect("blockcache lock poisoned");
            if !inner.initialized {
                return Err(CacheError::Stopped);
            }
            (
                inner.cache.clone().ok_or_else(|| {
                    CacheError::InvalidConfig("BlockCacheImpl not initialized".to_string())
                })?,
                inner.ssd_paths.clone(),
                inner.blockcache_clear_ssd_folder,
            )
        };
        cache.stop();
        if clear_ssd_folder {
            Self::clear_ssd_paths(&ssd_paths)?;
        }
        self.inner
            .write()
            .expect("blockcache lock poisoned")
            .initialized = false;
        Ok(())
    }

    pub fn put(&self, key: &str, value: impl AsRef<[u8]>) -> Result<(), CacheError> {
        let cache = self.running_cache()?;
        let value = value.as_ref().to_vec();
        cache.Insert(CacheKey::string(0, key), value.clone(), value.len())
    }

    pub fn get(&self, key: &str) -> Result<Vec<u8>, CacheError> {
        let cache = self.running_cache()?;
        let Some(handle) = cache.Acquire(&CacheKey::string(0, key))? else {
            return Err(CacheError::NotFound);
        };
        let value = handle.value().to_vec();
        cache.Release(handle);
        Ok(value)
    }

    pub fn get_string(&self, key: &str) -> Result<String, CacheError> {
        String::from_utf8(self.get(key)?).map_err(|err| CacheError::CorruptBlock(err.to_string()))
    }

    pub fn is_initialized(&self) -> bool {
        self.inner
            .read()
            .expect("blockcache lock poisoned")
            .initialized
    }

    fn running_cache(&self) -> Result<MultiLayerCache, CacheError> {
        let inner = self.inner.read().expect("blockcache lock poisoned");
        if !inner.initialized {
            return Err(CacheError::Stopped);
        }
        inner
            .cache
            .clone()
            .ok_or_else(|| CacheError::InvalidConfig("BlockCacheImpl not initialized".to_string()))
    }

    fn clear_ssd_paths(paths: &[PathBuf]) -> Result<(), CacheError> {
        for path in paths {
            match fs::remove_dir_all(path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(CacheError::Io(err)),
            }
        }
        Ok(())
    }

    #[allow(non_snake_case)]
    pub fn Init(&self, options: CacheOptions) {
        self.init(options);
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> Result<(), CacheError> {
        self.start()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> Result<(), CacheError> {
        self.stop()
    }

    #[allow(non_snake_case)]
    pub fn Put(&self, key: &str, value: impl AsRef<[u8]>) -> Result<(), CacheError> {
        self.put(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Get(&self, key: &str) -> Result<Vec<u8>, CacheError> {
        self.get(key)
    }

    #[allow(non_snake_case)]
    pub fn GetString(&self, key: &str) -> Result<String, CacheError> {
        self.get_string(key)
    }
}

type CacheInstanceEvictionCallback = Arc<dyn Fn(CacheEvictionRecord) + Send + Sync + 'static>;
type CacheInstanceEvictionMetricCallback = Arc<dyn Fn(usize) + Send + Sync + 'static>;

/// One tier's worth of cache, with its own replacement policy and storage engine.
///
/// Where [`MultiLayerCache`] manages every tier together, a `CacheInstance` is a
/// single [`CacheInstanceKind`] -- except `Unified`, which spans them. It is the
/// building block the string-valued facades are made of.
///
/// Implements [`L1CacheApi`], and also [`RecoverDataCallback`] and
/// [`GcCopyCallback`] so it can receive its own recovery and collection events.
#[derive(Clone)]
pub struct CacheInstance {
    cache: MultiLayerCache,
    instance_type: CacheInstanceKind,
    replacement_type: ReplacementPolicyKind,
    storage_type: StorageEngineKind,
    eviction_callback: Arc<RwLock<Option<CacheInstanceEvictionCallback>>>,
    eviction_metric_callback: Arc<RwLock<Option<CacheInstanceEvictionMetricCallback>>>,
}

impl std::fmt::Debug for CacheInstance {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CacheInstance")
            .field("instance_type", &self.instance_type)
            .field("replacement_type", &self.replacement_type)
            .field("storage_type", &self.storage_type)
            .finish_non_exhaustive()
    }
}

impl CacheInstance {
    pub fn new(
        capacity: usize,
        replacement_type: ReplacementPolicyKind,
        storage_type: StorageEngineKind,
        paths: Vec<PathBuf>,
    ) -> Self {
        let instance_type = storage_type.as_instance_type();
        let disk_dir = paths
            .first()
            .cloned()
            .unwrap_or_else(|| unique_temp_path("cache-instance"));
        let mut options = match storage_type {
            StorageEngineKind::Dram | StorageEngineKind::Simple => {
                CacheOptions::new(capacity, 0, 0)
            }
            StorageEngineKind::Pmem => CacheOptions::new(0, capacity, 0),
            StorageEngineKind::Ssd | StorageEngineKind::MultiSsd => {
                CacheOptions::new(0, 0, capacity).with_ssd_instance_only(true)
            }
        };
        let ssd_paths = if matches!(storage_type, StorageEngineKind::Ssd | StorageEngineKind::MultiSsd)
            && !paths.is_empty()
        {
            paths.clone()
        } else {
            vec![disk_dir]
        };
        options = options.with_ssd_paths(ssd_paths).with_tier_replacement_policy(
            instance_type.as_tier().expect("storage-backed instance"),
            replacement_type.as_cache_policy(),
        );
        if matches!(storage_type, StorageEngineKind::Pmem) {
            options = options.with_pmem_paths(paths);
        }

        Self {
            cache: MultiLayerCache::with_options(options),
            instance_type,
            replacement_type,
            storage_type,
            eviction_callback: Arc::new(RwLock::new(None)),
            eviction_metric_callback: Arc::new(RwLock::new(None)),
        }
    }

    pub fn from_path_strings(
        capacity: usize,
        replacement_type: ReplacementPolicyKind,
        storage_type: StorageEngineKind,
        paths: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::new(
            capacity,
            replacement_type,
            storage_type,
            paths.into_iter().map(PathBuf::from).collect(),
        )
    }

    pub fn inner_cache(&self) -> &MultiLayerCache {
        &self.cache
    }

    fn key(key: &str) -> CacheKey {
        CacheKey::string(0, key)
    }

    fn tier(&self) -> CacheTier {
        self.instance_type
            .as_tier()
            .expect("storage-backed instance")
    }

    fn install_eviction_dispatcher(&self) {
        let tier = self.tier();
        let eviction_callback = Arc::clone(&self.eviction_callback);
        let eviction_metric_callback = Arc::clone(&self.eviction_metric_callback);
        self.cache.RegisterEvictionCallback(move |record| {
            if record.tier != tier {
                return;
            }
            if let Some(callback) = eviction_callback
                .read()
                .expect("cache instance eviction callback lock poisoned")
                .clone()
            {
                callback(record);
            }
        });
        self.cache
            .register_eviction_metric_callback(move |record_tier, count| {
                if record_tier != tier {
                    return;
                }
                if let Some(callback) = eviction_metric_callback
                    .read()
                    .expect("cache instance metric callback lock poisoned")
                    .clone()
                {
                    callback(count);
                }
            });
    }

    pub fn start(&self) -> Result<(), CacheError> {
        self.cache.start()
    }

    pub fn stop(&self) -> Result<(), CacheError> {
        self.cache.stop();
        Ok(())
    }

    pub fn put(&self, key: &str, value: Vec<u8>) -> Result<(), CacheError> {
        let started = Instant::now();
        let size = value.len();
        let result = self
            .cache
            .test_insert(self.instance_type, Self::key(key), value, size);
        self.cache
            .inner
            .write()
            .expect("cache lock poisoned")
            .record_put_latency(started);
        result
    }

    pub fn put_returning_buffer(
        &self,
        key: &str,
        value: Vec<u8>,
    ) -> Result<CacheBuffer, CacheError> {
        self.put(key, value)?;
        self.get_cache_buffer(key)?.ok_or(CacheError::NotFound)
    }

    pub fn put_cache_buffer(&self, buffer: CacheBuffer) -> Result<CacheBuffer, CacheError> {
        if buffer.key().is_empty() {
            return Err(CacheError::NotFound);
        }
        let key = buffer.key().to_string();
        let value = buffer.to_vec();
        self.put(&key, value)?;
        self.get_cache_buffer(&key)?.ok_or(CacheError::NotFound)
    }

    pub fn async_put(&self, key: &str, value: Vec<u8>, _src: &str) -> Result<(), CacheError> {
        self.put(key, value)
    }

    pub fn async_put_buffer<F>(
        &self,
        buffer: CacheBuffer,
        _src: &str,
        cb: F,
    ) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>),
    {
        let result = self.put_cache_buffer(buffer);
        match result {
            Ok(inserted) => {
                let callback_result = self
                    .get_cache_buffer(inserted.key())
                    .and_then(|buffer| buffer.ok_or(CacheError::NotFound));
                cb(callback_result);
                Ok(inserted)
            }
            Err(err) => {
                cb(Err(cache_error_for_callback(&err)));
                Err(err)
            }
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let key = Self::key(key);
        let Some(read) = self.cache.get_with_tier(&key)? else {
            return Ok(None);
        };
        if read.tier.as_cache_tier() == Some(self.tier()) {
            Ok(Some(read.value))
        } else {
            Ok(None)
        }
    }

    pub fn get_cache_buffer(&self, key: &str) -> Result<Option<CacheBuffer>, CacheError> {
        let key_for_cache = Self::key(key);
        let Some(handle) = self
            .cache
            .test_acquire(self.instance_type, &key_for_cache)?
        else {
            return Ok(None);
        };
        Ok(Some(CacheBuffer::from_handle(
            key.to_string(),
            self.cache.clone(),
            handle,
        )))
    }

    pub fn update(
        &self,
        key: &str,
        old_buffer: &CacheBuffer,
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        if old_buffer.key() != key || new_buffer.key() != key {
            return Err(CacheError::ReplaceMismatch);
        }
        let Some(handle) = old_buffer.handle.as_ref() else {
            return Err(CacheError::ReplaceMismatch);
        };
        self.cache.update_cached_value_if_current_for_tier(
            self.tier(),
            &Self::key(key),
            handle,
            new_buffer.to_vec(),
        )
    }

    pub fn update_by_old_data(
        &self,
        key: &str,
        old_data: &[u8],
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        if new_buffer.key() != key {
            return Err(CacheError::ReplaceMismatch);
        }
        let current_buffer = self.get_cache_buffer(key)?.ok_or(CacheError::NotFound)?;
        if current_buffer.data_ptr() != old_data.as_ptr() {
            return Err(CacheError::ReplaceMismatch);
        }
        let Some(handle) = current_buffer.handle.as_ref() else {
            return Err(CacheError::ReplaceMismatch);
        };
        self.cache.update_cached_value_if_current_for_tier(
            self.tier(),
            &Self::key(key),
            handle,
            new_buffer.to_vec(),
        )
    }

    pub fn get_bypass_replacement_policy(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let key = Self::key(key);
        let Some(read) = self.cache.get_bypass_replacement_policy(&key)? else {
            return Ok(None);
        };
        if read.tier.as_cache_tier() == Some(self.tier()) {
            Ok(Some(read.value))
        } else {
            Ok(None)
        }
    }

    pub fn get_bypass_replacement_policy_buffer(
        &self,
        key: &str,
    ) -> Result<Option<CacheBuffer>, CacheError> {
        let Some(value) = self.get_bypass_replacement_policy(key)? else {
            return Ok(None);
        };
        let mut buffer = CacheBuffer::new(value);
        buffer.SetKey(key);
        buffer.tier = Some(match self.tier() {
            CacheTier::Memory => CacheReadTier::Memory,
            CacheTier::Pmem => CacheReadTier::Pmem,
            CacheTier::Ssd => CacheReadTier::Ssd,
            CacheTier::Reject => return Ok(None),
        });
        Ok(Some(buffer))
    }

    pub fn peek(&self, key: &str) -> bool {
        self.cache
            .peek_tier(&Self::key(key))
            .and_then(CacheReadTier::as_cache_tier)
            == Some(self.tier())
    }

    pub fn delete(&self, key: &str) -> Result<(), CacheError> {
        self.cache.test_remove(self.instance_type, &Self::key(key))
    }

    pub fn reset(&self) -> Result<(), CacheError> {
        self.cache.reset()
    }

    pub fn recover_data(&self) -> Result<CacheRecoverReport, CacheError> {
        match self.storage_type {
            StorageEngineKind::Pmem => self.cache.recover_pmem_index(),
            StorageEngineKind::Ssd | StorageEngineKind::MultiSsd => self.cache.recover_disk_index(),
            _ => Ok(CacheRecoverReport::default()),
        }
    }

    pub fn register_eviction_handler<F>(&self, callback: F)
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        *self
            .eviction_callback
            .write()
            .expect("cache instance eviction callback lock poisoned") = Some(Arc::new(callback));
        self.install_eviction_dispatcher();
    }

    pub fn register_eviction_metric_handler<F>(&self, callback: F)
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        *self
            .eviction_metric_callback
            .write()
            .expect("cache instance metric callback lock poisoned") = Some(Arc::new(callback));
        self.install_eviction_dispatcher();
    }

    pub fn set_eviction_handler_status(&self, status: bool) {
        self.cache.set_eviction_handler_enabled(status);
    }

    pub fn eviction_handler_status(&self) -> bool {
        self.cache.eviction_handler_enabled()
    }

    pub fn capacity(&self) -> usize {
        self.cache.get_capacity(self.instance_type)
    }

    pub fn set_capacity(&self, capacity: usize) {
        self.cache
            .set_capacity_for_instance(self.instance_type, capacity);
    }

    pub fn used_space(&self) -> usize {
        self.cache.used_space_for_tier(self.tier())
    }

    pub fn item_count(&self) -> usize {
        self.cache.item_count_for_tier(self.tier())
    }

    pub fn allocator_type(&self) -> AllocatorKind {
        match self.storage_type {
            StorageEngineKind::Dram | StorageEngineKind::Simple => {
                AllocatorKind::PoolBasedAllocator
            }
            StorageEngineKind::Pmem => AllocatorKind::LogBasedAllocator,
            StorageEngineKind::Ssd | StorageEngineKind::MultiSsd => {
                AllocatorKind::LogBasedAllocator
            }
        }
    }

    pub fn storage_engine_type(&self) -> StorageEngineKind {
        self.storage_type
    }

    pub fn test_storage_engine(&self) -> StorageEngineKind {
        self.storage_engine_type()
    }

    pub fn allocator_stats(&self) -> AllocatorStats {
        self.cache.allocator_stats_for_tier(self.tier())
    }

    pub fn put_bypass_storage(&self, key: &str, value: Vec<u8>) -> Result<(), CacheError> {
        self.cache
            .put_bypass_storage_for_tier(self.tier(), Self::key(key), value)
    }

    pub fn put_bypass_storage_buffer(
        &self,
        buffer: CacheBuffer,
    ) -> Result<CacheBuffer, CacheError> {
        if buffer.key().is_empty() {
            return Err(CacheError::NotFound);
        }
        let key = buffer.key().to_string();
        self.put_bypass_storage(&key, buffer.to_vec())?;
        Ok(buffer)
    }

    pub fn on_recover_data_buffer(
        &self,
        key: &str,
        mut buffer: CacheBuffer,
    ) -> Result<CacheBuffer, CacheError> {
        buffer.set_key(key);
        self.put_bypass_storage_buffer(buffer)
    }

    pub fn register_policy_mem_eviction_handler<F>(&self, callback: F)
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        self.register_eviction_handler(callback);
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> Result<(), CacheError> {
        self.start()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> Result<(), CacheError> {
        self.stop()
    }

    #[allow(non_snake_case)]
    pub fn Put(&self, key: &str, value: Vec<u8>) -> Result<(), CacheError> {
        self.put(key, value)
    }

    #[allow(non_snake_case)]
    pub fn PutReturningBuffer(&self, key: &str, value: Vec<u8>) -> Result<CacheBuffer, CacheError> {
        self.put_returning_buffer(key, value)
    }

    #[allow(non_snake_case)]
    pub fn PutBuffer(&self, buffer: CacheBuffer) -> Result<CacheBuffer, CacheError> {
        self.put_cache_buffer(buffer)
    }

    #[allow(non_snake_case)]
    pub fn AsyncPut(&self, key: &str, value: Vec<u8>, src: &str) -> Result<(), CacheError> {
        self.async_put(key, value, src)
    }

    #[allow(non_snake_case)]
    pub fn AsyncPutBuffer<F>(
        &self,
        buffer: CacheBuffer,
        src: &str,
        cb: F,
    ) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>),
    {
        self.async_put_buffer(buffer, src, cb)
    }

    #[allow(non_snake_case)]
    pub fn Get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        self.get(key)
    }

    #[allow(non_snake_case)]
    pub fn GetBuffer(&self, key: &str) -> Result<Option<CacheBuffer>, CacheError> {
        self.get_cache_buffer(key)
    }

    #[allow(non_snake_case)]
    pub fn Update(
        &self,
        key: &str,
        old_buffer: &CacheBuffer,
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        self.update(key, old_buffer, new_buffer)
    }

    #[allow(non_snake_case)]
    pub fn UpdateByOldData(
        &self,
        key: &str,
        old_data: &[u8],
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        self.update_by_old_data(key, old_data, new_buffer)
    }

    #[allow(non_snake_case)]
    pub fn UpdateByOldDataPtr(
        &self,
        key: &str,
        old_data: &[u8],
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        self.update_by_old_data(key, old_data, new_buffer)
    }

    #[allow(non_snake_case)]
    pub fn GetBypassReplacementPolicy(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        self.get_bypass_replacement_policy(key)
    }

    #[allow(non_snake_case)]
    pub fn GetBypassReplacementPolicyBuffer(
        &self,
        key: &str,
    ) -> Result<Option<CacheBuffer>, CacheError> {
        self.get_bypass_replacement_policy_buffer(key)
    }

    #[allow(non_snake_case)]
    pub fn Peek(&self, key: &str) -> bool {
        self.peek(key)
    }

    #[allow(non_snake_case)]
    pub fn Delete(&self, key: &str) -> Result<(), CacheError> {
        self.delete(key)
    }

    #[allow(non_snake_case)]
    pub fn Reset(&self) -> Result<(), CacheError> {
        self.reset()
    }

    #[allow(non_snake_case)]
    pub fn RecoverData(&self) -> Result<CacheRecoverReport, CacheError> {
        self.recover_data()
    }

    #[allow(non_snake_case)]
    pub fn RegisterEvictionHandler<F>(&self, callback: F)
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        self.register_eviction_handler(callback);
    }

    #[allow(non_snake_case)]
    pub fn RegisterEvictionMetricHandler<F>(&self, callback: F)
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        self.register_eviction_metric_handler(callback);
    }

    #[allow(non_snake_case)]
    pub fn SetEvictionHandlerStatus(&self, status: bool) {
        self.set_eviction_handler_status(status);
    }

    #[allow(non_snake_case)]
    pub fn GetEvictionHandlerStatus(&self) -> bool {
        self.eviction_handler_status()
    }

    #[allow(non_snake_case)]
    pub fn GetCapacity(&self) -> usize {
        self.capacity()
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    #[allow(non_snake_case)]
    pub fn GetUsedSpace(&self) -> usize {
        self.used_space()
    }

    #[allow(non_snake_case)]
    pub fn GetItemNum(&self) -> usize {
        self.item_count()
    }

    #[allow(non_snake_case)]
    pub fn GetAllocatorType(&self) -> AllocatorKind {
        self.allocator_type()
    }

    #[allow(non_snake_case)]
    pub fn StorageEngineType(&self) -> StorageEngineKind {
        self.storage_engine_type()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetStorageEngine(&self) -> StorageEngineKind {
        self.test_storage_engine()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetStorageEngineType(&self) -> StorageEngineKind {
        self.test_storage_engine()
    }

    #[allow(non_snake_case)]
    pub fn GetAllocatorStats(&self) -> AllocatorStats {
        self.allocator_stats()
    }

    #[allow(non_snake_case)]
    pub fn PutBypassStorage(&self, key: &str, value: Vec<u8>) -> Result<(), CacheError> {
        self.put_bypass_storage(key, value)
    }

    #[allow(non_snake_case)]
    pub fn PutBypassStorageBuffer(&self, buffer: CacheBuffer) -> Result<CacheBuffer, CacheError> {
        self.put_bypass_storage_buffer(buffer)
    }

    #[allow(non_snake_case)]
    pub fn OnRecoverData(&self, key: &str, buffer: CacheBuffer) -> Result<CacheBuffer, CacheError> {
        self.on_recover_data_buffer(key, buffer)
    }

    #[allow(non_snake_case)]
    pub fn RegisterPolicyMemEvictionHandler<F>(&self, callback: F)
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        self.register_policy_mem_eviction_handler(callback);
    }

    pub fn latency_summary_line(&self, comments: impl AsRef<str>) -> String {
        let report = self.cache.latency_metrics_report();
        format!(
            "matrixcache_latency comments={} get_count={} get_avg_us={} get_p50_us={} get_p95_us={} get_p99_us={} get_max_us={} put_count={} put_avg_us={} put_p50_us={} put_p95_us={} put_p99_us={} put_max_us={} read_through_count={} read_through_avg_us={} read_through_p50_us={} read_through_p95_us={} read_through_p99_us={} read_through_max_us={} refill_count={} refill_avg_us={} refill_p50_us={} refill_p95_us={} refill_p99_us={} refill_max_us={} writeback_count={} writeback_avg_us={} writeback_p50_us={} writeback_p95_us={} writeback_p99_us={} writeback_max_us={} eviction_count={} eviction_avg_us={} eviction_p50_us={} eviction_p95_us={} eviction_p99_us={} eviction_max_us={} compaction_count={} compaction_avg_us={} compaction_p50_us={} compaction_p95_us={} compaction_p99_us={} compaction_max_us={} histogram_ready={}",
            comments.as_ref(),
            report.get_count,
            report.get_avg_us,
            report.get_p50_us,
            report.get_p95_us,
            report.get_p99_us,
            report.get_max_us,
            report.put_count,
            report.put_avg_us,
            report.put_p50_us,
            report.put_p95_us,
            report.put_p99_us,
            report.put_max_us,
            report.read_through_count,
            report.read_through_avg_us,
            report.read_through_p50_us,
            report.read_through_p95_us,
            report.read_through_p99_us,
            report.read_through_max_us,
            report.refill_count,
            report.refill_avg_us,
            report.refill_p50_us,
            report.refill_p95_us,
            report.refill_p99_us,
            report.refill_max_us,
            report.writeback_count,
            report.writeback_avg_us,
            report.writeback_p50_us,
            report.writeback_p95_us,
            report.writeback_p99_us,
            report.writeback_max_us,
            report.eviction_count,
            report.eviction_avg_us,
            report.eviction_p50_us,
            report.eviction_p95_us,
            report.eviction_p99_us,
            report.eviction_max_us,
            report.compaction_count,
            report.compaction_avg_us,
            report.compaction_p50_us,
            report.compaction_p95_us,
            report.compaction_p99_us,
            report.compaction_max_us,
            report.histogram_ready
        )
    }

    pub fn print_latency(&self, comments: impl AsRef<str>) {
        println!("{}", self.latency_summary_line(comments));
    }

    #[allow(non_snake_case)]
    pub fn PrintLatency(&self, comments: impl AsRef<str>) {
        self.print_latency(comments);
    }

    #[allow(non_snake_case)]
    pub fn TEST_JoinPmemWriteExecutor(&self) {
        self.cache.test_join_pmem_write_executor();
    }
}

impl GcCopyCallback for CacheInstance {
    fn update(
        &mut self,
        key: &str,
        old_data: &[u8],
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        self.update_by_old_data(key, old_data, new_buffer)
    }
}

impl RecoverDataCallback for CacheInstance {
    fn on_recover_data(&mut self, key: &str, buffer: CacheBuffer) {
        let _ = self.on_recover_data_buffer(key, buffer);
    }
}

impl L1CacheApi for CacheInstance {
    fn get_bypass_replacement_policy_buffer(
        &self,
        key: &str,
    ) -> Result<Option<CacheBuffer>, CacheError> {
        CacheInstance::get_bypass_replacement_policy_buffer(self, key)
    }
}

/// A read that deliberately does not count as an access.
///
/// `get_bypass_replacement_policy_buffer` returns an entry without telling the
/// replacement policy it was touched, so it cannot promote the entry or delay
/// its eviction. Use it for inspection; use the ordinary read path for anything
/// that should affect what survives.
///
/// Implemented by [`CacheInstance`] and by [`DramPmemL1Cache`], which searches a
/// DRAM instance and then an optional persistent one.
pub trait L1CacheApi {
    fn get_bypass_replacement_policy_buffer(
        &self,
        key: &str,
    ) -> Result<Option<CacheBuffer>, CacheError>;

    #[allow(non_snake_case)]
    fn GetBypassReplacementPolicy(&self, key: &str) -> Result<Option<CacheBuffer>, CacheError> {
        self.get_bypass_replacement_policy_buffer(key)
    }
}

#[derive(Debug, Clone)]
pub struct DramPmemL1Cache {
    dram_instance: CacheInstance,
    pmem_instance: Option<CacheInstance>,
    l2_pulls: Arc<RwLock<u64>>,
}

impl DramPmemL1Cache {
    pub fn new(dram_instance: CacheInstance, pmem_instance: Option<CacheInstance>) -> Self {
        Self {
            dram_instance,
            pmem_instance,
            l2_pulls: Arc::new(RwLock::new(0)),
        }
    }

    pub fn l2_pulls(&self) -> u64 {
        *self.l2_pulls.read().expect("l1 pull counter lock poisoned")
    }

    fn buffer_from_value(key: &str, value: Vec<u8>, tier: CacheReadTier) -> CacheBuffer {
        let mut buffer = CacheBuffer::new(value);
        buffer.SetKey(key);
        buffer.tier = Some(tier);
        buffer
    }

    #[allow(non_snake_case)]
    pub fn L2Pulls(&self) -> u64 {
        self.l2_pulls()
    }
}

impl L1CacheApi for DramPmemL1Cache {
    fn get_bypass_replacement_policy_buffer(
        &self,
        key: &str,
    ) -> Result<Option<CacheBuffer>, CacheError> {
        if let Some(value) = self.dram_instance.get_bypass_replacement_policy(key)? {
            *self
                .l2_pulls
                .write()
                .expect("l1 pull counter lock poisoned") += 1;
            return Ok(Some(Self::buffer_from_value(
                key,
                value,
                CacheReadTier::Memory,
            )));
        }
        if let Some(pmem_instance) = self.pmem_instance.as_ref() {
            if let Some(value) = pmem_instance.get_bypass_replacement_policy(key)? {
                *self
                    .l2_pulls
                    .write()
                    .expect("l1 pull counter lock poisoned") += 1;
                return Ok(Some(Self::buffer_from_value(
                    key,
                    value,
                    CacheReadTier::Pmem,
                )));
            }
        }
        Ok(None)
    }
}

/// Milliseconds between access-record drain passes.
pub const L2_DEFAULT_ACCESS_INTERVAL_MS: u64 = 1;
/// Milliseconds between passes that pull tail keys out of the upper tier.
pub const L2_DEFAULT_TAIL_INTERVAL_MS: u64 = 1_000;
/// Milliseconds between passes that drain the lower-tier write queue.
pub const L2_DEFAULT_WRITE_INTERVAL_MS: u64 = 1_000;
/// Access records buffered before further records are dropped.
pub const L2_DEFAULT_ACCESS_BUFFER_CAPACITY: usize = 100_000;
/// Keys pulled from a tail in one pass.
pub const L2_DEFAULT_TAIL_BATCH_SIZE: usize = 1_000;
/// Buffers queued for the lower tier before enqueue starts failing.
pub const L2_DEFAULT_WRITE_BUFFER_CAPACITY: usize = 10_000;
/// Item capacity of the adaptive policy that decides migration order.
pub const L2_DEFAULT_MAX_ARC_CACHE_ITEMS: usize = 100_000;
/// Whether access records are buffered rather than applied inline.
pub const L2_DEFAULT_ASYNC_ON_ACCESS: bool = true;
/// Whether an evicted buffer is queued for the lower tier or dropped.
pub const L2_DEFAULT_USE_EVICTION_HANDLER: bool = false;

pub struct L2CachePolicy {
    l1_cache: DramPmemL1Cache,
    l2_cache: CacheInstance,
    arc_policy: ReplacementArc,
    tail_batch_size: usize,
    access_interval_ms: u64,
    tail_interval_ms: u64,
    write_interval_ms: u64,
    async_on_access: bool,
    use_eviction_handler: bool,
    last_access_pass: Option<Instant>,
    last_tail_pass: Option<Instant>,
    last_write_pass: Option<Instant>,
    access_drop_count: u64,
    access_queue: VecDeque<(AccessRecordKind, String)>,
    write_buffer_queue: VecDeque<CacheBuffer>,
    access_buffer_capacity: usize,
    write_buffer_capacity: usize,
    tail_data_from_fetch_list: bool,
    stopped: bool,
    paused: bool,
    remove_l2_policy_func: Option<Arc<dyn Fn(CacheBuffer) + Send + Sync + 'static>>,
    access_callback_count: u64,
    pull_success_count: u64,
    pull_fail_count: u64,
    write_exist_count: u64,
    write_success_count: u64,
    write_fail_count: u64,
    write_enqueue_fail_count: u64,
}

impl L2CachePolicy {
    pub fn new(
        l1_cache: DramPmemL1Cache,
        l2_cache: CacheInstance,
        arc_policy: ReplacementArc,
        access_buffer_capacity: usize,
        tail_batch_size: usize,
        write_buffer_capacity: usize,
    ) -> Self {
        Self {
            l1_cache,
            l2_cache,
            arc_policy,
            tail_batch_size,
            access_interval_ms: L2_DEFAULT_ACCESS_INTERVAL_MS,
            tail_interval_ms: L2_DEFAULT_TAIL_INTERVAL_MS,
            write_interval_ms: L2_DEFAULT_WRITE_INTERVAL_MS,
            async_on_access: L2_DEFAULT_ASYNC_ON_ACCESS,
            use_eviction_handler: L2_DEFAULT_USE_EVICTION_HANDLER,
            last_access_pass: None,
            last_tail_pass: None,
            last_write_pass: None,
            access_drop_count: 0,
            access_queue: VecDeque::with_capacity(access_buffer_capacity),
            write_buffer_queue: VecDeque::with_capacity(write_buffer_capacity),
            access_buffer_capacity,
            write_buffer_capacity,
            tail_data_from_fetch_list: true,
            stopped: true,
            paused: false,
            remove_l2_policy_func: None,
            access_callback_count: 0,
            pull_success_count: 0,
            pull_fail_count: 0,
            write_exist_count: 0,
            write_success_count: 0,
            write_fail_count: 0,
            write_enqueue_fail_count: 0,
        }
    }

    pub fn start(&mut self) {
        self.stopped = false;
    }

    pub fn stop(&mut self) {
        self.stopped = true;
    }

    pub fn access_interval_ms(&self) -> u64 {
        self.access_interval_ms
    }

    pub fn set_access_interval_ms(&mut self, interval_ms: u64) {
        self.access_interval_ms = interval_ms;
    }

    pub fn tail_interval_ms(&self) -> u64 {
        self.tail_interval_ms
    }

    pub fn set_tail_interval_ms(&mut self, interval_ms: u64) {
        self.tail_interval_ms = interval_ms;
    }

    pub fn write_interval_ms(&self) -> u64 {
        self.write_interval_ms
    }

    pub fn set_write_interval_ms(&mut self, interval_ms: u64) {
        self.write_interval_ms = interval_ms;
    }

    pub fn async_on_access(&self) -> bool {
        self.async_on_access
    }

    /// When set, `on_access` buffers the record instead of applying it to the
    /// migration-order policy inline. Buffering keeps the caller off that
    /// update path, at the cost of the policy lagging behind the workload;
    /// records arriving once the buffer is full are dropped and counted by
    /// [`L2CachePolicy::access_drop_count`].
    pub fn set_async_on_access(&mut self, async_on_access: bool) {
        self.async_on_access = async_on_access;
    }

    pub fn use_eviction_handler(&self) -> bool {
        self.use_eviction_handler
    }

    /// When set, a buffer handed to `on_evict` is queued for the lower tier.
    /// Off by default, so an eviction drops the data instead of writing it —
    /// the tail passes are then the only path into the lower tier.
    pub fn set_use_eviction_handler(&mut self, use_eviction_handler: bool) {
        self.use_eviction_handler = use_eviction_handler;
    }

    /// Access records dropped because the buffer was full.
    pub fn access_drop_count(&self) -> u64 {
        self.access_drop_count
    }

    /// Run whichever passes are due, honouring the configured intervals.
    ///
    /// This is the scheduling half of the policy. `flush_once` runs all three
    /// passes unconditionally; `poll` paces them the way independent timers
    /// would, so a caller driving it from one loop does not write to the lower
    /// tier faster than the write interval allows — the throttling exists to
    /// keep migration writes from crowding out reads on the device. Returns
    /// the number of buffers written by this call.
    pub fn poll(&mut self) -> Result<usize, CacheError> {
        if self.stopped || self.paused {
            return Ok(0);
        }
        let now = Instant::now();
        if Self::pass_due(self.last_access_pass, now, self.access_interval_ms) {
            self.last_access_pass = Some(now);
            self.access_task_internal();
        }
        if Self::pass_due(self.last_tail_pass, now, self.tail_interval_ms) {
            self.last_tail_pass = Some(now);
            self.tail_task_internal();
        }
        if Self::pass_due(self.last_write_pass, now, self.write_interval_ms) {
            self.last_write_pass = Some(now);
            return self.write_task_internal();
        }
        Ok(0)
    }

    fn pass_due(last: Option<Instant>, now: Instant, interval_ms: u64) -> bool {
        match last {
            None => true,
            Some(last) => now.duration_since(last) >= Duration::from_millis(interval_ms),
        }
    }

    pub fn on_access(&mut self, record_type: AccessRecordKind, key: &str) {
        if self.stopped {
            return;
        }
        if !self.async_on_access {
            self.do_access(record_type, key.to_string());
            return;
        }
        if self.access_buffer_capacity != 0
            && self.access_queue.len() >= self.access_buffer_capacity
        {
            self.access_drop_count = self.access_drop_count.saturating_add(1);
            return;
        }
        self.access_queue.push_back((record_type, key.to_string()));
    }

    pub fn on_evict(&mut self, cache_buffer: CacheBuffer) {
        if self.stopped || !self.use_eviction_handler {
            return;
        }
        self.put_queue(cache_buffer);
    }

    pub fn register_remove_l2_policy_handler<F>(&mut self, func: F)
    where
        F: Fn(CacheBuffer) + Send + Sync + 'static,
    {
        self.remove_l2_policy_func = Some(Arc::new(func));
    }

    fn do_access(&mut self, record_type: AccessRecordKind, key: String) {
        match record_type {
            AccessRecordKind::Put => self.arc_policy.Put(key),
            AccessRecordKind::Get => {
                self.arc_policy.Get(&key);
            }
            AccessRecordKind::Delete => {
                self.arc_policy.Delete(&key);
            }
        }
        self.access_callback_count = self.access_callback_count.saturating_add(1);
    }

    fn put_queue(&mut self, cache_buffer: CacheBuffer) {
        if self.write_buffer_capacity != 0
            && self.write_buffer_queue.len() >= self.write_buffer_capacity
        {
            self.write_enqueue_fail_count = self.write_enqueue_fail_count.saturating_add(1);
            if let Some(remove) = self.remove_l2_policy_func.as_ref() {
                remove(cache_buffer);
            }
            return;
        }
        self.write_buffer_queue.push_back(cache_buffer);
    }

    pub fn fetch_queue(&mut self) -> Option<CacheBuffer> {
        self.write_buffer_queue.pop_front()
    }

    pub fn do_one_write(&mut self, buffer: CacheBuffer) -> Result<(), CacheError> {
        let key = buffer.Key().to_string();
        if self.l2_cache.Peek(&key) {
            self.write_exist_count = self.write_exist_count.saturating_add(1);
            return Ok(());
        }
        match self.l2_cache.PutBuffer(buffer) {
            Ok(_) => {
                self.write_success_count = self.write_success_count.saturating_add(1);
                Ok(())
            }
            Err(err) => {
                self.write_fail_count = self.write_fail_count.saturating_add(1);
                Err(err)
            }
        }
    }

    pub fn access_task_internal(&mut self) {
        if self.paused {
            return;
        }
        while let Some((record_type, key)) = self.access_queue.pop_front() {
            self.do_access(record_type, key);
        }
    }

    pub fn tail_task_internal(&mut self) {
        if self.stopped || self.paused {
            return;
        }
        let tail = if self.tail_data_from_fetch_list {
            self.arc_policy.GetFetchTail(self.tail_batch_size)
        } else {
            self.arc_policy.GetActiveTail(self.tail_batch_size)
        };
        self.tail_data_from_fetch_list = !self.tail_data_from_fetch_list;

        for key in tail {
            match self.l1_cache.GetBypassReplacementPolicy(&key) {
                Ok(Some(buffer)) => {
                    self.pull_success_count = self.pull_success_count.saturating_add(1);
                    self.put_queue(buffer);
                }
                Ok(None) | Err(_) => {
                    self.pull_fail_count = self.pull_fail_count.saturating_add(1);
                }
            }
        }
    }

    /// Drain the write queue.
    ///
    /// A buffer that fails to write is counted and skipped rather than
    /// aborting the drain, so one bad key cannot stall every entry queued
    /// behind it. Failures stay visible through
    /// [`L2CachePolicy::write_fail_count`]. Returns how many buffers were
    /// handled, which includes keys already present in the lower tier.
    pub fn write_task_internal(&mut self) -> Result<usize, CacheError> {
        if self.stopped || self.paused {
            return Ok(0);
        }
        let mut written = 0usize;
        while let Some(buffer) = self.fetch_queue() {
            if self.do_one_write(buffer).is_ok() {
                written = written.saturating_add(1);
            }
        }
        Ok(written)
    }

    pub fn flush_once(&mut self) -> Result<usize, CacheError> {
        self.access_task_internal();
        self.tail_task_internal();
        self.write_task_internal()
    }

    pub fn test_pause(&mut self) {
        self.paused = true;
    }

    pub fn test_continue(&mut self) {
        self.paused = false;
    }

    pub fn test_wait_all_task_sleep(&self) {}

    pub fn arc_policy(&self) -> &ReplacementArc {
        &self.arc_policy
    }

    pub fn arc_policy_mut(&mut self) -> &mut ReplacementArc {
        &mut self.arc_policy
    }

    pub fn l2_cache(&self) -> &CacheInstance {
        &self.l2_cache
    }

    pub fn access_buffer_size(&self) -> usize {
        self.access_queue.len()
    }

    pub fn write_buffer_size(&self) -> usize {
        self.write_buffer_queue.len()
    }

    pub fn access_callback_count(&self) -> u64 {
        self.access_callback_count
    }

    pub fn pull_success_count(&self) -> u64 {
        self.pull_success_count
    }

    pub fn pull_fail_count(&self) -> u64 {
        self.pull_fail_count
    }

    pub fn write_success_count(&self) -> u64 {
        self.write_success_count
    }

    pub fn write_exist_count(&self) -> u64 {
        self.write_exist_count
    }

    pub fn write_fail_count(&self) -> u64 {
        self.write_fail_count
    }

    pub fn write_enqueue_fail_count(&self) -> u64 {
        self.write_enqueue_fail_count
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    #[allow(non_snake_case)]
    pub fn Start(&mut self) {
        self.start();
    }

    #[allow(non_snake_case)]
    pub fn Stop(&mut self) {
        self.stop();
    }

    #[allow(non_snake_case)]
    pub fn OnAccess(&mut self, record_type: AccessRecordKind, key: &str) {
        self.on_access(record_type, key);
    }

    #[allow(non_snake_case)]
    pub fn OnEvict(&mut self, cache_buffer: CacheBuffer) {
        self.on_evict(cache_buffer);
    }

    #[allow(non_snake_case)]
    pub fn RegisterRemoveL2PolicyHandler<F>(&mut self, func: F)
    where
        F: Fn(CacheBuffer) + Send + Sync + 'static,
    {
        self.register_remove_l2_policy_handler(func);
    }

    #[allow(non_snake_case)]
    pub fn TEST_Pause(&mut self) {
        self.test_pause();
    }

    #[allow(non_snake_case)]
    pub fn TEST_Continue(&mut self) {
        self.test_continue();
    }

    #[allow(non_snake_case)]
    pub fn TEST_WaitAllTaskSleep(&self) {
        self.test_wait_all_task_sleep();
    }
}

pub struct L2CachePolicyFactory;

impl L2CachePolicyFactory {
    pub fn create_l2_cache_policy(
        l1_cache: DramPmemL1Cache,
        l2_cache: CacheInstance,
    ) -> L2CachePolicy {
        let mut arc_policy = ReplacementArc::new(L2_DEFAULT_MAX_ARC_CACHE_ITEMS);
        let _ = arc_policy.Init();
        L2CachePolicy::new(
            l1_cache,
            l2_cache,
            arc_policy,
            L2_DEFAULT_ACCESS_BUFFER_CAPACITY,
            L2_DEFAULT_TAIL_BATCH_SIZE,
            L2_DEFAULT_WRITE_BUFFER_CAPACITY,
        )
    }

    #[allow(non_snake_case)]
    pub fn CreateL2CachePolicy(
        l1_cache: DramPmemL1Cache,
        l2_cache: CacheInstance,
    ) -> L2CachePolicy {
        Self::create_l2_cache_policy(l1_cache, l2_cache)
    }
}

impl CacheApi for MultiLayerCache {
    fn start_cache(&self) -> bool {
        self.start_bool()
    }

    fn stop_cache(&self) -> bool {
        self.stop_bool()
    }

    fn insert_cache(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.insert(key, value, size)
    }

    fn insert_batch_cache(
        &self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<usize, CacheError> {
        self.put_batch_sized(entries)
    }

    fn lookup_cache(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup(key)
    }

    fn lookup_batch_cache(&self, keys: &[CacheKey]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        self.get_batch(keys)
    }

    fn submit_async_writeback_or_write_through_cache(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        self.submit_async_writeback_or_write_through(key, value)
    }

    fn submit_async_writeback_batch_or_write_through_cache(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        self.submit_async_writeback_batch_or_write_through(entries)
    }

    fn remove_cache(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.remove(key)
    }

    fn remove_all_cache(&self) -> Result<(), CacheError> {
        self.remove_all()
    }

    fn reset_cache(&self) -> Result<(), CacheError> {
        self.reset()
    }

    fn capacity_cache(&self) -> usize {
        self.capacity()
    }

    fn capacity_for_instance_cache(&self, instance_type: CacheInstanceKind) -> usize {
        self.get_capacity(instance_type)
    }

    fn set_capacity_cache(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    fn set_capacity_for_instance_cache(
        &self,
        instance_type: CacheInstanceKind,
        capacity: usize,
    ) {
        self.set_capacity_for_instance(instance_type, capacity);
    }

    fn size_cache(&self) -> usize {
        self.size()
    }

    fn used_cache(&self, instance_type: CacheInstanceKind) -> usize {
        self.get_used(instance_type)
    }
}

impl ZeroCopyCacheApi for MultiLayerCache {
    fn acquire_cache(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire(key)
    }

    fn acquire_batch_cache(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        self.acquire_batch(keys)
    }

    fn acquire_no_promotion_cache(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire_no_promotion(key)
    }

    fn acquire_batch_no_promotion_cache(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        self.acquire_batch_no_promotion(keys)
    }

    fn release_cache(&self, handle: CachePinnedHandle) {
        self.release(handle);
    }

    fn release_batch_cache(&self, handles: Vec<CachePinnedHandle>) -> usize {
        self.release_batch(handles)
    }

    fn insert_pinned_cache(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned_sized(key, value, size)
    }
}

fn new_memory_only_lru_cache(capacity: usize, name: &str) -> MultiLayerCache {
    MultiLayerCache::with_tiering_policy(
        unique_temp_path(name),
        CacheTieringPolicy {
            memory_capacity_bytes: capacity,
            pmem_capacity_bytes: 0,
            ssd_capacity_bytes: 0,
            data_placement: CacheDataPlacement::Tiered,
            data_placement_threshold_bytes: usize::MAX,
            memory_hotness_threshold: 0,
            pmem_admit_hotness_threshold: u32::MAX,
            ssd_admit_hotness_threshold: u32::MAX,
            max_memory_block_bytes: capacity.max(1),
            max_pmem_block_bytes: 0,
            max_ssd_block_bytes: 0,
            ssd_write_through: false,
        },
        CacheBlockOptions::default(),
    )
}

/// A plain in-memory LRU cache over [`CacheKey`] and `Vec<u8>`.
///
/// No tiers, no persistence, no admission policy -- a capacity and a recency
/// order. Values are copied out on lookup; for borrowed reads use
/// [`ZeroCopySimpleLruCache`].
///
/// An entry larger than the whole capacity is rejected silently rather than
/// evicting everything to make room.
///
/// # Examples
///
/// ```
/// use matrixcache::{CacheKey, SimpleLruCache};
///
/// let cache = SimpleLruCache::new(64);
/// let key = CacheKey::string(0, "greeting");
///
/// cache.insert(key.clone(), b"hello".to_vec(), 5)?;
/// assert_eq!(cache.lookup(&key)?, Some(b"hello".to_vec()));
/// assert_eq!(cache.size(), 5);
/// # Ok::<(), matrixcache::CacheError>(())
/// ```
#[derive(Debug, Clone)]
pub struct SimpleLruCache {
    inner: Arc<Mutex<SimpleLruInner>>,
}

impl SimpleLruCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SimpleLruInner::new(capacity))),
        }
    }

    pub fn start(&self) -> bool {
        true
    }

    pub fn stop(&self) -> bool {
        true
    }

    pub fn insert(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.inner.lock().expect("simple lru lock poisoned").insert(
            key,
            Arc::<[u8]>::from(value),
            size,
        );
        Ok(())
    }

    pub fn insert_default_size(&self, key: CacheKey, value: Vec<u8>) -> Result<(), CacheError> {
        self.insert(key, value, 1)
    }

    #[allow(non_snake_case)]
    pub fn InsertDefaultSize(&self, key: CacheKey, value: Vec<u8>) -> Result<(), CacheError> {
        self.insert_default_size(key, value)
    }

    pub fn lookup(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        Ok(self
            .inner
            .lock()
            .expect("simple lru lock poisoned")
            .lookup(key)
            .map(|value| value.to_vec()))
    }

    pub fn remove(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .remove(key);
        Ok(())
    }

    pub fn remove_all(&self) -> Result<(), CacheError> {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .remove_all();
        Ok(())
    }

    pub fn capacity(&self) -> usize {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .capacity
    }

    pub fn set_capacity(&self, capacity: usize) {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .set_capacity(capacity);
    }

    pub fn size(&self) -> usize {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .current_size()
    }

    // Pre-existing spellings, kept compiling. Each forwards to the method above
    // it; none of them carries an implementation any more.

    #[allow(non_snake_case)]
    pub fn Start(&self) -> bool {
        self.start()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> bool {
        self.stop()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.insert(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup(key)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.remove(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveAll(&self) -> Result<(), CacheError> {
        self.remove_all()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> usize {
        self.capacity()
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }
}


impl CacheApi for SimpleLruCache {
    fn start_cache(&self) -> bool {
        self.start()
    }

    fn stop_cache(&self) -> bool {
        self.stop()
    }

    fn insert_cache(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.insert(key, value, size)
    }

    fn lookup_cache(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup(key)
    }

    fn remove_cache(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.remove(key)
    }

    fn remove_all_cache(&self) -> Result<(), CacheError> {
        self.remove_all()
    }

    fn capacity_cache(&self) -> usize {
        self.capacity()
    }

    fn set_capacity_cache(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    fn size_cache(&self) -> usize {
        self.size()
    }
}

/// [`SimpleLruCache`] with pinned reads.
///
/// The same LRU behaviour, plus `acquire`/`release` and `insert_pinned` from
/// [`ZeroCopyCacheApi`], which hand back the stored bytes instead of copying
/// them.
///
/// Pinning changes eviction here in a way it does not on the plain cache: a
/// pinned entry cannot be evicted, so a cache whose entries are all held can
/// grow past its capacity.
///
/// # Examples
///
/// Acquiring borrows the stored bytes; releasing gives the pin back.
///
/// ```
/// use matrixcache::{CacheKey, ZeroCopySimpleLruCache};
///
/// let cache = ZeroCopySimpleLruCache::new(1024);
/// let key = CacheKey::string(0, "greeting");
/// cache.insert(key.clone(), b"hello".to_vec(), 5)?;
///
/// let handle = cache.acquire(&key)?.expect("just inserted");
/// assert_eq!(handle.as_slice(), b"hello");
/// cache.release(handle);
/// # Ok::<(), matrixcache::CacheError>(())
/// ```
#[derive(Debug, Clone)]
pub struct ZeroCopySimpleLruCache {
    inner: Arc<Mutex<SimpleLruInner>>,
}

impl ZeroCopySimpleLruCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SimpleLruInner::new(capacity))),
        }
    }

    pub fn start(&self) -> bool {
        true
    }

    pub fn stop(&self) -> bool {
        true
    }

    pub fn insert(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        let _ = self.insert_pinned(key, value, size)?;
        Ok(())
    }

    pub fn insert_default_size(&self, key: CacheKey, value: Vec<u8>) -> Result<(), CacheError> {
        self.insert(key, value, 1)
    }

    #[allow(non_snake_case)]
    pub fn InsertDefaultSize(&self, key: CacheKey, value: Vec<u8>) -> Result<(), CacheError> {
        self.insert_default_size(key, value)
    }

    pub fn lookup(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        Ok(self
            .inner
            .lock()
            .expect("simple lru lock poisoned")
            .lookup(key)
            .map(|value| value.to_vec()))
    }

    pub fn remove(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .remove(key);
        Ok(())
    }

    pub fn remove_all(&self) -> Result<(), CacheError> {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .remove_all();
        Ok(())
    }

    pub fn capacity(&self) -> usize {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .capacity
    }

    pub fn set_capacity(&self, capacity: usize) {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .set_capacity(capacity);
    }

    pub fn size(&self) -> usize {
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .current_size()
    }

    pub fn acquire(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError> {
        Ok(self
            .inner
            .lock()
            .expect("simple lru lock poisoned")
            .lookup(key)
            .map(|value| CachePinnedHandle {
                key: key.clone(),
                value,
                tier: CacheReadTier::Memory,
            }))
    }

    pub fn release(&self, handle: CachePinnedHandle) {
        drop(handle);
        self.inner
            .lock()
            .expect("simple lru lock poisoned")
            .evict_unpinned();
    }

    pub fn insert_pinned(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        let value = Arc::<[u8]>::from(value);
        let inserted = self.inner.lock().expect("simple lru lock poisoned").insert(
            key.clone(),
            Arc::clone(&value),
            size,
        );
        if inserted {
            Ok(Some(CachePinnedHandle {
                key,
                value,
                tier: CacheReadTier::Memory,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert_pinned_default_size(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned(key, value, 1)
    }

    #[allow(non_snake_case)]
    pub fn InsertPinnedDefaultSize(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned_default_size(key, value)
    }

    // Pre-existing spellings, kept compiling. Each forwards to the method above
    // it; none of them carries an implementation any more.

    #[allow(non_snake_case)]
    pub fn Start(&self) -> bool {
        self.start()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> bool {
        self.stop()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.insert(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup(key)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.remove(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveAll(&self) -> Result<(), CacheError> {
        self.remove_all()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> usize {
        self.capacity()
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }

    #[allow(non_snake_case)]
    pub fn Acquire(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire(key)
    }

    #[allow(non_snake_case)]
    pub fn Release(&self, handle: CachePinnedHandle) {
        self.release(handle);
    }

    #[allow(non_snake_case)]
    pub fn InsertPinned(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned(key, value, size)
    }
}


impl CacheApi for ZeroCopySimpleLruCache {
    fn start_cache(&self) -> bool {
        self.start()
    }

    fn stop_cache(&self) -> bool {
        self.stop()
    }

    fn insert_cache(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.insert(key, value, size)
    }

    fn lookup_cache(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup(key)
    }

    fn remove_cache(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.remove(key)
    }

    fn remove_all_cache(&self) -> Result<(), CacheError> {
        self.remove_all()
    }

    fn capacity_cache(&self) -> usize {
        self.capacity()
    }

    fn set_capacity_cache(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    fn size_cache(&self) -> usize {
        self.size()
    }
}

impl ZeroCopyCacheApi for ZeroCopySimpleLruCache {
    fn acquire_cache(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire(key)
    }

    fn release_cache(&self, handle: CachePinnedHandle) {
        self.release(handle);
    }

    fn insert_pinned_cache(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned(key, value, size)
    }
}

#[derive(Debug)]
struct SimpleLruEntry {
    value: Arc<[u8]>,
    size: usize,
}

#[derive(Debug)]
struct SimpleLruInner {
    capacity: usize,
    size: usize,
    order: CacheKeyOrder,
    entries: HashMap<CacheKey, SimpleLruEntry>,
    /// Entries removed while a handle still pinned them.
    ///
    /// Their bytes are still resident, so they keep counting towards the
    /// cache size until the last pin is released. Dropping them from the
    /// accounting at removal would let the cache admit data it has no room
    /// for.
    pinned_removed: Vec<SimpleLruEntry>,
}

impl SimpleLruInner {
    fn new(capacity: usize) -> Self {
        assert!(
            capacity > 0,
            "simple lru capacity must be greater than zero"
        );
        Self {
            capacity,
            size: 0,
            order: CacheKeyOrder::new(),
            entries: HashMap::new(),
            pinned_removed: Vec::new(),
        }
    }

    fn insert(&mut self, key: CacheKey, value: Arc<[u8]>, size: usize) -> bool {
        assert!(size > 0, "simple lru entry size must be greater than zero");
        if size > self.capacity {
            return false;
        }
        self.reap_pinned_removed();
        self.remove(&key);
        self.size += size;
        self.order.push_front(key.clone());
        self.entries.insert(key, SimpleLruEntry { value, size });
        self.evict_unpinned();
        true
    }

    fn lookup(&mut self, key: &CacheKey) -> Option<Arc<[u8]>> {
        let value = self.entries.get(key)?.value.clone();
        self.touch(key);
        Some(value)
    }

    fn remove(&mut self, key: &CacheKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.order.remove(key);
            self.retire(entry);
        }
    }

    fn remove_all(&mut self) {
        let retired = self.entries.drain().map(|(_, entry)| entry).collect::<Vec<_>>();
        for entry in retired {
            self.retire(entry);
        }
        self.order.clear();
    }

    /// Drop an entry from the index, keeping its bytes counted while a handle
    /// still holds the value.
    fn retire(&mut self, entry: SimpleLruEntry) {
        if Arc::strong_count(&entry.value) > 1 {
            self.pinned_removed.push(entry);
        } else {
            self.size = self.size.saturating_sub(entry.size);
        }
    }

    /// Release the bytes of any removed entry whose last pin has now dropped.
    fn reap_pinned_removed(&mut self) {
        let mut released = 0usize;
        self.pinned_removed.retain(|entry| {
            if Arc::strong_count(&entry.value) > 1 {
                return true;
            }
            released = released.saturating_add(entry.size);
            false
        });
        self.size = self.size.saturating_sub(released);
    }

    /// Current size, after accounting for pins released since the last call.
    fn current_size(&mut self) -> usize {
        self.reap_pinned_removed();
        self.size
    }

    fn set_capacity(&mut self, capacity: usize) {
        assert!(
            capacity > 0,
            "simple lru capacity must be greater than zero"
        );
        self.capacity = capacity;
        self.evict_unpinned();
    }

    fn touch(&mut self, key: &CacheKey) {
        // Moves the key to the front if it is already tracked, so a lookup
        // costs the same whether the cache holds ten entries or ten million.
        self.order.push_front(key.clone());
    }

    fn evict_unpinned(&mut self) {
        self.reap_pinned_removed();
        while self.size > self.capacity {
            // Walk from the least recently used end and stop at the first
            // entry nobody is holding. Scanning forward for the last match
            // visited every entry on every eviction; from this end the usual
            // case stops immediately.
            let victim = self
                .order
                .iter_rev()
                .find(|key| {
                    self.entries
                        .get(key)
                        .is_some_and(|entry| Arc::strong_count(&entry.value) == 1)
                })
                .cloned();
            let Some(key) = victim else {
                break;
            };
            self.order.remove(&key);
            if let Some(entry) = self.entries.remove(&key) {
                self.size = self.size.saturating_sub(entry.size);
            }
        }
    }
}

/// A `String`-valued LRU cache backed by a [`MultiLayerCache`] shard.
///
/// Implements [`StringCacheApi`]. Despite the name it is not a variant of
/// [`SimpleLruCache`] -- it delegates to the multi-tier cache, which is what
/// makes it safe to share.
#[derive(Debug, Clone)]
pub struct ConcurrentSimpleLruCache {
    cache: MultiLayerCache,
    shard_id: ShardId,
}


impl ConcurrentSimpleLruCache {
    pub fn new(capacity: usize) -> Self {
        Self::with_shard_id(capacity, 0)
    }

    pub fn with_shard_id(capacity: usize, shard_id: ShardId) -> Self {
        let cache = new_memory_only_lru_cache(capacity, "simple-lru-string-cache");
        Self { cache, shard_id }
    }

    fn cache_key(&self, key: &str) -> CacheKey {
        CacheKey::string(self.shard_id, key)
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> bool {
        self.start_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> bool {
        self.stop_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        self.insert_string(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn InsertDefaultSize(&self, key: &str, value: String) -> Result<(), CacheError> {
        self.insert_string_default_size(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.lookup_string(key)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &str) -> Result<(), CacheError> {
        self.remove_string(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveAll(&self) -> Result<(), CacheError> {
        self.remove_all_string()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> usize {
        self.capacity_string()
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity_string(capacity);
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size_string()
    }
}

impl StringCacheApi for ConcurrentSimpleLruCache {
    fn start_string_cache(&self) -> bool {
        self.cache.Start()
    }

    fn stop_string_cache(&self) -> bool {
        self.cache.Stop()
    }

    fn insert_string(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        self.cache.put_with_admission(
            self.cache_key(key),
            value.into_bytes(),
            CacheAdmissionRequest {
                block_kind: CacheBlockKind::Object,
                shard_id: self.shard_id,
                routing_slot: None,
                block_bytes: size,
                hotness: u32::MAX,
                pinned: false,
            },
        )
    }

    fn lookup_string(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.cache
            .get(&self.cache_key(key))?
            .map(String::from_utf8)
            .transpose()
            .map_err(|err| CacheError::CorruptBlock(err.to_string()))
    }

    fn remove_string(&self, key: &str) -> Result<(), CacheError> {
        self.cache.remove(&self.cache_key(key))
    }

    fn remove_all_string(&self) -> Result<(), CacheError> {
        self.cache.remove_all()
    }

    fn capacity_string(&self) -> usize {
        self.cache.capacity_for_tier(CacheTier::Memory)
    }

    fn set_capacity_string(&self, capacity: usize) {
        self.cache
            .set_capacity_for_tier(CacheTier::Memory, capacity);
    }

    fn size_string(&self) -> usize {
        self.cache.size_for_tier(CacheTier::Memory)
    }
}

/// A `String`-valued cache offering a memcached-shaped surface, in process.
///
/// There is no memcached here and no daemon: storage is an in-process map, and
/// `client` hands back a synthetic id so code written against a client pool
/// compiles unchanged. Implements [`StringCacheApi`].
#[derive(Debug, Clone)]
pub struct InProcessMemcachedCache {
    capacity: Arc<RwLock<usize>>,
    started: Arc<RwLock<bool>>,
    entries: Arc<RwLock<HashMap<String, String>>>,
    sizes: Arc<RwLock<HashMap<String, usize>>>,
    reset_clients_count: Arc<RwLock<u64>>,
}

impl InProcessMemcachedCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: Arc::new(RwLock::new(capacity)),
            started: Arc::new(RwLock::new(false)),
            entries: Arc::new(RwLock::new(HashMap::new())),
            sizes: Arc::new(RwLock::new(HashMap::new())),
            reset_clients_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn client(&self) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::thread::current().id().hash(&mut hasher);
        hasher.finish() as usize
    }

    pub fn reset_clients(&self) {
        *self
            .reset_clients_count
            .write()
            .expect("memcached reset count lock poisoned") += 1;
    }

    pub fn reset_clients_count(&self) -> u64 {
        *self
            .reset_clients_count
            .read()
            .expect("memcached reset count lock poisoned")
    }

    pub fn configured_capacity(&self) -> usize {
        *self
            .capacity
            .read()
            .expect("memcached capacity lock poisoned")
    }

    pub fn is_started(&self) -> bool {
        *self
            .started
            .read()
            .expect("memcached started lock poisoned")
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> bool {
        self.start_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> bool {
        self.stop_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        self.insert_string(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn InsertDefaultSize(&self, key: &str, value: String) -> Result<(), CacheError> {
        self.insert_string_default_size(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.lookup_string(key)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &str) -> Result<(), CacheError> {
        self.remove_string(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveAll(&self) -> Result<(), CacheError> {
        self.remove_all_string()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> usize {
        self.capacity_string()
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity_string(capacity);
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size_string()
    }

    #[allow(non_snake_case)]
    pub fn ResetClients(&self) {
        self.reset_clients();
    }
}

impl Drop for InProcessMemcachedCache {
    fn drop(&mut self) {
        self.reset_clients();
    }
}

impl StringCacheApi for InProcessMemcachedCache {
    fn start_string_cache(&self) -> bool {
        *self
            .started
            .write()
            .expect("memcached started lock poisoned") = true;
        true
    }

    fn stop_string_cache(&self) -> bool {
        *self
            .started
            .write()
            .expect("memcached started lock poisoned") = false;
        true
    }

    fn insert_string(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        if !self.is_started() {
            return Err(CacheError::Stopped);
        }
        let capacity = self.configured_capacity();
        let old_size = self
            .sizes
            .read()
            .expect("memcached sizes lock poisoned")
            .get(key)
            .copied()
            .unwrap_or(0);
        let used = self.size_string().saturating_sub(old_size);
        if capacity > 0 && used.saturating_add(size) > capacity {
            return Err(CacheError::CapacityExceeded);
        }
        self.entries
            .write()
            .expect("memcached entries lock poisoned")
            .insert(key.to_string(), value);
        self.sizes
            .write()
            .expect("memcached sizes lock poisoned")
            .insert(key.to_string(), size);
        Ok(())
    }

    fn lookup_string(&self, key: &str) -> Result<Option<String>, CacheError> {
        if !self.is_started() {
            return Err(CacheError::Stopped);
        }
        Ok(self
            .entries
            .read()
            .expect("memcached entries lock poisoned")
            .get(key)
            .cloned())
    }

    fn remove_string(&self, key: &str) -> Result<(), CacheError> {
        self.entries
            .write()
            .expect("memcached entries lock poisoned")
            .remove(key);
        self.sizes
            .write()
            .expect("memcached sizes lock poisoned")
            .remove(key);
        Ok(())
    }

    fn remove_all_string(&self) -> Result<(), CacheError> {
        self.entries
            .write()
            .expect("memcached entries lock poisoned")
            .clear();
        self.sizes
            .write()
            .expect("memcached sizes lock poisoned")
            .clear();
        Ok(())
    }

    fn capacity_string(&self) -> usize {
        self.configured_capacity()
    }

    fn set_capacity_string(&self, capacity: usize) {
        *self
            .capacity
            .write()
            .expect("memcached capacity lock poisoned") = capacity;
    }

    fn size_string(&self) -> usize {
        self.sizes
            .read()
            .expect("memcached sizes lock poisoned")
            .values()
            .copied()
            .sum()
    }
}

/// A `String`-valued cache whose policy, engine and paths are chosen by name.
///
/// Wraps one [`CacheInstance`] configured from strings rather than typed
/// options, which is what "flexible" refers to. It keeps the resolved
/// [`ReplacementPolicyKind`], [`StorageEngineKind`] and paths so a caller can
/// see what the names became.
///
/// Note this is not confined to a single tier: an instance built as
/// [`CacheInstanceKind::Unified`] spans all of them.
#[derive(Debug, Clone)]
pub struct FlexibleCache {
    instance: CacheInstance,
    policy: ReplacementPolicyKind,
    engine: StorageEngineKind,
    paths: Vec<PathBuf>,
}

impl FlexibleCache {
    pub fn new(
        capacity: usize,
        policy: impl AsRef<str>,
        engine: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = PathBuf>,
        ssd_paths: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        let policy = ReplacementPolicyKind::from_config_name(policy.as_ref());
        let engine = StorageEngineKind::from_config_name(engine.as_ref());
        let paths = match engine {
            StorageEngineKind::Pmem => pmem_paths.into_iter().collect::<Vec<_>>(),
            StorageEngineKind::Ssd | StorageEngineKind::MultiSsd => {
                ssd_paths.into_iter().collect::<Vec<_>>()
            }
            StorageEngineKind::Dram | StorageEngineKind::Simple => Vec::new(),
        };
        Self {
            instance: CacheInstance::new(capacity, policy, engine, paths.clone()),
            policy,
            engine,
            paths,
        }
    }

    pub fn from_path_strings(
        capacity: usize,
        policy: impl AsRef<str>,
        engine: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = String>,
        ssd_paths: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::new(
            capacity,
            policy,
            engine,
            pmem_paths.into_iter().map(PathBuf::from),
            ssd_paths.into_iter().map(PathBuf::from),
        )
    }

    pub fn instance(&self) -> &CacheInstance {
        &self.instance
    }

    pub fn policy(&self) -> ReplacementPolicyKind {
        self.policy
    }

    pub fn engine(&self) -> StorageEngineKind {
        self.engine
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn calculate_space_amplification(&self) -> Option<f64> {
        let logical = self
            .instance
            .inner_cache()
            .used_space_for_tier(self.engine.as_instance_type().as_tier()?);
        let physical = self.instance.GetUsedSpace();
        if logical == 0 {
            None
        } else {
            Some(physical as f64 / logical as f64)
        }
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> bool {
        self.start_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> bool {
        self.stop_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        self.insert_string(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn InsertDefaultSize(&self, key: &str, value: String) -> Result<(), CacheError> {
        self.insert_string_default_size(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.lookup_string(key)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &str) -> Result<(), CacheError> {
        self.remove_string(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveAll(&self) -> Result<(), CacheError> {
        self.remove_all_string()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> usize {
        self.capacity_string()
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity_string(capacity);
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size_string()
    }

    #[allow(non_snake_case)]
    pub fn CalculateSpaceAmplification(&self) -> Option<f64> {
        self.calculate_space_amplification()
    }
}

impl StringCacheApi for FlexibleCache {
    fn start_string_cache(&self) -> bool {
        self.instance.Start().is_ok()
    }

    fn stop_string_cache(&self) -> bool {
        self.instance.Stop().is_ok()
    }

    fn insert_string(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        let mut buffer = CacheBuffer::new(value.into_bytes());
        buffer.SetKey(key);
        let _ = size;
        self.instance.PutBuffer(buffer).map(|_| ())
    }

    fn lookup_string(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.instance
            .Get(key)?
            .map(String::from_utf8)
            .transpose()
            .map_err(|err| CacheError::CorruptBlock(err.to_string()))
    }

    fn remove_string(&self, key: &str) -> Result<(), CacheError> {
        self.instance.Delete(key)
    }

    fn remove_all_string(&self) -> Result<(), CacheError> {
        self.instance.Reset()
    }

    fn capacity_string(&self) -> usize {
        self.instance.GetCapacity()
    }

    fn set_capacity_string(&self, capacity: usize) {
        self.instance.SetCapacity(capacity);
    }

    fn size_string(&self) -> usize {
        self.instance.GetUsedSpace()
    }
}

/// A minimal `String`-valued view of a [`MultiLayerCache`] shard.
///
/// [`MultiTierCache`] without the configuration it was built from -- just the
/// [`StringCacheApi`] surface over one shard.
#[derive(Debug, Clone)]
pub struct MultiTierStringCache {
    cache: MultiLayerCache,
    shard_id: ShardId,
}

impl MultiTierStringCache {
    pub fn new(options: CacheOptions) -> Self {
        Self::with_shard_id(options, 0)
    }

    pub fn with_shard_id(options: CacheOptions, shard_id: ShardId) -> Self {
        Self {
            cache: MatrixCacheBuilder::build_zero_copy_cache(options),
            shard_id,
        }
    }

    fn cache_key(&self, key: &str) -> CacheKey {
        CacheKey::string(self.shard_id, key)
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> bool {
        self.start_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> bool {
        self.stop_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        self.insert_string(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn InsertDefaultSize(&self, key: &str, value: String) -> Result<(), CacheError> {
        self.insert_string_default_size(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.lookup_string(key)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &str) -> Result<(), CacheError> {
        self.remove_string(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveAll(&self) -> Result<(), CacheError> {
        self.remove_all_string()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> usize {
        self.capacity_string()
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity_string(capacity);
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size_string()
    }

    pub fn inner(&self) -> &MultiLayerCache {
        &self.cache
    }
}

impl StringCacheApi for MultiTierStringCache {
    fn start_string_cache(&self) -> bool {
        self.cache.Start()
    }

    fn stop_string_cache(&self) -> bool {
        self.cache.Stop()
    }

    fn insert_string(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        self.cache
            .Insert(self.cache_key(key), value.into_bytes(), size)
    }

    fn lookup_string(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.cache
            .Lookup(&self.cache_key(key))?
            .map(String::from_utf8)
            .transpose()
            .map_err(|err| CacheError::CorruptBlock(err.to_string()))
    }

    fn remove_string(&self, key: &str) -> Result<(), CacheError> {
        self.cache.Remove(&self.cache_key(key))
    }

    fn remove_all_string(&self) -> Result<(), CacheError> {
        self.cache.RemoveAll()
    }

    fn capacity_string(&self) -> usize {
        self.cache.Capacity()
    }

    fn set_capacity_string(&self, capacity: usize) {
        self.cache.SetCapacity(capacity);
    }

    fn size_string(&self) -> usize {
        self.cache.Size()
    }
}

/// A `String`-valued cache over every tier, built from [`CacheOptions`].
///
/// The fullest of the string facades: it keeps the options it was built from and
/// exposes the policy, storage engine and eviction setting it resolved them to,
/// along with the latency and statistics summaries.
///
/// Implements [`StringCacheApi`].
#[derive(Debug, Clone)]
pub struct MultiTierCache {
    cache: MultiLayerCache,
    options: CacheOptions,
    policy: ReplacementPolicyKind,
    ssd_storage_engine: StorageEngineKind,
    enable_eviction: bool,
    shard_id: ShardId,
}

impl MultiTierCache {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        policy: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = PathBuf>,
        ssd_paths: impl IntoIterator<Item = PathBuf>,
        dram_pmem_data_placement_type: impl AsRef<str>,
        enable_eviction: bool,
        side_by_side_dram_pmem_placement_threshold: usize,
        ssd_storage_engine: impl AsRef<str>,
    ) -> Self {
        Self::try_new(
            dram_capacity,
            pmem_capacity,
            ssd_capacity,
            policy,
            pmem_paths,
            ssd_paths,
            dram_pmem_data_placement_type,
            enable_eviction,
            side_by_side_dram_pmem_placement_threshold,
            ssd_storage_engine,
        )
        .expect("valid placement config")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        policy: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = PathBuf>,
        ssd_paths: impl IntoIterator<Item = PathBuf>,
        dram_pmem_data_placement_type: impl AsRef<str>,
        enable_eviction: bool,
        side_by_side_dram_pmem_placement_threshold: usize,
        ssd_storage_engine: impl AsRef<str>,
    ) -> Result<Self, CacheError> {
        let policy_type = ReplacementPolicyKind::from_config_name(policy.as_ref());
        let replacement_policy = policy_type.as_cache_policy();
        let ssd_storage_engine = StorageEngineKind::from_config_name(ssd_storage_engine.as_ref());
        let data_placement =
            CacheDataPlacement::try_from_config_name(dram_pmem_data_placement_type.as_ref())?;
        let options = CacheOptions::new(dram_capacity, pmem_capacity, ssd_capacity)
            .with_pmem_paths(pmem_paths)
            .with_ssd_paths(ssd_paths)
            .with_replacement_policy(replacement_policy)
            .with_dram_pmem_data_placement(
                data_placement,
                side_by_side_dram_pmem_placement_threshold,
            );
        Ok(Self::from_options(
            options,
            policy_type,
            ssd_storage_engine,
            enable_eviction,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_path_strings(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        policy: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = String>,
        ssd_paths: impl IntoIterator<Item = String>,
        dram_pmem_data_placement_type: impl AsRef<str>,
        enable_eviction: bool,
        side_by_side_dram_pmem_placement_threshold: usize,
        ssd_storage_engine: impl AsRef<str>,
    ) -> Self {
        Self::try_from_path_strings(
            dram_capacity,
            pmem_capacity,
            ssd_capacity,
            policy,
            pmem_paths,
            ssd_paths,
            dram_pmem_data_placement_type,
            enable_eviction,
            side_by_side_dram_pmem_placement_threshold,
            ssd_storage_engine,
        )
        .expect("valid placement config")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_from_path_strings(
        dram_capacity: usize,
        pmem_capacity: usize,
        ssd_capacity: usize,
        policy: impl AsRef<str>,
        pmem_paths: impl IntoIterator<Item = String>,
        ssd_paths: impl IntoIterator<Item = String>,
        dram_pmem_data_placement_type: impl AsRef<str>,
        enable_eviction: bool,
        side_by_side_dram_pmem_placement_threshold: usize,
        ssd_storage_engine: impl AsRef<str>,
    ) -> Result<Self, CacheError> {
        Self::try_new(
            dram_capacity,
            pmem_capacity,
            ssd_capacity,
            policy,
            pmem_paths.into_iter().map(PathBuf::from),
            ssd_paths.into_iter().map(PathBuf::from),
            dram_pmem_data_placement_type,
            enable_eviction,
            side_by_side_dram_pmem_placement_threshold,
            ssd_storage_engine,
        )
    }

    pub fn from_options(
        options: CacheOptions,
        policy: ReplacementPolicyKind,
        ssd_storage_engine: StorageEngineKind,
        enable_eviction: bool,
    ) -> Self {
        let cache = MatrixCacheBuilder::build_zero_copy_cache(options.clone());
        cache.set_eviction_handler_enabled(enable_eviction);
        Self {
            cache,
            options,
            policy,
            ssd_storage_engine,
            enable_eviction,
            shard_id: 0,
        }
    }

    fn cache_key(&self, key: &str) -> CacheKey {
        CacheKey::string(self.shard_id, key)
    }

    pub fn options(&self) -> &CacheOptions {
        &self.options
    }

    pub fn policy(&self) -> ReplacementPolicyKind {
        self.policy
    }

    pub fn ssd_storage_engine(&self) -> StorageEngineKind {
        self.ssd_storage_engine
    }

    pub fn eviction_enabled(&self) -> bool {
        self.enable_eviction && self.cache.eviction_handler_enabled()
    }

    pub fn inner(&self) -> &MultiLayerCache {
        &self.cache
    }

    pub fn latency_summary_line(&self, comments: impl AsRef<str>) -> String {
        let report = self.cache.latency_metrics_report();
        format!(
            "matrixcache_latency comments={} get_count={} get_avg_us={} get_p50_us={} get_p95_us={} get_p99_us={} get_max_us={} put_count={} put_avg_us={} put_p50_us={} put_p95_us={} put_p99_us={} put_max_us={} read_through_count={} read_through_avg_us={} read_through_p50_us={} read_through_p95_us={} read_through_p99_us={} read_through_max_us={} refill_count={} refill_avg_us={} refill_p50_us={} refill_p95_us={} refill_p99_us={} refill_max_us={} writeback_count={} writeback_avg_us={} writeback_p50_us={} writeback_p95_us={} writeback_p99_us={} writeback_max_us={} eviction_count={} eviction_avg_us={} eviction_p50_us={} eviction_p95_us={} eviction_p99_us={} eviction_max_us={} compaction_count={} compaction_avg_us={} compaction_p50_us={} compaction_p95_us={} compaction_p99_us={} compaction_max_us={} histogram_ready={}",
            comments.as_ref(),
            report.get_count,
            report.get_avg_us,
            report.get_p50_us,
            report.get_p95_us,
            report.get_p99_us,
            report.get_max_us,
            report.put_count,
            report.put_avg_us,
            report.put_p50_us,
            report.put_p95_us,
            report.put_p99_us,
            report.put_max_us,
            report.read_through_count,
            report.read_through_avg_us,
            report.read_through_p50_us,
            report.read_through_p95_us,
            report.read_through_p99_us,
            report.read_through_max_us,
            report.refill_count,
            report.refill_avg_us,
            report.refill_p50_us,
            report.refill_p95_us,
            report.refill_p99_us,
            report.refill_max_us,
            report.writeback_count,
            report.writeback_avg_us,
            report.writeback_p50_us,
            report.writeback_p95_us,
            report.writeback_p99_us,
            report.writeback_max_us,
            report.eviction_count,
            report.eviction_avg_us,
            report.eviction_p50_us,
            report.eviction_p95_us,
            report.eviction_p99_us,
            report.eviction_max_us,
            report.compaction_count,
            report.compaction_avg_us,
            report.compaction_p50_us,
            report.compaction_p95_us,
            report.compaction_p99_us,
            report.compaction_max_us,
            report.histogram_ready
        )
    }

    pub fn print_latency(&self, comments: impl AsRef<str>) {
        println!("{}", self.latency_summary_line(comments));
    }

    pub fn cache_stats_summary_line(
        &self,
        metrics: impl AsRef<str>,
        comments: impl AsRef<str>,
    ) -> String {
        let stats = self.cache.stats();
        let eviction = self.cache.eviction_report();
        let writeback = self.cache.writeback_backpressure_report();
        format!(
            "matrixcache_stats metrics={} comments={} policy={} ssd_engine={} placement={} eviction_enabled={} memory_bytes={} pmem_bytes={} disk_bytes={} pinned_entries={} pinned_bytes={} memory_evictions={} pmem_evictions={} ssd_evictions={} ssd_admission_rejections={} async_writeback_queue_depth={} async_writeback_queue_bytes={} writeback_backpressure_events={}",
            metrics.as_ref(),
            comments.as_ref(),
            self.policy.as_config_name(),
            self.ssd_storage_engine.as_config_name(),
            self.cache.production_tiering_policy().data_placement.as_config_name(),
            self.eviction_enabled(),
            stats.memory_bytes,
            stats.pmem_bytes,
            stats.disk_bytes,
            stats.pinned_entries,
            stats.pinned_bytes,
            eviction.memory_evictions,
            eviction.pmem_evictions,
            eviction.ssd_evictions,
            writeback.ssd_admission_rejections,
            stats.async_writeback_queue_depth,
            stats.async_writeback_queue_bytes,
            writeback.backpressure_events
        )
    }

    pub fn print_cache_stats(&self, metrics: impl AsRef<str>, comments: impl AsRef<str>) {
        println!("{}", self.cache_stats_summary_line(metrics, comments));
    }

    pub fn measurement_summary_line(&self) -> String {
        format!(
            "{} {}",
            self.cache_stats_summary_line("measurement", ""),
            self.latency_summary_line("")
        )
    }

    pub fn print_measurement(&self) {
        println!("{}", self.measurement_summary_line());
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> bool {
        self.start_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> bool {
        self.stop_string_cache()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        self.insert_string(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn InsertDefaultSize(&self, key: &str, value: String) -> Result<(), CacheError> {
        self.insert_string_default_size(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.lookup_string(key)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &str) -> Result<(), CacheError> {
        self.remove_string(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveAll(&self) -> Result<(), CacheError> {
        self.remove_all_string()
    }

    #[allow(non_snake_case)]
    pub fn Capacity(&self) -> usize {
        self.capacity_string()
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity_string(capacity);
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size_string()
    }

    #[allow(non_snake_case)]
    pub fn PrintLatency(&self, comments: impl AsRef<str>) {
        self.print_latency(comments);
    }

    #[allow(non_snake_case)]
    pub fn PrintCacheStats(&self, metrics: impl AsRef<str>, comments: impl AsRef<str>) {
        self.print_cache_stats(metrics, comments);
    }

    #[allow(non_snake_case)]
    pub fn PrintMeasurement(&self) {
        self.print_measurement();
    }
}

impl StringCacheApi for MultiTierCache {
    fn start_string_cache(&self) -> bool {
        self.cache.Start()
    }

    fn stop_string_cache(&self) -> bool {
        self.cache.Stop()
    }

    fn insert_string(&self, key: &str, value: String, size: usize) -> Result<(), CacheError> {
        self.cache
            .Insert(self.cache_key(key), value.into_bytes(), size)
    }

    fn lookup_string(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.cache
            .Lookup(&self.cache_key(key))?
            .map(String::from_utf8)
            .transpose()
            .map_err(|err| CacheError::CorruptBlock(err.to_string()))
    }

    fn remove_string(&self, key: &str) -> Result<(), CacheError> {
        self.cache.Remove(&self.cache_key(key))
    }

    fn remove_all_string(&self) -> Result<(), CacheError> {
        self.cache.RemoveAll()
    }

    fn capacity_string(&self) -> usize {
        self.cache.Capacity()
    }

    fn set_capacity_string(&self, capacity: usize) {
        self.cache.SetCapacity(capacity);
    }

    fn size_string(&self) -> usize {
        self.cache.Size()
    }
}

const CACHE_ORDER_NIL: u32 = u32::MAX;

#[derive(Debug, Clone)]
struct CacheOrderNode {
    key: CacheKey,
    prev: u32,
    next: u32,
    access_prev: u32,
    access_next: u32,
    /// Whether this node sits strictly between the eviction end and the
    /// insertion point. Carried here because the alternative -- walking to find
    /// out -- is `len() >> spec` steps, and `touch_access` would pay it on
    /// every hit.
    access_cold: bool,
}

/// Recency ordering over cache keys, from least recently used at the front to
/// most recently used at the back.
///
/// A `VecDeque<CacheKey>` has to be rescanned to move a key to the back, so a
/// cache hit costs O(n) in the number of resident entries. This keeps the same
/// ordering in an intrusive doubly-linked list over a node arena, with an index
/// from key to node, so recording a hit is O(1) regardless of how much is
/// cached. Freed nodes are recycled, so churn does not grow the arena.
///
/// Bulk removal by predicate is still a scan; those run on invalidation paths,
/// not on a hit.
#[derive(Debug, Clone)]
pub struct CacheKeyOrder {
    nodes: Vec<CacheOrderNode>,
    free: Vec<u32>,
    index: HashMap<CacheKey, u32>,
    head: u32,
    tail: u32,
    access_head: u32,
    access_tail: u32,
    /// The node a new entry is linked in front of, on the eviction side.
    /// `CACHE_ORDER_NIL` means new entries go to the most-recently-used end,
    /// which is the behaviour when the spec is zero.
    access_insert: u32,
    /// How many entries sit between `access_head` and `access_insert` -- the
    /// ones that would be evicted before a newly inserted entry. CacheLib
    /// calls this the tail size; the orientation here is reversed.
    access_cold: usize,
    /// New entries are placed with `len() >> spec` entries closer to eviction
    /// than themselves. Zero puts them at the most-recently-used end.
    insertion_spec: u8,
}

impl Default for CacheKeyOrder {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheKeyOrder {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
            head: CACHE_ORDER_NIL,
            tail: CACHE_ORDER_NIL,
            access_head: CACHE_ORDER_NIL,
            access_tail: CACHE_ORDER_NIL,
            access_insert: CACHE_ORDER_NIL,
            access_cold: 0,
            insertion_spec: 0,
        }
    }

    /// Sets where a new entry is placed in the access order.
    ///
    /// Zero -- the default -- puts it at the most-recently-used end, so it has
    /// the whole order to traverse before it can be evicted. One puts it
    /// halfway down, two a quarter of the way from the eviction end, and so on:
    /// `len() >> spec` entries will be evicted before it.
    ///
    /// Non-zero is scan resistance. An entry read once and never again is then
    /// evicted from where it was put, instead of walking the whole order and
    /// pushing everything genuinely hot ahead of it.
    ///
    /// This is CacheLib's `lruInsertionPointSpec`.
    pub fn set_insertion_spec(&mut self, spec: u8) {
        self.insertion_spec = spec;
        self.rebalance_insertion_point();
    }

    /// Where new entries are currently placed.
    pub fn insertion_spec(&self) -> u8 {
        self.insertion_spec
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn contains(&self, key: &CacheKey) -> bool {
        self.index.contains_key(key)
    }

    /// Most recently used key.
    pub fn back(&self) -> Option<&CacheKey> {
        if self.tail == CACHE_ORDER_NIL {
            return None;
        }
        Some(&self.nodes[self.tail as usize].key)
    }

    /// Least recently used key.
    pub fn front(&self) -> Option<&CacheKey> {
        if self.head == CACHE_ORDER_NIL {
            return None;
        }
        Some(&self.nodes[self.head as usize].key)
    }

    /// Append `key` as most recently used, or move it there if already present.
    pub fn push_back(&mut self, key: CacheKey) {
        if let Some(&node) = self.index.get(&key) {
            self.unlink(node);
            self.link_back(node);
            self.unlink_access(node);
            self.link_access_back(node);
            return;
        }
        let node = self.alloc(key.clone());
        self.link_back(node);
        self.link_access_back(node);
        self.index.insert(key, node);
    }

    /// Append `key` at the back only if it is not already tracked.
    ///
    /// First-in first-out ordering must not move a key that is rewritten, so
    /// this leaves an existing key exactly where it is. It still inserts a key
    /// that is missing, which keeps the order consistent with an index that
    /// already holds the key.
    pub fn push_back_if_absent(&mut self, key: CacheKey) {
        if self.index.contains_key(&key) {
            return;
        }
        let node = self.alloc(key.clone());
        self.link_back(node);
        // Index first: the insertion point is balanced against `len()`, and
        // linking before the index knows about this node leaves it balancing
        // against a list one entry short -- which puts every new entry one
        // place nearer eviction than the spec asks for.
        self.index.insert(key, node);
        // Insertion order goes to the back regardless; the access order
        // honours the insertion spec, because this is a new entry and that is
        // exactly the case the spec governs.
        self.link_access_insert(node);
    }

    /// Insert `key` as least recently used, or move it there if already present.
    pub fn push_front(&mut self, key: CacheKey) {
        if let Some(&node) = self.index.get(&key) {
            self.unlink(node);
            self.link_front(node);
            self.unlink_access(node);
            self.link_access_front(node);
            return;
        }
        let node = self.alloc(key.clone());
        self.link_front(node);
        self.link_access_front(node);
        self.index.insert(key, node);
    }

    /// Move `key` to the back of the insertion order if it is present.
    /// Returns whether the key was there.
    ///
    /// This is not how a hit is recorded — a hit must leave the insertion
    /// order alone, which is what `touch_access` is for.
    pub fn move_to_back(&mut self, key: &CacheKey) -> bool {
        let Some(&node) = self.index.get(key) else {
            return false;
        };
        if node == self.tail {
            return true;
        }
        self.unlink(node);
        self.link_back(node);
        true
    }

    /// Record an access: move `key` to the back of the access order, leaving
    /// the insertion order alone. Returns whether the key was there.
    ///
    /// The two orders answer different questions. Eviction that only ever
    /// looks at the front of a list needs the front to hold entries nobody
    /// wants; insertion order never moves an entry no matter how often it is
    /// read, so a popular entry written early sits at the front forever and is
    /// offered up on every pass.
    pub fn touch_access(&mut self, key: &CacheKey) -> bool {
        // Hashing a CacheKey means hashing three Strings, and a read touches
        // the access order of every tier. An order holding nothing cannot
        // match, so check that before paying for the hash.
        if self.index.is_empty() {
            return false;
        }
        let Some(&node) = self.index.get(key) else {
            return false;
        };
        if node == self.access_tail {
            return true;
        }
        self.unlink_access(node);
        self.link_access_back(node);
        true
    }

    /// Remove `key`, returning whether it was present.
    pub fn remove(&mut self, key: &CacheKey) -> bool {
        let Some(node) = self.index.remove(key) else {
            return false;
        };
        self.unlink(node);
        self.unlink_access(node);
        self.release(node);
        true
    }

    /// Remove and return the least recently used key.
    pub fn pop_front(&mut self) -> Option<CacheKey> {
        if self.head == CACHE_ORDER_NIL {
            return None;
        }
        let node = self.head;
        let key = self.nodes[node as usize].key.clone();
        self.unlink(node);
        self.unlink_access(node);
        self.index.remove(&key);
        self.release(node);
        Some(key)
    }

    /// Keep only the keys for which `predicate` returns true. Order preserved.
    pub fn retain<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&CacheKey) -> bool,
    {
        let mut cursor = self.head;
        while cursor != CACHE_ORDER_NIL {
            let next = self.nodes[cursor as usize].next;
            if !predicate(&self.nodes[cursor as usize].key) {
                let key = self.nodes[cursor as usize].key.clone();
                self.index.remove(&key);
                self.unlink(cursor);
                self.unlink_access(cursor);
                self.release(cursor);
            }
            cursor = next;
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.free.clear();
        self.index.clear();
        self.head = CACHE_ORDER_NIL;
        self.tail = CACHE_ORDER_NIL;
        self.access_head = CACHE_ORDER_NIL;
        self.access_tail = CACHE_ORDER_NIL;
        self.access_insert = CACHE_ORDER_NIL;
        self.access_cold = 0;
    }

    /// Iterate from most to least recently used.
    pub fn iter_rev(&self) -> CacheKeyOrderRevIter<'_> {
        CacheKeyOrderRevIter {
            order: self,
            cursor: self.tail,
        }
    }

    /// Iterate from least to most recently used.
    pub fn iter(&self) -> CacheKeyOrderIter<'_> {
        CacheKeyOrderIter {
            order: self,
            cursor: self.head,
        }
    }

    /// Iterate the access order, least recently accessed first.
    pub fn iter_access(&self) -> CacheKeyOrderAccessIter<'_> {
        CacheKeyOrderAccessIter {
            order: self,
            cursor: self.access_head,
        }
    }

    fn alloc(&mut self, key: CacheKey) -> u32 {
        if let Some(index) = self.free.pop() {
            let node = &mut self.nodes[index as usize];
            node.key = key;
            node.prev = CACHE_ORDER_NIL;
            node.next = CACHE_ORDER_NIL;
            node.access_prev = CACHE_ORDER_NIL;
            node.access_next = CACHE_ORDER_NIL;
            node.access_cold = false;
            return index;
        }
        self.nodes.push(CacheOrderNode {
            key,
            prev: CACHE_ORDER_NIL,
            next: CACHE_ORDER_NIL,
            access_prev: CACHE_ORDER_NIL,
            access_next: CACHE_ORDER_NIL,
            access_cold: false,
        });
        (self.nodes.len() - 1) as u32
    }

    fn release(&mut self, node: u32) {
        self.free.push(node);
    }

    fn link_back(&mut self, node: u32) {
        let old_tail = self.tail;
        self.nodes[node as usize].prev = old_tail;
        self.nodes[node as usize].next = CACHE_ORDER_NIL;
        if old_tail == CACHE_ORDER_NIL {
            self.head = node;
        } else {
            self.nodes[old_tail as usize].next = node;
        }
        self.tail = node;
    }

    fn link_front(&mut self, node: u32) {
        let old_head = self.head;
        self.nodes[node as usize].next = old_head;
        self.nodes[node as usize].prev = CACHE_ORDER_NIL;
        if old_head == CACHE_ORDER_NIL {
            self.tail = node;
        } else {
            self.nodes[old_head as usize].prev = node;
        }
        self.head = node;
    }

    /// Links a newly-inserted node according to the insertion spec.
    ///
    /// Only new entries go through here. A hit still moves its entry all the
    /// way to the most-recently-used end -- the point of the insertion spec is
    /// where something *starts*, not where a hit puts it.
    /// Verifies the access list's internal invariants.
    ///
    /// Returns a description of the first violation found, so a failing test
    /// says what is wrong rather than only that something is. Walks the list,
    /// so it is a debugging aid rather than something to call on a hot path.
    ///
    /// The invariants: the forward and backward walks visit the same nodes in
    /// the same order and agree with `len()`; and when an insertion spec is
    /// set, the insertion point is a member of the list and `access_cold` is
    /// exactly its distance from the eviction end.
    pub fn check_access_invariants(&self) -> Result<(), String> {
        let mut forward = Vec::new();
        let mut cursor = self.access_head;
        while cursor != CACHE_ORDER_NIL {
            forward.push(cursor);
            if forward.len() > self.nodes.len() + 1 {
                return Err("access list cycles".to_string());
            }
            cursor = self.nodes[cursor as usize].access_next;
        }
        if forward.len() != self.len() {
            return Err(format!(
                "access list holds {} nodes, index holds {}",
                forward.len(),
                self.len()
            ));
        }

        let mut backward = Vec::new();
        cursor = self.access_tail;
        while cursor != CACHE_ORDER_NIL {
            backward.push(cursor);
            if backward.len() > self.nodes.len() + 1 {
                return Err("access list cycles backwards".to_string());
            }
            cursor = self.nodes[cursor as usize].access_prev;
        }
        backward.reverse();
        if backward != forward {
            return Err("forward and backward walks disagree".to_string());
        }

        if self.insertion_spec != 0 && !forward.is_empty() {
            let position = forward
                .iter()
                .position(|node| *node == self.access_insert)
                .ok_or_else(|| "insertion point is not in the access list".to_string())?;
            if position != self.access_cold {
                return Err(format!(
                    "cold count is {} but the insertion point sits at {}",
                    self.access_cold, position
                ));
            }
        }
        Ok(())
    }

    fn link_access_insert(&mut self, node: u32) {
        if self.insertion_spec == 0 || self.access_insert == CACHE_ORDER_NIL {
            self.link_access_back(node);
            self.rebalance_insertion_point();
            return;
        }
        let successor = self.access_insert;
        let predecessor = self.nodes[successor as usize].access_prev;
        self.nodes[node as usize].access_prev = predecessor;
        self.nodes[node as usize].access_next = successor;
        self.nodes[successor as usize].access_prev = node;
        if predecessor == CACHE_ORDER_NIL {
            self.access_head = node;
        } else {
            self.nodes[predecessor as usize].access_next = node;
        }
        // The new node lands strictly on the eviction side of the point, so it
        // is cold and the point is now one step further from the eviction end.
        self.nodes[node as usize].access_cold = true;
        self.access_cold += 1;
        self.rebalance_insertion_point();
    }

    /// Moves the insertion point until `access_cold` matches `len() >> spec`.
    ///
    /// Called after every change to the access list, and moves the point by at
    /// most a step or two each time, because the list changes by one entry at a
    /// time. Walking it here rather than recomputing from scratch is what keeps
    /// insertion O(1).
    fn rebalance_insertion_point(&mut self) {
        if self.insertion_spec == 0 {
            self.access_insert = CACHE_ORDER_NIL;
            self.access_cold = 0;
            return;
        }
        let target = self.len() >> self.insertion_spec;
        if self.access_head == CACHE_ORDER_NIL {
            self.access_insert = CACHE_ORDER_NIL;
            self.access_cold = 0;
            return;
        }
        if self.access_insert == CACHE_ORDER_NIL {
            self.access_insert = self.access_head;
            self.access_cold = 0;
        }
        // Toward the most-recently-used end while too few entries are cold.
        // The node the point leaves behind becomes cold.
        while self.access_cold < target {
            let next = self.nodes[self.access_insert as usize].access_next;
            if next == CACHE_ORDER_NIL {
                break;
            }
            self.nodes[self.access_insert as usize].access_cold = true;
            self.access_insert = next;
            self.access_cold += 1;
        }
        // And back toward the eviction end while too many are. The node it
        // lands on was cold and is now the point itself, so it stops being.
        while self.access_cold > target {
            let prev = self.nodes[self.access_insert as usize].access_prev;
            if prev == CACHE_ORDER_NIL {
                break;
            }
            self.access_insert = prev;
            self.nodes[prev as usize].access_cold = false;
            self.access_cold -= 1;
        }
        self.nodes[self.access_insert as usize].access_cold = false;
    }

    fn link_access_back(&mut self, node: u32) {
        let old_tail = self.access_tail;
        self.nodes[node as usize].access_prev = old_tail;
        self.nodes[node as usize].access_next = CACHE_ORDER_NIL;
        if old_tail == CACHE_ORDER_NIL {
            self.access_head = node;
        } else {
            self.nodes[old_tail as usize].access_next = node;
        }
        self.access_tail = node;
    }

    fn link_access_front(&mut self, node: u32) {
        let old_head = self.access_head;
        self.nodes[node as usize].access_next = old_head;
        self.nodes[node as usize].access_prev = CACHE_ORDER_NIL;
        if old_head == CACHE_ORDER_NIL {
            self.access_tail = node;
        } else {
            self.nodes[old_head as usize].access_prev = node;
        }
        self.access_head = node;
    }

    fn unlink_access(&mut self, node: u32) {
        // Keep the insertion point off the node about to leave, and keep the
        // cold count honest about which side of it the node was on.
        if self.insertion_spec != 0 && self.access_insert != CACHE_ORDER_NIL {
            if node == self.access_insert {
                // Hand the point to the neighbour towards the hot end; if there
                // is none, fall back towards the eviction end, which loses one
                // cold entry with it.
                let next = self.nodes[node as usize].access_next;
                if next != CACHE_ORDER_NIL {
                    self.access_insert = next;
                    self.nodes[next as usize].access_cold = false;
                } else {
                    let prev = self.nodes[node as usize].access_prev;
                    self.access_insert = prev;
                    if prev != CACHE_ORDER_NIL {
                        self.nodes[prev as usize].access_cold = false;
                        self.access_cold = self.access_cold.saturating_sub(1);
                    } else {
                        self.access_cold = 0;
                    }
                }
            } else if self.nodes[node as usize].access_cold {
                self.access_cold = self.access_cold.saturating_sub(1);
            }
        }
        self.nodes[node as usize].access_cold = false;
        self.unlink_access_inner(node)
    }

    fn unlink_access_inner(&mut self, node: u32) {
        let prev = self.nodes[node as usize].access_prev;
        let next = self.nodes[node as usize].access_next;
        if prev == CACHE_ORDER_NIL {
            self.access_head = next;
        } else {
            self.nodes[prev as usize].access_next = next;
        }
        if next == CACHE_ORDER_NIL {
            self.access_tail = prev;
        } else {
            self.nodes[next as usize].access_prev = prev;
        }
        self.nodes[node as usize].access_prev = CACHE_ORDER_NIL;
        self.nodes[node as usize].access_next = CACHE_ORDER_NIL;
    }

    fn unlink(&mut self, node: u32) {
        let prev = self.nodes[node as usize].prev;
        let next = self.nodes[node as usize].next;
        if prev == CACHE_ORDER_NIL {
            self.head = next;
        } else {
            self.nodes[prev as usize].next = next;
        }
        if next == CACHE_ORDER_NIL {
            self.tail = prev;
        } else {
            self.nodes[next as usize].prev = prev;
        }
        self.nodes[node as usize].prev = CACHE_ORDER_NIL;
        self.nodes[node as usize].next = CACHE_ORDER_NIL;
    }
}

impl FromIterator<CacheKey> for CacheKeyOrder {
    fn from_iter<I: IntoIterator<Item = CacheKey>>(iter: I) -> Self {
        let mut order = Self::new();
        for key in iter {
            order.push_back(key);
        }
        order
    }
}

pub struct CacheKeyOrderIter<'a> {
    order: &'a CacheKeyOrder,
    cursor: u32,
}

pub struct CacheKeyOrderRevIter<'a> {
    order: &'a CacheKeyOrder,
    cursor: u32,
}

pub struct CacheKeyOrderAccessIter<'a> {
    order: &'a CacheKeyOrder,
    cursor: u32,
}

impl<'a> Iterator for CacheKeyOrderAccessIter<'a> {
    type Item = &'a CacheKey;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor == CACHE_ORDER_NIL {
            return None;
        }
        let node = &self.order.nodes[self.cursor as usize];
        self.cursor = node.access_next;
        Some(&node.key)
    }
}

impl<'a> Iterator for CacheKeyOrderRevIter<'a> {
    type Item = &'a CacheKey;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor == CACHE_ORDER_NIL {
            return None;
        }
        let node = &self.order.nodes[self.cursor as usize];
        self.cursor = node.prev;
        Some(&node.key)
    }
}

impl<'a> Iterator for CacheKeyOrderIter<'a> {
    type Item = &'a CacheKey;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor == CACHE_ORDER_NIL {
            return None;
        }
        let node = &self.order.nodes[self.cursor as usize];
        self.cursor = node.next;
        Some(&node.key)
    }
}
