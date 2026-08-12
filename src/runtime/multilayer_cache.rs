// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

pub trait CacheApi {
    fn start_cache(&self) -> bool;
    fn stop_cache(&self) -> bool;
    fn insert_cache(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError>;
    fn insert_batch_cache(
        &self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<usize, CacheError> {
        let mut inserted = 0usize;
        for (key, value, size) in entries {
            self.insert_cache(key, value, size)?;
            inserted = inserted.saturating_add(1);
        }
        Ok(inserted)
    }
    fn lookup_cache(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError>;
    fn lookup_batch_cache(&self, keys: &[CacheKey]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        keys.iter().map(|key| self.lookup_cache(key)).collect()
    }
    fn lookup_no_promotion_cache(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup_cache(key)
    }
    fn lookup_batch_no_promotion_cache(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        keys.iter()
            .map(|key| self.lookup_no_promotion_cache(key))
            .collect()
    }
    fn submit_async_writeback_or_write_through_cache(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        let size = value.len();
        self.insert_cache(key, value, size)?;
        Ok(CacheWritebackSubmitReport {
            queued: 0,
            write_through: 1,
        })
    }
    fn submit_async_writeback_batch_or_write_through_cache(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        let mut report = CacheWritebackSubmitReport::default();
        for (key, value) in entries {
            let submitted = self.submit_async_writeback_or_write_through_cache(key, value)?;
            report.queued = report.queued.saturating_add(submitted.queued);
            report.write_through = report.write_through.saturating_add(submitted.write_through);
        }
        Ok(report)
    }
    fn remove_cache(&self, key: &CacheKey) -> Result<(), CacheError>;
    fn remove_batch_cache(&self, keys: &[CacheKey]) -> Result<usize, CacheError> {
        let mut removed = 0usize;
        for key in keys {
            self.remove_cache(key)?;
            removed = removed.saturating_add(1);
        }
        Ok(removed)
    }
    fn remove_all_cache(&self) -> Result<(), CacheError>;
    fn reset_cache(&self) -> Result<(), CacheError> {
        self.remove_all_cache()
    }
    fn capacity_cache(&self) -> usize;
    fn capacity_for_instance_cache(&self, instance_type: CacheInstanceType) -> usize {
        match instance_type {
            CacheInstanceType::kUnified => self.capacity_cache(),
            _ => self.capacity_cache(),
        }
    }
    fn set_capacity_cache(&self, capacity: usize);
    fn set_capacity_for_instance_cache(&self, instance_type: CacheInstanceType, capacity: usize) {
        match instance_type {
            CacheInstanceType::kUnified => self.set_capacity_cache(capacity),
            _ => self.set_capacity_cache(capacity),
        }
    }
    fn size_cache(&self) -> usize;
    fn used_cache(&self, instance_type: CacheInstanceType) -> usize {
        match instance_type {
            CacheInstanceType::kUnified => self.size_cache(),
            _ => self.size_cache(),
        }
    }
}

pub trait ZeroCopyCacheApi: CacheApi {
    fn acquire_cache(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError>;
    fn acquire_batch_cache(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        keys.iter().map(|key| self.acquire_cache(key)).collect()
    }
    fn acquire_no_promotion_cache(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire_cache(key)
    }
    fn acquire_batch_no_promotion_cache(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        keys.iter()
            .map(|key| self.acquire_no_promotion_cache(key))
            .collect()
    }
    fn release_cache(&self, handle: CachePinnedHandle);
    fn release_batch_cache(&self, handles: Vec<CachePinnedHandle>) -> usize {
        let released = handles.len();
        for handle in handles {
            self.release_cache(handle);
        }
        released
    }
    fn insert_pinned_cache(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<Option<CachePinnedHandle>, CacheError>;
    fn insert_pinned_batch_cache(
        &self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        entries
            .into_iter()
            .map(|(key, value, size)| self.insert_pinned_cache(key, value, size))
            .collect()
    }
}

pub trait StringCacheApi {
    fn start_string_cache(&self) -> bool;
    fn stop_string_cache(&self) -> bool;
    fn insert_string(&self, key: &str, value: String, size: usize) -> Result<(), CacheError>;
    fn insert_string_default_size(&self, key: &str, value: String) -> Result<(), CacheError> {
        self.insert_string(key, value, 1)
    }
    fn lookup_string(&self, key: &str) -> Result<Option<String>, CacheError>;
    fn remove_string(&self, key: &str) -> Result<(), CacheError>;
    fn remove_all_string(&self) -> Result<(), CacheError>;
    fn capacity_string(&self) -> usize;
    fn set_capacity_string(&self, capacity: usize);
    fn size_string(&self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheWritebackJob {
    pub key: CacheKey,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheWritebackDrainReport {
    pub requested: usize,
    pub drained: usize,
    pub remaining: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CacheWritebackSubmitReport {
    pub queued: usize,
    pub write_through: usize,
}

#[derive(Debug, Clone)]
struct SlotEvictionGroup {
    group_score: EvictionScore,
    victim: CacheKey,
    victim_score: EvictionScore,
}

impl SlotEvictionGroup {
    fn new(victim: CacheKey, score: EvictionScore) -> Self {
        Self {
            group_score: score,
            victim,
            victim_score: score,
        }
    }

    fn observe(&mut self, key: CacheKey, score: EvictionScore) {
        self.group_score.hotness = self.group_score.hotness.max(score.hotness);
        self.group_score.hits = self.group_score.hits.saturating_add(score.hits);
        self.group_score.last_access_epoch = self
            .group_score
            .last_access_epoch
            .max(score.last_access_epoch);
        if score < self.victim_score || (score == self.victim_score && key < self.victim) {
            self.victim = key;
            self.victim_score = score;
        }
    }
}

#[derive(Debug, Clone)]
pub struct MultiLayerCache {
    inner: Arc<RwLock<CacheInner>>,
    async_writeback_worker: Arc<Mutex<Option<CacheAsyncWritebackWorker>>>,
}

#[derive(Debug)]
struct CacheAsyncWritebackWorker {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug)]
struct CacheInner {
    started: bool,
    auto_recover_on_start: bool,
    memory_capacity_bytes: usize,
    memory_bytes: usize,
    pmem_capacity_bytes: usize,
    pmem_bytes: usize,
    ssd_capacity_bytes: usize,
    ssd_bytes: u64,
    disk_dir: PathBuf,
    pmem_paths: Vec<PathBuf>,
    ssd_store: SsdTierStore,
    tiering_policy: CacheTieringPolicy,
    block_options: CacheBlockOptions,
    memory: HashMap<CacheKey, Arc<[u8]>>,
    pmem: HashMap<CacheKey, Arc<[u8]>>,
    disk_index: HashMap<CacheKey, u64>,
    disk_order: VecDeque<CacheKey>,
    disk_fifo_order: VecDeque<CacheKey>,
    pinned: HashMap<CacheKey, u64>,
    pinned_handle_bytes: HashMap<CacheKey, usize>,
    pinned_removed_bytes: HashMap<CacheKey, usize>,
    order: VecDeque<CacheKey>,
    memory_fifo_order: VecDeque<CacheKey>,
    pmem_order: VecDeque<CacheKey>,
    pmem_fifo_order: VecDeque<CacheKey>,
    async_writeback_queue: VecDeque<CacheWritebackJob>,
    async_writeback_positions: HashMap<CacheKey, usize>,
    async_writeback_queue_bytes: u64,
    max_async_writeback_queue: usize,
    access_record_callback: Option<CacheAccessRecordCallback>,
    eviction_callback: Option<CacheEvictionCallback>,
    eviction_handler_enabled: bool,
    pending_eviction_records: VecDeque<CacheEvictionRecord>,
    ssd_instance_only: bool,
    memory_replacement_policy: CacheReplacementPolicy,
    pmem_replacement_policy: CacheReplacementPolicy,
    ssd_replacement_policy: CacheReplacementPolicy,
    metadata: HashMap<CacheKey, CacheEntryMeta>,
    access_epoch: u64,
    stats: CacheStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheEntryMeta {
    block_kind: CacheBlockKind,
    routing_slot: Option<u32>,
    hotness: u32,
    hits: u64,
    last_access_epoch: u64,
    admission_reason: CacheAdmissionReason,
}
struct StagedSsdBatchWrite {
    key: CacheKey,
    block: Vec<u8>,
    block_len: u64,
    value_len: usize,
    request: CacheAdmissionRequest,
    admission_reason: CacheAdmissionReason,
}

#[derive(Debug, Clone)]
enum SsdTierStore {
    Single(StorageEngineRocksDB),
    Multi(StorageEngineMultiSSD),
}

impl SsdTierStore {
    fn new(disk_dir: &Path, ssd_paths: &[PathBuf], capacity_bytes: usize) -> Self {
        let capacity = capacity_bytes as u64;
        let default_path = disk_dir.join("rocksdb-cache-blocks");
        let paths = if ssd_paths.is_empty() {
            vec![default_path]
        } else {
            ssd_paths
                .iter()
                .map(|path| path.join("rocksdb-cache-blocks"))
                .collect::<Vec<_>>()
        };
        if paths.len() > 1 {
            Self::Multi(StorageEngineMultiSSD::with_paths(paths, capacity))
        } else {
            let mut storage = StorageEngineRocksDB::new(
                paths
                    .into_iter()
                    .next()
                    .expect("ssd tier path")
                    .to_string_lossy()
                    .into_owned(),
            );
            if capacity != 0 {
                storage.SetCapacity(capacity);
            }
            Self::Single(storage)
        }
    }

    fn start(&mut self) -> bool {
        match self {
            Self::Single(storage) => storage.start(),
            Self::Multi(storage) => storage.start(),
        }
    }

    fn stop(&mut self) -> bool {
        match self {
            Self::Single(storage) => storage.stop(),
            Self::Multi(storage) => storage.stop(),
        }
    }

    fn is_started(&self) -> bool {
        match self {
            Self::Single(storage) => storage.is_started(),
            Self::Multi(storage) => storage.is_started(),
        }
    }

    fn peek(&self, key: &str) -> bool {
        match self {
            Self::Single(storage) => storage.peek(key),
            Self::Multi(storage) => storage.peek(key),
        }
    }

    fn get(&self, key: &str) -> Result<CacheBuffer, CacheError> {
        match self {
            Self::Single(storage) => storage.get(key),
            Self::Multi(storage) => storage.get(key),
        }
    }

    fn get_batch(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        match self {
            Self::Single(storage) => storage.get_batch(keys),
            Self::Multi(storage) => keys
                .iter()
                .map(|key| match storage.get(key) {
                    Ok(buffer) => Ok(Some(buffer.to_vec())),
                    Err(CacheError::NotFound) => Ok(None),
                    Err(err) => Err(err),
                })
                .collect(),
        }
    }

    fn put(&mut self, key: &str, value: Vec<u8>) -> Result<(), CacheError> {
        match self {
            Self::Single(storage) => storage.put(key, value).map(|_| ()),
            Self::Multi(storage) => storage.put(key, value).map(|_| ()),
        }
    }

    fn put_batch(&mut self, entries: Vec<(String, Vec<u8>)>) -> Result<(), CacheError> {
        match self {
            Self::Single(storage) => storage.put_batch(entries).map(|_| ()),
            Self::Multi(storage) => {
                for (key, value) in entries {
                    storage.put(&key, value)?;
                }
                Ok(())
            }
        }
    }

    fn delete(&mut self, key: &str) -> Result<(), CacheError> {
        match self {
            Self::Single(storage) => storage.delete(key),
            Self::Multi(storage) => storage.delete(key),
        }
    }

    fn delete_batch(&mut self, keys: &[String]) -> Result<(), CacheError> {
        match self {
            Self::Single(storage) => storage.delete_batch(keys).map(|_| ()),
            Self::Multi(storage) => {
                for key in keys {
                    match storage.delete(key) {
                        Ok(()) | Err(CacheError::NotFound) => {}
                        Err(err) => return Err(err),
                    }
                }
                Ok(())
            }
        }
    }

    fn reset(&mut self) -> Result<(), CacheError> {
        match self {
            Self::Single(storage) => storage.reset(),
            Self::Multi(storage) => storage.reset(),
        }
    }

    #[cfg(feature = "rocksdb-ssd")]
    fn recover_view_data<F>(&mut self, callback: &mut F) -> Result<(), CacheError>
    where
        F: FnMut(&str, StringViewBuffer),
    {
        match self {
            Self::Single(storage) => storage.recover_view_data(callback),
            Self::Multi(storage) => storage.recover_view_data(callback),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheManifestRecord {
    shard_id: ShardId,
    record_key: String,
    namespace: String,
    selector: String,
    block_len: u64,
}

impl CacheManifestRecord {
    fn from_entry(key: &CacheKey, block_len: u64) -> Self {
        Self {
            shard_id: key.shard_id,
            record_key: key.record_key.clone(),
            namespace: key.namespace.clone(),
            selector: key.selector.clone(),
            block_len,
        }
    }

    fn key(&self) -> CacheKey {
        CacheKey {
            shard_id: self.shard_id,
            record_key: self.record_key.clone(),
            namespace: self.namespace.clone(),
            selector: self.selector.clone(),
        }
    }

    fn encode_line(&self) -> String {
        format!(
            "v1\t{}\t{}\t{}\t{}\t{}",
            self.shard_id,
            encode_manifest_field(&self.record_key),
            encode_manifest_field(&self.namespace),
            encode_manifest_field(&self.selector),
            self.block_len
        )
    }

    fn decode_line(line: &str) -> Option<Self> {
        let mut fields = line.split('\t');
        if fields.next()? != "v1" {
            return None;
        }
        let shard_id = fields.next()?.parse::<ShardId>().ok()?;
        let record_key = decode_manifest_field(fields.next()?)?;
        let namespace = decode_manifest_field(fields.next()?)?;
        let selector = decode_manifest_field(fields.next()?)?;
        let block_len = fields.next()?.parse::<u64>().ok()?;
        if fields.next().is_some() {
            return None;
        }
        Some(Self {
            shard_id,
            record_key,
            namespace,
            selector,
            block_len,
        })
    }
}

// The payloads are consumed only by the file-backed compatibility store's manifest
// replay (`#[cfg(not(feature = "rocksdb-ssd"))]`), so they read as dead under the
// default RocksDB feature.
#[allow(dead_code)]
enum CacheManifestOp {
    Put(CacheManifestRecord),
    Delete(CacheKey),
}

fn unique_cache_keys(keys: &[CacheKey]) -> Vec<CacheKey> {
    let mut seen = HashSet::with_capacity(keys.len());
    let mut unique = Vec::with_capacity(keys.len());
    for key in keys {
        if seen.insert(key.clone()) {
            unique.push(key.clone());
        }
    }
    unique
}

impl CacheManifestOp {
    #[cfg(not(feature = "rocksdb-ssd"))]
    fn encode_line(&self) -> String {
        match self {
            Self::Put(record) => record.encode_line(),
            Self::Delete(key) => format!(
                "d1\t{}\t{}\t{}\t{}",
                key.shard_id,
                encode_manifest_field(&key.record_key),
                encode_manifest_field(&key.namespace),
                encode_manifest_field(&key.selector)
            ),
        }
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn decode_line(line: &str) -> Option<Self> {
        if let Some(record) = CacheManifestRecord::decode_line(line) {
            return Some(Self::Put(record));
        }
        let mut fields = line.split('\t');
        if fields.next()? != "d1" {
            return None;
        }
        let shard_id = fields.next()?.parse::<ShardId>().ok()?;
        let record_key = decode_manifest_field(fields.next()?)?;
        let namespace = decode_manifest_field(fields.next()?)?;
        let selector = decode_manifest_field(fields.next()?)?;
        if fields.next().is_some() {
            return None;
        }
        Some(Self::Delete(CacheKey {
            shard_id,
            record_key,
            namespace,
            selector,
        }))
    }
}

impl MultiLayerCache {
    pub fn new(memory_capacity_bytes: usize, disk_dir: impl Into<PathBuf>) -> Self {
        Self::with_block_options(
            memory_capacity_bytes,
            disk_dir,
            CacheBlockOptions::default(),
        )
    }

    pub fn with_block_options(
        memory_capacity_bytes: usize,
        disk_dir: impl Into<PathBuf>,
        block_options: CacheBlockOptions,
    ) -> Self {
        let policy = CacheTieringPolicy {
            memory_capacity_bytes,
            ..CacheTieringPolicy::default()
        };
        Self::with_tiering_policy(disk_dir, policy, block_options)
    }

    pub fn with_options(options: CacheOptions) -> Self {
        match Self::try_with_options(options.clone()) {
            Ok(cache) => cache,
            Err(_) => {
                let mut fallback = options;
                fallback.auto_recover_on_start = false;
                Self::build_with_options(fallback)
            }
        }
    }

    pub fn try_with_options(options: CacheOptions) -> Result<Self, CacheError> {
        let auto_recover_on_start = options.auto_recover_on_start;
        let cache = Self::build_with_options(options);
        if auto_recover_on_start {
            cache.recover_persistent_tiers()?;
        }
        Ok(cache)
    }

    fn build_with_options(options: CacheOptions) -> Self {
        let cache = Self::with_tiering_policy_and_ssd_paths(
            options.disk_dir(),
            options.ssd_paths.clone(),
            options.tiering_policy(),
            options.block_options,
        );
        cache.set_replacement_policy_for_tier(
            CacheTier::Memory,
            CacheReplacementPolicy::from_reference_name(&options.cache_dram_replacement_policy),
        );
        cache.set_replacement_policy_for_tier(
            CacheTier::Pmem,
            CacheReplacementPolicy::from_reference_name(&options.cache_pmem_replacement_policy),
        );
        cache.set_replacement_policy_for_tier(
            CacheTier::Ssd,
            CacheReplacementPolicy::from_reference_name(&options.cache_ssd_replacement_policy),
        );
        cache.set_ssd_instance_only(options.cache_ssd_instance_only);
        cache.set_pmem_paths(options.pmem_paths);
        cache.set_auto_recover_on_start(options.auto_recover_on_start);
        cache
    }

    pub fn with_tiering_policy(
        disk_dir: impl Into<PathBuf>,
        tiering_policy: CacheTieringPolicy,
        block_options: CacheBlockOptions,
    ) -> Self {
        Self::with_tiering_policy_and_ssd_paths(disk_dir, Vec::new(), tiering_policy, block_options)
    }

    fn with_tiering_policy_and_ssd_paths(
        disk_dir: impl Into<PathBuf>,
        ssd_paths: Vec<PathBuf>,
        tiering_policy: CacheTieringPolicy,
        block_options: CacheBlockOptions,
    ) -> Self {
        let disk_dir = disk_dir.into();
        let _ = fs::create_dir_all(&disk_dir);
        let mut ssd_store =
            SsdTierStore::new(&disk_dir, &ssd_paths, tiering_policy.ssd_capacity_bytes);
        let _ = ssd_store.start();
        Self {
            inner: Arc::new(RwLock::new(CacheInner {
                started: true,
                auto_recover_on_start: false,
                memory_capacity_bytes: tiering_policy.memory_capacity_bytes,
                memory_bytes: 0,
                pmem_capacity_bytes: tiering_policy.pmem_capacity_bytes,
                pmem_bytes: 0,
                ssd_capacity_bytes: tiering_policy.ssd_capacity_bytes,
                ssd_bytes: 0,
                disk_dir,
                pmem_paths: Vec::new(),
                ssd_store,
                tiering_policy,
                block_options,
                memory: HashMap::new(),
                pmem: HashMap::new(),
                disk_index: HashMap::new(),
                disk_order: VecDeque::new(),
                disk_fifo_order: VecDeque::new(),
                pinned: HashMap::new(),
                pinned_handle_bytes: HashMap::new(),
                pinned_removed_bytes: HashMap::new(),
                order: VecDeque::new(),
                memory_fifo_order: VecDeque::new(),
                pmem_order: VecDeque::new(),
                pmem_fifo_order: VecDeque::new(),
                async_writeback_queue: VecDeque::new(),
                async_writeback_positions: HashMap::new(),
                async_writeback_queue_bytes: 0,
                max_async_writeback_queue: 1024,
                access_record_callback: None,
                eviction_callback: None,
                eviction_handler_enabled: true,
                pending_eviction_records: VecDeque::new(),
                ssd_instance_only: false,
                memory_replacement_policy: CacheReplacementPolicy::WeightedHotnessLru,
                pmem_replacement_policy: CacheReplacementPolicy::WeightedHotnessLru,
                ssd_replacement_policy: CacheReplacementPolicy::WeightedHotnessLru,
                metadata: HashMap::new(),
                access_epoch: 0,
                stats: CacheStats::default(),
            })),
            async_writeback_worker: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        fs::create_dir_all(&inner.disk_dir)?;
        if !inner.ssd_store.is_started() && !inner.ssd_store.start() {
            return Err(CacheError::Stopped);
        }
        inner.started = true;
        if inner.auto_recover_on_start {
            inner.recover_persistent_tiers_locked()?;
        }
        Ok(())
    }

    pub fn start_bool(&self) -> bool {
        self.start().is_ok()
    }

    pub fn stop(&self) -> bool {
        self.stop_async_writeback_worker();
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.started = false;
        inner.ssd_store.stop();
        true
    }

    pub fn stop_bool(&self) -> bool {
        self.stop()
    }

    pub fn set_auto_recover_on_start(&self, enabled: bool) {
        self.inner
            .write()
            .expect("cache lock poisoned")
            .auto_recover_on_start = enabled;
    }

    pub fn auto_recover_on_start(&self) -> bool {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .auto_recover_on_start
    }

    pub fn is_started(&self) -> bool {
        self.inner.read().expect("cache lock poisoned").started
    }

    pub fn capacity(&self) -> usize {
        let inner = self.inner.read().expect("cache lock poisoned");
        inner
            .memory_capacity_bytes
            .saturating_add(inner.pmem_capacity_bytes)
            .max(inner.ssd_capacity_bytes)
    }

    pub fn capacity_for_tier(&self, tier: CacheTier) -> usize {
        let inner = self.inner.read().expect("cache lock poisoned");
        match tier {
            CacheTier::Memory => inner.memory_capacity_bytes,
            CacheTier::Pmem => inner.pmem_capacity_bytes,
            CacheTier::Ssd => inner.ssd_capacity_bytes,
            CacheTier::Reject => 0,
        }
    }

    pub fn get_capacity(&self, instance_type: CacheInstanceType) -> usize {
        match instance_type.as_tier() {
            Some(tier) => self.capacity_for_tier(tier),
            None => self.capacity(),
        }
    }

    pub fn set_capacity_for_tier(&self, tier: CacheTier, capacity_bytes: usize) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        match tier {
            CacheTier::Memory => {
                inner.memory_capacity_bytes = capacity_bytes;
                inner.tiering_policy.memory_capacity_bytes = capacity_bytes;
                inner.evict_memory_to_capacity();
            }
            CacheTier::Pmem => {
                inner.pmem_capacity_bytes = capacity_bytes;
                inner.tiering_policy.pmem_capacity_bytes = capacity_bytes;
                inner.evict_pmem_to_capacity();
            }
            CacheTier::Ssd => {
                inner.ssd_capacity_bytes = capacity_bytes;
                inner.tiering_policy.ssd_capacity_bytes = capacity_bytes;
                inner.evict_ssd_to_capacity();
            }
            CacheTier::Reject => {}
        }
        inner.refresh_usage_stats();
        inner.refresh_pin_stats();
        drop(inner);
        self.drain_eviction_records();
    }

    pub fn set_capacity_for_instance(
        &self,
        instance_type: CacheInstanceType,
        capacity_bytes: usize,
    ) {
        match instance_type.as_tier() {
            Some(tier) => self.set_capacity_for_tier(tier, capacity_bytes),
            None => self.set_capacity(capacity_bytes),
        }
    }

    pub fn set_capacity(&self, capacity_bytes: usize) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        let old_memory_capacity = inner.memory_capacity_bytes;
        let old_pmem_capacity = inner.pmem_capacity_bytes;
        let volatile_capacity = old_memory_capacity.saturating_add(old_pmem_capacity);

        if inner.ssd_capacity_bytes > 0 || inner.ssd_instance_only {
            inner.ssd_capacity_bytes = capacity_bytes;
            inner.tiering_policy.ssd_capacity_bytes = capacity_bytes;
        }

        if volatile_capacity > 0 {
            if old_memory_capacity > 0 && old_pmem_capacity > 0 {
                let memory_capacity =
                    capacity_bytes.saturating_mul(old_memory_capacity) / volatile_capacity;
                inner.memory_capacity_bytes = memory_capacity;
                inner.pmem_capacity_bytes = capacity_bytes.saturating_sub(memory_capacity);
            } else if old_memory_capacity > 0 {
                inner.memory_capacity_bytes = capacity_bytes;
                inner.pmem_capacity_bytes = 0;
            } else {
                inner.memory_capacity_bytes = 0;
                inner.pmem_capacity_bytes = capacity_bytes;
            }
        } else if inner.ssd_capacity_bytes == 0 {
            inner.memory_capacity_bytes = capacity_bytes;
        }

        inner.tiering_policy.memory_capacity_bytes = inner.memory_capacity_bytes;
        inner.tiering_policy.pmem_capacity_bytes = inner.pmem_capacity_bytes;
        inner.evict_memory_to_capacity();
        inner.evict_pmem_to_capacity();
        inner.evict_ssd_to_capacity();
        inner.refresh_usage_stats();
        inner.refresh_pin_stats();
        drop(inner);
        self.drain_eviction_records();
    }

    pub fn size(&self) -> usize {
        let inner = self.inner.read().expect("cache lock poisoned");
        let volatile_bytes = if matches!(
            inner.tiering_policy.data_placement,
            CacheDataPlacement::SideBySide
        ) {
            inner.memory_bytes.saturating_add(inner.pmem_bytes)
        } else {
            inner.memory_bytes.max(inner.pmem_bytes)
        };
        let ssd_bytes = inner
            .ssd_bytes
            .min(usize::MAX as u64)
            .try_into()
            .unwrap_or(usize::MAX);
        volatile_bytes
            .max(ssd_bytes)
            .saturating_add(inner.pinned_removed_bytes_total())
    }

    pub fn size_for_tier(&self, tier: CacheTier) -> usize {
        let inner = self.inner.read().expect("cache lock poisoned");
        match tier {
            CacheTier::Memory => inner.memory_bytes,
            CacheTier::Pmem => inner.pmem_bytes,
            CacheTier::Ssd => inner
                .ssd_bytes
                .min(usize::MAX as u64)
                .try_into()
                .unwrap_or(usize::MAX),
            CacheTier::Reject => 0,
        }
    }

    pub fn used_space_for_tier(&self, tier: CacheTier) -> usize {
        let inner = self.inner.read().expect("cache lock poisoned");
        match tier {
            CacheTier::Memory => inner
                .memory
                .iter()
                .map(|(key, value)| key.logical_size().saturating_add(value.len()))
                .sum(),
            CacheTier::Pmem => inner
                .pmem
                .iter()
                .map(|(key, value)| key.logical_size().saturating_add(value.len()))
                .sum(),
            CacheTier::Ssd => inner.disk_index.iter().fold(0usize, |total, (key, bytes)| {
                total
                    .saturating_add(key.logical_size())
                    .saturating_add((*bytes).min(usize::MAX as u64) as usize)
            }),
            CacheTier::Reject => 0,
        }
    }

    pub fn allocator_stats_for_tier(&self, tier: CacheTier) -> AllocatorStats {
        let occupied = self.used_space_for_tier(tier);
        let live_bytes = self.size_for_tier(tier);
        AllocatorStats {
            num_allocated_bytes: occupied.max(live_bytes),
            num_freed_bytes: occupied.saturating_sub(live_bytes),
            num_occupied_bytes: occupied,
        }
    }

    pub fn get_used(&self, instance_type: CacheInstanceType) -> usize {
        match instance_type.as_tier() {
            Some(tier) => self.used_space_for_tier(tier),
            None => self.size(),
        }
    }

    pub fn item_count_for_tier(&self, tier: CacheTier) -> usize {
        let inner = self.inner.read().expect("cache lock poisoned");
        match tier {
            CacheTier::Memory => inner.memory.len(),
            CacheTier::Pmem => inner.pmem.len(),
            CacheTier::Ssd => inner.disk_index.len(),
            CacheTier::Reject => 0,
        }
    }

    pub fn replacement_policy_for_tier(&self, tier: CacheTier) -> CacheReplacementPolicy {
        let inner = self.inner.read().expect("cache lock poisoned");
        match tier {
            CacheTier::Memory => inner.memory_replacement_policy,
            CacheTier::Pmem => inner.pmem_replacement_policy,
            CacheTier::Ssd => inner.ssd_replacement_policy,
            CacheTier::Reject => CacheReplacementPolicy::WeightedHotnessLru,
        }
    }

    pub fn get_replacement_policy_type(
        &self,
        instance_type: CacheInstanceType,
    ) -> CacheReplacementPolicy {
        match instance_type.as_tier() {
            Some(tier) => self.replacement_policy_for_tier(tier),
            None => CacheReplacementPolicy::WeightedHotnessLru,
        }
    }

    pub fn set_replacement_policy_for_tier(&self, tier: CacheTier, policy: CacheReplacementPolicy) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        match tier {
            CacheTier::Memory => inner.memory_replacement_policy = policy,
            CacheTier::Pmem => inner.pmem_replacement_policy = policy,
            CacheTier::Ssd => inner.ssd_replacement_policy = policy,
            CacheTier::Reject => {}
        }
    }

    pub fn try_set_replacement_policy_for_tier(
        &self,
        tier: CacheTier,
        policy: CacheReplacementPolicy,
    ) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if inner.started {
            return Err(CacheError::AlreadyStarted);
        }
        match tier {
            CacheTier::Memory => inner.memory_replacement_policy = policy,
            CacheTier::Pmem => inner.pmem_replacement_policy = policy,
            CacheTier::Ssd => inner.ssd_replacement_policy = policy,
            CacheTier::Reject => return Err(CacheError::UnsupportedTier(tier)),
        }
        Ok(())
    }

    pub fn set_replacement_policy_type(
        &self,
        instance_type: CacheInstanceType,
        policy: CacheReplacementPolicy,
    ) {
        if let Some(tier) = instance_type.as_tier() {
            self.set_replacement_policy_for_tier(tier, policy);
        }
    }

    pub fn try_set_replacement_policy_type(
        &self,
        instance_type: CacheInstanceType,
        policy: CacheReplacementPolicy,
    ) -> Result<(), CacheError> {
        let Some(tier) = instance_type.as_tier() else {
            return Err(CacheError::UnsupportedInstance(instance_type));
        };
        self.try_set_replacement_policy_for_tier(tier, policy)
    }

    pub fn remove_all(&self) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.clear_all_locked(true)
    }

    pub fn reset(&self) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.clear_all_locked(true)
    }

    pub fn register_access_record_callback<F>(&self, callback: F)
    where
        F: Fn(CacheAccessRecord) + Send + Sync + 'static,
    {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.access_record_callback = Some(CacheAccessRecordCallback::new(callback));
    }

    pub fn clear_access_record_callback(&self) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.access_record_callback = None;
    }

    pub fn register_eviction_callback<F>(&self, callback: F)
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.eviction_callback = Some(CacheEvictionCallback::new(callback));
    }

    pub fn clear_eviction_callback(&self) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.eviction_callback = None;
        inner.pending_eviction_records.clear();
    }

    pub fn set_eviction_handler_enabled(&self, enabled: bool) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.eviction_handler_enabled = enabled;
        if !enabled {
            inner.pending_eviction_records.clear();
        }
    }

    pub fn eviction_handler_enabled(&self) -> bool {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .eviction_handler_enabled
    }

    fn emit_access_record(&self, record_type: CacheAccessRecordType, key: &CacheKey) {
        let callback = {
            self.inner
                .read()
                .expect("cache lock poisoned")
                .access_record_callback
                .clone()
        };
        if let Some(callback) = callback {
            callback.call(CacheAccessRecord {
                record_type,
                key: key.clone(),
            });
        }
    }

    fn drain_eviction_records(&self) {
        let (callback, records) = {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            let callback = inner.eviction_callback.clone();
            let records = inner.pending_eviction_records.drain(..).collect::<Vec<_>>();
            (callback, records)
        };
        if let Some(callback) = callback {
            for record in records {
                callback.call(record);
            }
        }
    }

    pub fn recover_disk_index(&self) -> Result<CacheRecoverReport, CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.recover_disk_index_locked()
    }

    pub fn recover_pmem_index(&self) -> Result<CacheRecoverReport, CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.recover_pmem_index_locked()
    }

    pub fn recover_persistent_tiers(&self) -> Result<CacheRecoverReport, CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.recover_persistent_tiers_locked()
    }

    pub fn peek(&self, key: &CacheKey) -> bool {
        self.peek_tier(key).is_some()
    }

    pub fn peek_tier(&self, key: &CacheKey) -> Option<CacheReadTier> {
        let inner = self.inner.read().expect("cache lock poisoned");
        if inner.memory.contains_key(key) {
            return Some(CacheReadTier::Memory);
        }
        if inner.pmem.contains_key(key) {
            return Some(CacheReadTier::Pmem);
        }
        if inner.disk_index.contains_key(key) || inner.ssd_block_exists(key) {
            return Some(CacheReadTier::Ssd);
        }
        None
    }

    pub fn get(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        Ok(self.get_with_tier(key)?.map(|result| result.value))
    }

    pub fn lookup(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.get(key)
    }

    pub fn get_no_promotion(&self, key: &CacheKey) -> Result<Option<CacheReadResult>, CacheError> {
        {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            if let Some(value) = inner.memory.get(key).cloned() {
                return Ok(Some(CacheReadResult {
                    value: value.to_vec(),
                    tier: CacheReadTier::Memory,
                }));
            }
            if let Some(value) = inner.pmem.get(key).cloned() {
                return Ok(Some(CacheReadResult {
                    value: value.to_vec(),
                    tier: CacheReadTier::Pmem,
                }));
            }
        }

        let block = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_block(key)?
        };
        match block {
            Some(block) => Ok(Some(CacheReadResult {
                value: decode_cache_block(&block)?,
                tier: CacheReadTier::Ssd,
            })),
            None => Ok(None),
        }
    }

    pub fn get_batch_no_promotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CacheReadResult>>, CacheError> {
        keys.iter().map(|key| self.get_no_promotion(key)).collect()
    }

    pub fn lookup_no_promotion(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        Ok(self.get_no_promotion(key)?.map(|result| result.value))
    }

    pub fn lookup_batch_no_promotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        self.get_batch_no_promotion(keys).map(|results| {
            results
                .into_iter()
                .map(|result| result.map(|read| read.value))
                .collect()
        })
    }

    pub fn get_bypass_replacement_policy(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CacheReadResult>, CacheError> {
        self.get_no_promotion(key)
    }

    pub fn get_with_tier(&self, key: &CacheKey) -> Result<Option<CacheReadResult>, CacheError> {
        self.emit_access_record(CacheAccessRecordType::Get, key);
        let started = Instant::now();
        {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            if !inner.ssd_instance_only {
                if let Some(value) = inner.memory.get(key).cloned() {
                    inner.stats.memory_hits += 1;
                    inner.touch_key(key);
                    inner.record_hit(key, value.len());
                    inner.record_get_latency(started);
                    inner.record_read_through_latency(started);
                    return Ok(Some(CacheReadResult {
                        value: value.to_vec(),
                        tier: CacheReadTier::Memory,
                    }));
                }
                if let Some(value) = inner.pmem.get(key).cloned() {
                    inner.stats.pmem_hits = inner.stats.pmem_hits.saturating_add(1);
                    inner.touch_key(key);
                    inner.record_hit(key, value.len());
                    let decoded = value.to_vec();
                    if !inner.put_memory(key.clone(), decoded.clone()) {
                        inner.stats.refill_failures += 1;
                    }
                    inner.record_get_latency(started);
                    inner.record_read_through_latency(started);
                    inner.record_refill_latency(started);
                    drop(inner);
                    self.drain_eviction_records();
                    return Ok(Some(CacheReadResult {
                        value: decoded,
                        tier: CacheReadTier::Pmem,
                    }));
                }
            }
        }

        let refill_started = Instant::now();
        let block = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_block(key)?
        };
        match block {
            Some(block) => {
                let decoded = decode_cache_block(&block)?;
                let mut inner = self.inner.write().expect("cache lock poisoned");
                inner.stats.disk_hits += 1;
                if is_encoded_compressed_block(&block) {
                    inner.stats.compressed_hits += 1;
                }
                inner.record_hit(key, decoded.len());
                if !inner.ssd_instance_only && !inner.refill_from_ssd(key.clone(), decoded.clone())
                {
                    inner.stats.refill_failures += 1;
                }
                inner.record_get_latency(started);
                inner.record_read_through_latency(started);
                inner.record_refill_latency(refill_started);
                drop(inner);
                self.drain_eviction_records();
                Ok(Some(CacheReadResult {
                    value: decoded,
                    tier: CacheReadTier::Ssd,
                }))
            }
            None => {
                let mut inner = self.inner.write().expect("cache lock poisoned");
                inner.stats.misses += 1;
                inner.record_get_latency(started);
                inner.record_read_through_latency(started);
                Ok(None)
            }
        }
    }

    pub fn get_batch(&self, keys: &[CacheKey]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        for key in keys {
            self.emit_access_record(CacheAccessRecordType::Get, key);
        }

        let mut results = vec![None; keys.len()];
        let mut ssd_candidates = Vec::new();
        let mut needs_eviction_drain = false;
        {
            let mut memory_touches = Vec::new();
            let mut pmem_touches = Vec::new();
            let mut disk_touches = Vec::new();
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            for (index, key) in keys.iter().enumerate() {
                let started = Instant::now();
                if !inner.ssd_instance_only {
                    if let Some(value) = inner.memory.get(key).cloned() {
                        inner.stats.memory_hits = inner.stats.memory_hits.saturating_add(1);
                        inner.record_hit_metadata(key, value.len());
                        memory_touches.push(key.clone());
                        if inner.disk_index.contains_key(key) {
                            disk_touches.push(key.clone());
                        }
                        inner.record_get_latency(started);
                        inner.record_read_through_latency(started);
                        results[index] = Some(value.to_vec());
                        continue;
                    }
                    if let Some(value) = inner.pmem.get(key).cloned() {
                        inner.stats.pmem_hits = inner.stats.pmem_hits.saturating_add(1);
                        inner.record_hit_metadata(key, value.len());
                        pmem_touches.push(key.clone());
                        if inner.disk_index.contains_key(key) {
                            disk_touches.push(key.clone());
                        }
                        let decoded = value.to_vec();
                        if !inner.put_memory(key.clone(), decoded.clone()) {
                            inner.stats.refill_failures =
                                inner.stats.refill_failures.saturating_add(1);
                        }
                        inner.record_get_latency(started);
                        inner.record_read_through_latency(started);
                        inner.record_refill_latency(started);
                        results[index] = Some(decoded);
                        needs_eviction_drain = true;
                        continue;
                    }
                }
                ssd_candidates.push((index, key.clone(), started));
            }
            inner.touch_hit_queues_batch(&disk_touches, &memory_touches, &pmem_touches);
        }

        if ssd_candidates.is_empty() {
            if needs_eviction_drain {
                self.drain_eviction_records();
            }
            return Ok(results);
        }

        let mut unique_ssd_candidates = Vec::<(CacheKey, Vec<(usize, Instant)>)>::new();
        let mut unique_ssd_positions = HashMap::<CacheKey, usize>::new();
        for (index, key, started) in ssd_candidates {
            if let Some(position) = unique_ssd_positions.get(&key).copied() {
                unique_ssd_candidates[position].1.push((index, started));
            } else {
                unique_ssd_positions.insert(key.clone(), unique_ssd_candidates.len());
                unique_ssd_candidates.push((key, vec![(index, started)]));
            }
        }
        let candidate_keys = unique_ssd_candidates
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let refill_started = Instant::now();
        let blocks = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_blocks(&candidate_keys)?
        };
        let mut ssd_reads = Vec::with_capacity(unique_ssd_candidates.len());
        for ((key, occurrences), block) in unique_ssd_candidates.into_iter().zip(blocks)
        {
            let decoded = match block {
                Some(block) => Some((
                    decode_cache_block(&block)?,
                    is_encoded_compressed_block(&block),
                )),
                None => None,
            };
            ssd_reads.push((key, occurrences, refill_started, decoded));
        }

        {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            let mut disk_touches = Vec::new();
            for (key, occurrences, refill_started, decoded) in ssd_reads {
                match decoded {
                    Some((value, compressed)) => {
                        if inner.disk_index.contains_key(&key) {
                            disk_touches.push(key.clone());
                        }
                        if !inner.ssd_instance_only
                            && !inner.refill_from_ssd(key.clone(), value.clone())
                        {
                            inner.stats.refill_failures =
                                inner.stats.refill_failures.saturating_add(1);
                        }
                        for (index, started) in occurrences {
                            inner.stats.disk_hits = inner.stats.disk_hits.saturating_add(1);
                            if compressed {
                                inner.stats.compressed_hits =
                                    inner.stats.compressed_hits.saturating_add(1);
                            }
                            inner.record_hit_metadata(&key, value.len());
                            inner.record_get_latency(started);
                            inner.record_read_through_latency(started);
                            inner.record_refill_latency(refill_started);
                            results[index] = Some(value.clone());
                        }
                        needs_eviction_drain = true;
                    }
                    None => {
                        for (_index, started) in occurrences {
                            inner.stats.misses = inner.stats.misses.saturating_add(1);
                            inner.record_get_latency(started);
                            inner.record_read_through_latency(started);
                        }
                    }
                }
            }
            inner.touch_hit_queues_batch(&disk_touches, &[], &[]);
        }
        if needs_eviction_drain {
            self.drain_eviction_records();
        }
        Ok(results)
    }

    pub fn get_memory(&self, key: &CacheKey) -> Option<Vec<u8>> {
        self.emit_access_record(CacheAccessRecordType::Get, key);
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return None;
        }
        let value = inner.memory.get(key).cloned();
        if value.is_some() {
            inner.stats.memory_hits += 1;
            inner.touch_key(key);
            inner.record_hit(
                key,
                value.as_ref().map(|bytes| bytes.len()).unwrap_or_default(),
            );
        } else {
            inner.stats.misses += 1;
        }
        value.map(|value| value.to_vec())
    }

    pub fn get_pinned_handle(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire(key)
    }

    pub fn acquire(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError> {
        let started = Instant::now();
        {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            if let Some(value) = inner.memory.get(key).cloned() {
                inner.increment_pin_with_size(key, value.len());
                inner.stats.zero_copy_handle_hits =
                    inner.stats.zero_copy_handle_hits.saturating_add(1);
                inner.stats.memory_hits = inner.stats.memory_hits.saturating_add(1);
                inner.touch_key(key);
                inner.record_hit(key, value.len());
                inner.refresh_pin_stats();
                inner.record_get_latency(started);
                return Ok(Some(CachePinnedHandle {
                    key: key.clone(),
                    value,
                    tier: CacheReadTier::Memory,
                }));
            }
            if let Some(value) = inner.pmem.get(key).cloned() {
                let decoded = value.to_vec();
                inner.increment_pin_with_size(key, value.len());
                if !inner.put_memory(key.clone(), decoded) {
                    inner.stats.refill_failures = inner.stats.refill_failures.saturating_add(1);
                }
                inner.stats.zero_copy_handle_hits =
                    inner.stats.zero_copy_handle_hits.saturating_add(1);
                inner.stats.pmem_hits = inner.stats.pmem_hits.saturating_add(1);
                inner.touch_key(key);
                inner.record_hit(key, value.len());
                inner.refresh_pin_stats();
                inner.record_get_latency(started);
                inner.record_read_through_latency(started);
                inner.record_refill_latency(started);
                return Ok(Some(CachePinnedHandle {
                    key: key.clone(),
                    value,
                    tier: CacheReadTier::Pmem,
                }));
            }
        }

        let block = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_block(key)?
        };
        match block {
            Some(block) => {
                let decoded = Arc::<[u8]>::from(decode_cache_block(&block)?);
                let mut inner = self.inner.write().expect("cache lock poisoned");
                inner.increment_pin_with_size(key, decoded.len());
                if !inner.ssd_instance_only && !inner.refill_from_ssd(key.clone(), decoded.to_vec())
                {
                    inner.stats.refill_failures = inner.stats.refill_failures.saturating_add(1);
                }
                inner.stats.zero_copy_handle_hits =
                    inner.stats.zero_copy_handle_hits.saturating_add(1);
                inner.stats.disk_hits = inner.stats.disk_hits.saturating_add(1);
                if is_encoded_compressed_block(&block) {
                    inner.stats.compressed_hits = inner.stats.compressed_hits.saturating_add(1);
                }
                inner.record_hit(key, decoded.len());
                inner.refresh_pin_stats();
                inner.record_get_latency(started);
                inner.record_read_through_latency(started);
                inner.record_refill_latency(started);
                Ok(Some(CachePinnedHandle {
                    key: key.clone(),
                    value: decoded,
                    tier: CacheReadTier::Ssd,
                }))
            }
            None => {
                let mut inner = self.inner.write().expect("cache lock poisoned");
                inner.stats.zero_copy_handle_misses =
                    inner.stats.zero_copy_handle_misses.saturating_add(1);
                inner.stats.misses = inner.stats.misses.saturating_add(1);
                inner.record_get_latency(started);
                inner.record_read_through_latency(started);
                Ok(None)
            }
        }
    }

    pub fn acquire_no_promotion(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            if let Some(value) = inner.memory.get(key).cloned() {
                inner.increment_pin_with_size(key, value.len());
                inner.refresh_pin_stats();
                return Ok(Some(CachePinnedHandle {
                    key: key.clone(),
                    value,
                    tier: CacheReadTier::Memory,
                }));
            }
            if let Some(value) = inner.pmem.get(key).cloned() {
                inner.increment_pin_with_size(key, value.len());
                inner.refresh_pin_stats();
                return Ok(Some(CachePinnedHandle {
                    key: key.clone(),
                    value,
                    tier: CacheReadTier::Pmem,
                }));
            }
        }

        let block = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_block(key)?
        };
        let Some(block) = block else {
            return Ok(None);
        };
        let value = Arc::<[u8]>::from(decode_cache_block(&block)?);
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.increment_pin_with_size(key, value.len());
        inner.refresh_pin_stats();
        Ok(Some(CachePinnedHandle {
            key: key.clone(),
            value,
            tier: CacheReadTier::Ssd,
        }))
    }

    pub fn acquire_batch_no_promotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        let mut results = (0..keys.len()).map(|_| None).collect::<Vec<_>>();
        let mut positions_by_key = Vec::<(CacheKey, Vec<usize>)>::new();
        let mut unique_positions = HashMap::<CacheKey, usize>::new();
        for (position, key) in keys.iter().cloned().enumerate() {
            if let Some(unique_position) = unique_positions.get(&key).copied() {
                positions_by_key[unique_position].1.push(position);
            } else {
                unique_positions.insert(key.clone(), positions_by_key.len());
                positions_by_key.push((key, vec![position]));
            }
        }
        for (key, positions) in positions_by_key {
            let Some(handle) = self.acquire_no_promotion(&key)? else {
                continue;
            };
            let first_position = positions[0];
            results[first_position] = Some(handle);
            for position in positions.into_iter().skip(1) {
                let cloned = self.clone_handle(
                    results[first_position]
                        .as_ref()
                        .expect("first batch handle is installed"),
                );
                results[position] = Some(cloned);
            }
        }
        Ok(results)
    }

    pub fn release(&self, handle: CachePinnedHandle) {
        self.unpin(&handle.key);
    }

    pub fn release_batch(&self, handles: Vec<CachePinnedHandle>) -> usize {
        if handles.is_empty() {
            return 0;
        }
        let released = handles.len();
        let mut inner = self.inner.write().expect("cache lock poisoned");
        for handle in handles {
            inner.decrement_pin(&handle.key);
        }
        inner.refresh_pin_stats();
        released
    }

    pub fn clone_handle(&self, handle: &CachePinnedHandle) -> CachePinnedHandle {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.increment_pin(&handle.key);
        CachePinnedHandle {
            key: handle.key.clone(),
            value: handle.value.clone(),
            tier: handle.tier,
        }
    }

    pub fn acquire_batch(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        let mut results = (0..keys.len()).map(|_| None).collect::<Vec<_>>();
        let mut positions_by_key = Vec::<(CacheKey, Vec<usize>)>::new();
        let mut unique_positions = HashMap::<CacheKey, usize>::new();
        for (position, key) in keys.iter().cloned().enumerate() {
            if let Some(unique_position) = unique_positions.get(&key).copied() {
                positions_by_key[unique_position].1.push(position);
            } else {
                unique_positions.insert(key.clone(), positions_by_key.len());
                positions_by_key.push((key, vec![position]));
            }
        }
        for (key, positions) in positions_by_key {
            let Some(handle) = self.acquire(&key)? else {
                continue;
            };
            let first_position = positions[0];
            results[first_position] = Some(handle);
            for position in positions.into_iter().skip(1) {
                let cloned = self.clone_handle(
                    results[first_position]
                        .as_ref()
                        .expect("first batch handle is installed"),
                );
                results[position] = Some(cloned);
            }
        }
        Ok(results)
    }

    pub fn update_cached_value_if_current(
        &self,
        key: &CacheKey,
        old_handle: &CachePinnedHandle,
        new_value: Vec<u8>,
    ) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.update_cached_value_if_current(key, old_handle, new_value)
    }

    pub fn update_cached_value_if_current_for_tier(
        &self,
        tier: CacheTier,
        key: &CacheKey,
        old_handle: &CachePinnedHandle,
        new_value: Vec<u8>,
    ) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.update_cached_value_if_current_for_tier(tier, key, old_handle, new_value)
    }

    pub fn acquire_scoped(&self, key: &CacheKey) -> Result<Option<CacheScopedHandle>, CacheError> {
        Ok(self.acquire(key)?.map(|handle| CacheScopedHandle {
            cache: self.clone(),
            handle: Some(handle),
        }))
    }

    pub fn scoped_lookup(&self, key: &CacheKey) -> Result<CacheScopedLookup, CacheError> {
        Ok(CacheScopedLookup {
            scoped: self.acquire_scoped(key)?,
        })
    }

    pub fn insert_pinned(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        let logical_size = value.len();
        self.insert_pinned_sized(key, value, logical_size)
    }

    pub fn insert_pinned_default_size(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned_sized(key, value, 1)
    }

    pub fn insert_pinned_sized(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        logical_size: usize,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            inner.stats.insert_pinned_operations =
                inner.stats.insert_pinned_operations.saturating_add(1);
        }
        self.put_sized(key.clone(), value.clone(), logical_size)?;
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        let handle_value = if !inner.ssd_instance_only {
            if let Some(memory_value) = inner.memory.get(&key).cloned() {
                Some((memory_value, CacheReadTier::Memory))
            } else if let Some(pmem_value) = inner.pmem.get(&key).cloned() {
                Some((pmem_value, CacheReadTier::Pmem))
            } else if inner.disk_index.contains_key(&key) || inner.ssd_block_exists(&key) {
                Some((Arc::<[u8]>::from(value), CacheReadTier::Ssd))
            } else {
                None
            }
        } else if inner.disk_index.contains_key(&key) || inner.ssd_block_exists(&key) {
            Some((Arc::<[u8]>::from(value), CacheReadTier::Ssd))
        } else {
            None
        };
        if let Some((value, tier)) = handle_value {
            inner.increment_pin_with_size(&key, value.len());
            inner.refresh_pin_stats();
            Ok(Some(CachePinnedHandle { key, value, tier }))
        } else {
            Ok(None)
        }
    }

    pub fn put(&self, key: CacheKey, value: Vec<u8>) -> Result<(), CacheError> {
        let started = Instant::now();
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        let result = inner.put_with_request(key.clone(), value, None);
        inner.record_put_latency(started);
        drop(inner);
        if result.is_ok() {
            self.drain_eviction_records();
            self.emit_access_record(CacheAccessRecordType::Put, &key);
        }
        result
    }

    pub fn put_sized(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        logical_size: usize,
    ) -> Result<(), CacheError> {
        let started = Instant::now();
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        let request = inner.default_insert_request(&key, value.len(), logical_size);
        let result = inner.put_with_request(key.clone(), value, Some(request));
        inner.record_put_latency(started);
        drop(inner);
        if result.is_ok() {
            self.drain_eviction_records();
            self.emit_access_record(CacheAccessRecordType::Put, &key);
        }
        result
    }

    pub fn put_batch_sized(
        &self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<usize, CacheError> {
        if entries.is_empty() {
            return Ok(0);
        }
        let started = Instant::now();
        let inserted_keys = entries
            .iter()
            .map(|(key, _, _)| key.clone())
            .collect::<Vec<_>>();
        let inserted = {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            let inserted = inner.put_batch_with_requests(entries)?;
            for _ in 0..inserted {
                inner.record_put_latency(started);
            }
            inserted
        };
        self.drain_eviction_records();
        for key in &inserted_keys {
            self.emit_access_record(CacheAccessRecordType::Put, key);
        }
        Ok(inserted)
    }

    pub fn put_batch(&self, entries: Vec<(CacheKey, Vec<u8>)>) -> Result<usize, CacheError> {
        self.put_batch_sized(
            entries
                .into_iter()
                .map(|(key, value)| {
                    let logical_size = value.len();
                    (key, value, logical_size)
                })
                .collect(),
        )
    }

    pub fn insert(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.put_sized(key, value, size)
    }

    pub fn insert_default_size(&self, key: CacheKey, value: Vec<u8>) -> Result<(), CacheError> {
        self.insert(key, value, 1)
    }

    pub fn put_with_admission(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        request: CacheAdmissionRequest,
    ) -> Result<(), CacheError> {
        let started = Instant::now();
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        let result = inner.put_with_request(key.clone(), value, Some(request));
        inner.record_put_latency(started);
        drop(inner);
        if result.is_ok() {
            self.drain_eviction_records();
            self.emit_access_record(CacheAccessRecordType::Put, &key);
        }
        result
    }

    pub fn put_memory_only(&self, key: CacheKey, value: Vec<u8>) {
        let started = Instant::now();
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return;
        }
        inner.stats.puts += 1;
        inner.record_metadata(
            &key,
            CacheBlockKind::Other,
            extract_routing_slot(&key),
            value.len(),
            0,
            CacheAdmissionReason::MemoryOnly,
        );
        if !inner.put_memory(key.clone(), value) {
            inner.stats.refill_failures += 1;
        }
        inner.record_put_latency(started);
        drop(inner);
        self.drain_eviction_records();
        self.emit_access_record(CacheAccessRecordType::Put, &key);
    }

    pub fn put_bypass_storage_for_tier(
        &self,
        tier: CacheTier,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.put_bypass_storage_for_tier(tier, key, value)?;
        drop(inner);
        self.drain_eviction_records();
        Ok(())
    }

    pub fn test_insert(
        &self,
        instance_type: CacheInstanceType,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<(), CacheError> {
        let Some(tier) = instance_type.as_tier() else {
            return Err(CacheError::UnsupportedInstance(instance_type));
        };
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.test_insert_for_tier(tier, key, value, size)?;
        drop(inner);
        self.drain_eviction_records();
        Ok(())
    }

    pub fn test_acquire(
        &self,
        instance_type: CacheInstanceType,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        let Some(tier) = instance_type.as_tier() else {
            return Err(CacheError::UnsupportedInstance(instance_type));
        };

        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        match tier {
            CacheTier::Memory => {
                let Some(value) = inner.memory.get(key).cloned() else {
                    return Ok(None);
                };
                inner.increment_pin_with_size(key, value.len());
                inner.refresh_pin_stats();
                Ok(Some(CachePinnedHandle {
                    key: key.clone(),
                    value,
                    tier: CacheReadTier::Memory,
                }))
            }
            CacheTier::Pmem => {
                let Some(value) = inner.pmem.get(key).cloned() else {
                    return Ok(None);
                };
                inner.increment_pin_with_size(key, value.len());
                inner.refresh_pin_stats();
                Ok(Some(CachePinnedHandle {
                    key: key.clone(),
                    value,
                    tier: CacheReadTier::Pmem,
                }))
            }
            CacheTier::Ssd => match inner.read_ssd_block(key)? {
                Some(block) => {
                    let value = Arc::<[u8]>::from(decode_cache_block(&block)?);
                    inner.increment_pin_with_size(key, value.len());
                    inner.refresh_pin_stats();
                    Ok(Some(CachePinnedHandle {
                        key: key.clone(),
                        value,
                        tier: CacheReadTier::Ssd,
                    }))
                }
                None => Ok(None),
            },
            CacheTier::Reject => Err(CacheError::UnsupportedTier(tier)),
        }
    }

    pub fn test_remove(
        &self,
        instance_type: CacheInstanceType,
        key: &CacheKey,
    ) -> Result<(), CacheError> {
        let Some(tier) = instance_type.as_tier() else {
            return Err(CacheError::UnsupportedInstance(instance_type));
        };
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.test_remove_for_tier(tier, key)?;
        inner.refresh_usage_stats();
        inner.refresh_pin_stats();
        drop(inner);
        self.drain_eviction_records();
        Ok(())
    }

    pub fn test_get_unified_acquire_count(&self) -> u64 {
        self.stats().zero_copy_handle_hits
    }

    pub fn test_get_unified_put_count(&self) -> u64 {
        self.stats().puts
    }

    pub fn test_get_unified_insert_pinned_count(&self) -> u64 {
        self.stats().insert_pinned_operations
    }

    pub fn test_join_pmem_write_executor(&self) {}

    pub fn test_get_pmem_paths(&self) -> Vec<String> {
        self.pmem_paths()
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    }

    pub fn start_async_writeback_worker(
        &self,
        max_jobs_per_round: usize,
        interval: Duration,
    ) -> bool {
        let max_jobs_per_round = max_jobs_per_round.max(1);
        let mut worker = self
            .async_writeback_worker
            .lock()
            .expect("async writeback worker lock poisoned");
        if worker.is_some() {
            return false;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let cache = self.clone();
        let handle = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                match cache.drain_async_writeback(max_jobs_per_round) {
                    Ok(report) if report.drained > 0 => std::thread::yield_now(),
                    Ok(_) | Err(CacheError::Stopped) => std::thread::sleep(interval),
                    Err(_) => std::thread::sleep(interval),
                }
            }
            let _ = cache.drain_async_writeback(max_jobs_per_round);
        });
        *worker = Some(CacheAsyncWritebackWorker {
            stop,
            handle: Some(handle),
        });
        true
    }

    pub fn stop_async_writeback_worker(&self) -> bool {
        let worker = self
            .async_writeback_worker
            .lock()
            .expect("async writeback worker lock poisoned")
            .take();
        let Some(mut worker) = worker else {
            return false;
        };
        worker.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = worker.handle.take() {
            let _ = handle.join();
        }
        true
    }

    pub fn async_writeback_worker_running(&self) -> bool {
        self.async_writeback_worker
            .lock()
            .expect("async writeback worker lock poisoned")
            .is_some()
    }

    #[allow(non_snake_case)]
    pub fn StartAsyncWritebackWorker(&self, max_jobs_per_round: usize, interval_ms: u64) -> bool {
        self.start_async_writeback_worker(max_jobs_per_round, Duration::from_millis(interval_ms))
    }

    #[allow(non_snake_case)]
    pub fn StopAsyncWritebackWorker(&self) -> bool {
        self.stop_async_writeback_worker()
    }
    pub fn enqueue_async_writeback(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<(), CacheWritebackJob> {
        self.enqueue_async_writeback_batch(vec![(key, value)])
            .map(|_| ())
            .map_err(|mut rejected| {
                rejected
                    .pop()
                    .expect("single enqueue must return the rejected job")
            })
    }

    pub fn enqueue_async_writeback_batch(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<usize, Vec<CacheWritebackJob>> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(entries
                .into_iter()
                .map(|(key, value)| CacheWritebackJob { key, value })
                .collect());
        }

        let mut enqueued = 0usize;
        let mut rejected = Vec::new();
        for (key, value) in entries {
            let value_len = value.len() as u64;
            if let Some(index) = inner.async_writeback_positions.get(&key).copied() {
                if let Some(existing) = inner.async_writeback_queue.get_mut(index) {
                    let old_len = existing.value.len() as u64;
                    existing.value = value;
                    inner.async_writeback_queue_bytes = inner
                        .async_writeback_queue_bytes
                        .saturating_sub(old_len)
                        .saturating_add(value_len);
                    enqueued = enqueued.saturating_add(1);
                } else {
                    inner.async_writeback_positions.remove(&key);
                    rejected.push(CacheWritebackJob { key, value });
                }
            } else if inner.async_writeback_queue.len() < inner.max_async_writeback_queue {
                let index = inner.async_writeback_queue.len();
                inner.async_writeback_queue.push_back(CacheWritebackJob {
                    key: key.clone(),
                    value,
                });
                inner.async_writeback_queue_bytes =
                    inner.async_writeback_queue_bytes.saturating_add(value_len);
                inner.async_writeback_positions.insert(key, index);
                enqueued = enqueued.saturating_add(1);
            } else {
                rejected.push(CacheWritebackJob { key, value });
            }
        }
        if enqueued > 0 {
            inner.stats.async_writeback_enqueued = inner
                .stats
                .async_writeback_enqueued
                .saturating_add(enqueued as u64);
        }
        if !rejected.is_empty() {
            inner.stats.async_writeback_backpressure_rejections = inner
                .stats
                .async_writeback_backpressure_rejections
                .saturating_add(rejected.len() as u64);
        }
        inner.refresh_async_writeback_pressure_stats();
        if rejected.is_empty() {
            Ok(enqueued)
        } else {
            Err(rejected)
        }
    }

    pub fn submit_async_writeback_or_write_through(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        match self.enqueue_async_writeback(key, value) {
            Ok(()) => Ok(CacheWritebackSubmitReport {
                queued: 1,
                write_through: 0,
            }),
            Err(job) => {
                self.put(job.key, job.value)?;
                Ok(CacheWritebackSubmitReport {
                    queued: 0,
                    write_through: 1,
                })
            }
        }
    }

    pub fn submit_async_writeback_batch_or_write_through(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        let mut report = CacheWritebackSubmitReport::default();
        for (key, value) in entries {
            let submitted = self.submit_async_writeback_or_write_through(key, value)?;
            report.queued = report.queued.saturating_add(submitted.queued);
            report.write_through = report.write_through.saturating_add(submitted.write_through);
        }
        Ok(report)
    }

    pub fn drain_async_writeback(
        &self,
        max_jobs: usize,
    ) -> Result<CacheWritebackDrainReport, CacheError> {
        let started = Instant::now();
        let jobs = {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            let mut jobs = Vec::new();
            for _ in 0..max_jobs {
                let Some(job) = inner.async_writeback_queue.pop_front() else {
                    break;
                };
                inner.async_writeback_queue_bytes = inner
                    .async_writeback_queue_bytes
                    .saturating_sub(job.value.len() as u64);
                inner.async_writeback_positions.remove(&job.key);
                jobs.push(job);
            }
            inner.rebuild_async_writeback_positions();
            inner.refresh_async_writeback_pressure_stats();
            jobs
        };
        let drained = jobs.len();
        let mut coalesced = Vec::<(CacheKey, Vec<u8>)>::new();
        let mut coalesced_positions = HashMap::<CacheKey, usize>::new();
        for job in jobs {
            if let Some(index) = coalesced_positions.get(&job.key).copied() {
                coalesced[index].1 = job.value;
            } else {
                let index = coalesced.len();
                coalesced_positions.insert(job.key.clone(), index);
                coalesced.push((job.key, job.value));
            }
        }
        self.put_batch_sized(
            coalesced
                .into_iter()
                .map(|(key, value)| {
                    let logical_size = value.len();
                    (key, value, logical_size)
                })
                .collect(),
        )?;
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.stats.async_writeback_drained = inner
            .stats
            .async_writeback_drained
            .saturating_add(drained as u64);
        inner.refresh_async_writeback_pressure_stats();
        inner.record_writeback_latency(started);
        Ok(CacheWritebackDrainReport {
            requested: max_jobs,
            drained,
            remaining: inner.async_writeback_queue.len(),
        })
    }

    pub fn flush_async_writeback(&self) -> Result<CacheWritebackDrainReport, CacheError> {
        let queued = {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            inner.async_writeback_queue.len()
        };
        self.drain_async_writeback(queued)
    }

    #[allow(non_snake_case)]
    pub fn FlushAsyncWriteback(&self) -> Result<CacheWritebackDrainReport, CacheError> {
        self.flush_async_writeback()
    }

    pub fn set_async_writeback_queue_limit_for_test(&self, limit: usize) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.max_async_writeback_queue = limit;
    }

    pub fn record_compaction_latency_micros(&self, micros: u64) {
        self.inner
            .write()
            .expect("cache lock poisoned")
            .record_compaction_latency_micros(micros);
    }

    pub fn pin(&self, key: CacheKey) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.increment_pin(&key);
        inner.refresh_pin_stats();
    }

    pub fn unpin(&self, key: &CacheKey) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.decrement_pin(key);
        inner.refresh_pin_stats();
    }

    pub fn invalidate(&self, key: &CacheKey) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.invalidate_key_locked(key, true);
        drop(inner);
        self.emit_access_record(CacheAccessRecordType::Delete, key);
        Ok(())
    }

    pub fn invalidate_batch(&self, keys: &[CacheKey]) -> Result<usize, CacheError> {
        if keys.is_empty() {
            return Ok(0);
        }
        let unique_keys = unique_cache_keys(keys);
        let removed = {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            inner.invalidate_keys_locked(&unique_keys, true, Some(&unique_keys));
            keys.len()
        };
        for key in keys {
            self.emit_access_record(CacheAccessRecordType::Delete, key);
        }
        Ok(removed)
    }

    pub fn remove(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.invalidate(key)
    }

    pub fn remove_batch(&self, keys: &[CacheKey]) -> Result<usize, CacheError> {
        self.invalidate_batch(keys)
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> bool {
        self.start_bool()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self) -> bool {
        self.stop_bool()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.insert(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn InsertDefaultSize(&self, key: CacheKey, value: Vec<u8>) -> Result<(), CacheError> {
        self.insert_default_size(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Lookup(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup(key)
    }

    #[allow(non_snake_case)]
    pub fn LookupNoPromotion(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup_no_promotion(key)
    }

    #[allow(non_snake_case)]
    pub fn LookupBatchNoPromotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        self.lookup_batch_no_promotion(keys)
    }

    #[allow(non_snake_case)]
    pub fn GetNoPromotion(&self, key: &CacheKey) -> Result<Option<CacheReadResult>, CacheError> {
        self.get_no_promotion(key)
    }

    #[allow(non_snake_case)]
    pub fn GetBatchNoPromotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CacheReadResult>>, CacheError> {
        self.get_batch_no_promotion(keys)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.remove(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveBatch(&self, keys: &[CacheKey]) -> Result<usize, CacheError> {
        self.remove_batch(keys)
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
    pub fn GetCapacity(&self, instance_type: CacheInstanceType) -> usize {
        self.get_capacity(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    #[allow(non_snake_case)]
    pub fn SetCapacityForInstance(&self, instance_type: CacheInstanceType, capacity: usize) {
        self.set_capacity_for_instance(instance_type, capacity);
    }

    #[allow(non_snake_case)]
    pub fn GetUsed(&self, instance_type: CacheInstanceType) -> usize {
        self.get_used(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceType,
        policy: CacheReplacementPolicy,
    ) {
        self.set_replacement_policy_type(instance_type, policy);
    }

    #[allow(non_snake_case)]
    pub fn TrySetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceType,
        policy: CacheReplacementPolicy,
    ) -> Result<(), CacheError> {
        self.try_set_replacement_policy_type(instance_type, policy)
    }

    #[allow(non_snake_case)]
    pub fn GetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceType,
    ) -> CacheReplacementPolicy {
        self.get_replacement_policy_type(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetDataPlacementType(&self, placement: CacheDataPlacement) {
        self.set_data_placement(placement);
    }

    #[allow(non_snake_case)]
    pub fn SetDRAMPMEMDataPlacementType(&self, placement: DRAMPMEMDataPlacementType) {
        self.set_reference_data_placement_type(placement);
    }

    #[allow(non_snake_case)]
    pub fn GetDataPlacementType(&self) -> CacheDataPlacement {
        self.data_placement()
    }

    #[allow(non_snake_case)]
    pub fn GetDRAMPMEMDataPlacementType(&self) -> DRAMPMEMDataPlacementType {
        self.reference_data_placement_type()
    }

    #[allow(non_snake_case)]
    pub fn SetDataPlacementThreshold(&self, threshold: usize) {
        self.set_data_placement_threshold_bytes(threshold);
    }

    #[allow(non_snake_case)]
    pub fn GetDataPlacementThreshold(&self) -> usize {
        self.data_placement_threshold_bytes()
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }

    #[allow(non_snake_case)]
    pub fn RegisterAccessRecordCallback<F>(&self, callback: F)
    where
        F: Fn(CacheAccessRecord) + Send + Sync + 'static,
    {
        self.register_access_record_callback(callback);
    }

    #[allow(non_snake_case)]
    pub fn DeregisterAccessRecordCallback(&self) {
        self.clear_access_record_callback();
    }

    #[allow(non_snake_case)]
    pub fn RegisterEvictionCallback<F>(&self, callback: F)
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        self.register_eviction_callback(callback);
    }

    #[allow(non_snake_case)]
    pub fn RegisterEvictionHandler(&self) {
        self.set_eviction_handler_enabled(true);
    }

    #[allow(non_snake_case)]
    pub fn DeregisterEvictionHandler(&self) {
        self.set_eviction_handler_enabled(false);
    }

    #[allow(non_snake_case)]
    pub fn DisablePolicyMemEvictionHandler(&self) {
        self.set_eviction_handler_enabled(false);
    }

    #[allow(non_snake_case)]
    pub fn EvictionHandlerEnabled(&self) -> bool {
        self.eviction_handler_enabled()
    }

    #[allow(non_snake_case)]
    pub fn Acquire(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire(key)
    }

    #[allow(non_snake_case)]
    pub fn AcquireBatch(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        self.acquire_batch(keys)
    }

    #[allow(non_snake_case)]
    pub fn AcquireNoPromotion(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire_no_promotion(key)
    }

    #[allow(non_snake_case)]
    pub fn AcquireBatchNoPromotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        self.acquire_batch_no_promotion(keys)
    }

    #[allow(non_snake_case)]
    pub fn Release(&self, handle: CachePinnedHandle) {
        self.release(handle);
    }

    #[allow(non_snake_case)]
    pub fn ReleaseBatch(&self, handles: Vec<CachePinnedHandle>) -> usize {
        self.release_batch(handles)
    }

    #[allow(non_snake_case)]
    pub fn InsertPinned(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned(key, value)
    }

    #[allow(non_snake_case)]
    pub fn InsertPinnedDefaultSize(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned_default_size(key, value)
    }

    #[allow(non_snake_case)]
    pub fn InsertPinnedSized(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.insert_pinned_sized(key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn TEST_Insert(
        &self,
        instance_type: CacheInstanceType,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<(), CacheError> {
        self.test_insert(instance_type, key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn TEST_Acquire(
        &self,
        instance_type: CacheInstanceType,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.test_acquire(instance_type, key)
    }

    #[allow(non_snake_case)]
    pub fn TEST_Remove(
        &self,
        instance_type: CacheInstanceType,
        key: &CacheKey,
    ) -> Result<(), CacheError> {
        self.test_remove(instance_type, key)
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetUnifiedAcquireCount(&self) -> u64 {
        self.test_get_unified_acquire_count()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetUnifiedPutCount(&self) -> u64 {
        self.test_get_unified_put_count()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetUnifiedInsertPinnedCount(&self) -> u64 {
        self.test_get_unified_insert_pinned_count()
    }

    #[allow(non_snake_case)]
    pub fn TEST_JoinPmemWriteExecutor(&self) {
        self.test_join_pmem_write_executor();
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetPmemPaths(&self) -> Vec<String> {
        self.test_get_pmem_paths()
    }

    pub fn invalidate_memory_only(&self, key: &CacheKey) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        let key_pinned = inner.pinned.contains_key(key);
        let mut removed_pinned_bytes = 0usize;
        if let Some(value) = inner.memory.remove(key) {
            removed_pinned_bytes = removed_pinned_bytes.max(value.len());
            inner.memory_bytes = inner.memory_bytes.saturating_sub(value.len());
        }
        inner.order.retain(|candidate| candidate != key);
        if let Some(value) = inner.pmem.remove(key) {
            removed_pinned_bytes = removed_pinned_bytes.max(value.len());
            inner.pmem_bytes = inner.pmem_bytes.saturating_sub(value.len());
        }
        inner.pmem_order.retain(|candidate| candidate != key);
        if key_pinned && removed_pinned_bytes > 0 {
            inner
                .pinned_removed_bytes
                .insert(key.clone(), removed_pinned_bytes);
        } else {
            inner.pinned.remove(key);
            inner.pinned_handle_bytes.remove(key);
            inner.pinned_removed_bytes.remove(key);
        }
        inner.stats.invalidations += 1;
        inner.stats.memory_bytes = inner.memory_bytes as u64;
        inner.stats.pmem_bytes = inner.pmem_bytes as u64;
        inner.refresh_pin_stats();
        drop(inner);
        self.emit_access_record(CacheAccessRecordType::Delete, key);
    }

    pub fn production_tiering_policy(&self) -> CacheTieringPolicy {
        let inner = self.inner.read().expect("cache lock poisoned");
        inner.tiering_policy
    }

    pub fn data_placement(&self) -> CacheDataPlacement {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .tiering_policy
            .data_placement
    }

    pub fn reference_data_placement_type(&self) -> DRAMPMEMDataPlacementType {
        self.data_placement().into()
    }

    pub fn set_data_placement(&self, placement: CacheDataPlacement) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.tiering_policy.data_placement = placement;
    }

    pub fn set_reference_data_placement_type(&self, placement: DRAMPMEMDataPlacementType) {
        self.set_data_placement(placement.into());
    }

    pub fn data_placement_threshold_bytes(&self) -> usize {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .tiering_policy
            .data_placement_threshold_bytes
    }

    pub fn set_data_placement_threshold_bytes(&self, threshold_bytes: usize) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.tiering_policy.data_placement_threshold_bytes = threshold_bytes;
    }

    pub fn ssd_instance_only(&self) -> bool {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .ssd_instance_only
    }

    pub fn set_ssd_instance_only(&self, enabled: bool) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.ssd_instance_only = enabled;
        if enabled {
            inner.memory.clear();
            inner.pmem.clear();
            inner.order.clear();
            inner.memory_fifo_order.clear();
            inner.pmem_order.clear();
            inner.pmem_fifo_order.clear();
            inner.memory_bytes = 0;
            inner.pmem_bytes = 0;
            inner.refresh_usage_stats();
            inner.refresh_pin_stats();
        }
    }

    pub fn pmem_paths(&self) -> Vec<PathBuf> {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .pmem_paths
            .clone()
    }

    fn set_pmem_paths(&self, paths: Vec<PathBuf>) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.pmem_paths = paths;
    }

    pub fn update_production_tiering_policy(&self, policy: CacheTieringPolicy) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.memory_capacity_bytes = policy.memory_capacity_bytes;
        inner.pmem_capacity_bytes = policy.pmem_capacity_bytes;
        inner.ssd_capacity_bytes = policy.ssd_capacity_bytes;
        inner.tiering_policy = policy;
        inner.evict_memory_to_capacity();
        inner.evict_pmem_to_capacity();
        inner.evict_ssd_to_capacity();
        inner.refresh_usage_stats();
        inner.refresh_pin_stats();
        drop(inner);
        self.drain_eviction_records();
    }
}

#[derive(Debug, Clone)]
pub struct ShardedMultiLayerCache {
    shards: Arc<Vec<MultiLayerCache>>,
}

impl ShardedMultiLayerCache {
    pub fn with_options(options: CacheOptions, shard_count: usize) -> Self {
        let shard_count = shard_count.max(1);
        let shards = (0..shard_count)
            .map(|index| {
                MultiLayerCache::with_options(Self::options_for_shard(&options, shard_count, index))
            })
            .collect::<Vec<_>>();
        Self {
            shards: Arc::new(shards),
        }
    }

    pub fn new(options: CacheOptions, shard_count: usize) -> Self {
        Self::with_options(options, shard_count)
    }

    fn split_capacity(total: usize, shard_count: usize, index: usize) -> usize {
        let base = total / shard_count;
        let remainder = total % shard_count;
        base.saturating_add(usize::from(index < remainder))
    }

    fn options_for_shard(options: &CacheOptions, shard_count: usize, index: usize) -> CacheOptions {
        let mut shard_options = options.clone();
        shard_options.dram_capacity =
            Self::split_capacity(options.dram_capacity, shard_count, index);
        shard_options.pmem_capacity =
            Self::split_capacity(options.pmem_capacity, shard_count, index);
        shard_options.ssd_capacity = Self::split_capacity(options.ssd_capacity, shard_count, index);
        shard_options.ssd_paths = if options.ssd_paths.is_empty() {
            vec![options.disk_dir().join(format!("shard-{index}"))]
        } else {
            options
                .ssd_paths
                .iter()
                .map(|path| path.join(format!("shard-{index}")))
                .collect()
        };
        shard_options.pmem_paths = options
            .pmem_paths
            .iter()
            .map(|path| path.join(format!("shard-{index}")))
            .collect();
        if !options.metric_id_prefix.is_empty() {
            shard_options.metric_id_prefix = format!("{}-shard-{index}", options.metric_id_prefix);
        }
        shard_options
    }

    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    pub fn shard_index_for_key(&self, key: &CacheKey) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }

    fn shard_for_key(&self, key: &CacheKey) -> &MultiLayerCache {
        &self.shards[self.shard_index_for_key(key)]
    }

    pub fn start(&self) -> Result<(), CacheError> {
        for shard in self.shards.iter() {
            shard.start()?;
        }
        Ok(())
    }

    pub fn start_bool(&self) -> bool {
        self.start().is_ok()
    }

    pub fn stop(&self) -> bool {
        self.stop_async_writeback_workers();
        self.shards.iter().all(MultiLayerCache::stop)
    }

    pub fn start_async_writeback_workers(
        &self,
        max_jobs_per_round: usize,
        interval: Duration,
    ) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.start_async_writeback_worker(max_jobs_per_round, interval))
            .count()
    }

    pub fn stop_async_writeback_workers(&self) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.stop_async_writeback_worker())
            .count()
    }

    pub fn async_writeback_workers_running(&self) -> usize {
        self.shards
            .iter()
            .filter(|shard| shard.async_writeback_worker_running())
            .count()
    }

    pub fn set_async_writeback_queue_limit_for_test(&self, limit: usize) {
        for shard in self.shards.iter() {
            shard.set_async_writeback_queue_limit_for_test(limit);
        }
    }

    pub fn enqueue_async_writeback(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<(), CacheWritebackJob> {
        self.shard_for_key(&key).enqueue_async_writeback(key, value)
    }

    pub fn enqueue_async_writeback_batch(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<usize, Vec<CacheWritebackJob>> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut groups = vec![Vec::new(); self.shard_count()];
        for (key, value) in entries {
            let index = self.shard_index_for_key(&key);
            groups[index].push((key, value));
        }

        std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for (index, group) in groups.into_iter().enumerate() {
                if !group.is_empty() {
                    let shard = &self.shards[index];
                    workers.push(scope.spawn(move || shard.enqueue_async_writeback_batch(group)));
                }
            }

            let mut enqueued = 0usize;
            let mut rejected = Vec::new();
            for worker in workers {
                match worker.join() {
                    Ok(Ok(count)) => enqueued = enqueued.saturating_add(count),
                    Ok(Err(mut jobs)) => rejected.append(&mut jobs),
                    Err(_) => {
                        return Err(vec![CacheWritebackJob {
                            key: CacheKey::string(
                                0,
                                "sharded-async-writeback-enqueue-worker-panicked",
                            ),
                            value: Vec::new(),
                        }]);
                    }
                }
            }
            if rejected.is_empty() {
                Ok(enqueued)
            } else {
                Err(rejected)
            }
        })
    }

    pub fn submit_async_writeback_or_write_through(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        self.shard_for_key(&key)
            .submit_async_writeback_or_write_through(key, value)
    }

    pub fn submit_async_writeback_batch_or_write_through(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        let mut report = CacheWritebackSubmitReport::default();
        for (key, value) in entries {
            let submitted = self.submit_async_writeback_or_write_through(key, value)?;
            report.queued = report.queued.saturating_add(submitted.queued);
            report.write_through = report.write_through.saturating_add(submitted.write_through);
        }
        Ok(report)
    }

    pub fn drain_async_writeback(
        &self,
        max_jobs_per_shard: usize,
    ) -> Result<CacheWritebackDrainReport, CacheError> {
        let reports = std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for shard in self.shards.iter() {
                workers.push(scope.spawn(move || shard.drain_async_writeback(max_jobs_per_shard)));
            }

            let mut reports = Vec::with_capacity(workers.len());
            for worker in workers {
                match worker.join() {
                    Ok(Ok(report)) => reports.push(report),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded async writeback drain worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(reports)
        })?;

        Ok(CacheWritebackDrainReport {
            requested: max_jobs_per_shard.saturating_mul(self.shard_count()),
            drained: reports.iter().map(|report| report.drained).sum(),
            remaining: reports.iter().map(|report| report.remaining).sum(),
        })
    }

    pub fn flush_async_writeback(&self) -> Result<CacheWritebackDrainReport, CacheError> {
        let reports = std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for shard in self.shards.iter() {
                workers.push(scope.spawn(move || shard.flush_async_writeback()));
            }

            let mut reports = Vec::with_capacity(workers.len());
            for worker in workers {
                match worker.join() {
                    Ok(Ok(report)) => reports.push(report),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded async writeback flush worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(reports)
        })?;

        Ok(CacheWritebackDrainReport {
            requested: reports.iter().map(|report| report.requested).sum(),
            drained: reports.iter().map(|report| report.drained).sum(),
            remaining: reports.iter().map(|report| report.remaining).sum(),
        })
    }

    #[allow(non_snake_case)]
    pub fn FlushAsyncWriteback(&self) -> Result<CacheWritebackDrainReport, CacheError> {
        self.flush_async_writeback()
    }

    pub fn async_writeback_queue_depth(&self) -> u64 {
        self.shards
            .iter()
            .map(|shard| shard.stats().async_writeback_queue_depth)
            .sum()
    }

    pub fn async_writeback_queue_bytes(&self) -> u64 {
        self.shards
            .iter()
            .map(|shard| shard.stats().async_writeback_queue_bytes)
            .sum()
    }

    pub fn stats(&self) -> CacheStats {
        let mut total = CacheStats::default();
        macro_rules! add_stats {
            ($stats:expr, [$($field:ident),+ $(,)?]) => {
                $(
                    total.$field = total.$field.saturating_add($stats.$field);
                )+
            };
        }

        for shard in self.shards.iter() {
            let stats = shard.stats();
            add_stats!(
                stats,
                [
                    memory_hits,
                    disk_hits,
                    misses,
                    puts,
                    invalidations,
                    memory_evictions,
                    pmem_hits,
                    pmem_fills,
                    pmem_evictions,
                    pmem_admission_accepted,
                    pmem_admission_rejected,
                    pmem_eviction_capacity,
                    pmem_eviction_pinned_skips,
                    memory_admission_accepted,
                    memory_admission_rejected,
                    memory_fills,
                    disk_fills,
                    ssd_admission_accepted,
                    ssd_admission_rejected,
                    ssd_evictions,
                    ssd_eviction_capacity,
                    ssd_eviction_pinned_skips,
                    ssd_oversize_rejections,
                    ssd_write_through_admissions,
                    hotness_promotions,
                    refill_failures,
                    eviction_capacity,
                    eviction_oversize,
                    eviction_cold,
                    eviction_low_hit,
                    eviction_stale,
                    pinned_entries,
                    pinned_bytes,
                    pin_operations,
                    unpin_operations,
                    insert_pinned_operations,
                    eviction_pinned_skips,
                    zero_copy_handle_hits,
                    zero_copy_handle_misses,
                    async_writeback_enqueued,
                    async_writeback_drained,
                    async_writeback_backpressure_rejections,
                    writeback_backpressure_events,
                    async_writeback_queue_depth,
                    async_writeback_queue_bytes,
                    async_writeback_max_queue_depth,
                    async_writeback_max_queue_bytes,
                    get_latency_samples,
                    put_latency_samples,
                    get_latency_total_micros,
                    put_latency_total_micros,
                    get_latency_max_micros,
                    put_latency_max_micros,
                    get_latency_le_10us,
                    get_latency_le_100us,
                    get_latency_le_1ms,
                    get_latency_le_10ms,
                    get_latency_gt_10ms,
                    put_latency_le_10us,
                    put_latency_le_100us,
                    put_latency_le_1ms,
                    put_latency_le_10ms,
                    put_latency_gt_10ms,
                    read_through_latency_samples,
                    read_through_latency_le_10us,
                    read_through_latency_le_100us,
                    read_through_latency_le_1ms,
                    read_through_latency_le_10ms,
                    read_through_latency_gt_10ms,
                    refill_latency_samples,
                    refill_latency_le_10us,
                    refill_latency_le_100us,
                    refill_latency_le_1ms,
                    refill_latency_le_10ms,
                    refill_latency_gt_10ms,
                    writeback_latency_samples,
                    writeback_latency_le_10us,
                    writeback_latency_le_100us,
                    writeback_latency_le_1ms,
                    writeback_latency_le_10ms,
                    writeback_latency_gt_10ms,
                    eviction_latency_samples,
                    eviction_latency_le_10us,
                    eviction_latency_le_100us,
                    eviction_latency_le_1ms,
                    eviction_latency_le_10ms,
                    eviction_latency_gt_10ms,
                    compaction_latency_samples,
                    compaction_latency_le_10us,
                    compaction_latency_le_100us,
                    compaction_latency_le_1ms,
                    compaction_latency_le_10ms,
                    compaction_latency_gt_10ms,
                    eviction_sampled_groups,
                    memory_slot_evictions,
                    ssd_slot_evictions,
                    ssd_eviction_cold,
                    ssd_eviction_low_hit,
                    ssd_eviction_stale,
                    compressed_puts,
                    compressed_hits,
                    compression_bytes_saved,
                    get_latency_count,
                    get_latency_total_us,
                    get_latency_max_us,
                    put_latency_count,
                    put_latency_total_us,
                    put_latency_max_us,
                    memory_bytes,
                    pmem_bytes,
                    disk_bytes,
                ]
            );
        }
        total
    }

    pub fn eviction_report(&self) -> CacheEvictionReport {
        self.shards
            .iter()
            .map(MultiLayerCache::eviction_report)
            .fold(CacheEvictionReport::default(), |mut total, report| {
                total.memory_evictions = total
                    .memory_evictions
                    .saturating_add(report.memory_evictions);
                total.memory_capacity_evictions = total
                    .memory_capacity_evictions
                    .saturating_add(report.memory_capacity_evictions);
                total.memory_cold_evictions = total
                    .memory_cold_evictions
                    .saturating_add(report.memory_cold_evictions);
                total.memory_low_hit_evictions = total
                    .memory_low_hit_evictions
                    .saturating_add(report.memory_low_hit_evictions);
                total.memory_stale_evictions = total
                    .memory_stale_evictions
                    .saturating_add(report.memory_stale_evictions);
                total.memory_pinned_skips = total
                    .memory_pinned_skips
                    .saturating_add(report.memory_pinned_skips);
                total.pmem_evictions = total.pmem_evictions.saturating_add(report.pmem_evictions);
                total.pmem_capacity_evictions = total
                    .pmem_capacity_evictions
                    .saturating_add(report.pmem_capacity_evictions);
                total.pmem_pinned_skips = total
                    .pmem_pinned_skips
                    .saturating_add(report.pmem_pinned_skips);
                total.ssd_evictions = total.ssd_evictions.saturating_add(report.ssd_evictions);
                total.ssd_capacity_evictions = total
                    .ssd_capacity_evictions
                    .saturating_add(report.ssd_capacity_evictions);
                total.ssd_cold_evictions = total
                    .ssd_cold_evictions
                    .saturating_add(report.ssd_cold_evictions);
                total.ssd_low_hit_evictions = total
                    .ssd_low_hit_evictions
                    .saturating_add(report.ssd_low_hit_evictions);
                total.ssd_stale_evictions = total
                    .ssd_stale_evictions
                    .saturating_add(report.ssd_stale_evictions);
                total.ssd_pinned_skips = total
                    .ssd_pinned_skips
                    .saturating_add(report.ssd_pinned_skips);
                total.sampled_eviction_groups = total
                    .sampled_eviction_groups
                    .saturating_add(report.sampled_eviction_groups);
                total.memory_slot_evictions = total
                    .memory_slot_evictions
                    .saturating_add(report.memory_slot_evictions);
                total.ssd_slot_evictions = total
                    .ssd_slot_evictions
                    .saturating_add(report.ssd_slot_evictions);
                total.replacement_policy = report.replacement_policy;
                total
            })
    }

    pub fn writeback_backpressure_report(&self) -> CacheWritebackBackpressureReport {
        self.shards
            .iter()
            .map(MultiLayerCache::writeback_backpressure_report)
            .fold(
                CacheWritebackBackpressureReport {
                    ssd_write_through_enabled: true,
                    bounded_queue_ready: true,
                    ..CacheWritebackBackpressureReport::default()
                },
                |mut total, report| {
                    total.ssd_write_through_enabled &=
                        report.ssd_write_through_enabled || self.shards.is_empty();
                    total.write_through_admissions = total
                        .write_through_admissions
                        .saturating_add(report.write_through_admissions);
                    total.ssd_admission_rejections = total
                        .ssd_admission_rejections
                        .saturating_add(report.ssd_admission_rejections);
                    total.ssd_evictions = total.ssd_evictions.saturating_add(report.ssd_evictions);
                    total.ssd_oversize_rejections = total
                        .ssd_oversize_rejections
                        .saturating_add(report.ssd_oversize_rejections);
                    total.backpressure_events = total
                        .backpressure_events
                        .saturating_add(report.backpressure_events);
                    total.bounded_queue_ready &= report.bounded_queue_ready;
                    total
                },
            )
    }

    pub fn latency_metrics_report(&self) -> CacheLatencyMetricsReport {
        let mut get_count = 0u64;
        let mut get_total_us = 0u64;
        let mut get_max_us = 0u64;
        let mut put_count = 0u64;
        let mut put_total_us = 0u64;
        let mut put_max_us = 0u64;
        let mut histogram_ready = false;

        for shard in self.shards.iter() {
            let report = shard.latency_metrics_report();
            get_count = get_count.saturating_add(report.get_count);
            get_total_us =
                get_total_us.saturating_add(report.get_count.saturating_mul(report.get_avg_us));
            get_max_us = get_max_us.max(report.get_max_us);
            put_count = put_count.saturating_add(report.put_count);
            put_total_us =
                put_total_us.saturating_add(report.put_count.saturating_mul(report.put_avg_us));
            put_max_us = put_max_us.max(report.put_max_us);
            histogram_ready |= report.histogram_ready;
        }

        CacheLatencyMetricsReport {
            get_count,
            get_avg_us: if get_count == 0 {
                0
            } else {
                get_total_us / get_count
            },
            get_max_us,
            put_count,
            put_avg_us: if put_count == 0 {
                0
            } else {
                put_total_us / put_count
            },
            put_max_us,
            histogram_ready,
        }
    }

    pub fn replacement_policy_soak(
        &self,
        iterations_per_shard: usize,
    ) -> CacheReplacementPolicySoakReport {
        let reports = self
            .shards
            .iter()
            .map(|shard| shard.replacement_policy_soak(iterations_per_shard))
            .collect::<Vec<_>>();
        let mut aggregate = CacheReplacementPolicySoakReport {
            pinned_memory_survived: true,
            restart_disk_refill_ready: true,
            read_through_latency_bucketed: true,
            refill_latency_bucketed: true,
            writeback_latency_bucketed: true,
            eviction_latency_bucketed: true,
            compaction_latency_bucketed: true,
            passed: true,
            ..CacheReplacementPolicySoakReport::default()
        };

        for (index, report) in reports.into_iter().enumerate() {
            aggregate.iterations = aggregate.iterations.saturating_add(report.iterations);
            aggregate.hot_key_count = aggregate.hot_key_count.saturating_add(report.hot_key_count);
            aggregate.cold_key_count = aggregate
                .cold_key_count
                .saturating_add(report.cold_key_count);
            aggregate.hot_memory_survivors = aggregate
                .hot_memory_survivors
                .saturating_add(report.hot_memory_survivors);
            aggregate.cold_memory_survivors = aggregate
                .cold_memory_survivors
                .saturating_add(report.cold_memory_survivors);
            aggregate.pinned_memory_survived &= report.pinned_memory_survived;
            aggregate.restart_disk_refill_ready &= report.restart_disk_refill_ready;
            aggregate.observed_evictions = aggregate
                .observed_evictions
                .saturating_add(report.observed_evictions);
            aggregate.observed_pinned_skips = aggregate
                .observed_pinned_skips
                .saturating_add(report.observed_pinned_skips);
            aggregate.observed_disk_refills = aggregate
                .observed_disk_refills
                .saturating_add(report.observed_disk_refills);
            aggregate.observed_async_writeback_backpressure = aggregate
                .observed_async_writeback_backpressure
                .saturating_add(report.observed_async_writeback_backpressure);
            aggregate.async_writeback_max_queue_depth = aggregate
                .async_writeback_max_queue_depth
                .max(report.async_writeback_max_queue_depth);
            aggregate.async_writeback_max_queue_bytes = aggregate
                .async_writeback_max_queue_bytes
                .max(report.async_writeback_max_queue_bytes);
            aggregate.get_latency_samples = aggregate
                .get_latency_samples
                .saturating_add(report.get_latency_samples);
            aggregate.put_latency_samples = aggregate
                .put_latency_samples
                .saturating_add(report.put_latency_samples);
            aggregate.read_through_latency_samples = aggregate
                .read_through_latency_samples
                .saturating_add(report.read_through_latency_samples);
            aggregate.refill_latency_samples = aggregate
                .refill_latency_samples
                .saturating_add(report.refill_latency_samples);
            aggregate.writeback_latency_samples = aggregate
                .writeback_latency_samples
                .saturating_add(report.writeback_latency_samples);
            aggregate.eviction_latency_samples = aggregate
                .eviction_latency_samples
                .saturating_add(report.eviction_latency_samples);
            aggregate.compaction_latency_samples = aggregate
                .compaction_latency_samples
                .saturating_add(report.compaction_latency_samples);
            aggregate.read_through_latency_bucketed &= report.read_through_latency_bucketed;
            aggregate.refill_latency_bucketed &= report.refill_latency_bucketed;
            aggregate.writeback_latency_bucketed &= report.writeback_latency_bucketed;
            aggregate.eviction_latency_bucketed &= report.eviction_latency_bucketed;
            aggregate.compaction_latency_bucketed &= report.compaction_latency_bucketed;
            aggregate.passed &= report.passed;
            aggregate.reasons.extend(
                report
                    .reasons
                    .into_iter()
                    .map(|reason| format!("shard-{index}:{reason}")),
            );
        }
        aggregate.passed = aggregate.passed && aggregate.reasons.is_empty();
        aggregate
    }

    #[allow(non_snake_case)]
    pub fn EvictionReport(&self) -> CacheEvictionReport {
        self.eviction_report()
    }

    #[allow(non_snake_case)]
    pub fn WritebackBackpressureReport(&self) -> CacheWritebackBackpressureReport {
        self.writeback_backpressure_report()
    }

    #[allow(non_snake_case)]
    pub fn LatencyMetricsReport(&self) -> CacheLatencyMetricsReport {
        self.latency_metrics_report()
    }

    #[allow(non_snake_case)]
    pub fn Stats(&self) -> CacheStats {
        self.stats()
    }

    #[allow(non_snake_case)]
    pub fn ReplacementPolicySoak(
        &self,
        iterations_per_shard: usize,
    ) -> CacheReplacementPolicySoakReport {
        self.replacement_policy_soak(iterations_per_shard)
    }

    #[allow(non_snake_case)]
    pub fn EnqueueAsyncWriteback(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<(), CacheWritebackJob> {
        self.enqueue_async_writeback(key, value)
    }

    #[allow(non_snake_case)]
    pub fn EnqueueAsyncWritebackBatch(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<usize, Vec<CacheWritebackJob>> {
        self.enqueue_async_writeback_batch(entries)
    }

    #[allow(non_snake_case)]
    pub fn DrainAsyncWriteback(
        &self,
        max_jobs_per_shard: usize,
    ) -> Result<CacheWritebackDrainReport, CacheError> {
        self.drain_async_writeback(max_jobs_per_shard)
    }
    #[allow(non_snake_case)]
    pub fn SubmitAsyncWritebackOrWriteThrough(
        &self,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        self.submit_async_writeback_or_write_through(key, value)
    }

    #[allow(non_snake_case)]
    pub fn SubmitAsyncWritebackBatchOrWriteThrough(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<CacheWritebackSubmitReport, CacheError> {
        self.submit_async_writeback_batch_or_write_through(entries)
    }

    pub fn stop_bool(&self) -> bool {
        self.stop()
    }

    pub fn put_sized(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.shard_for_key(&key).put_sized(key, value, size)
    }

    pub fn put(&self, key: CacheKey, value: Vec<u8>) -> Result<(), CacheError> {
        self.shard_for_key(&key).put(key, value)
    }

    pub fn insert(&self, key: CacheKey, value: Vec<u8>, size: usize) -> Result<(), CacheError> {
        self.put_sized(key, value, size)
    }

    pub fn put_batch_sized(
        &self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<usize, CacheError> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut grouped = vec![Vec::new(); self.shard_count()];
        for (key, value, size) in entries {
            grouped[self.shard_index_for_key(&key)].push((key, value, size));
        }
        std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for (index, group) in grouped.into_iter().enumerate() {
                if !group.is_empty() {
                    let shard = &self.shards[index];
                    workers.push(scope.spawn(move || shard.put_batch_sized(group)));
                }
            }

            let mut inserted = 0usize;
            for worker in workers {
                match worker.join() {
                    Ok(Ok(count)) => inserted = inserted.saturating_add(count),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache batch put worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(inserted)
        })
    }

    pub fn put_batch(&self, entries: Vec<(CacheKey, Vec<u8>)>) -> Result<usize, CacheError> {
        self.put_batch_sized(
            entries
                .into_iter()
                .map(|(key, value)| {
                    let logical_size = value.len();
                    (key, value, logical_size)
                })
                .collect(),
        )
    }

    pub fn get(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.shard_for_key(key).get(key)
    }

    pub fn lookup(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.get(key)
    }

    pub fn get_memory(&self, key: &CacheKey) -> Option<Vec<u8>> {
        self.shard_for_key(key).get_memory(key)
    }

    fn get_batch_with_shard_reader<F, T>(
        &self,
        keys: &[CacheKey],
        reader: F,
    ) -> Result<Vec<Option<T>>, CacheError>
    where
        F: Fn(&MultiLayerCache, &[CacheKey]) -> Result<Vec<Option<T>>, CacheError> + Copy + Send,
        T: Clone + Send,
    {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut grouped = vec![Vec::new(); self.shard_count()];
        for (position, key) in keys.iter().cloned().enumerate() {
            grouped[self.shard_index_for_key(&key)].push((position, key));
        }
        let shard_results = std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for (index, group) in grouped.into_iter().enumerate() {
                if group.is_empty() {
                    continue;
                }
                let shard = &self.shards[index];
                workers.push(scope.spawn(move || {
                    let mut unique_positions = HashMap::new();
                    let mut unique_keys = Vec::new();
                    let mut requested_positions = Vec::with_capacity(group.len());
                    for (position, key) in group {
                        let unique_position =
                            if let Some(position) = unique_positions.get(&key).copied() {
                                position
                            } else {
                                let position = unique_keys.len();
                                unique_positions.insert(key.clone(), position);
                                unique_keys.push(key);
                                position
                            };
                        requested_positions.push((position, unique_position));
                    }
                    let values = reader(shard, &unique_keys)?;
                    Ok::<_, CacheError>(
                        requested_positions
                            .into_iter()
                            .map(|(position, unique_position)| {
                                (position, values[unique_position].clone())
                            })
                            .collect::<Vec<_>>(),
                    )
                }));
            }

            let mut merged = Vec::new();
            for worker in workers {
                match worker.join() {
                    Ok(Ok(values)) => merged.extend(values),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache batch get worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(merged)
        })?;

        let mut results = vec![None; keys.len()];
        for (position, value) in shard_results {
            results[position] = value;
        }
        Ok(results)
    }

    pub fn get_batch(&self, keys: &[CacheKey]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        self.get_batch_with_shard_reader(keys, MultiLayerCache::get_batch)
    }

    pub fn get_no_promotion(&self, key: &CacheKey) -> Result<Option<CacheReadResult>, CacheError> {
        self.shard_for_key(key).get_no_promotion(key)
    }

    pub fn get_batch_no_promotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CacheReadResult>>, CacheError> {
        self.get_batch_with_shard_reader(keys, MultiLayerCache::get_batch_no_promotion)
    }

    pub fn lookup_no_promotion(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        Ok(self.get_no_promotion(key)?.map(|result| result.value))
    }

    pub fn lookup_batch_no_promotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        self.get_batch_no_promotion(keys).map(|results| {
            results
                .into_iter()
                .map(|result| result.map(|read| read.value))
                .collect()
        })
    }

    fn aggregate_gc_reports(
        shard_id: ShardId,
        reports: impl IntoIterator<Item = CacheGcReport>,
    ) -> CacheGcReport {
        reports.into_iter().fold(
            CacheGcReport {
                shard_id,
                ..CacheGcReport::default()
            },
            |mut total, report| {
                total.memory_entries_removed = total
                    .memory_entries_removed
                    .saturating_add(report.memory_entries_removed);
                total.disk_bytes_removed = total
                    .disk_bytes_removed
                    .saturating_add(report.disk_bytes_removed);
                total
            },
        )
    }

    pub fn invalidate_shard(&self, shard_id: ShardId) -> Result<CacheGcReport, CacheError> {
        let reports = std::thread::scope(|scope| {
            let workers = self
                .shards
                .iter()
                .map(|shard| scope.spawn(move || shard.invalidate_shard(shard_id)))
                .collect::<Vec<_>>();

            let mut reports = Vec::with_capacity(workers.len());
            for worker in workers {
                match worker.join() {
                    Ok(Ok(report)) => reports.push(report),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache invalidate_shard worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(reports)
        })?;
        Ok(Self::aggregate_gc_reports(shard_id, reports))
    }

    pub fn invalidate_slot(
        &self,
        shard_id: ShardId,
        routing_slot: u32,
    ) -> Result<CacheGcReport, CacheError> {
        let reports = std::thread::scope(|scope| {
            let workers = self
                .shards
                .iter()
                .map(|shard| scope.spawn(move || shard.invalidate_slot(shard_id, routing_slot)))
                .collect::<Vec<_>>();

            let mut reports = Vec::with_capacity(workers.len());
            for worker in workers {
                match worker.join() {
                    Ok(Ok(report)) => reports.push(report),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache invalidate_slot worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(reports)
        })?;
        Ok(Self::aggregate_gc_reports(shard_id, reports))
    }

    pub fn invalidate_page_segment(
        &self,
        shard_id: ShardId,
        page_segment_id: u64,
    ) -> Result<CacheGcReport, CacheError> {
        let reports = std::thread::scope(|scope| {
            let workers = self
                .shards
                .iter()
                .map(|shard| {
                    scope.spawn(move || shard.invalidate_page_segment(shard_id, page_segment_id))
                })
                .collect::<Vec<_>>();

            let mut reports = Vec::with_capacity(workers.len());
            for worker in workers {
                match worker.join() {
                    Ok(Ok(report)) => reports.push(report),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache invalidate_page_segment worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(reports)
        })?;
        Ok(Self::aggregate_gc_reports(shard_id, reports))
    }

    pub fn entries_for_shard(&self, shard_id: ShardId) -> Vec<CacheEntryInfo> {
        let mut entries = self
            .shards
            .iter()
            .flat_map(|shard| shard.entries_for_shard(shard_id))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.namespace
                .cmp(&right.namespace)
                .then(left.record_key.cmp(&right.record_key))
                .then(left.selector.cmp(&right.selector))
        });
        entries
    }

    pub fn all_entries(&self) -> Vec<CacheEntryInfo> {
        let mut entries = self
            .shards
            .iter()
            .flat_map(MultiLayerCache::all_entries)
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.shard_id
                .cmp(&right.shard_id)
                .then(left.namespace.cmp(&right.namespace))
                .then(left.record_key.cmp(&right.record_key))
                .then(left.selector.cmp(&right.selector))
        });
        entries
    }

    #[allow(non_snake_case)]
    pub fn EntriesForShard(&self, shard_id: ShardId) -> Vec<CacheEntryInfo> {
        self.entries_for_shard(shard_id)
    }

    #[allow(non_snake_case)]
    pub fn AllEntries(&self) -> Vec<CacheEntryInfo> {
        self.all_entries()
    }

    #[allow(non_snake_case)]
    pub fn InvalidateShard(&self, shard_id: ShardId) -> Result<CacheGcReport, CacheError> {
        self.invalidate_shard(shard_id)
    }

    #[allow(non_snake_case)]
    pub fn InvalidateSlot(
        &self,
        shard_id: ShardId,
        routing_slot: u32,
    ) -> Result<CacheGcReport, CacheError> {
        self.invalidate_slot(shard_id, routing_slot)
    }

    #[allow(non_snake_case)]
    pub fn InvalidatePageSegment(
        &self,
        shard_id: ShardId,
        page_segment_id: u64,
    ) -> Result<CacheGcReport, CacheError> {
        self.invalidate_page_segment(shard_id, page_segment_id)
    }

    pub fn acquire(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.shard_for_key(key).acquire(key)
    }

    pub fn acquire_no_promotion(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.shard_for_key(key).acquire_no_promotion(key)
    }

    pub fn acquire_batch_no_promotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        let mut results = (0..keys.len()).map(|_| None).collect::<Vec<_>>();
        let mut positions_by_key = Vec::<(CacheKey, Vec<usize>)>::new();
        let mut unique_positions = HashMap::<CacheKey, usize>::new();
        for (position, key) in keys.iter().cloned().enumerate() {
            if let Some(unique_position) = unique_positions.get(&key).copied() {
                positions_by_key[unique_position].1.push(position);
            } else {
                unique_positions.insert(key.clone(), positions_by_key.len());
                positions_by_key.push((key, vec![position]));
            }
        }
        for (key, positions) in positions_by_key {
            let Some(handle) = self.acquire_no_promotion(&key)? else {
                continue;
            };
            let first_position = positions[0];
            results[first_position] = Some(handle);
            for position in positions.into_iter().skip(1) {
                let cloned = self.shard_for_key(&key).clone_handle(
                    results[first_position]
                        .as_ref()
                        .expect("first batch handle is installed"),
                );
                results[position] = Some(cloned);
            }
        }
        Ok(results)
    }

    pub fn acquire_batch(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        let mut results = (0..keys.len()).map(|_| None).collect::<Vec<_>>();
        let mut positions_by_key = Vec::<(CacheKey, Vec<usize>)>::new();
        let mut unique_positions = HashMap::<CacheKey, usize>::new();
        for (position, key) in keys.iter().cloned().enumerate() {
            if let Some(unique_position) = unique_positions.get(&key).copied() {
                positions_by_key[unique_position].1.push(position);
            } else {
                unique_positions.insert(key.clone(), positions_by_key.len());
                positions_by_key.push((key, vec![position]));
            }
        }
        for (key, positions) in positions_by_key {
            let Some(handle) = self.acquire(&key)? else {
                continue;
            };
            let first_position = positions[0];
            results[first_position] = Some(handle);
            for position in positions.into_iter().skip(1) {
                let cloned = self.shard_for_key(&key).clone_handle(
                    results[first_position]
                        .as_ref()
                        .expect("first sharded batch handle is installed"),
                );
                results[position] = Some(cloned);
            }
        }
        Ok(results)
    }

    pub fn release(&self, handle: CachePinnedHandle) {
        self.shard_for_key(&handle.key).release(handle);
    }

    pub fn release_batch(&self, handles: Vec<CachePinnedHandle>) -> usize {
        if handles.is_empty() {
            return 0;
        }
        let mut groups = (0..self.shard_count())
            .map(|_| Vec::<CachePinnedHandle>::new())
            .collect::<Vec<_>>();
        let released = handles.len();
        for handle in handles {
            groups[self.shard_index_for_key(&handle.key)].push(handle);
        }
        std::thread::scope(|scope| {
            let workers = groups
                .into_iter()
                .enumerate()
                .filter_map(|(index, group)| {
                    if group.is_empty() {
                        None
                    } else {
                        let shard = &self.shards[index];
                        Some(scope.spawn(move || shard.release_batch(group)))
                    }
                })
                .collect::<Vec<_>>();
            for worker in workers {
                worker
                    .join()
                    .expect("sharded cache batch release worker panicked");
            }
        });
        released
    }

    pub fn pin(&self, key: CacheKey) {
        self.shard_for_key(&key).pin(key);
    }

    pub fn pin_batch(&self, keys: Vec<CacheKey>) -> usize {
        if keys.is_empty() {
            return 0;
        }
        let mut groups = (0..self.shard_count())
            .map(|_| Vec::<CacheKey>::new())
            .collect::<Vec<_>>();
        let pinned = keys.len();
        for key in keys {
            groups[self.shard_index_for_key(&key)].push(key);
        }
        std::thread::scope(|scope| {
            let workers = groups
                .into_iter()
                .enumerate()
                .filter_map(|(index, group)| {
                    if group.is_empty() {
                        None
                    } else {
                        let shard = &self.shards[index];
                        Some(scope.spawn(move || {
                            for key in group {
                                shard.pin(key);
                            }
                        }))
                    }
                })
                .collect::<Vec<_>>();
            for worker in workers {
                worker
                    .join()
                    .expect("sharded cache batch pin worker panicked");
            }
        });
        pinned
    }

    pub fn unpin(&self, key: &CacheKey) {
        self.shard_for_key(key).unpin(key);
    }

    pub fn unpin_batch(&self, keys: &[CacheKey]) -> usize {
        if keys.is_empty() {
            return 0;
        }
        let mut groups = (0..self.shard_count())
            .map(|_| Vec::<CacheKey>::new())
            .collect::<Vec<_>>();
        let unpinned = keys.len();
        for key in keys.iter().cloned() {
            groups[self.shard_index_for_key(&key)].push(key);
        }
        std::thread::scope(|scope| {
            let workers = groups
                .into_iter()
                .enumerate()
                .filter_map(|(index, group)| {
                    if group.is_empty() {
                        None
                    } else {
                        let shard = &self.shards[index];
                        Some(scope.spawn(move || {
                            for key in group {
                                shard.unpin(&key);
                            }
                        }))
                    }
                })
                .collect::<Vec<_>>();
            for worker in workers {
                worker
                    .join()
                    .expect("sharded cache batch unpin worker panicked");
            }
        });
        unpinned
    }

    #[allow(non_snake_case)]
    pub fn Pin(&self, key: CacheKey) {
        self.pin(key);
    }

    #[allow(non_snake_case)]
    pub fn PinBatch(&self, keys: Vec<CacheKey>) -> usize {
        self.pin_batch(keys)
    }

    #[allow(non_snake_case)]
    pub fn Unpin(&self, key: &CacheKey) {
        self.unpin(key);
    }

    #[allow(non_snake_case)]
    pub fn UnpinBatch(&self, keys: &[CacheKey]) -> usize {
        self.unpin_batch(keys)
    }

    pub fn insert_pinned_sized(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.shard_for_key(&key)
            .insert_pinned_sized(key, value, size)
    }

    pub fn insert_pinned_batch_sized(
        &self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let mut groups = (0..self.shard_count())
            .map(|_| Vec::<(usize, CacheKey, Vec<u8>, usize)>::new())
            .collect::<Vec<_>>();
        let entry_count = entries.len();
        for (position, (key, value, size)) in entries.into_iter().enumerate() {
            groups[self.shard_index_for_key(&key)].push((position, key, value, size));
        }

        let shard_results = std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for (index, group) in groups.into_iter().enumerate() {
                if !group.is_empty() {
                    let shard = &self.shards[index];
                    workers.push(scope.spawn(move || {
                        let mut handles = Vec::with_capacity(group.len());
                        for (position, key, value, size) in group {
                            handles.push((position, shard.insert_pinned_sized(key, value, size)?));
                        }
                        Ok::<_, CacheError>(handles)
                    }));
                }
            }

            let mut merged = Vec::new();
            for worker in workers {
                match worker.join() {
                    Ok(Ok(handles)) => merged.extend(handles),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache batch pinned insert worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(merged)
        })?;

        let mut results = (0..entry_count).map(|_| None).collect::<Vec<_>>();
        for (position, handle) in shard_results {
            results[position] = handle;
        }
        Ok(results)
    }

    pub fn insert_pinned_batch(
        &self,
        entries: Vec<(CacheKey, Vec<u8>)>,
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        self.insert_pinned_batch_sized(
            entries
                .into_iter()
                .map(|(key, value)| {
                    let logical_size = value.len();
                    (key, value, logical_size)
                })
                .collect(),
        )
    }

    #[allow(non_snake_case)]
    pub fn InsertPinnedBatch(
        &self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        self.insert_pinned_batch_sized(entries)
    }

    pub fn remove(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.shard_for_key(key).remove(key)
    }

    pub fn remove_batch(&self, keys: &[CacheKey]) -> Result<usize, CacheError> {
        if keys.is_empty() {
            return Ok(0);
        }
        let mut groups = vec![Vec::new(); self.shard_count()];
        for key in unique_cache_keys(keys) {
            groups[self.shard_index_for_key(&key)].push(key);
        }
        std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for (index, group) in groups.into_iter().enumerate() {
                if !group.is_empty() {
                    let shard = &self.shards[index];
                    workers.push(scope.spawn(move || shard.remove_batch(&group)));
                }
            }

            let mut removed = 0usize;
            for worker in workers {
                match worker.join() {
                    Ok(Ok(count)) => removed = removed.saturating_add(count),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache batch remove worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(removed)
        })
        .map(|_| keys.len())
    }

    pub fn remove_all(&self) -> Result<(), CacheError> {
        std::thread::scope(|scope| {
            let workers: Vec<_> = self
                .shards
                .iter()
                .map(|shard| scope.spawn(move || shard.remove_all()))
                .collect();

            for worker in workers {
                match worker.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache remove_all worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(())
        })
    }

    pub fn reset(&self) -> Result<(), CacheError> {
        std::thread::scope(|scope| {
            let workers: Vec<_> = self
                .shards
                .iter()
                .map(|shard| scope.spawn(move || shard.reset()))
                .collect();

            for worker in workers {
                match worker.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache reset worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(())
        })
    }

    pub fn capacity(&self) -> usize {
        self.shards.iter().map(MultiLayerCache::capacity).sum()
    }

    pub fn capacity_for_tier(&self, tier: CacheTier) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.capacity_for_tier(tier))
            .sum()
    }

    pub fn get_capacity(&self, instance_type: CacheInstanceType) -> usize {
        match instance_type.as_tier() {
            Some(tier) => self.capacity_for_tier(tier),
            None => self.capacity(),
        }
    }

    pub fn set_capacity(&self, capacity: usize) {
        let shard_count = self.shard_count();
        std::thread::scope(|scope| {
            let workers: Vec<_> = self
                .shards
                .iter()
                .enumerate()
                .map(|(index, shard)| {
                    let shard_capacity = Self::split_capacity(capacity, shard_count, index);
                    scope.spawn(move || shard.set_capacity(shard_capacity))
                })
                .collect();

            for worker in workers {
                worker
                    .join()
                    .expect("sharded cache set_capacity worker panicked");
            }
        });
    }

    pub fn set_capacity_for_tier(&self, tier: CacheTier, capacity: usize) {
        let shard_count = self.shard_count();
        std::thread::scope(|scope| {
            let workers = self
                .shards
                .iter()
                .enumerate()
                .map(|(index, shard)| {
                    let shard_capacity = Self::split_capacity(capacity, shard_count, index);
                    scope.spawn(move || shard.set_capacity_for_tier(tier, shard_capacity))
                })
                .collect::<Vec<_>>();

            for worker in workers {
                worker
                    .join()
                    .expect("sharded cache set_capacity_for_tier worker panicked");
            }
        });
    }

    pub fn set_capacity_for_instance(&self, instance_type: CacheInstanceType, capacity: usize) {
        match instance_type.as_tier() {
            Some(tier) => self.set_capacity_for_tier(tier, capacity),
            None => self.set_capacity(capacity),
        }
    }

    pub fn size(&self) -> usize {
        self.shards.iter().map(MultiLayerCache::size).sum()
    }

    pub fn size_for_tier(&self, tier: CacheTier) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.size_for_tier(tier))
            .sum()
    }

    pub fn used_space_for_tier(&self, tier: CacheTier) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.used_space_for_tier(tier))
            .sum()
    }

    pub fn get_used(&self, instance_type: CacheInstanceType) -> usize {
        match instance_type.as_tier() {
            Some(tier) => self.used_space_for_tier(tier),
            None => self.size(),
        }
    }

    pub fn item_count_for_tier(&self, tier: CacheTier) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.item_count_for_tier(tier))
            .sum()
    }

    pub fn replacement_policy_for_tier(&self, tier: CacheTier) -> CacheReplacementPolicy {
        self.shards
            .first()
            .map(|shard| shard.replacement_policy_for_tier(tier))
            .unwrap_or(CacheReplacementPolicy::WeightedHotnessLru)
    }

    pub fn get_replacement_policy_type(
        &self,
        instance_type: CacheInstanceType,
    ) -> CacheReplacementPolicy {
        match instance_type.as_tier() {
            Some(tier) => self.replacement_policy_for_tier(tier),
            None => CacheReplacementPolicy::WeightedHotnessLru,
        }
    }

    pub fn set_replacement_policy_for_tier(&self, tier: CacheTier, policy: CacheReplacementPolicy) {
        std::thread::scope(|scope| {
            let workers = self
                .shards
                .iter()
                .map(|shard| {
                    scope.spawn(move || shard.set_replacement_policy_for_tier(tier, policy))
                })
                .collect::<Vec<_>>();
            for worker in workers {
                worker
                    .join()
                    .expect("sharded cache set replacement policy worker panicked");
            }
        });
    }

    pub fn try_set_replacement_policy_for_tier(
        &self,
        tier: CacheTier,
        policy: CacheReplacementPolicy,
    ) -> Result<(), CacheError> {
        if matches!(tier, CacheTier::Reject) {
            return Err(CacheError::UnsupportedTier(tier));
        }
        if self.shards.iter().any(MultiLayerCache::is_started) {
            return Err(CacheError::AlreadyStarted);
        }
        self.set_replacement_policy_for_tier(tier, policy);
        Ok(())
    }

    pub fn set_replacement_policy_type(
        &self,
        instance_type: CacheInstanceType,
        policy: CacheReplacementPolicy,
    ) {
        if let Some(tier) = instance_type.as_tier() {
            self.set_replacement_policy_for_tier(tier, policy);
        }
    }

    pub fn try_set_replacement_policy_type(
        &self,
        instance_type: CacheInstanceType,
        policy: CacheReplacementPolicy,
    ) -> Result<(), CacheError> {
        let Some(tier) = instance_type.as_tier() else {
            return Err(CacheError::UnsupportedInstance(instance_type));
        };
        self.try_set_replacement_policy_for_tier(tier, policy)
    }

    #[allow(non_snake_case)]
    pub fn WithOptions(options: CacheOptions, shard_count: usize) -> Self {
        Self::with_options(options, shard_count)
    }

    #[allow(non_snake_case)]
    pub fn ShardCount(&self) -> usize {
        self.shard_count()
    }

    #[allow(non_snake_case)]
    pub fn CapacityForTier(&self, tier: CacheTier) -> usize {
        self.capacity_for_tier(tier)
    }

    #[allow(non_snake_case)]
    pub fn GetCapacity(&self, instance_type: CacheInstanceType) -> usize {
        self.get_capacity(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetCapacityForTier(&self, tier: CacheTier, capacity: usize) {
        self.set_capacity_for_tier(tier, capacity);
    }

    #[allow(non_snake_case)]
    pub fn SetCapacityForInstance(&self, instance_type: CacheInstanceType, capacity: usize) {
        self.set_capacity_for_instance(instance_type, capacity);
    }

    #[allow(non_snake_case)]
    pub fn SizeForTier(&self, tier: CacheTier) -> usize {
        self.size_for_tier(tier)
    }

    #[allow(non_snake_case)]
    pub fn GetUsed(&self, instance_type: CacheInstanceType) -> usize {
        self.get_used(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn GetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceType,
    ) -> CacheReplacementPolicy {
        self.get_replacement_policy_type(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceType,
        policy: CacheReplacementPolicy,
    ) {
        self.set_replacement_policy_type(instance_type, policy);
    }

    #[allow(non_snake_case)]
    pub fn TrySetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceType,
        policy: CacheReplacementPolicy,
    ) -> Result<(), CacheError> {
        self.try_set_replacement_policy_type(instance_type, policy)
    }

    #[allow(non_snake_case)]
    pub fn ReplacementPolicyForTier(&self, tier: CacheTier) -> CacheReplacementPolicy {
        self.replacement_policy_for_tier(tier)
    }

    #[allow(non_snake_case)]
    pub fn SetReplacementPolicyForTier(&self, tier: CacheTier, policy: CacheReplacementPolicy) {
        self.set_replacement_policy_for_tier(tier, policy);
    }

    #[allow(non_snake_case)]
    pub fn TrySetReplacementPolicyForTier(
        &self,
        tier: CacheTier,
        policy: CacheReplacementPolicy,
    ) -> Result<(), CacheError> {
        self.try_set_replacement_policy_for_tier(tier, policy)
    }

    #[allow(non_snake_case)]
    pub fn RemoveAll(&self) -> Result<(), CacheError> {
        self.remove_all()
    }

    #[allow(non_snake_case)]
    pub fn Reset(&self) -> Result<(), CacheError> {
        self.reset()
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
    pub fn LookupNoPromotion(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup_no_promotion(key)
    }

    #[allow(non_snake_case)]
    pub fn LookupBatchNoPromotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        self.lookup_batch_no_promotion(keys)
    }

    #[allow(non_snake_case)]
    pub fn GetNoPromotion(&self, key: &CacheKey) -> Result<Option<CacheReadResult>, CacheError> {
        self.get_no_promotion(key)
    }

    #[allow(non_snake_case)]
    pub fn GetBatchNoPromotion(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CacheReadResult>>, CacheError> {
        self.get_batch_no_promotion(keys)
    }

    #[allow(non_snake_case)]
    pub fn Remove(&self, key: &CacheKey) -> Result<(), CacheError> {
        self.remove(key)
    }

    #[allow(non_snake_case)]
    pub fn RemoveBatch(&self, keys: &[CacheKey]) -> Result<usize, CacheError> {
        self.remove_batch(keys)
    }

    #[allow(non_snake_case)]
    pub fn AcquireBatch(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        self.acquire_batch(keys)
    }

    #[allow(non_snake_case)]
    pub fn ReleaseBatch(&self, handles: Vec<CachePinnedHandle>) -> usize {
        self.release_batch(handles)
    }
}

impl CacheApi for ShardedMultiLayerCache {
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

    fn lookup_no_promotion_cache(&self, key: &CacheKey) -> Result<Option<Vec<u8>>, CacheError> {
        self.lookup_no_promotion(key)
    }

    fn lookup_batch_no_promotion_cache(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        self.lookup_batch_no_promotion(keys)
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

    fn remove_batch_cache(&self, keys: &[CacheKey]) -> Result<usize, CacheError> {
        self.remove_batch(keys)
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

    fn capacity_for_instance_cache(&self, instance_type: CacheInstanceType) -> usize {
        self.get_capacity(instance_type)
    }

    fn set_capacity_cache(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    fn set_capacity_for_instance_cache(&self, instance_type: CacheInstanceType, capacity: usize) {
        self.set_capacity_for_instance(instance_type, capacity);
    }

    fn size_cache(&self) -> usize {
        self.size()
    }

    fn used_cache(&self, instance_type: CacheInstanceType) -> usize {
        self.get_used(instance_type)
    }
}

impl ZeroCopyCacheApi for ShardedMultiLayerCache {
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

    fn insert_pinned_batch_cache(
        &self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<Vec<Option<CachePinnedHandle>>, CacheError> {
        self.insert_pinned_batch_sized(entries)
    }
}
impl CacheInner {
    fn put_with_request(
        &mut self,
        key: CacheKey,
        value: Vec<u8>,
        request: Option<CacheAdmissionRequest>,
    ) -> Result<(), CacheError> {
        if !self.pinned.contains_key(&key) {
            self.pinned_handle_bytes.remove(&key);
            self.pinned_removed_bytes.remove(&key);
        }
        let request = request.unwrap_or_else(|| self.default_request(&key, value.len()));
        let decision = self.tiering_policy.decide(&request);
        let ssd_enabled = self.ssd_capacity_bytes > 0 || self.ssd_instance_only;
        let admit_ssd = ssd_enabled
            && (decision.admit_ssd
                || self.tiering_policy.ssd_write_through
                || self.ssd_instance_only);
        let admit_memory = decision.admit_memory && !self.ssd_instance_only;
        let admit_pmem = decision.admit_pmem && !self.ssd_instance_only;
        let mut pmem_admitted_via_memory_fallback = false;

        if admit_memory {
            self.record_metadata(
                &key,
                request.block_kind,
                request.routing_slot,
                value.len(),
                request.hotness,
                decision.reason,
            );
            if !self.put_memory(key.clone(), value.clone())
                && matches!(
                    self.tiering_policy.data_placement,
                    CacheDataPlacement::Tiered
                )
                && self.pmem_capacity_bytes > 0
            {
                self.record_metadata(
                    &key,
                    request.block_kind,
                    request.routing_slot,
                    value.len(),
                    request.hotness,
                    CacheAdmissionReason::PersistentMemory,
                );
                pmem_admitted_via_memory_fallback = self.put_pmem(key.clone(), value.clone());
            }
        } else if !self.ssd_instance_only {
            self.stats.memory_admission_rejected += 1;
        }

        if admit_pmem && !pmem_admitted_via_memory_fallback {
            self.record_metadata(
                &key,
                request.block_kind,
                request.routing_slot,
                value.len(),
                request.hotness,
                decision.reason,
            );
            let _ = self.put_pmem(key.clone(), value.clone());
        } else if !self.ssd_instance_only {
            self.stats.pmem_admission_rejected =
                self.stats.pmem_admission_rejected.saturating_add(1);
        }

        if !admit_ssd {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            self.stats.writeback_backpressure_events =
                self.stats.writeback_backpressure_events.saturating_add(1);
            self.stats.puts += 1;
            return Ok(());
        }
        if value.len() > self.tiering_policy.max_ssd_block_bytes
            || value.len() > self.ssd_capacity_bytes
        {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            self.stats.ssd_oversize_rejections =
                self.stats.ssd_oversize_rejections.saturating_add(1);
            self.stats.writeback_backpressure_events =
                self.stats.writeback_backpressure_events.saturating_add(1);
            self.stats.puts += 1;
            return Ok(());
        }
        if self.tiering_policy.ssd_write_through && !decision.admit_ssd {
            self.stats.ssd_write_through_admissions =
                self.stats.ssd_write_through_admissions.saturating_add(1);
        }
        let block = encode_cache_block(&value, self.block_options)?;
        let compressed = is_encoded_compressed_block(&block);
        let block_len = block.len();
        if block_len > self.tiering_policy.max_ssd_block_bytes
            || block_len > self.ssd_capacity_bytes
        {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            self.stats.ssd_oversize_rejections =
                self.stats.ssd_oversize_rejections.saturating_add(1);
            self.stats.writeback_backpressure_events =
                self.stats.writeback_backpressure_events.saturating_add(1);
            self.stats.puts += 1;
            return Ok(());
        }
        if self.ssd_bytes.saturating_add(block_len as u64) > self.ssd_capacity_bytes as u64
            && self.incoming_ssd_block_is_colder_than_existing_groups(&key, &request, value.len())
        {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            self.stats.writeback_backpressure_events =
                self.stats.writeback_backpressure_events.saturating_add(1);
            self.stats.puts += 1;
            return Ok(());
        }
        self.evict_ssd_for(block_len as u64);
        if self.ssd_bytes.saturating_add(block_len as u64) > self.ssd_capacity_bytes as u64 {
            self.stats.ssd_admission_rejected = self.stats.ssd_admission_rejected.saturating_add(1);
            self.stats.writeback_backpressure_events =
                self.stats.writeback_backpressure_events.saturating_add(1);
            self.stats.puts += 1;
            return Ok(());
        }
        if compressed {
            self.stats.compressed_puts += 1;
            self.stats.compression_bytes_saved += value.len().saturating_sub(block.len()) as u64;
        }
        self.write_ssd_block(&key, &block)?;
        if let Some(old_len) = self.disk_index.insert(key.clone(), block_len as u64) {
            self.ssd_bytes = self.ssd_bytes.saturating_sub(old_len);
            self.disk_order.retain(|candidate| candidate != &key);
            self.disk_fifo_order.retain(|candidate| candidate != &key);
        }
        self.disk_order.push_back(key.clone());
        self.disk_fifo_order.push_back(key.clone());
        self.ssd_bytes = self.ssd_bytes.saturating_add(block_len as u64);
        self.record_metadata(
            &key,
            request.block_kind,
            request.routing_slot,
            value.len(),
            request.hotness,
            decision.reason,
        );
        self.stats.puts += 1;
        self.stats.disk_fills += 1;
        self.stats.ssd_admission_accepted = self.stats.ssd_admission_accepted.saturating_add(1);
        self.stats.disk_bytes = self.ssd_bytes;
        self.append_disk_manifest_put(&key, block_len as u64)?;
        Ok(())
    }

    fn put_batch_with_requests(
        &mut self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<usize, CacheError> {
        let mut staged_ssd = Vec::<StagedSsdBatchWrite>::new();
        let mut staged_ssd_positions = HashMap::<CacheKey, usize>::new();
        let mut staged_ssd_bytes = self.ssd_bytes;
        let mut inserted = 0usize;

        for (key, value, logical_size) in entries {
            if !self.pinned.contains_key(&key) {
                self.pinned_handle_bytes.remove(&key);
                self.pinned_removed_bytes.remove(&key);
            }
            let request = self.default_insert_request(&key, value.len(), logical_size);
            let decision = self.tiering_policy.decide(&request);
            let ssd_enabled = self.ssd_capacity_bytes > 0 || self.ssd_instance_only;
            let admit_ssd = ssd_enabled
                && (decision.admit_ssd
                    || self.tiering_policy.ssd_write_through
                    || self.ssd_instance_only);
            let admit_memory = decision.admit_memory && !self.ssd_instance_only;
            let admit_pmem = decision.admit_pmem && !self.ssd_instance_only;
            let mut pmem_admitted_via_memory_fallback = false;

            if admit_memory {
                self.record_metadata(
                    &key,
                    request.block_kind,
                    request.routing_slot,
                    value.len(),
                    request.hotness,
                    decision.reason,
                );
                if !self.put_memory(key.clone(), value.clone())
                    && matches!(
                        self.tiering_policy.data_placement,
                        CacheDataPlacement::Tiered
                    )
                    && self.pmem_capacity_bytes > 0
                {
                    self.record_metadata(
                        &key,
                        request.block_kind,
                        request.routing_slot,
                        value.len(),
                        request.hotness,
                        CacheAdmissionReason::PersistentMemory,
                    );
                    pmem_admitted_via_memory_fallback = self.put_pmem(key.clone(), value.clone());
                }
            } else if !self.ssd_instance_only {
                self.stats.memory_admission_rejected =
                    self.stats.memory_admission_rejected.saturating_add(1);
            }

            if admit_pmem && !pmem_admitted_via_memory_fallback {
                self.record_metadata(
                    &key,
                    request.block_kind,
                    request.routing_slot,
                    value.len(),
                    request.hotness,
                    decision.reason,
                );
                let _ = self.put_pmem(key.clone(), value.clone());
            } else if !self.ssd_instance_only {
                self.stats.pmem_admission_rejected =
                    self.stats.pmem_admission_rejected.saturating_add(1);
            }

            if !admit_ssd {
                self.stats.ssd_admission_rejected =
                    self.stats.ssd_admission_rejected.saturating_add(1);
                self.stats.writeback_backpressure_events =
                    self.stats.writeback_backpressure_events.saturating_add(1);
                self.stats.puts = self.stats.puts.saturating_add(1);
                inserted = inserted.saturating_add(1);
                continue;
            }
            if value.len() > self.tiering_policy.max_ssd_block_bytes
                || value.len() > self.ssd_capacity_bytes
            {
                self.stats.ssd_admission_rejected =
                    self.stats.ssd_admission_rejected.saturating_add(1);
                self.stats.ssd_oversize_rejections =
                    self.stats.ssd_oversize_rejections.saturating_add(1);
                self.stats.writeback_backpressure_events =
                    self.stats.writeback_backpressure_events.saturating_add(1);
                self.stats.puts = self.stats.puts.saturating_add(1);
                inserted = inserted.saturating_add(1);
                continue;
            }
            if self.tiering_policy.ssd_write_through && !decision.admit_ssd {
                self.stats.ssd_write_through_admissions =
                    self.stats.ssd_write_through_admissions.saturating_add(1);
            }
            let block = encode_cache_block(&value, self.block_options)?;
            let compressed = is_encoded_compressed_block(&block);
            let block_len = block.len();
            if block_len > self.tiering_policy.max_ssd_block_bytes
                || block_len > self.ssd_capacity_bytes
            {
                self.stats.ssd_admission_rejected =
                    self.stats.ssd_admission_rejected.saturating_add(1);
                self.stats.ssd_oversize_rejections =
                    self.stats.ssd_oversize_rejections.saturating_add(1);
                self.stats.writeback_backpressure_events =
                    self.stats.writeback_backpressure_events.saturating_add(1);
                self.stats.puts = self.stats.puts.saturating_add(1);
                inserted = inserted.saturating_add(1);
                continue;
            }
            let staged_existing_len = staged_ssd_positions
                .get(&key)
                .and_then(|index| staged_ssd.get(*index))
                .map(|entry| entry.block_len)
                .unwrap_or_default();
            if staged_ssd_bytes
                .saturating_sub(staged_existing_len)
                .saturating_add(block_len as u64)
                > self.ssd_capacity_bytes as u64
                && self.incoming_ssd_block_is_colder_than_existing_groups(
                    &key,
                    &request,
                    value.len(),
                )
            {
                self.stats.ssd_admission_rejected =
                    self.stats.ssd_admission_rejected.saturating_add(1);
                self.stats.writeback_backpressure_events =
                    self.stats.writeback_backpressure_events.saturating_add(1);
                self.stats.puts = self.stats.puts.saturating_add(1);
                inserted = inserted.saturating_add(1);
                continue;
            }
            self.evict_ssd_for((block_len as u64).saturating_sub(staged_existing_len));
            staged_ssd_bytes = self.ssd_bytes;
            let existing_len = if staged_existing_len > 0 {
                staged_existing_len
            } else {
                self.disk_index.get(&key).copied().unwrap_or_default()
            };
            let projected_bytes = staged_ssd_bytes
                .saturating_sub(existing_len)
                .saturating_add(block_len as u64);
            if projected_bytes > self.ssd_capacity_bytes as u64 {
                self.stats.ssd_admission_rejected =
                    self.stats.ssd_admission_rejected.saturating_add(1);
                self.stats.writeback_backpressure_events =
                    self.stats.writeback_backpressure_events.saturating_add(1);
                self.stats.puts = self.stats.puts.saturating_add(1);
                inserted = inserted.saturating_add(1);
                continue;
            }
            if compressed {
                self.stats.compressed_puts = self.stats.compressed_puts.saturating_add(1);
                self.stats.compression_bytes_saved = self
                    .stats
                    .compression_bytes_saved
                    .saturating_add(value.len().saturating_sub(block.len()) as u64);
            }
            staged_ssd_bytes = projected_bytes;
            let entry = StagedSsdBatchWrite {
                key: key.clone(),
                block,
                block_len: block_len as u64,
                value_len: value.len(),
                request,
                admission_reason: decision.reason,
            };
            if let Some(index) = staged_ssd_positions.get(&key).copied() {
                staged_ssd[index] = entry;
            } else {
                let index = staged_ssd.len();
                staged_ssd_positions.insert(key.clone(), index);
                staged_ssd.push(entry);
            }
            inserted = inserted.saturating_add(1);
        }

        let storage_entries = staged_ssd
            .iter()
            .map(|entry| (entry.key.clone(), entry.block.clone()))
            .collect::<Vec<_>>();
        self.write_ssd_blocks(&storage_entries)?;
        if !staged_ssd.is_empty() {
            const SET_MEMBERSHIP_THRESHOLD: usize = 8;
            let staged_keys = staged_ssd
                .iter()
                .map(|entry| entry.key.clone())
                .collect::<Vec<_>>();
            if staged_keys.len() > SET_MEMBERSHIP_THRESHOLD {
                let staged_key_set = staged_keys.iter().cloned().collect::<HashSet<_>>();
                self.disk_order
                    .retain(|candidate| !staged_key_set.contains(candidate));
                self.disk_fifo_order
                    .retain(|candidate| !staged_key_set.contains(candidate));
            } else {
                self.disk_order
                    .retain(|candidate| !staged_keys.contains(candidate));
                self.disk_fifo_order
                    .retain(|candidate| !staged_keys.contains(candidate));
            }
        }
        for entry in staged_ssd {
            if let Some(old_len) = self.disk_index.insert(entry.key.clone(), entry.block_len) {
                self.ssd_bytes = self.ssd_bytes.saturating_sub(old_len);
            }
            self.disk_order.push_back(entry.key.clone());
            self.disk_fifo_order.push_back(entry.key.clone());
            self.ssd_bytes = self.ssd_bytes.saturating_add(entry.block_len);
            self.record_metadata(
                &entry.key,
                entry.request.block_kind,
                entry.request.routing_slot,
                entry.value_len,
                entry.request.hotness,
                entry.admission_reason,
            );
            self.stats.puts = self.stats.puts.saturating_add(1);
            self.stats.disk_fills = self.stats.disk_fills.saturating_add(1);
            self.stats.ssd_admission_accepted = self.stats.ssd_admission_accepted.saturating_add(1);
            self.stats.disk_bytes = self.ssd_bytes;
            self.append_disk_manifest_put(&entry.key, entry.block_len)?;
        }
        self.refresh_usage_stats();
        self.refresh_pin_stats();
        Ok(inserted)
    }
    fn put_bypass_storage_for_tier(
        &mut self,
        tier: CacheTier,
        key: CacheKey,
        value: Vec<u8>,
    ) -> Result<(), CacheError> {
        let block_kind = infer_block_kind(&key);
        let routing_slot = extract_routing_slot(&key);
        match tier {
            CacheTier::Memory => {
                self.record_metadata(
                    &key,
                    block_kind,
                    routing_slot,
                    value.len(),
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                if !self.put_memory(key, value) {
                    self.stats.refill_failures = self.stats.refill_failures.saturating_add(1);
                }
            }
            CacheTier::Pmem => {
                self.record_metadata(
                    &key,
                    block_kind,
                    routing_slot,
                    value.len(),
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                if !self.put_pmem(key, value) {
                    self.stats.refill_failures = self.stats.refill_failures.saturating_add(1);
                }
            }
            CacheTier::Ssd => {
                let block = encode_cache_block(&value, self.block_options)?;
                let existing_block_len = match self.read_ssd_block(&key)? {
                    Some(existing_block)
                        if decode_cache_block(&existing_block).ok().as_deref()
                            == Some(value.as_slice()) =>
                    {
                        Some(existing_block.len() as u64)
                    }
                    Some(_) | None => None,
                };
                let block_len = if let Some(existing_block_len) = existing_block_len {
                    existing_block_len
                } else {
                    let block_len = block.len();
                    self.evict_ssd_for(block_len as u64);
                    self.write_ssd_block(&key, &block)?;
                    block_len as u64
                };
                if let Some(old_len) = self.disk_index.insert(key.clone(), block_len) {
                    self.ssd_bytes = self.ssd_bytes.saturating_sub(old_len);
                    self.disk_order.retain(|candidate| candidate != &key);
                    self.disk_fifo_order.retain(|candidate| candidate != &key);
                }
                self.disk_order.push_back(key.clone());
                self.disk_fifo_order.push_back(key.clone());
                self.ssd_bytes = self.ssd_bytes.saturating_add(block_len);
                self.record_metadata(
                    &key,
                    block_kind,
                    routing_slot,
                    value.len(),
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                self.stats.disk_bytes = self.ssd_bytes;
                self.append_disk_manifest_put(&key, block_len)?;
            }
            CacheTier::Reject => {}
        }
        Ok(())
    }

    fn test_insert_for_tier(
        &mut self,
        tier: CacheTier,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<(), CacheError> {
        let block_kind = infer_block_kind(&key);
        let routing_slot = extract_routing_slot(&key);
        match tier {
            CacheTier::Memory => {
                self.record_metadata(
                    &key,
                    block_kind,
                    routing_slot,
                    size,
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                if !self.put_memory(key, value) {
                    self.stats.refill_failures = self.stats.refill_failures.saturating_add(1);
                }
            }
            CacheTier::Pmem => {
                self.record_metadata(
                    &key,
                    block_kind,
                    routing_slot,
                    size,
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                if !self.put_pmem(key, value) {
                    self.stats.refill_failures = self.stats.refill_failures.saturating_add(1);
                }
            }
            CacheTier::Ssd => {
                let block = encode_cache_block(&value, self.block_options)?;
                let block_len = block.len();
                self.evict_ssd_for(block_len as u64);
                self.write_ssd_block(&key, &block)?;
                if let Some(old_len) = self.disk_index.insert(key.clone(), block_len as u64) {
                    self.ssd_bytes = self.ssd_bytes.saturating_sub(old_len);
                    self.disk_order.retain(|candidate| candidate != &key);
                    self.disk_fifo_order.retain(|candidate| candidate != &key);
                }
                self.disk_order.push_back(key.clone());
                self.disk_fifo_order.push_back(key.clone());
                self.ssd_bytes = self.ssd_bytes.saturating_add(block_len as u64);
                self.record_metadata(
                    &key,
                    block_kind,
                    routing_slot,
                    size,
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                self.stats.disk_bytes = self.ssd_bytes;
                self.append_disk_manifest_put(&key, block_len as u64)?;
            }
            CacheTier::Reject => return Err(CacheError::UnsupportedTier(tier)),
        }
        self.stats.puts = self.stats.puts.saturating_add(1);
        self.refresh_usage_stats();
        self.refresh_pin_stats();
        Ok(())
    }

    fn test_remove_for_tier(&mut self, tier: CacheTier, key: &CacheKey) -> Result<(), CacheError> {
        match tier {
            CacheTier::Memory => {
                if let Some(value) = self.memory.remove(key) {
                    self.memory_bytes = self.memory_bytes.saturating_sub(value.len());
                    if self.pinned.contains_key(key) {
                        self.pinned_removed_bytes.insert(key.clone(), value.len());
                    }
                }
                self.order.retain(|candidate| candidate != key);
                self.memory_fifo_order.retain(|candidate| candidate != key);
            }
            CacheTier::Pmem => {
                if let Some(value) = self.pmem.remove(key) {
                    self.pmem_bytes = self.pmem_bytes.saturating_sub(value.len());
                    if self.pinned.contains_key(key) {
                        self.pinned_removed_bytes.insert(key.clone(), value.len());
                    }
                }
                self.pmem_order.retain(|candidate| candidate != key);
                self.pmem_fifo_order.retain(|candidate| candidate != key);
                self.persist_pmem_delete(key)?;
            }
            CacheTier::Ssd => {
                if let Some(old_len) = self.disk_index.remove(key) {
                    self.ssd_bytes = self.ssd_bytes.saturating_sub(old_len);
                }
                self.delete_ssd_block(key)?;
                self.disk_order.retain(|candidate| candidate != key);
                self.disk_fifo_order.retain(|candidate| candidate != key);
                self.stats.disk_bytes = self.ssd_bytes;
                self.append_disk_manifest_delete(key)?;
            }
            CacheTier::Reject => return Err(CacheError::UnsupportedTier(tier)),
        }
        if !self.memory.contains_key(key)
            && !self.pmem.contains_key(key)
            && !self.disk_index.contains_key(key)
        {
            self.metadata.remove(key);
        }
        self.stats.invalidations = self.stats.invalidations.saturating_add(1);
        Ok(())
    }

    fn update_cached_value_if_current(
        &mut self,
        key: &CacheKey,
        old_handle: &CachePinnedHandle,
        new_value: Vec<u8>,
    ) -> Result<(), CacheError> {
        let memory_result = self.update_cached_value_if_current_for_tier(
            CacheTier::Memory,
            key,
            old_handle,
            new_value.clone(),
        );
        match memory_result {
            Ok(()) => return Ok(()),
            Err(CacheError::ReplaceMismatch) => return Err(CacheError::ReplaceMismatch),
            Err(CacheError::NotFound) => {}
            Err(other) => return Err(other),
        }

        self.update_cached_value_if_current_for_tier(CacheTier::Pmem, key, old_handle, new_value)
    }

    fn update_cached_value_if_current_for_tier(
        &mut self,
        tier: CacheTier,
        key: &CacheKey,
        old_handle: &CachePinnedHandle,
        new_value: Vec<u8>,
    ) -> Result<(), CacheError> {
        if old_handle.key != *key {
            return Err(CacheError::ReplaceMismatch);
        }
        let new_value = Arc::<[u8]>::from(new_value);
        match tier {
            CacheTier::Memory => {
                let Some(current) = self.memory.get_mut(key) else {
                    return Err(CacheError::NotFound);
                };
                if !Arc::ptr_eq(current, &old_handle.value) {
                    return Err(CacheError::ReplaceMismatch);
                }
                self.memory_bytes = self
                    .memory_bytes
                    .saturating_sub(current.len())
                    .saturating_add(new_value.len());
                *current = Arc::clone(&new_value);
                self.touch_key(key);
                self.stats.memory_bytes = self.memory_bytes as u64;
                self.refresh_pin_stats();
                Ok(())
            }
            CacheTier::Pmem => {
                let Some(current) = self.pmem.get_mut(key) else {
                    return Err(CacheError::NotFound);
                };
                if !Arc::ptr_eq(current, &old_handle.value) {
                    return Err(CacheError::ReplaceMismatch);
                }
                self.pmem_bytes = self
                    .pmem_bytes
                    .saturating_sub(current.len())
                    .saturating_add(new_value.len());
                *current = Arc::clone(&new_value);
                self.touch_key(key);
                self.stats.pmem_bytes = self.pmem_bytes as u64;
                self.refresh_pin_stats();
                Ok(())
            }
            CacheTier::Ssd => {
                if old_handle.tier != CacheReadTier::Ssd {
                    return Err(CacheError::ReplaceMismatch);
                }
                let existing_block = self.read_ssd_block(key)?.ok_or(CacheError::NotFound)?;
                let existing_value = decode_cache_block(&existing_block)?;
                if existing_value.as_slice() != old_handle.value.as_ref() {
                    return Err(CacheError::ReplaceMismatch);
                }

                let block = encode_cache_block(new_value.as_ref(), self.block_options)?;
                if block.len() > self.ssd_capacity_bytes {
                    return Err(CacheError::CapacityExceeded);
                }
                let indexed_old_len = self
                    .disk_index
                    .remove(key)
                    .unwrap_or(existing_block.len() as u64);
                self.ssd_bytes = self.ssd_bytes.saturating_sub(indexed_old_len);
                self.disk_order.retain(|candidate| candidate != key);
                self.disk_fifo_order.retain(|candidate| candidate != key);
                self.evict_ssd_for(block.len() as u64);
                if self.ssd_bytes.saturating_add(block.len() as u64)
                    > self.ssd_capacity_bytes as u64
                {
                    self.disk_index.insert(key.clone(), indexed_old_len);
                    self.disk_order.push_back(key.clone());
                    self.disk_fifo_order.push_back(key.clone());
                    self.ssd_bytes = self.ssd_bytes.saturating_add(indexed_old_len);
                    self.stats.disk_bytes = self.ssd_bytes;
                    return Err(CacheError::CapacityExceeded);
                }
                self.write_ssd_block(key, &block)?;
                self.disk_index.insert(key.clone(), block.len() as u64);
                self.disk_order.push_back(key.clone());
                self.disk_fifo_order.push_back(key.clone());
                self.ssd_bytes = self.ssd_bytes.saturating_add(block.len() as u64);
                self.record_metadata(
                    key,
                    infer_block_kind(key),
                    extract_routing_slot(key),
                    new_value.len(),
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                self.stats.disk_bytes = self.ssd_bytes;
                self.append_disk_manifest_put(key, block.len() as u64)?;
                self.refresh_pin_stats();
                Ok(())
            }
            CacheTier::Reject => Err(CacheError::UnsupportedTier(tier)),
        }
    }
}

impl MultiLayerCache {
    pub fn invalidate_record(
        &self,
        shard_id: ShardId,
        namespace: &str,
        record_key: &str,
    ) -> Result<(), CacheError> {
        let keys = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner
                .memory
                .keys()
                .chain(inner.pmem.keys())
                .chain(inner.disk_index.keys())
                .filter(|key| {
                    key.shard_id == shard_id
                        && key.namespace == namespace
                        && key.record_key == record_key
                })
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        };
        for key in keys {
            self.invalidate(&key)?;
        }
        Ok(())
    }

    pub fn invalidate_shard(&self, shard_id: ShardId) -> Result<CacheGcReport, CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        let memory_keys = inner
            .memory
            .keys()
            .chain(inner.pmem.keys())
            .filter(|key| key.shard_id == shard_id)
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let memory_entries_removed = memory_keys.len();
        for key in &memory_keys {
            if let Some(value) = inner.memory.remove(key) {
                inner.memory_bytes = inner.memory_bytes.saturating_sub(value.len());
            }
            if let Some(value) = inner.pmem.remove(key) {
                inner.pmem_bytes = inner.pmem_bytes.saturating_sub(value.len());
            }
        }
        inner.order.retain(|key| key.shard_id != shard_id);
        inner
            .memory_fifo_order
            .retain(|key| key.shard_id != shard_id);
        inner.pmem_order.retain(|key| key.shard_id != shard_id);
        inner.pmem_fifo_order.retain(|key| key.shard_id != shard_id);
        let disk_keys = inner
            .disk_index
            .keys()
            .filter(|key| key.shard_id == shard_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut disk_bytes_before = 0u64;
        for key in &disk_keys {
            disk_bytes_before =
                disk_bytes_before.saturating_add(inner.disk_index.remove(key).unwrap_or_default());
        }
        let _ = inner.delete_ssd_blocks(&disk_keys);
        inner.disk_order.retain(|key| key.shard_id != shard_id);
        inner.disk_fifo_order.retain(|key| key.shard_id != shard_id);
        inner.metadata.retain(|key, _| key.shard_id != shard_id);
        inner.pinned.retain(|key, _| key.shard_id != shard_id);
        inner
            .pinned_handle_bytes
            .retain(|key, _| key.shard_id != shard_id);
        inner
            .pinned_removed_bytes
            .retain(|key, _| key.shard_id != shard_id);
        inner.stats.invalidations = inner
            .stats
            .invalidations
            .saturating_add((memory_entries_removed + disk_keys.len()) as u64);
        inner.stats.memory_bytes = inner.memory_bytes as u64;
        inner.stats.pmem_bytes = inner.pmem_bytes as u64;
        inner.ssd_bytes = inner.ssd_bytes.saturating_sub(disk_bytes_before);
        inner.stats.disk_bytes = inner.ssd_bytes;
        inner.refresh_pin_stats();
        inner.rewrite_disk_manifest()?;
        Ok(CacheGcReport {
            shard_id,
            memory_entries_removed,
            disk_bytes_removed: disk_bytes_before,
        })
    }

    pub fn invalidate_slot(
        &self,
        shard_id: ShardId,
        routing_slot: u32,
    ) -> Result<CacheGcReport, CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        let prefix = format!("slot-{routing_slot}:");
        let slot_keys = inner
            .memory
            .keys()
            .chain(inner.pmem.keys())
            .chain(inner.disk_index.keys())
            .filter(|key| key.shard_id == shard_id && key.selector.starts_with(&prefix))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let memory_entries_removed = slot_keys
            .iter()
            .filter(|key| inner.memory.contains_key(*key))
            .count();
        let mut disk_bytes_removed = 0u64;
        let mut disk_delete_keys = Vec::new();
        for key in &slot_keys {
            if let Some(value) = inner.memory.remove(key) {
                inner.memory_bytes = inner.memory_bytes.saturating_sub(value.len());
            }
            if let Some(value) = inner.pmem.remove(key) {
                inner.pmem_bytes = inner.pmem_bytes.saturating_sub(value.len());
            }
            if let Some(bytes) = inner.disk_index.remove(key) {
                disk_bytes_removed = disk_bytes_removed.saturating_add(bytes);
                disk_delete_keys.push(key.clone());
            }
            inner.pinned.remove(key);
            inner.pinned_handle_bytes.remove(key);
            inner.pinned_removed_bytes.remove(key);
            inner.metadata.remove(key);
        }
        let _ = inner.delete_ssd_blocks(&disk_delete_keys);
        inner
            .order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner
            .memory_fifo_order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner
            .pmem_order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner
            .pmem_fifo_order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner
            .disk_order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner
            .disk_fifo_order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner.stats.invalidations = inner
            .stats
            .invalidations
            .saturating_add(slot_keys.len() as u64);
        inner.stats.memory_bytes = inner.memory_bytes as u64;
        inner.stats.pmem_bytes = inner.pmem_bytes as u64;
        inner.ssd_bytes = inner.ssd_bytes.saturating_sub(disk_bytes_removed);
        inner.stats.disk_bytes = inner.ssd_bytes;
        inner.refresh_pin_stats();
        inner.rewrite_disk_manifest()?;
        Ok(CacheGcReport {
            shard_id,
            memory_entries_removed,
            disk_bytes_removed,
        })
    }

    pub fn invalidate_page_segment(
        &self,
        shard_id: ShardId,
        page_segment_id: u64,
    ) -> Result<CacheGcReport, CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        let record_key = format!("segment-{page_segment_id:020}");
        let segment_keys = inner
            .memory
            .keys()
            .chain(inner.pmem.keys())
            .chain(inner.disk_index.keys())
            .filter(|key| {
                key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key
            })
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let memory_entries_removed = segment_keys
            .iter()
            .filter(|key| inner.memory.contains_key(*key))
            .count();
        let mut disk_bytes_removed = 0u64;
        let mut disk_delete_keys = Vec::new();
        for key in &segment_keys {
            if let Some(value) = inner.memory.remove(key) {
                inner.memory_bytes = inner.memory_bytes.saturating_sub(value.len());
            }
            if let Some(value) = inner.pmem.remove(key) {
                inner.pmem_bytes = inner.pmem_bytes.saturating_sub(value.len());
            }
            if let Some(bytes) = inner.disk_index.remove(key) {
                disk_bytes_removed = disk_bytes_removed.saturating_add(bytes);
                disk_delete_keys.push(key.clone());
            }
            inner.pinned.remove(key);
            inner.pinned_handle_bytes.remove(key);
            inner.pinned_removed_bytes.remove(key);
            inner.metadata.remove(key);
        }
        let _ = inner.delete_ssd_blocks(&disk_delete_keys);
        inner.order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.memory_fifo_order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.pmem_order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.pmem_fifo_order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.disk_order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.disk_fifo_order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.stats.invalidations = inner
            .stats
            .invalidations
            .saturating_add(segment_keys.len() as u64);
        inner.stats.memory_bytes = inner.memory_bytes as u64;
        inner.stats.pmem_bytes = inner.pmem_bytes as u64;
        inner.ssd_bytes = inner.ssd_bytes.saturating_sub(disk_bytes_removed);
        inner.stats.disk_bytes = inner.ssd_bytes;
        inner.refresh_pin_stats();
        inner.rewrite_disk_manifest()?;
        Ok(CacheGcReport {
            shard_id,
            memory_entries_removed,
            disk_bytes_removed,
        })
    }

    pub fn entries_for_shard(&self, shard_id: ShardId) -> Vec<CacheEntryInfo> {
        let inner = self.inner.read().expect("cache lock poisoned");
        let keys = inner
            .memory
            .keys()
            .chain(inner.pmem.keys())
            .chain(inner.disk_index.keys())
            .filter(|key| key.shard_id == shard_id)
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let mut entries = keys
            .into_iter()
            .map(|key| {
                let pinned = inner.pinned.contains_key(&key);
                let memory_bytes = inner
                    .memory
                    .get(&key)
                    .map(|value| value.len() as u64)
                    .unwrap_or_default();
                let pmem_bytes = inner
                    .pmem
                    .get(&key)
                    .map(|value| value.len() as u64)
                    .unwrap_or_default();
                let disk_bytes = inner.disk_index.get(&key).copied().unwrap_or_else(|| {
                    inner
                        .disk_path(&key)
                        .metadata()
                        .map(|metadata| metadata.len())
                        .unwrap_or_default()
                });
                let meta = inner.metadata.get(&key).copied();
                CacheEntryInfo {
                    shard_id: key.shard_id,
                    namespace: key.namespace,
                    record_key: key.record_key,
                    selector: key.selector,
                    memory_bytes,
                    pmem_bytes,
                    disk_bytes,
                    pinned,
                    block_kind: meta.map(|meta| meta.block_kind),
                    routing_slot: meta.and_then(|meta| meta.routing_slot),
                    hotness: meta.map(|meta| meta.hotness).unwrap_or_default(),
                    hits: meta.map(|meta| meta.hits).unwrap_or_default(),
                    last_access_epoch: meta.map(|meta| meta.last_access_epoch).unwrap_or_default(),
                    admission_reason: meta.map(|meta| meta.admission_reason),
                }
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.namespace
                .cmp(&right.namespace)
                .then(left.record_key.cmp(&right.record_key))
                .then(left.selector.cmp(&right.selector))
        });
        entries
    }

    pub fn all_entries(&self) -> Vec<CacheEntryInfo> {
        let inner = self.inner.read().expect("cache lock poisoned");
        let keys = inner
            .memory
            .keys()
            .chain(inner.pmem.keys())
            .chain(inner.disk_index.keys())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let mut entries = keys
            .into_iter()
            .map(|key| {
                let pinned = inner.pinned.contains_key(&key);
                let memory_bytes = inner
                    .memory
                    .get(&key)
                    .map(|value| value.len() as u64)
                    .unwrap_or_default();
                let pmem_bytes = inner
                    .pmem
                    .get(&key)
                    .map(|value| value.len() as u64)
                    .unwrap_or_default();
                let disk_bytes = inner.disk_index.get(&key).copied().unwrap_or_else(|| {
                    inner
                        .disk_path(&key)
                        .metadata()
                        .map(|metadata| metadata.len())
                        .unwrap_or_default()
                });
                let meta = inner.metadata.get(&key).copied();
                CacheEntryInfo {
                    shard_id: key.shard_id,
                    namespace: key.namespace,
                    record_key: key.record_key,
                    selector: key.selector,
                    memory_bytes,
                    pmem_bytes,
                    disk_bytes,
                    pinned,
                    block_kind: meta.map(|meta| meta.block_kind),
                    routing_slot: meta.and_then(|meta| meta.routing_slot),
                    hotness: meta.map(|meta| meta.hotness).unwrap_or_default(),
                    hits: meta.map(|meta| meta.hits).unwrap_or_default(),
                    last_access_epoch: meta.map(|meta| meta.last_access_epoch).unwrap_or_default(),
                    admission_reason: meta.map(|meta| meta.admission_reason),
                }
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.shard_id
                .cmp(&right.shard_id)
                .then(left.namespace.cmp(&right.namespace))
                .then(left.record_key.cmp(&right.record_key))
                .then(left.selector.cmp(&right.selector))
        });
        entries
    }

    pub fn stats(&self) -> CacheStats {
        let inner = self.inner.read().expect("cache lock poisoned");
        CacheStats {
            memory_bytes: inner.memory_bytes as u64,
            pmem_bytes: inner.pmem_bytes as u64,
            disk_bytes: inner.ssd_bytes,
            pinned_entries: inner.pinned.len() as u64,
            pinned_bytes: inner.pinned_memory_bytes(),
            async_writeback_queue_depth: inner.async_writeback_queue.len() as u64,
            async_writeback_queue_bytes: inner.async_writeback_queue_bytes,
            ..inner.stats
        }
    }

    pub fn eviction_report(&self) -> CacheEvictionReport {
        let stats = self.stats();
        CacheEvictionReport {
            memory_evictions: stats.memory_evictions,
            memory_capacity_evictions: stats.eviction_capacity,
            memory_cold_evictions: stats.eviction_cold,
            memory_low_hit_evictions: stats.eviction_low_hit,
            memory_stale_evictions: stats.eviction_stale,
            memory_pinned_skips: stats.eviction_pinned_skips,
            pmem_evictions: stats.pmem_evictions,
            pmem_capacity_evictions: stats.pmem_eviction_capacity,
            pmem_pinned_skips: stats.pmem_eviction_pinned_skips,
            ssd_evictions: stats.ssd_evictions,
            ssd_capacity_evictions: stats.ssd_eviction_capacity,
            ssd_cold_evictions: stats.ssd_eviction_cold,
            ssd_low_hit_evictions: stats.ssd_eviction_low_hit,
            ssd_stale_evictions: stats.ssd_eviction_stale,
            ssd_pinned_skips: stats.ssd_eviction_pinned_skips,
            sampled_eviction_groups: stats.eviction_sampled_groups,
            memory_slot_evictions: stats.memory_slot_evictions,
            ssd_slot_evictions: stats.ssd_slot_evictions,
            replacement_policy: CacheReplacementPolicy::WeightedHotnessLru,
        }
    }

    pub fn writeback_backpressure_report(&self) -> CacheWritebackBackpressureReport {
        let stats = self.stats();
        CacheWritebackBackpressureReport {
            ssd_write_through_enabled: self.production_tiering_policy().ssd_write_through,
            write_through_admissions: stats.ssd_write_through_admissions,
            ssd_admission_rejections: stats.ssd_admission_rejected,
            ssd_evictions: stats.ssd_evictions,
            ssd_oversize_rejections: stats.ssd_oversize_rejections,
            backpressure_events: stats.writeback_backpressure_events
                + stats.async_writeback_backpressure_rejections,
            bounded_queue_ready: true,
        }
    }

    pub fn latency_metrics_report(&self) -> CacheLatencyMetricsReport {
        let stats = self.stats();
        let get_count = stats.get_latency_count.max(stats.get_latency_samples);
        let put_count = stats.put_latency_count.max(stats.put_latency_samples);
        let get_total = stats
            .get_latency_total_us
            .max(stats.get_latency_total_micros);
        let put_total = stats
            .put_latency_total_us
            .max(stats.put_latency_total_micros);
        let get_max = stats.get_latency_max_us.max(stats.get_latency_max_micros);
        let put_max = stats.put_latency_max_us.max(stats.put_latency_max_micros);
        CacheLatencyMetricsReport {
            get_count,
            get_avg_us: if get_count == 0 {
                0
            } else {
                get_total / get_count
            },
            get_max_us: get_max,
            put_count,
            put_avg_us: if put_count == 0 {
                0
            } else {
                put_total / put_count
            },
            put_max_us: put_max,
            histogram_ready: stats.get_latency_le_10us
                + stats.get_latency_le_100us
                + stats.get_latency_le_1ms
                + stats.get_latency_le_10ms
                + stats.get_latency_gt_10ms
                + stats.put_latency_le_10us
                + stats.put_latency_le_100us
                + stats.put_latency_le_1ms
                + stats.put_latency_le_10ms
                + stats.put_latency_gt_10ms
                > 0,
        }
    }

    #[doc(hidden)]
    pub fn clear_memory_for_test(&self) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.memory.clear();
        inner.pmem.clear();
        inner.order.clear();
        inner.pmem_order.clear();
        inner.memory_bytes = 0;
        inner.pmem_bytes = 0;
        inner.stats.memory_bytes = 0;
        inner.stats.pmem_bytes = 0;
        inner.refresh_pin_stats();
    }

    pub fn replacement_policy_soak(&self, iterations: usize) -> CacheReplacementPolicySoakReport {
        let hot_keys = (0..4)
            .map(|idx| CacheKey::page_with_slot(7, 1, idx * 8, 8, Some(3)))
            .collect::<Vec<_>>();
        let pinned_key = hot_keys[0].clone();
        for (idx, key) in hot_keys.iter().enumerate() {
            let _ = self.put(key.clone(), vec![b'h' + idx as u8; 8]);
        }
        self.pin(pinned_key.clone());

        let mut cold_keys = Vec::new();
        for idx in 0..iterations {
            for hot in &hot_keys[1..] {
                let _ = self.get(hot);
            }
            let cold = CacheKey::page_with_slot(7, 2 + idx as u64, idx as u64 * 16, 8, Some(4));
            let _ = self.put(cold.clone(), vec![b'c'; 8]);
            cold_keys.push(cold);
        }

        let restart_probe_key = cold_keys[0].clone();
        let restart_probe_value = self.get(&restart_probe_key).ok().flatten();
        let (memory_capacity_bytes, disk_dir) = {
            let inner = self.inner.read().expect("cache lock poisoned");
            (inner.memory_capacity_bytes, inner.disk_dir.clone())
        };
        let restarted_cache = MultiLayerCache::new(memory_capacity_bytes, disk_dir);
        let restart_disk_refill_ready = restart_probe_value.is_some()
            && restarted_cache
                .get(&restart_probe_key)
                .ok()
                .flatten()
                .as_ref()
                == restart_probe_value.as_ref();
        let hot_memory_survivors = hot_keys
            .iter()
            .filter(|key| self.get_memory(key).is_some())
            .count();
        let recent_cold = cold_keys
            .iter()
            .rev()
            .take(hot_keys.len())
            .cloned()
            .collect::<BTreeSet<_>>();
        let cold_memory_survivors = cold_keys
            .iter()
            .filter(|key| recent_cold.contains(*key) && self.get_memory(key).is_some())
            .count();
        self.set_async_writeback_queue_limit_for_test(1);
        let _ = self.enqueue_async_writeback(
            CacheKey::page_with_slot(7, 999, 0, 8, Some(9)),
            b"writeback".to_vec(),
        );
        let _ = self.enqueue_async_writeback(
            CacheKey::page_with_slot(7, 1_000, 0, 8, Some(9)),
            b"overflow".to_vec(),
        );
        let _ = self.drain_async_writeback(8);
        self.record_compaction_latency_micros(500);
        let stats = self.stats();
        let read_through_latency_bucketed = latency_bucket_count(
            stats.read_through_latency_le_10us,
            stats.read_through_latency_le_100us,
            stats.read_through_latency_le_1ms,
            stats.read_through_latency_le_10ms,
            stats.read_through_latency_gt_10ms,
        ) == stats.read_through_latency_samples;
        let refill_latency_bucketed = latency_bucket_count(
            stats.refill_latency_le_10us,
            stats.refill_latency_le_100us,
            stats.refill_latency_le_1ms,
            stats.refill_latency_le_10ms,
            stats.refill_latency_gt_10ms,
        ) == stats.refill_latency_samples;
        let writeback_latency_bucketed = latency_bucket_count(
            stats.writeback_latency_le_10us,
            stats.writeback_latency_le_100us,
            stats.writeback_latency_le_1ms,
            stats.writeback_latency_le_10ms,
            stats.writeback_latency_gt_10ms,
        ) == stats.writeback_latency_samples;
        let eviction_latency_bucketed = latency_bucket_count(
            stats.eviction_latency_le_10us,
            stats.eviction_latency_le_100us,
            stats.eviction_latency_le_1ms,
            stats.eviction_latency_le_10ms,
            stats.eviction_latency_gt_10ms,
        ) == stats.eviction_latency_samples;
        let compaction_latency_bucketed = latency_bucket_count(
            stats.compaction_latency_le_10us,
            stats.compaction_latency_le_100us,
            stats.compaction_latency_le_1ms,
            stats.compaction_latency_le_10ms,
            stats.compaction_latency_gt_10ms,
        ) == stats.compaction_latency_samples;
        let mut report = CacheReplacementPolicySoakReport {
            iterations,
            hot_key_count: hot_keys.len(),
            cold_key_count: cold_keys.len(),
            hot_memory_survivors,
            cold_memory_survivors,
            pinned_memory_survived: self.get_memory(&pinned_key).is_some(),
            restart_disk_refill_ready,
            observed_evictions: stats.memory_evictions,
            observed_pinned_skips: stats.eviction_pinned_skips,
            observed_disk_refills: stats.disk_hits,
            observed_async_writeback_backpressure: stats.async_writeback_backpressure_rejections,
            async_writeback_max_queue_depth: stats.async_writeback_max_queue_depth,
            async_writeback_max_queue_bytes: stats.async_writeback_max_queue_bytes,
            get_latency_samples: stats.get_latency_samples,
            put_latency_samples: stats.put_latency_samples,
            read_through_latency_samples: stats.read_through_latency_samples,
            refill_latency_samples: stats.refill_latency_samples,
            writeback_latency_samples: stats.writeback_latency_samples,
            eviction_latency_samples: stats.eviction_latency_samples,
            compaction_latency_samples: stats.compaction_latency_samples,
            read_through_latency_bucketed,
            refill_latency_bucketed,
            writeback_latency_bucketed,
            eviction_latency_bucketed,
            compaction_latency_bucketed,
            ..CacheReplacementPolicySoakReport::default()
        };
        if report.iterations < 64 {
            report
                .reasons
                .push("insufficient_soak_iterations".to_string());
        }
        if report.observed_evictions == 0 {
            report
                .reasons
                .push("missing_eviction_observation".to_string());
        }
        if !report.pinned_memory_survived || report.observed_pinned_skips == 0 {
            report.reasons.push("missing_pinned_survival".to_string());
        }
        if report.hot_memory_survivors < report.hot_key_count {
            report
                .reasons
                .push("hot_working_set_not_retained".to_string());
        }
        if report.cold_memory_survivors >= report.hot_memory_survivors {
            report
                .reasons
                .push("cold_set_retained_like_hot_set".to_string());
        }
        if report.observed_disk_refills == 0 {
            report
                .reasons
                .push("missing_disk_refill_observation".to_string());
        }
        if !report.restart_disk_refill_ready {
            report
                .reasons
                .push("missing_restart_disk_refill_observation".to_string());
        }
        if report.observed_async_writeback_backpressure == 0
            || report.async_writeback_max_queue_depth == 0
            || report.async_writeback_max_queue_bytes == 0
        {
            report
                .reasons
                .push("missing_async_writeback_backpressure".to_string());
        }
        if report.get_latency_samples == 0 || report.put_latency_samples == 0 {
            report.reasons.push("missing_latency_samples".to_string());
        }
        if report.read_through_latency_samples == 0
            || report.refill_latency_samples == 0
            || report.writeback_latency_samples == 0
            || report.eviction_latency_samples == 0
            || report.compaction_latency_samples == 0
        {
            report
                .reasons
                .push("missing_operation_latency_samples".to_string());
        }
        if !report.read_through_latency_bucketed
            || !report.refill_latency_bucketed
            || !report.writeback_latency_bucketed
            || !report.eviction_latency_bucketed
            || !report.compaction_latency_bucketed
        {
            report
                .reasons
                .push("missing_operation_latency_histograms".to_string());
        }
        report.passed = report.reasons.is_empty();
        report
    }
}

impl Default for MultiLayerCache {
    fn default() -> Self {
        Self::new(16 * 1024 * 1024, unique_temp_path("cache"))
    }
}
