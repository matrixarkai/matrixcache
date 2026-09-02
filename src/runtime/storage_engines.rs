// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

/// Everything a cache can be configured with.
///
/// Tier capacities and paths are typed; the policy and engine choices are
/// **strings**, parsed when the cache is built -- `cache_dram_replacement_policy`
/// takes names like `"FIFO"` or `"SLRU"`, and
/// `cache_dram_pmem_data_placement_type` takes `"SideBySide"` or `"Tiered"`.
/// That is deliberate: these come from configuration files, so the vocabulary is
/// the file's rather than Rust's.
///
/// Build one with [`CacheOptions::new`] and the `with_*` methods, then hand it
/// to [`MultiLayerCache::with_options`] or [`MatrixCacheBuilder`].
///
/// # Examples
///
/// ```
/// use matrixcache::{CacheKey, CacheOptions, MatrixCacheBuilder};
///
/// let dir = tempfile::tempdir()?;
/// let options = CacheOptions::new(1 << 20, 0, 1 << 22)
///     .with_ssd_paths([dir.path().to_path_buf()]);
///
/// let cache = MatrixCacheBuilder::build_cache(options);
/// let key = CacheKey::string(0, "greeting");
/// cache.put(key.clone(), b"hello".to_vec())?;
/// assert_eq!(cache.get(&key)?, Some(b"hello".to_vec()));
/// # Ok::<(), matrixcache::CacheError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheOptions {
    pub dram_capacity: usize,
    #[serde(default)]
    pub pmem_capacity: usize,
    #[serde(default)]
    pub ssd_capacity: usize,
    #[serde(default)]
    pub pmem_paths: Vec<PathBuf>,
    #[serde(default)]
    pub ssd_paths: Vec<PathBuf>,
    #[serde(default)]
    pub cache_dram_replacement_policy: String,
    #[serde(default)]
    pub cache_pmem_replacement_policy: String,
    #[serde(default)]
    pub cache_ssd_replacement_policy: String,
    #[serde(default)]
    pub cache_dram_pmem_data_placement_type: String,
    #[serde(default)]
    pub cache_dram_pmem_data_placement_threshold: usize,
    #[serde(default)]
    pub metric_id_prefix: String,
    #[serde(default)]
    pub metric_registry_tags: HashMap<String, String>,
    /// Bytes per second the SSD tier may absorb. Zero, the default, is no cap.
    #[serde(default)]
    pub ssd_write_bytes_per_sec: u64,
    #[serde(default)]
    pub cache_ssd_instance_only: bool,
    #[serde(default)]
    pub blockcache_clear_ssd_folder: bool,
    #[serde(default)]
    pub auto_recover_on_start: bool,
    /// Whether an SSD block write also survives the machine losing power.
    ///
    /// True by default, which is what it has always done: the block is
    /// flushed and the directory entry flushed after the rename. That is two
    /// `fsync` calls per write and most of what a block write costs.
    ///
    /// Turning it off keeps the block atomic -- a reader still sees a whole
    /// block or none -- and gives up only surviving a crash of the machine. On
    /// a tier that is a cache, a block lost that way is a miss. It is not a
    /// miss if something recovers this tier and expects it to be complete, so
    /// leave it on if `auto_recover_on_start` is set or anything calls
    /// `recover_disk_index`.
    #[serde(default = "default_ssd_block_durability")]
    pub ssd_block_durability: bool,
    /// Whether a write to the persistent tier also survives the machine losing
    /// power.
    ///
    /// **False by default, which is what this tier has always done.** It writes
    /// the block and renames it into place and never flushes either, so the
    /// block is always whole and is not always there after a crash.
    ///
    /// The name invites the opposite assumption. Real persistent memory is
    /// durable without being flushed; this tier is files standing in for it,
    /// and files are not. Anything that recovers this tier and expects it to be
    /// complete wants this on.
    ///
    /// The default is false rather than true only because that is the existing
    /// behaviour and turning it on costs what flushing costs. The SSD tier
    /// defaults the other way for the same reason -- each keeps what it did.
    #[serde(default)]
    pub pmem_block_durability: bool,
    #[serde(default)]
    pub block_options: CacheBlockOptions,
}

impl Default for CacheOptions {
    fn default() -> Self {
        let policy = CacheTieringPolicy::default();
        Self {
            dram_capacity: policy.memory_capacity_bytes,
            pmem_capacity: policy.pmem_capacity_bytes,
            ssd_capacity: policy.ssd_capacity_bytes,
            pmem_paths: Vec::new(),
            ssd_paths: Vec::new(),
            ssd_write_bytes_per_sec: 0,
            cache_dram_replacement_policy: "WeightedHotnessLru".to_string(),
            cache_pmem_replacement_policy: "WeightedHotnessLru".to_string(),
            cache_ssd_replacement_policy: "WeightedHotnessLru".to_string(),
            cache_dram_pmem_data_placement_type: "Tiered".to_string(),
            cache_dram_pmem_data_placement_threshold: policy.data_placement_threshold_bytes,
            metric_id_prefix: String::new(),
            metric_registry_tags: HashMap::new(),
            cache_ssd_instance_only: false,
            blockcache_clear_ssd_folder: false,
            auto_recover_on_start: false,
            ssd_block_durability: true,
            pmem_block_durability: false,
            block_options: CacheBlockOptions::default(),
        }
    }
}

impl CacheOptions {
    pub fn new(dram_capacity: usize, pmem_capacity: usize, ssd_capacity: usize) -> Self {
        Self {
            dram_capacity,
            pmem_capacity,
            ssd_capacity,
            ..Self::default()
        }
    }

    pub fn with_pmem_paths(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.pmem_paths = paths.into_iter().collect();
        self
    }

    pub fn with_ssd_paths(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.ssd_paths = paths.into_iter().collect();
        self
    }

    pub fn with_replacement_policy(mut self, policy: CacheReplacementPolicy) -> Self {
        let name = policy.as_config_name().to_string();
        self.cache_dram_replacement_policy = name.clone();
        self.cache_pmem_replacement_policy = name.clone();
        self.cache_ssd_replacement_policy = name;
        self
    }

    pub fn with_tier_replacement_policy(
        mut self,
        tier: CacheTier,
        policy: CacheReplacementPolicy,
    ) -> Self {
        let name = policy.as_config_name().to_string();
        match tier {
            CacheTier::Memory => self.cache_dram_replacement_policy = name,
            CacheTier::Pmem => self.cache_pmem_replacement_policy = name,
            CacheTier::Ssd => self.cache_ssd_replacement_policy = name,
            CacheTier::Reject => {}
        }
        self
    }

    pub fn with_dram_pmem_data_placement(
        mut self,
        placement: CacheDataPlacement,
        threshold: usize,
    ) -> Self {
        self.cache_dram_pmem_data_placement_type = placement.as_config_name().to_string();
        self.cache_dram_pmem_data_placement_threshold = threshold;
        self
    }

    pub fn with_config_dram_pmem_data_placement(
        self,
        placement: DramPmemDataPlacement,
        threshold: usize,
    ) -> Self {
        self.with_dram_pmem_data_placement(placement.into(), threshold)
    }

    #[allow(non_snake_case)]
    pub fn WithDRAMPMEMDataPlacement(
        self,
        placement: DramPmemDataPlacement,
        threshold: usize,
    ) -> Self {
        self.with_config_dram_pmem_data_placement(placement, threshold)
    }

    pub fn with_metric_id_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.metric_id_prefix = prefix.into();
        self
    }

    pub fn with_metric_registry_tags(
        mut self,
        tags: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        self.metric_registry_tags = tags.into_iter().collect();
        self
    }

    pub fn with_ssd_instance_only(mut self, enabled: bool) -> Self {
        self.cache_ssd_instance_only = enabled;
        self
    }

    pub fn with_auto_recover_on_start(mut self, enabled: bool) -> Self {
        self.auto_recover_on_start = enabled;
        self
    }

    #[allow(non_snake_case)]
    pub fn WithAutoRecoverOnStart(self, enabled: bool) -> Self {
        self.with_auto_recover_on_start(enabled)
    }

    pub fn disk_dir(&self) -> PathBuf {
        self.ssd_paths
            .first()
            .cloned()
            .unwrap_or_else(|| unique_temp_path("matrixcache-options-ssd"))
    }

    pub fn tiering_policy(&self) -> CacheTieringPolicy {
        let defaults = CacheTieringPolicy::default();
        CacheTieringPolicy {
            memory_capacity_bytes: self.dram_capacity,
            pmem_capacity_bytes: self.pmem_capacity,
            ssd_capacity_bytes: self.ssd_capacity,
            data_placement: CacheDataPlacement::from_config_name(
                &self.cache_dram_pmem_data_placement_type,
            ),
            data_placement_threshold_bytes: self.cache_dram_pmem_data_placement_threshold,
            ..defaults
        }
    }
}

/// Constructors for every cache this crate exposes.
///
/// A namespace rather than a value -- the methods are associated functions, so
/// call `MatrixCacheBuilder::build_cache(options)` without making one.
///
/// Each `build_*` returns a concrete type; the `*_api` variants return the same
/// cache boxed as `dyn CacheApi` or `dyn ZeroCopyCacheApi` for a caller that
/// wants to hold several kinds behind one type.
#[derive(Debug, Default, Clone, Copy)]
pub struct MatrixCacheBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvictionReason {
    Cold,
    LowHit,
    Stale,
    /// The entry had passed its time to live, so evicting it costs nothing.
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EvictionScore {
    hotness: u32,
    hits: u64,
    last_access_epoch: u64,
}

/// Borrowed bytes from a cache, held resident until the handle is released.
///
/// Returned by the [`ZeroCopyCacheApi`] reads. The entry cannot be evicted while
/// this exists, and its bytes are still counted by `size` -- an entry removed
/// while pinned is *retired* rather than freed, and only released once the last
/// handle drops.
///
/// Release it through the cache rather than merely dropping it, so the eviction
/// accounting is told. [`CacheScopedHandle`] does that on drop if you would
/// rather not track it.
#[derive(Debug)]
pub struct CachePinnedHandle {
    pub key: CacheKey,
    pub value: Arc<[u8]>,
    pub tier: CacheReadTier,
}

impl CachePinnedHandle {
    pub fn key(&self) -> &CacheKey {
        &self.key
    }

    #[allow(non_snake_case)]
    pub fn Key(&self) -> &CacheKey {
        self.key()
    }

    pub fn value(&self) -> &[u8] {
        &self.value
    }

    #[allow(non_snake_case)]
    pub fn Value(&self) -> &[u8] {
        self.value()
    }

    pub fn tier(&self) -> CacheReadTier {
        self.tier
    }

    pub fn as_slice(&self) -> &[u8] {
        self.value()
    }

    pub fn buffer(&self) -> CacheBuffer {
        CacheBuffer {
            key: self.key.record_key.clone(),
            value: Arc::clone(&self.value),
            logical_size: None,
            tier: Some(self.tier),
            cache: None,
            handle: None,
        }
    }

    pub fn clone_with_cache(&self, cache: &MultiLayerCache) -> CachePinnedHandle {
        cache.clone_handle(self)
    }

    pub fn clone_detached(&self) -> CachePinnedHandle {
        CachePinnedHandle {
            key: self.key.clone(),
            value: Arc::clone(&self.value),
            tier: self.tier,
        }
    }

    #[allow(non_snake_case)]
    pub fn Clone(&self) -> CachePinnedHandle {
        self.clone_detached()
    }

    #[allow(non_snake_case)]
    pub fn Buffer(&self) -> CacheBuffer {
        self.buffer()
    }

    #[allow(non_snake_case)]
    pub fn CloneWithCache(&self, cache: &MultiLayerCache) -> CachePinnedHandle {
        self.clone_with_cache(cache)
    }
}

/// A [`CachePinnedHandle`] that releases itself.
///
/// Holds the cache alongside the handle so `Drop` can return the pin properly
/// rather than merely dropping it, which would leave the eviction accounting
/// believing the entry is still held. Prefer this to a bare handle unless you
/// need to move the pin somewhere the scope does not reach.
#[derive(Debug)]
pub struct CacheScopedHandle {
    cache: MultiLayerCache,
    handle: Option<CachePinnedHandle>,
}

impl CacheScopedHandle {
    pub fn key(&self) -> &CacheKey {
        &self.handle.as_ref().expect("cache scoped handle").key
    }

    #[allow(non_snake_case)]
    pub fn Key(&self) -> &CacheKey {
        self.key()
    }

    pub fn value(&self) -> &[u8] {
        self.handle.as_ref().expect("cache scoped handle").value()
    }

    #[allow(non_snake_case)]
    pub fn Value(&self) -> &[u8] {
        self.value()
    }

    pub fn tier(&self) -> CacheReadTier {
        self.handle.as_ref().expect("cache scoped handle").tier()
    }

    pub fn as_slice(&self) -> &[u8] {
        self.value()
    }

    pub fn buffer(&self) -> CacheBuffer {
        let handle = self.handle.as_ref().expect("cache scoped handle");
        CacheBuffer::from_handle(
            handle.key.record_key.clone(),
            self.cache.clone(),
            handle.clone_with_cache(&self.cache),
        )
    }

    #[allow(non_snake_case)]
    pub fn Buffer(&self) -> CacheBuffer {
        self.buffer()
    }

    pub fn into_handle(mut self) -> CachePinnedHandle {
        self.handle.take().expect("cache scoped handle")
    }
}

impl Drop for CacheScopedHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.cache.release(handle);
        }
    }
}

#[derive(Debug)]
pub struct CacheScopedLookup {
    scoped: Option<CacheScopedHandle>,
}

impl CacheScopedLookup {
    pub fn found(&self) -> bool {
        self.scoped.is_some()
    }

    #[allow(non_snake_case)]
    pub fn Found(&self) -> bool {
        self.found()
    }

    pub fn key(&self) -> Option<&CacheKey> {
        self.scoped.as_ref().map(CacheScopedHandle::key)
    }

    pub fn key_ref(&self) -> &CacheKey {
        self.scoped
            .as_ref()
            .expect("cache scoped lookup is empty")
            .key()
    }

    #[allow(non_snake_case)]
    pub fn Key(&self) -> Option<&CacheKey> {
        self.key()
    }

    #[allow(non_snake_case)]
    pub fn KeyRef(&self) -> &CacheKey {
        self.key_ref()
    }

    pub fn as_slice(&self) -> Option<&[u8]> {
        self.value()
    }

    pub fn value(&self) -> Option<&[u8]> {
        self.scoped.as_ref().map(CacheScopedHandle::value)
    }

    pub fn value_ref(&self) -> &[u8] {
        self.scoped
            .as_ref()
            .expect("cache scoped lookup is empty")
            .value()
    }

    #[allow(non_snake_case)]
    pub fn Value(&self) -> Option<&[u8]> {
        self.value()
    }

    #[allow(non_snake_case)]
    pub fn ValueRef(&self) -> &[u8] {
        self.value_ref()
    }

    pub fn tier(&self) -> Option<CacheReadTier> {
        self.scoped.as_ref().map(CacheScopedHandle::tier)
    }

    pub fn buffer(&self) -> Option<CacheBuffer> {
        self.scoped.as_ref().map(CacheScopedHandle::buffer)
    }

    #[allow(non_snake_case)]
    pub fn Buffer(&self) -> Option<CacheBuffer> {
        self.buffer()
    }

    pub fn into_handle(mut self) -> Option<CachePinnedHandle> {
        self.scoped.take().map(CacheScopedHandle::into_handle)
    }
}

/// Which tier served a read.
///
/// The read-side counterpart of [`CacheTier`], and deliberately narrower: a
/// value that was returned necessarily came from somewhere, so there is no
/// rejection case here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheReadTier {
    Memory,
    Pmem,
    Ssd,
}

impl CacheReadTier {
    pub fn as_cache_tier(self) -> Option<CacheTier> {
        match self {
            CacheReadTier::Memory => Some(CacheTier::Memory),
            CacheReadTier::Pmem => Some(CacheTier::Pmem),
            CacheReadTier::Ssd => Some(CacheTier::Ssd),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheReadResult {
    pub value: Vec<u8>,
    pub tier: CacheReadTier,
}

/// A value read from a cache, together with where it came from.
///
/// Carries the key, the bytes, the [`CacheReadTier`] that served them and -- when
/// the read was a pinned one -- the handle keeping them resident. Dropping the
/// buffer releases that pin, which is why it has a `Drop` impl and why holding
/// one holds an entry in the cache.
#[derive(Debug)]
pub struct CacheBuffer {
    key: String,
    value: Arc<[u8]>,
    logical_size: Option<usize>,
    tier: Option<CacheReadTier>,
    cache: Option<MultiLayerCache>,
    handle: Option<CachePinnedHandle>,
}

impl CacheBuffer {
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self {
            key: String::new(),
            value: Arc::from(value.into()),
            logical_size: None,
            tier: None,
            cache: None,
            handle: None,
        }
    }

    pub fn string(value: impl Into<String>) -> Self {
        Self::new(value.into().into_bytes())
    }

    fn from_handle(key: String, cache: MultiLayerCache, handle: CachePinnedHandle) -> Self {
        Self {
            key,
            value: Arc::clone(&handle.value),
            logical_size: None,
            tier: Some(handle.tier),
            cache: Some(cache),
            handle: Some(handle),
        }
    }

    pub fn view(key: impl Into<String>, logical_size: usize) -> Self {
        Self {
            key: key.into(),
            value: Arc::from(Vec::new()),
            logical_size: Some(logical_size),
            tier: None,
            cache: None,
            handle: None,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn set_key(&mut self, key: impl Into<String>) {
        self.key = key.into();
    }

    pub fn data(&self) -> &[u8] {
        &self.value
    }

    pub fn data_ptr(&self) -> *const u8 {
        self.value.as_ptr()
    }

    pub fn value(&self) -> &[u8] {
        self.data()
    }

    pub fn size(&self) -> usize {
        self.logical_size.unwrap_or(self.value.len())
    }

    pub fn tier(&self) -> Option<CacheReadTier> {
        self.tier
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.value.to_vec()
    }

    #[allow(non_snake_case)]
    pub fn Key(&self) -> &str {
        self.key()
    }

    #[allow(non_snake_case)]
    pub fn SetKey(&mut self, key: impl Into<String>) {
        self.set_key(key);
    }

    #[allow(non_snake_case)]
    pub fn Data(&self) -> &[u8] {
        self.data()
    }

    #[allow(non_snake_case)]
    pub fn DataPtr(&self) -> *const u8 {
        self.data_ptr()
    }

    #[allow(non_snake_case)]
    pub fn Value(&self) -> &[u8] {
        self.value()
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }
}

impl Drop for CacheBuffer {
    fn drop(&mut self) {
        if let (Some(cache), Some(handle)) = (self.cache.as_ref(), self.handle.take()) {
            cache.release(handle);
        }
    }
}

#[derive(Debug)]
pub struct StringBuffer {
    key: String,
    value: String,
}

impl StringBuffer {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            key: String::new(),
            value: value.into(),
        }
    }

    pub fn string(value: impl Into<String>) -> Self {
        Self::new(value)
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn set_key(&mut self, key: impl Into<String>) {
        self.key = key.into();
    }

    pub fn data(&self) -> &[u8] {
        self.value.as_bytes()
    }

    pub fn data_ptr(&self) -> *const u8 {
        self.value.as_ptr()
    }

    pub fn value(&self) -> &[u8] {
        self.data()
    }

    pub fn string_value(&self) -> &str {
        &self.value
    }

    pub fn size(&self) -> usize {
        self.value.len()
    }

    pub fn into_string(self) -> String {
        self.value
    }

    pub fn into_cache_buffer(self) -> CacheBuffer {
        let mut buffer = CacheBuffer::new(self.value.into_bytes());
        buffer.set_key(self.key);
        buffer
    }

    #[allow(non_snake_case)]
    pub fn Key(&self) -> &str {
        self.key()
    }

    #[allow(non_snake_case)]
    pub fn SetKey(&mut self, key: impl Into<String>) {
        self.set_key(key);
    }

    #[allow(non_snake_case)]
    pub fn Data(&self) -> &[u8] {
        self.data()
    }

    #[allow(non_snake_case)]
    pub fn DataPtr(&self) -> *const u8 {
        self.data_ptr()
    }

    #[allow(non_snake_case)]
    pub fn Value(&self) -> &[u8] {
        self.value()
    }

    #[allow(non_snake_case)]
    pub fn StringValue(&self) -> &str {
        self.string_value()
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }
}

impl From<StringBuffer> for CacheBuffer {
    fn from(buffer: StringBuffer) -> Self {
        buffer.into_cache_buffer()
    }
}


#[derive(Debug)]
pub struct IoBufBuffer {
    key: String,
    value: Vec<u8>,
}

impl IoBufBuffer {
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self {
            key: String::new(),
            value: value.into(),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn set_key(&mut self, key: impl Into<String>) {
        self.key = key.into();
    }

    pub fn data(&self) -> &[u8] {
        &self.value
    }

    pub fn data_ptr(&self) -> *const u8 {
        self.value.as_ptr()
    }

    pub fn value(&self) -> &[u8] {
        self.data()
    }

    pub fn size(&self) -> usize {
        self.value.len()
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.value
    }

    #[allow(non_snake_case)]
    pub fn Key(&self) -> &str {
        self.key()
    }

    #[allow(non_snake_case)]
    pub fn SetKey(&mut self, key: impl Into<String>) {
        self.set_key(key);
    }

    #[allow(non_snake_case)]
    pub fn Data(&self) -> &[u8] {
        self.data()
    }

    #[allow(non_snake_case)]
    pub fn DataPtr(&self) -> *const u8 {
        self.data_ptr()
    }

    #[allow(non_snake_case)]
    pub fn Value(&self) -> &[u8] {
        self.value()
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }
}

impl From<IoBufBuffer> for CacheBuffer {
    fn from(buffer: IoBufBuffer) -> Self {
        let mut converted = CacheBuffer::new(buffer.value);
        converted.set_key(buffer.key);
        converted
    }
}

#[derive(Debug)]
pub struct RawBuffer {
    key: String,
    value: Option<Vec<u8>>,
    storage_engine: Option<StorageEngineKind>,
    async_delete: bool,
}

impl RawBuffer {
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self {
            key: String::new(),
            value: Some(value.into()),
            storage_engine: None,
            async_delete: false,
        }
    }

    pub fn with_storage_engine(
        value: impl Into<Vec<u8>>,
        storage_engine: StorageEngineKind,
        async_delete: bool,
    ) -> Self {
        Self {
            key: String::new(),
            value: Some(value.into()),
            storage_engine: Some(storage_engine),
            async_delete,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn set_key(&mut self, key: impl Into<String>) {
        self.key = key.into();
    }

    pub fn data(&self) -> &[u8] {
        self.value.as_deref().unwrap_or(&[])
    }

    pub fn data_ptr(&self) -> *const u8 {
        self.value
            .as_deref()
            .map_or(std::ptr::null(), <[u8]>::as_ptr)
    }

    pub fn value(&self) -> &[u8] {
        self.data()
    }

    pub fn size(&self) -> usize {
        self.value.as_ref().map_or(0, Vec::len)
    }

    pub fn storage_engine(&self) -> Option<StorageEngineKind> {
        self.storage_engine
    }

    pub fn async_delete(&self) -> bool {
        self.async_delete
    }

    pub fn reset(&mut self) {
        self.value = None;
        self.storage_engine = None;
        self.async_delete = false;
        self.key.clear();
    }

    pub fn into_cache_buffer(mut self) -> CacheBuffer {
        let mut buffer = CacheBuffer::new(self.value.take().unwrap_or_default());
        buffer.set_key(std::mem::take(&mut self.key));
        buffer
    }

    #[allow(non_snake_case)]
    pub fn Key(&self) -> &str {
        self.key()
    }

    #[allow(non_snake_case)]
    pub fn SetKey(&mut self, key: impl Into<String>) {
        self.set_key(key);
    }

    #[allow(non_snake_case)]
    pub fn Data(&self) -> &[u8] {
        self.data()
    }

    #[allow(non_snake_case)]
    pub fn DataPtr(&self) -> *const u8 {
        self.data_ptr()
    }

    #[allow(non_snake_case)]
    pub fn Value(&self) -> &[u8] {
        self.value()
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }

    #[allow(non_snake_case)]
    pub fn Reset(&mut self) {
        self.reset();
    }
}

impl From<RawBuffer> for CacheBuffer {
    fn from(buffer: RawBuffer) -> Self {
        buffer.into_cache_buffer()
    }
}


/// Receives each record found while a tier is recovered at startup.
///
/// `on_recover_data` is called once per record read back from persistent
/// storage, in the order the store enumerates them, before the cache is
/// serving.
pub trait RecoverDataCallback {
    fn on_recover_data(&mut self, key: &str, buffer: CacheBuffer);
}

/// Repairs an index when collection moves a record.
///
/// `update` is handed the record's new location so an index pointing at the old
/// one can be corrected.
///
/// **Nothing in this crate calls it.** Collection here sweeps freed slots rather
/// than relocating live ones, so no record ever changes address and the callback
/// has no occasion to fire. It is implemented — [`CacheInstance`] provides it —
/// for callers driving collection themselves, and to keep the surface the
/// reference defines, where collection does compact and an implementor that
/// ignored this would be left with dangling entries.
pub trait GcCopyCallback {
    fn update(
        &mut self,
        key: &str,
        old_data: &[u8],
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError>;
}

#[derive(Debug, Default)]
pub struct RecoverDataCallbackMock {
    last_recover_key: String,
    recovered_record_count: i64,
    recovered: Vec<(String, CacheBuffer)>,
}

impl RecoverDataCallbackMock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_recover_key(&self) -> &str {
        &self.last_recover_key
    }

    pub fn recovered_record_count(&self) -> i64 {
        self.recovered_record_count
    }

    pub fn recovered(&self) -> &[(String, CacheBuffer)] {
        &self.recovered
    }

    #[allow(non_snake_case)]
    pub fn GetLastRecoverKey(&self) -> &str {
        self.last_recover_key()
    }

    #[allow(non_snake_case)]
    pub fn GetRecoveredRecordCnt(&self) -> i64 {
        self.recovered_record_count()
    }

    #[allow(non_snake_case)]
    pub fn OnRecoverData(&mut self, key: &str, buffer: CacheBuffer) {
        self.on_recover_data(key, buffer);
    }
}

impl RecoverDataCallback for RecoverDataCallbackMock {
    fn on_recover_data(&mut self, key: &str, buffer: CacheBuffer) {
        self.last_recover_key = key.to_string();
        self.recovered_record_count = self.recovered_record_count.saturating_add(1);
        self.recovered.push((key.to_string(), buffer));
    }
}

#[derive(Debug, Default)]
pub struct GcCopyCallbackMock {
    map: HashMap<String, CacheBuffer>,
}

impl GcCopyCallbackMock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn delete_cache_buffer(&mut self, key: &str) -> bool {
        self.map.remove(key).is_some()
    }

    pub fn add_cache_buffer(&mut self, key: impl Into<String>, buffer: CacheBuffer) -> bool {
        match self.map.entry(key.into()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(buffer);
                true
            }
            std::collections::hash_map::Entry::Occupied(_) => false,
        }
    }

    pub fn get_cache_buffer(&self, key: &str) -> Option<&CacheBuffer> {
        self.map.get(key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[allow(non_snake_case)]
    pub fn DeleteCacheBuffer(&mut self, key: &str) -> bool {
        self.delete_cache_buffer(key)
    }

    #[allow(non_snake_case)]
    pub fn AddCacheBuffer(&mut self, key: impl Into<String>, buffer: CacheBuffer) -> bool {
        self.add_cache_buffer(key, buffer)
    }

    #[allow(non_snake_case)]
    pub fn GetCacheBuffer(&self, key: &str) -> Option<&CacheBuffer> {
        self.get_cache_buffer(key)
    }

    #[allow(non_snake_case)]
    pub fn Update(
        &mut self,
        key: &str,
        old_data: &[u8],
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        self.update(key, old_data, new_buffer)
    }
}

impl GcCopyCallback for GcCopyCallbackMock {
    fn update(
        &mut self,
        key: &str,
        old_data: &[u8],
        new_buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        let old_buffer = self.map.get(key).ok_or(CacheError::NotFound)?;
        if old_buffer.Data() != old_data {
            return Err(CacheError::ReplaceMismatch);
        }
        self.map.insert(key.to_string(), new_buffer);
        Ok(())
    }
}

/// What a tier's storage engine must provide.
///
/// Implemented by [`StorageEngineRocksDb`] (the default SSD backend),
/// [`StorageEngineSimple`] (the file-backed store used when the `rocksdb-ssd`
/// feature is off) and [`StorageEngineMultiSsd`] (several device paths, one
/// chosen per key by hash).
///
/// `peek` is the read that does not disturb anything the engine tracks; `get` is
/// the ordinary one.
pub trait StorageEngineApi {
    fn start(&mut self) -> bool;
    fn stop(&mut self) -> bool;
    fn put(&mut self, key: &str, value: Vec<u8>) -> Result<CacheBuffer, CacheError>;
    fn async_put<F>(&mut self, buffer: CacheBuffer, cb: F) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>);
    fn get(&self, key: &str) -> Result<CacheBuffer, CacheError>;
    fn peek(&self, key: &str) -> bool;
    fn delete_buffer(&mut self, buffer: &CacheBuffer) -> Result<(), CacheError>;
    fn async_delete<F>(&mut self, buffer: &CacheBuffer, cb: F) -> Result<(), CacheError>
    where
        F: FnOnce(Result<(), CacheError>);
    fn delete(&mut self, key: &str) -> Result<(), CacheError>;
    fn reset(&mut self) -> Result<(), CacheError>;
    fn recover_data<C: RecoverDataCallback>(&self, callback: &mut C) -> Result<(), CacheError>;
    fn set_capacity(&mut self, capacity: u64);
    fn capacity(&self) -> u64;
    fn is_started(&self) -> bool;
    fn storage_engine_type(&self) -> StorageEngineKind;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemStorageRecordHandle {
    pub record_address: AllocatorAddress,
    pub data_address: AllocatorAddress,
    pub record_len: usize,
    pub value_len: usize,
    pub key_len: usize,
}

impl MemStorageRecordHandle {
    pub fn payload_offset(&self) -> usize {
        self.data_address.saturating_sub(self.record_address)
    }

    #[allow(non_snake_case)]
    pub fn PayloadOffset(&self) -> usize {
        self.payload_offset()
    }

    #[allow(non_snake_case)]
    pub fn RecordPtr(&self) -> AllocatorAddress {
        self.record_address
    }

    #[allow(non_snake_case)]
    pub fn DataPtr(&self) -> AllocatorAddress {
        self.data_address
    }

    #[allow(non_snake_case)]
    pub fn RecordLen(&self) -> usize {
        self.record_len
    }

    #[allow(non_snake_case)]
    pub fn ValueLen(&self) -> usize {
        self.value_len
    }

    #[allow(non_snake_case)]
    pub fn KeyLen(&self) -> usize {
        self.key_len
    }
}

pub struct MemStorage;

/// Continue a CRC-32C (Castagnoli) checksum over `bytes`, starting from the
/// checksum `seed` returned by an earlier call. Chaining a seed gives the same
/// result as checksumming the concatenated input in one go, which is what lets
/// a record be checksummed header-then-value-then-key without copying it into
/// one buffer first.
pub fn crc32c_with_seed(bytes: &[u8], seed: u32) -> u32 {
    let mut crc = !seed;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
        }
    }
    !crc
}

/// CRC-32C (Castagnoli) checksum of `bytes`.
pub fn crc32c(bytes: &[u8]) -> u32 {
    crc32c_with_seed(bytes, 0)
}

impl MemStorage {
    pub const HEADER_BYTES: usize = 8;

    /// Checksum of a cache record: the length header first, then the value,
    /// then the key.
    ///
    /// Covering the header matters. Two records whose value and key bytes
    /// concatenate to the same sequence but split differently are only
    /// distinguishable by their lengths, so a checksum over the payload alone
    /// would accept a record whose length header had been corrupted.
    pub fn compute_crc(key: &str, value: &[u8]) -> u32 {
        let mut lengths = [0u8; Self::HEADER_BYTES];
        lengths[..4].copy_from_slice(&(value.len() as u32).to_le_bytes());
        lengths[4..].copy_from_slice(&(key.len() as u32).to_le_bytes());
        let crc = crc32c(&lengths);
        let crc = crc32c_with_seed(value, crc);
        crc32c_with_seed(key.as_bytes(), crc)
    }

    pub fn do_put(key: &str, value: &[u8]) -> Vec<u8> {
        let mut record =
            Vec::with_capacity(Self::HEADER_BYTES + value.len() + key.len());
        record.extend_from_slice(&(value.len() as u32).to_le_bytes());
        record.extend_from_slice(&(key.len() as u32).to_le_bytes());
        record.extend_from_slice(value);
        record.extend_from_slice(key.as_bytes());
        record
    }

    pub fn do_put_with_crc(key: &str, value: &[u8], crc: u32) -> Result<Vec<u8>, CacheError> {
        let actual = Self::compute_crc(key, value);
        if actual != crc {
            return Err(CacheError::CorruptBlock(format!(
                "crc mismatch for key {key}: expected {crc}, got {actual}"
            )));
        }
        Ok(Self::do_put(key, value))
    }

    pub fn do_put_to_allocator<A>(
        allocator: &mut A,
        key: &str,
        value: &[u8],
    ) -> Result<MemStorageRecordHandle, CacheError>
    where
        A: MemStorageAllocatorApi,
    {
        Self::do_put_to_allocator_with_crc(allocator, key, value, Self::compute_crc(key, value))
    }

    pub fn do_put_to_allocator_with_crc<A>(
        allocator: &mut A,
        key: &str,
        value: &[u8],
        crc: u32,
    ) -> Result<MemStorageRecordHandle, CacheError>
    where
        A: MemStorageAllocatorApi,
    {
        let record = Self::do_put_with_crc(key, value, crc)?;
        let record_address = allocator.allocate(record.len())?;
        if let Err(err) = allocator.write_region(record_address, &record) {
            let _ = allocator.free(record_address, record.len());
            return Err(err);
        }
        if let Err(err) = allocator.seal_with_crc(record_address, record.len(), crc) {
            let _ = allocator.free(record_address, record.len());
            return Err(err);
        }
        Ok(MemStorageRecordHandle {
            record_address,
            data_address: record_address.saturating_add(Self::HEADER_BYTES),
            record_len: record.len(),
            value_len: value.len(),
            key_len: key.len(),
        })
    }

    pub fn get_value_from_data(data: &[u8]) -> Result<&[u8], CacheError> {
        let (value_len, key_len) = Self::record_lengths(data)?;
        let value_start = Self::HEADER_BYTES;
        let value_end = value_start + value_len;
        let record_end = value_end + key_len;
        if data.len() < record_end {
            return Err(CacheError::CorruptBlock(
                "mem storage record shorter than declared length".to_string(),
            ));
        }
        Ok(&data[value_start..value_end])
    }

    pub fn get_key_from_data(data: &[u8]) -> Result<&str, CacheError> {
        let (value_len, key_len) = Self::record_lengths(data)?;
        let key_start = Self::HEADER_BYTES + value_len;
        let key_end = key_start + key_len;
        if data.len() < key_end {
            return Err(CacheError::CorruptBlock(
                "mem storage record shorter than declared key length".to_string(),
            ));
        }
        std::str::from_utf8(&data[key_start..key_end])
            .map_err(|err| CacheError::CorruptBlock(err.to_string()))
    }

    pub fn create_cache_buffer_from_data(
        data: &[u8],
        storage_engine: StorageEngineKind,
        async_delete: bool,
    ) -> Result<CacheBuffer, CacheError> {
        let key = Self::get_key_from_data(data)?.to_string();
        let value = Self::get_value_from_data(data)?.to_vec();
        let mut raw = RawBuffer::with_storage_engine(value, storage_engine, async_delete);
        raw.set_key(key);
        Ok(raw.into_cache_buffer())
    }

    pub fn create_cache_buffer_from_allocator_data<A>(
        allocator: &A,
        handle: MemStorageRecordHandle,
        storage_engine: StorageEngineKind,
        async_delete: bool,
    ) -> Result<CacheBuffer, CacheError>
    where
        A: MemStorageAllocatorApi,
    {
        let record = allocator.read_region(handle.record_address)?;
        let expected_offset = Self::HEADER_BYTES;
        if handle.payload_offset() != expected_offset {
            return Err(CacheError::CorruptBlock(
                "mem storage data pointer does not point at payload".to_string(),
            ));
        }
        Self::create_cache_buffer_from_data(record, storage_engine, async_delete)
    }

    pub fn do_delete(_data: &[u8]) -> Result<(), CacheError> {
        Ok(())
    }

    pub fn do_delete_from_allocator<A>(
        allocator: &mut A,
        handle: MemStorageRecordHandle,
    ) -> Result<(), CacheError>
    where
        A: MemStorageAllocatorApi,
    {
        let (value_len, key_len, record_len) = {
            let record = allocator.read_region(handle.record_address)?;
            let (value_len, key_len) = Self::record_lengths(record)?;
            let record_len = Self::HEADER_BYTES
                .saturating_add(value_len)
                .saturating_add(key_len);
            (value_len, key_len, record_len)
        };
        if record_len != handle.record_len
            || value_len != handle.value_len
            || key_len != handle.key_len
        {
            return Err(CacheError::CorruptBlock(
                "mem storage handle length mismatch".to_string(),
            ));
        }
        allocator.free(handle.record_address, handle.record_len)
    }

    fn record_lengths(data: &[u8]) -> Result<(usize, usize), CacheError> {
        if data.len() < Self::HEADER_BYTES {
            return Err(CacheError::CorruptBlock(
                "mem storage record missing header".to_string(),
            ));
        }
        let value_len =
            u32::from_le_bytes(data[0..4].try_into().expect("value length header")) as usize;
        let key_len =
            u32::from_le_bytes(data[4..8].try_into().expect("key length header")) as usize;
        Ok((value_len, key_len))
    }

    #[allow(non_snake_case)]
    pub fn ComputeCRC(key: &str, value: &[u8]) -> u32 {
        Self::compute_crc(key, value)
    }

    #[allow(non_snake_case)]
    pub fn DoPut(key: &str, value: &[u8]) -> Vec<u8> {
        Self::do_put(key, value)
    }

    #[allow(non_snake_case)]
    pub fn DoPutWithCRC(key: &str, value: &[u8], crc: u32) -> Result<Vec<u8>, CacheError> {
        Self::do_put_with_crc(key, value, crc)
    }

    #[allow(non_snake_case)]
    pub fn DoPutToAllocator<A>(
        allocator: &mut A,
        key: &str,
        value: &[u8],
    ) -> Result<MemStorageRecordHandle, CacheError>
    where
        A: MemStorageAllocatorApi,
    {
        Self::do_put_to_allocator(allocator, key, value)
    }

    #[allow(non_snake_case)]
    pub fn DoPutToAllocatorWithCRC<A>(
        allocator: &mut A,
        key: &str,
        value: &[u8],
        crc: u32,
    ) -> Result<MemStorageRecordHandle, CacheError>
    where
        A: MemStorageAllocatorApi,
    {
        Self::do_put_to_allocator_with_crc(allocator, key, value, crc)
    }

    #[allow(non_snake_case)]
    pub fn DoDelete(data: &[u8]) -> Result<(), CacheError> {
        Self::do_delete(data)
    }

    #[allow(non_snake_case)]
    pub fn DoDeleteFromAllocator<A>(
        allocator: &mut A,
        handle: MemStorageRecordHandle,
    ) -> Result<(), CacheError>
    where
        A: MemStorageAllocatorApi,
    {
        Self::do_delete_from_allocator(allocator, handle)
    }

    #[allow(non_snake_case)]
    pub fn GetKeyFromData(data: &[u8]) -> Result<&str, CacheError> {
        Self::get_key_from_data(data)
    }

    #[allow(non_snake_case)]
    pub fn GetValueFromData(data: &[u8]) -> Result<&[u8], CacheError> {
        Self::get_value_from_data(data)
    }

    #[allow(non_snake_case)]
    pub fn CreateCacheBufferFromData(
        data: &[u8],
        storage_engine: StorageEngineKind,
        async_delete: bool,
    ) -> Result<CacheBuffer, CacheError> {
        Self::create_cache_buffer_from_data(data, storage_engine, async_delete)
    }

    #[allow(non_snake_case)]
    pub fn CreateCacheBufferFromAllocatorData<A>(
        allocator: &A,
        handle: MemStorageRecordHandle,
        storage_engine: StorageEngineKind,
        async_delete: bool,
    ) -> Result<CacheBuffer, CacheError>
    where
        A: MemStorageAllocatorApi,
    {
        Self::create_cache_buffer_from_allocator_data(
            allocator,
            handle,
            storage_engine,
            async_delete,
        )
    }
}

#[derive(Debug, Default, Clone)]
pub struct PmemAllocatorRecoverListenerImpl {
    records: HashMap<String, Option<Vec<u8>>>,
    duplicate_records: Vec<Vec<u8>>,
}

impl PmemAllocatorRecoverListenerImpl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_estimate_items(num_estimate_items: usize) -> Self {
        Self {
            records: HashMap::with_capacity(num_estimate_items),
            duplicate_records: Vec::new(),
        }
    }

    pub fn on_scan_record_data(&mut self, data: &[u8]) -> Result<bool, CacheError> {
        let key = MemStorage::get_key_from_data(data)?.to_string();
        match self.records.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Some(data.to_vec()));
                Ok(false)
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                entry.insert(None);
                self.duplicate_records.push(data.to_vec());
                Ok(true)
            }
        }
    }

    pub fn finish_recover<C: RecoverDataCallback>(
        &mut self,
        callback: &mut C,
    ) -> Result<u64, CacheError> {
        let mut valid_records = 0_u64;
        let mut keys = self.records.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        for key in keys {
            let Some(Some(record)) = self.records.get(&key) else {
                continue;
            };
            let buffer =
                MemStorage::create_cache_buffer_from_data(record, StorageEngineKind::Pmem, true)?;
            callback.on_recover_data(&key, buffer);
            valid_records = valid_records.saturating_add(1);
        }
        self.duplicate_records.clear();
        Ok(valid_records)
    }

    pub fn duplicate_record_count(&self) -> usize {
        self.duplicate_records.len()
    }

    pub fn scanned_record_count(&self) -> usize {
        self.records.len()
    }

    #[allow(non_snake_case)]
    pub fn OnScanRecord(&mut self, data: &[u8]) -> Result<bool, CacheError> {
        self.on_scan_record_data(data)
    }

    #[allow(non_snake_case)]
    pub fn FinishRecover<C: RecoverDataCallback>(
        &mut self,
        callback: &mut C,
    ) -> Result<u64, CacheError> {
        self.finish_recover(callback)
    }
}

#[derive(Debug, Clone)]
/// An in-memory storage engine: a `HashMap<String, Vec<u8>>` with a capacity.
///
/// Also what [`StorageEngineDram`] and [`StorageEnginePmem`] name -- both are
/// aliases for this type, so the three tier names share one implementation and
/// differ only in what a signature calls them. Nothing in this crate constructs
/// any of the three; the tiers of a [`MultiLayerCache`] are owned directly.
///
/// It implements more than the reference engine of the same name, which returns
/// "not implemented" from `Get` and from recovery. In particular
/// [`StorageEngineApi::recover_data`] here succeeds by replaying the records it
/// currently holds, rather than reporting that a volatile tier has nothing to
/// recover. On a fresh instance that is a successful recovery of zero records,
/// which is not the same answer as "this tier cannot be recovered".
pub struct StorageEngineSimple {
    initialized: bool,
    capacity: u64,
    records: HashMap<String, Vec<u8>>,
    delete_completed_count: u32,
}

impl StorageEngineSimple {
    pub fn new() -> Self {
        Self {
            initialized: false,
            capacity: u64::MAX,
            records: HashMap::new(),
            delete_completed_count: 0,
        }
    }

    pub fn with_capacity(capacity: u64) -> Self {
        Self {
            capacity,
            ..Self::new()
        }
    }

    fn used_bytes(&self) -> u64 {
        self.records
            .values()
            .map(|record| record.len() as u64)
            .sum()
    }

    fn buffer_from_record(&self, record: &[u8]) -> Result<CacheBuffer, CacheError> {
        MemStorage::create_cache_buffer_from_data(record, StorageEngineKind::Simple, false)
    }

    pub fn test_num_delete_completed_count(&self) -> u32 {
        self.delete_completed_count
    }

    pub fn test_increase_delete_completed_count(&mut self) {
        self.delete_completed_count = self.delete_completed_count.saturating_add(1);
    }

    pub fn test_join_pmem_write_executor(&self) {}

    pub fn test_put_to_numa(
        &mut self,
        key: &str,
        value: Vec<u8>,
        _numa_id: i32,
    ) -> Result<CacheBuffer, CacheError> {
        self.put(key, value)
    }

    pub fn test_recover_stats(&self) -> PmemRecoverStats {
        let mut stats = PmemRecoverStats::default();
        for record in self.records.values() {
            match (
                MemStorage::get_value_from_data(record),
                MemStorage::get_key_from_data(record),
            ) {
                (Ok(value), Ok(_key)) => stats.AddChunkStats(ChunkRecoverStats {
                    valid_bytes: value.len(),
                    freed_bytes: 0,
                    corrupted_bytes: 0,
                }),
                _ => stats.AddChunkStats(ChunkRecoverStats {
                    valid_bytes: 0,
                    freed_bytes: 0,
                    corrupted_bytes: record.len(),
                }),
            }
        }
        stats
    }
}

impl Default for StorageEngineSimple {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageEngineApi for StorageEngineSimple {
    fn start(&mut self) -> bool {
        self.initialized = true;
        true
    }

    fn stop(&mut self) -> bool {
        self.initialized = false;
        true
    }

    fn put(&mut self, key: &str, value: Vec<u8>) -> Result<CacheBuffer, CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        let record = MemStorage::do_put(key, &value);
        let old_len = self.records.get(key).map_or(0u64, |old| old.len() as u64);
        let next_used = self
            .used_bytes()
            .saturating_sub(old_len)
            .saturating_add(record.len() as u64);
        if next_used > self.capacity {
            return Err(CacheError::CapacityExceeded);
        }
        self.records.insert(key.to_string(), record);
        let mut buffer = CacheBuffer::new(value);
        buffer.set_key(key);
        Ok(buffer)
    }

    fn async_put<F>(&mut self, buffer: CacheBuffer, cb: F) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>),
    {
        let key = buffer.key().to_string();
        let value = buffer.to_vec();
        let result = self.put(&key, value);
        match result {
            Ok(buf) => {
                let mut cloned = CacheBuffer::new(buf.to_vec());
                cloned.set_key(buf.key());
                cb(Ok(cloned));
                Ok(buf)
            }
            Err(err) => {
                cb(Err(cache_error_for_callback(&err)));
                Err(err)
            }
        }
    }

    fn get(&self, key: &str) -> Result<CacheBuffer, CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        let record = self.records.get(key).ok_or(CacheError::NotFound)?;
        self.buffer_from_record(record)
    }

    fn peek(&self, key: &str) -> bool {
        self.records.contains_key(key)
    }

    fn delete_buffer(&mut self, buffer: &CacheBuffer) -> Result<(), CacheError> {
        self.delete(buffer.key())
    }

    fn async_delete<F>(&mut self, buffer: &CacheBuffer, cb: F) -> Result<(), CacheError>
    where
        F: FnOnce(Result<(), CacheError>),
    {
        let result = self.delete_buffer(buffer);
        match result {
            Ok(()) => {
                cb(Ok(()));
                Ok(())
            }
            Err(err) => {
                cb(Err(cache_error_for_callback(&err)));
                Err(err)
            }
        }
    }

    fn delete(&mut self, key: &str) -> Result<(), CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        self.records.remove(key).ok_or(CacheError::NotFound)?;
        self.delete_completed_count = self.delete_completed_count.saturating_add(1);
        Ok(())
    }

    fn reset(&mut self) -> Result<(), CacheError> {
        self.records.clear();
        self.delete_completed_count = 0;
        Ok(())
    }

    fn recover_data<C: RecoverDataCallback>(&self, callback: &mut C) -> Result<(), CacheError> {
        for (key, record) in &self.records {
            callback.on_recover_data(key, self.buffer_from_record(record)?);
        }
        Ok(())
    }

    fn set_capacity(&mut self, capacity: u64) {
        self.capacity = capacity;
    }

    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn is_started(&self) -> bool {
        self.initialized
    }

    fn storage_engine_type(&self) -> StorageEngineKind {
        StorageEngineKind::Simple
    }
}

#[allow(non_snake_case)]
impl StorageEngineSimple {
    pub fn Start(&mut self) -> bool {
        self.start()
    }

    pub fn Stop(&mut self) -> bool {
        self.stop()
    }

    pub fn Put(&mut self, key: &str, value: Vec<u8>) -> Result<CacheBuffer, CacheError> {
        self.put(key, value)
    }

    pub fn AsyncPut<F>(&mut self, buffer: CacheBuffer, cb: F) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>),
    {
        self.async_put(buffer, cb)
    }

    pub fn Get(&self, key: &str) -> Result<CacheBuffer, CacheError> {
        self.get(key)
    }

    pub fn Peek(&self, key: &str) -> bool {
        self.peek(key)
    }

    pub fn DeleteBuffer(&mut self, buffer: &CacheBuffer) -> Result<(), CacheError> {
        self.delete_buffer(buffer)
    }

    pub fn AsyncDelete<F>(&mut self, buffer: &CacheBuffer, cb: F) -> Result<(), CacheError>
    where
        F: FnOnce(Result<(), CacheError>),
    {
        self.async_delete(buffer, cb)
    }

    pub fn Delete(&mut self, key: &str) -> Result<(), CacheError> {
        self.delete(key)
    }

    pub fn Reset(&mut self) -> Result<(), CacheError> {
        self.reset()
    }

    pub fn RecoverData<C: RecoverDataCallback>(&self, callback: &mut C) -> Result<(), CacheError> {
        self.recover_data(callback)
    }

    pub fn SetCapacity(&mut self, capacity: u64) {
        self.set_capacity(capacity);
    }

    pub fn Capacity(&self) -> u64 {
        self.capacity()
    }

    pub fn StorageEngineType(&self) -> StorageEngineKind {
        self.storage_engine_type()
    }

    pub fn TEST_GetNumDeleteCompletedCount(&self) -> u32 {
        self.test_num_delete_completed_count()
    }

    pub fn TEST_IncreaseDeleteCompletedCount(&mut self) {
        self.test_increase_delete_completed_count();
    }

    pub fn TEST_JoinPmemWriteExecutor(&self) {
        self.test_join_pmem_write_executor();
    }

    pub fn TEST_PutToNuma(
        &mut self,
        key: &str,
        value: Vec<u8>,
        numa_id: i32,
    ) -> Result<CacheBuffer, CacheError> {
        self.test_put_to_numa(key, value, numa_id)
    }

    pub fn TEST_GetRecoverStats(&self) -> PmemRecoverStats {
        self.test_recover_stats()
    }
}

pub type StorageEngineDram = StorageEngineSimple;
pub type StorageEnginePmem = StorageEngineSimple;

/// The default SSD engine, backed by RocksDB.
///
/// Present only with the `rocksdb-ssd` feature, which is on by default. Keeps an
/// in-memory index alongside the database so `peek` and capacity accounting do
/// not have to touch it; `recover_data` walks the database itself, which is what
/// makes a restart able to re-enumerate what is on disk.
pub struct StorageEngineRocksDb {
    db_path: String,
    initialized: bool,
    records: HashMap<String, Vec<u8>>,
    /// Bytes appended to the journal since the last snapshot, so compaction
    /// can be triggered by how much has accumulated rather than by a count of
    /// operations, which says nothing about the size of them.
    #[cfg(not(feature = "rocksdb-ssd"))]
    journal_bytes: u64,
    /// The journal, held open between appends.
    ///
    /// Every record used to open the file, write, and close it again -- three
    /// syscalls of overhead for one append, on a path that runs once per
    /// write. It is an append-only log with a single writer, so there is
    /// nothing to reopen for.
    ///
    /// `None` means "not opened yet, or the file underneath is gone". It MUST
    /// be cleared wherever the journal is removed, or appends would go to an
    /// unlinked inode and be lost at the next restart -- which is why the only
    /// place that removes it also clears this, and a test writes across a
    /// compaction to prove it.
    #[cfg(not(feature = "rocksdb-ssd"))]
    journal_file: Option<File>,
    /// How large the snapshot was when it was last written or read.
    ///
    /// Compaction compares the journal against it, and that comparison happens
    /// on every append. Asking the filesystem each time cost one `statx` per
    /// write -- and failed, because before the first compaction there is no
    /// snapshot to stat. The size is known at the two moments it changes.
    snapshot_bytes: u64,
    recover_finished: bool,
    capacity: u64,
    #[cfg(feature = "rocksdb-ssd")]
    db: Option<Arc<rocksdb::DB>>,
}

#[cfg(feature = "rocksdb-ssd")]
static ROCKSDB_HANDLE_REGISTRY: OnceLock<Mutex<HashMap<String, std::sync::Weak<rocksdb::DB>>>> =
    OnceLock::new();

impl std::fmt::Debug for StorageEngineRocksDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageEngineRocksDb")
            .field("db_path", &self.db_path)
            .field("initialized", &self.initialized)
            .field("records", &self.records)
            .field("recover_finished", &self.recover_finished)
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

impl Clone for StorageEngineRocksDb {
    fn clone(&self) -> Self {
        Self {
            db_path: self.db_path.clone(),
            initialized: self.initialized,
            records: self.records.clone(),
            #[cfg(not(feature = "rocksdb-ssd"))]
            journal_bytes: self.journal_bytes,
            // A clone opens its own handle when it first appends: a file
            // descriptor is not shared state, and two writers on one
            // append-only log is not a thing this wants.
            #[cfg(not(feature = "rocksdb-ssd"))]
            journal_file: None,
            snapshot_bytes: self.snapshot_bytes,
            recover_finished: self.recover_finished,
            capacity: self.capacity,
            #[cfg(feature = "rocksdb-ssd")]
            db: self.db.clone(),
        }
    }
}

impl StorageEngineRocksDb {
    #[cfg(not(feature = "rocksdb-ssd"))]
    const STORE_FILE_NAME: &'static str = "matrixcache_rocksdb_compat_store.bin";
    #[cfg(not(feature = "rocksdb-ssd"))]
    const STORE_MAGIC: &'static [u8] = b"matrixcache-rocksdb-v1\0";
    #[cfg(not(feature = "rocksdb-ssd"))]
    const JOURNAL_FILE_NAME: &'static str = "matrixcache_rocksdb_compat_journal.bin";
    #[cfg(not(feature = "rocksdb-ssd"))]
    const JOURNAL_MAGIC: &'static [u8] = b"matrixcache-rocksdb-journal-v1\0";
    /// A journal is compacted once it is larger than the snapshot it is
    /// appended to, so the bytes written to keep a store of n records is
    /// proportional to n rather than to n squared: each snapshot is paid for
    /// by at least its own size in appends.
    #[cfg(not(feature = "rocksdb-ssd"))]
    const JOURNAL_COMPACTION_FLOOR_BYTES: u64 = 64 * 1024;
    #[cfg(not(feature = "rocksdb-ssd"))]
    const JOURNAL_PUT: u8 = 1;
    #[cfg(not(feature = "rocksdb-ssd"))]
    const JOURNAL_DELETE: u8 = 2;

    pub fn new(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
            initialized: false,
            records: HashMap::new(),
            #[cfg(not(feature = "rocksdb-ssd"))]
            journal_bytes: 0,
            #[cfg(not(feature = "rocksdb-ssd"))]
            journal_file: None,
            snapshot_bytes: 0,
            recover_finished: false,
            capacity: u64::MAX,
            #[cfg(feature = "rocksdb-ssd")]
            db: None,
        }
    }

    pub fn with_metric_registry(db_path: impl Into<String>, _registry: ()) -> Self {
        Self::new(db_path)
    }

    /// Memtable size for the SSD tier, in MiB. Default 64, matching the previous constant.
    ///
    /// RocksDB preallocates the write-ahead log to hold a full memtable flush, so this figure is
    /// also a floor on the DB's on-disk size -- paid whether or not anything is cached. Measured
    /// on a TemporalStore block cache holding 0.32 MB of content: the WAL file reported 331,697
    /// bytes of data against 73,822,208 bytes allocated, and the cache directory was 74.5 MB, of
    /// which 74.1 MB was preallocated air. For a small or short-lived cache that fixed overhead
    /// dwarfs the data.
    ///
    /// Left at 64 MiB by default so existing deployments are unchanged; a deployment that knows
    /// its cache is small can set `MATRIXCACHE_ROCKSDB_WRITE_BUFFER_MB` lower and get the disk
    /// back. Smaller memtables flush more often, so this trades write amplification against that
    /// fixed cost -- which is the right trade only when the cache is small relative to 64 MiB.
    #[cfg(feature = "rocksdb-ssd")]
    pub(crate) fn rocksdb_write_buffer_bytes() -> usize {
        const DEFAULT_MB: usize = 64;
        std::env::var("MATRIXCACHE_ROCKSDB_WRITE_BUFFER_MB")
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .filter(|mb| *mb > 0)
            .unwrap_or(DEFAULT_MB)
            * 1024
            * 1024
    }

    #[cfg(feature = "rocksdb-ssd")]
    fn rocksdb_options() -> rocksdb::Options {
        let mut options = rocksdb::Options::default();
        options.create_if_missing(true);
        options.increase_parallelism(4);
        options.optimize_for_point_lookup(64);
        options.set_write_buffer_size(Self::rocksdb_write_buffer_bytes());
        options.set_max_write_buffer_number(4);
        options.set_level_zero_file_num_compaction_trigger(16);
        options.set_level_zero_slowdown_writes_trigger(24);
        options.set_level_zero_stop_writes_trigger(32);
        options
    }

    #[cfg(feature = "rocksdb-ssd")]
    fn rocksdb_write_options() -> rocksdb::WriteOptions {
        let mut options = rocksdb::WriteOptions::default();
        options.set_sync(false);
        options
    }

    #[cfg(feature = "rocksdb-ssd")]
    fn open_rocksdb(&self) -> Result<rocksdb::DB, CacheError> {
        rocksdb::DB::open(&Self::rocksdb_options(), &self.db_path)
            .map_err(|err| CacheError::RocksDb(err.to_string()))
    }

    #[cfg(feature = "rocksdb-ssd")]
    fn open_rocksdb_arc(&self) -> Result<Arc<rocksdb::DB>, CacheError> {
        let registry = ROCKSDB_HANDLE_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut registry = registry.lock().expect("rocksdb registry lock poisoned");
        if let Some(existing) = registry
            .get(&self.db_path)
            .and_then(std::sync::Weak::upgrade)
        {
            return Ok(existing);
        }
        let db = Arc::new(self.open_rocksdb()?);
        registry.insert(self.db_path.clone(), Arc::downgrade(&db));
        Ok(db)
    }

    #[cfg(feature = "rocksdb-ssd")]
    fn rocksdb(&self) -> Result<&rocksdb::DB, CacheError> {
        self.db.as_deref().ok_or(CacheError::Stopped)
    }

    pub fn ssd_backend_name(&self) -> &'static str {
        if cfg!(feature = "rocksdb-ssd") {
            "rocksdb"
        } else {
            "file-compat"
        }
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn store_path(&self) -> PathBuf {
        PathBuf::from(&self.db_path).join(Self::STORE_FILE_NAME)
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn journal_path(&self) -> PathBuf {
        PathBuf::from(&self.db_path).join(Self::JOURNAL_FILE_NAME)
    }

    /// Record one change, and take a fresh snapshot once enough have built up.
    ///
    /// The store used to be rewritten in full after every put, which costs the
    /// whole store per write and so n squared to fill it: measured, 250, 500
    /// and 1000 puts wrote 3.4 MB, 13.3 MB and 53.0 MB -- four times the bytes
    /// for twice the entries, each time.
    ///
    /// An append also cannot destroy what is already there. `fs::write`
    /// truncates first, so a crash during any put left the store shorter than
    /// it should be, and it is the whole store.
    #[cfg(not(feature = "rocksdb-ssd"))]
    fn journal_change(&mut self, key: &str, value: Option<&[u8]>) -> Result<(), CacheError> {
        let mut record = Vec::with_capacity(key.len() + value.map_or(0, <[u8]>::len) + 24);
        match value {
            Some(value) => {
                record.push(Self::JOURNAL_PUT);
                put_fixed_uint64(&mut record, key.len() as u64);
                put_fixed_uint64(&mut record, value.len() as u64);
                record.extend_from_slice(key.as_bytes());
                record.extend_from_slice(value);
            }
            None => {
                record.push(Self::JOURNAL_DELETE);
                put_fixed_uint64(&mut record, key.len() as u64);
                record.extend_from_slice(key.as_bytes());
            }
        }

        let appended = record.len() as u64;
        if self.journal_file.is_none() {
            let path = self.journal_path();
            let directory = PathBuf::from(&self.db_path);
            // The `metadata` call that decided whether to write the magic ran
            // on every append and is now paid once per open, because the only
            // time a journal can be empty is when it has just been created.
            let mut file = creating_the_directory_if_missing(&directory, || {
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(CacheError::Io)
            })?;
            if file.metadata().map_err(CacheError::Io)?.len() == 0 {
                use std::io::Write as _;
                file.write_all(Self::JOURNAL_MAGIC).map_err(CacheError::Io)?;
            }
            self.journal_file = Some(file);
        }
        {
            use std::io::Write as _;
            let file = self
                .journal_file
                .as_mut()
                .expect("the journal was just opened");
            file.write_all(&record).map_err(CacheError::Io)?;
        }
        self.journal_bytes = self.journal_bytes.saturating_add(appended);

        // Compact when the journal has grown past the snapshot it sits beside,
        // so the cost of snapshots is spread across at least their own size in
        // appends. The floor keeps a small store from snapshotting constantly.
        if self.journal_bytes > self.snapshot_bytes.max(Self::JOURNAL_COMPACTION_FLOOR_BYTES) {
            self.persist_records()?;
        }
        Ok(())
    }

    /// Replay the journal over the snapshot.
    ///
    /// Applied in order, so a later delete removes an earlier put, exactly as
    /// the operations happened. A journal that ends mid-record is truncated
    /// rather than rejected: a torn tail is a write that was interrupted, and
    /// the records before it are still the ones that were made.
    #[cfg(not(feature = "rocksdb-ssd"))]
    fn replay_journal(&mut self) -> Result<(), CacheError> {
        let path = self.journal_path();
        let Ok(raw) = fs::read(&path) else {
            self.journal_bytes = 0;
            return Ok(());
        };
        if !raw.starts_with(Self::JOURNAL_MAGIC) {
            if raw.is_empty() {
                self.journal_bytes = 0;
                return Ok(());
            }
            return Err(CacheError::CorruptBlock(
                "rocksdb compatibility journal has invalid magic".to_string(),
            ));
        }
        let mut offset = Self::JOURNAL_MAGIC.len();
        while offset < raw.len() {
            let tag = raw[offset];
            offset += 1;
            let Some((key_len, next)) = get_fixed_uint64(&raw, offset) else {
                break;
            };
            offset = next;
            let value_len = if tag == Self::JOURNAL_PUT {
                let Some((value_len, next)) = get_fixed_uint64(&raw, offset) else {
                    break;
                };
                offset = next;
                value_len as usize
            } else if tag == Self::JOURNAL_DELETE {
                0
            } else {
                return Err(CacheError::CorruptBlock(
                    "rocksdb compatibility journal has an unknown operation".to_string(),
                ));
            };
            let key_end = offset + key_len as usize;
            let value_end = key_end + value_len;
            if value_end > raw.len() {
                break;
            }
            let Ok(key) = std::str::from_utf8(&raw[offset..key_end]) else {
                return Err(CacheError::CorruptBlock(
                    "rocksdb compatibility journal key is not utf8".to_string(),
                ));
            };
            if tag == Self::JOURNAL_PUT {
                self.records
                    .insert(key.to_string(), raw[key_end..value_end].to_vec());
            } else {
                self.records.remove(key);
            }
            offset = value_end;
        }
        self.journal_bytes = raw.len() as u64;
        Ok(())
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn load_records(&mut self) -> Result<(), CacheError> {
        let store_path = self.store_path();
        if !store_path.exists() {
            // No snapshot yet does not mean no records: everything written
            // since the last compaction is in the journal, and before the
            // first compaction that is everything there is.
            self.records.clear();
            self.snapshot_bytes = 0;
            return self.replay_journal();
        }
        let raw = fs::read(store_path)?;
        // The other moment the snapshot's size is known without asking for it.
        self.snapshot_bytes = raw.len() as u64;
        if raw.len() < Self::STORE_MAGIC.len() || !raw.starts_with(Self::STORE_MAGIC) {
            return Err(CacheError::CorruptBlock(
                "rocksdb compatibility store has invalid magic".to_string(),
            ));
        }
        let mut offset = Self::STORE_MAGIC.len();
        let (count, next) = get_fixed_uint64(&raw, offset).ok_or_else(|| {
            CacheError::CorruptBlock("rocksdb compatibility store missing count".to_string())
        })?;
        offset = next;
        let mut records = HashMap::with_capacity(count.min(usize::MAX as u64) as usize);
        for _ in 0..count {
            let (key_len, next) = get_fixed_uint64(&raw, offset).ok_or_else(|| {
                CacheError::CorruptBlock("rocksdb compatibility store missing key len".to_string())
            })?;
            offset = next;
            let (value_len, next) = get_fixed_uint64(&raw, offset).ok_or_else(|| {
                CacheError::CorruptBlock(
                    "rocksdb compatibility store missing value len".to_string(),
                )
            })?;
            offset = next;
            let key_end = offset
                .checked_add(key_len as usize)
                .ok_or_else(|| CacheError::CorruptBlock("rocksdb key length overflow".into()))?;
            let key_bytes = raw
                .get(offset..key_end)
                .ok_or_else(|| CacheError::CorruptBlock("rocksdb key out of bounds".into()))?;
            let key = std::str::from_utf8(key_bytes)
                .map_err(|_| CacheError::CorruptBlock("rocksdb key is not utf8".into()))?
                .to_string();
            offset = key_end;
            let value_end = offset
                .checked_add(value_len as usize)
                .ok_or_else(|| CacheError::CorruptBlock("rocksdb value length overflow".into()))?;
            let value = raw
                .get(offset..value_end)
                .ok_or_else(|| CacheError::CorruptBlock("rocksdb value out of bounds".into()))?
                .to_vec();
            offset = value_end;
            records.insert(key, value);
        }
        if offset != raw.len() {
            return Err(CacheError::CorruptBlock(
                "rocksdb compatibility store has trailing bytes".to_string(),
            ));
        }
        self.records = records;
        self.replay_journal()
    }

    #[cfg(not(feature = "rocksdb-ssd"))]
    fn persist_records(&mut self) -> Result<(), CacheError> {
        let path = PathBuf::from(&self.db_path);
        fs::create_dir_all(&path)?;
        let mut raw = Vec::new();
        raw.extend_from_slice(Self::STORE_MAGIC);
        put_fixed_uint64(&mut raw, self.records.len() as u64);
        let mut keys = self.records.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        for key in keys {
            let value = self
                .records
                .get(&key)
                .expect("record key collected from map must exist");
            put_fixed_uint64(&mut raw, key.len() as u64);
            put_fixed_uint64(&mut raw, value.len() as u64);
            raw.extend_from_slice(key.as_bytes());
            raw.extend_from_slice(value);
        }
        // Written beside the store and renamed over it, so a crash leaves
        // either the previous snapshot or the new one, never half of either.
        let temporary = path.join(format!("{}.tmp.{}", Self::STORE_FILE_NAME, std::process::id()));
        let written = raw.len() as u64;
        fs::write(&temporary, raw)?;
        fs::rename(&temporary, self.store_path())?;
        self.snapshot_bytes = written;
        // Everything in the journal is now in the snapshot. Drop the handle
        // with the file: appending through it after this would write to an
        // unlinked inode, and those records would vanish at the next restart.
        self.journal_file = None;
        let _ = fs::remove_file(self.journal_path());
        self.journal_bytes = 0;
        Ok(())
    }

    pub fn path(&self) -> &str {
        &self.db_path
    }

    pub fn put_view(
        &mut self,
        key: &str,
        value: impl Into<Vec<u8>>,
    ) -> Result<StringViewBuffer, CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        let value = value.into();
        let mut view = StringViewBuffer::new(value.len());
        view.SetKey(key);
        #[cfg(feature = "rocksdb-ssd")]
        {
            self.rocksdb()?
                .put_opt(key.as_bytes(), &value, &Self::rocksdb_write_options())
                .map_err(|err| CacheError::RocksDb(err.to_string()))?;
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            self.records.insert(key.to_string(), value);
            let recorded = self
                .records
                .get(key)
                .expect("the record just inserted must be present")
                .clone();
            self.journal_change(key, Some(&recorded))?;
        }
        Ok(view)
    }
    pub fn get_batch(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut unique_keys = Vec::<String>::new();
        let mut unique_positions = HashMap::<String, usize>::new();
        let mut output_positions = Vec::with_capacity(keys.len());
        for key in keys {
            let position = if let Some(position) = unique_positions.get(key).copied() {
                position
            } else {
                let position = unique_keys.len();
                unique_positions.insert(key.clone(), position);
                unique_keys.push(key.clone());
                position
            };
            output_positions.push(position);
        }
        #[cfg(feature = "rocksdb-ssd")]
        let unique_values = {
            self.rocksdb()?
                .multi_get(unique_keys.iter().map(|key| key.as_bytes()))
                .into_iter()
                .map(|result| result.map_err(|err| CacheError::RocksDb(err.to_string())))
                .collect::<Result<Vec<_>, _>>()?
        };
        #[cfg(not(feature = "rocksdb-ssd"))]
        let unique_values = {
            unique_keys
                .iter()
                .map(|key| self.records.get(key).cloned())
                .collect::<Vec<_>>()
        };
        Ok(output_positions
            .into_iter()
            .map(|position| unique_values[position].clone())
            .collect())
    }

    pub fn put_batch(&mut self, entries: Vec<(String, Vec<u8>)>) -> Result<usize, CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        let count = entries.len();
        let mut coalesced = Vec::<(String, Vec<u8>)>::new();
        let mut coalesced_positions = HashMap::<String, usize>::new();
        for (key, value) in entries {
            if let Some(position) = coalesced_positions.get(&key).copied() {
                coalesced[position] = (key, value);
            } else {
                coalesced_positions.insert(key.clone(), coalesced.len());
                coalesced.push((key, value));
            }
        }
        #[cfg(feature = "rocksdb-ssd")]
        {
            let mut batch = rocksdb::WriteBatch::default();
            for (key, value) in coalesced {
                batch.put(key.as_bytes(), value);
            }
            self.rocksdb()?
                .write_opt(batch, &Self::rocksdb_write_options())
                .map_err(|err| CacheError::RocksDb(err.to_string()))?;
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            for (key, value) in coalesced {
                self.records.insert(key.clone(), value);
                let recorded = self
                    .records
                    .get(&key)
                    .expect("the record just inserted must be present")
                    .clone();
                self.journal_change(&key, Some(&recorded))?;
            }
        }
        Ok(count)
    }
    pub fn delete_batch(&mut self, keys: &[String]) -> Result<usize, CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        if keys.is_empty() {
            return Ok(0);
        }
        let mut unique_keys = Vec::<String>::new();
        let mut seen = HashSet::<String>::new();
        for key in keys {
            if seen.insert(key.clone()) {
                unique_keys.push(key.clone());
            }
        }

        let mut deleted = 0usize;
        #[cfg(feature = "rocksdb-ssd")]
        {
            let db = self.rocksdb()?;
            let existing = db
                .multi_get(unique_keys.iter().map(|key| key.as_bytes()))
                .into_iter()
                .map(|result| result.map_err(|err| CacheError::RocksDb(err.to_string())))
                .collect::<Result<Vec<_>, _>>()?;
            let mut batch = rocksdb::WriteBatch::default();
            for (key, value) in unique_keys.iter().zip(existing) {
                if value.is_some() {
                    batch.delete(key.as_bytes());
                    deleted = deleted.saturating_add(1);
                }
            }
            if deleted > 0 {
                db.write_opt(batch, &Self::rocksdb_write_options())
                    .map_err(|err| CacheError::RocksDb(err.to_string()))?;
            }
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            for key in &unique_keys {
                if self.records.remove(key).is_some() {
                    deleted = deleted.saturating_add(1);
                    self.journal_change(key, None)?;
                }
            }
        }
        Ok(deleted)
    }

    #[allow(non_snake_case)]
    pub fn DeleteBatch(&mut self, keys: &[String]) -> Result<usize, CacheError> {
        self.delete_batch(keys)
    }

    pub fn recover_view_data<C>(&mut self, callback: &mut C) -> Result<(), CacheError>
    where
        C: FnMut(&str, StringViewBuffer),
    {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        #[cfg(feature = "rocksdb-ssd")]
        {
            for item in self.rocksdb()?.iterator(rocksdb::IteratorMode::Start) {
                let (key, value) = item.map_err(|err| CacheError::RocksDb(err.to_string()))?;
                let key = str::from_utf8(&key)
                    .map_err(|_| CacheError::CorruptBlock("rocksdb key is not utf8".into()))?;
                let mut view = StringViewBuffer::new(value.len());
                view.SetKey(key);
                callback(key, view);
            }
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            let mut keys = self.records.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                let size = self.records.get(&key).map_or(0, Vec::len);
                let mut view = StringViewBuffer::new(size);
                view.SetKey(&key);
                callback(&key, view);
            }
        }
        self.recover_finished = true;
        Ok(())
    }

    pub fn is_data_recovered(&self) -> bool {
        self.recover_finished
    }

    #[allow(non_snake_case)]
    pub fn WithMetricRegistry(db_path: impl Into<String>, registry: ()) -> Self {
        Self::with_metric_registry(db_path, registry)
    }

    #[allow(non_snake_case)]
    pub fn Path(&self) -> &str {
        self.path()
    }

    #[allow(non_snake_case)]
    pub fn PutView(
        &mut self,
        key: &str,
        value: impl Into<Vec<u8>>,
    ) -> Result<StringViewBuffer, CacheError> {
        self.put_view(key, value)
    }

    #[allow(non_snake_case)]
    pub fn RecoverViewData<C>(&mut self, callback: &mut C) -> Result<(), CacheError>
    where
        C: FnMut(&str, StringViewBuffer),
    {
        self.recover_view_data(callback)
    }

    #[allow(non_snake_case)]
    pub fn IsDataRecovered(&self) -> bool {
        self.is_data_recovered()
    }
}

impl StorageEngineApi for StorageEngineRocksDb {
    fn start(&mut self) -> bool {
        let path = PathBuf::from(&self.db_path);
        if path.is_file() {
            return false;
        }
        if fs::create_dir_all(&path).is_err() {
            return false;
        }
        #[cfg(feature = "rocksdb-ssd")]
        {
            match self.open_rocksdb_arc() {
                Ok(db) => self.db = Some(db),
                Err(_) => return false,
            }
            self.records.clear();
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            if self.load_records().is_err() {
                return false;
            }
        }
        self.initialized = true;
        self.recover_finished = false;
        true
    }

    fn stop(&mut self) -> bool {
        #[cfg(feature = "rocksdb-ssd")]
        {
            self.db = None;
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            if self.initialized && self.persist_records().is_err() {
                return false;
            }
        }
        self.initialized = false;
        true
    }

    fn put(&mut self, key: &str, value: Vec<u8>) -> Result<CacheBuffer, CacheError> {
        let view = self.put_view(key, value.clone())?;
        Ok(view.into_cache_buffer_with_value(value))
    }

    fn async_put<F>(&mut self, buffer: CacheBuffer, cb: F) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>),
    {
        let key = buffer.Key().to_string();
        let value = buffer.to_vec();
        let result = self.put(&key, value);
        match result {
            Ok(buf) => {
                let mut cloned = CacheBuffer::new(buf.to_vec());
                cloned.SetKey(buf.Key());
                cb(Ok(cloned));
                Ok(buf)
            }
            Err(err) => {
                cb(Err(cache_error_for_callback(&err)));
                Err(err)
            }
        }
    }

    fn get(&self, key: &str) -> Result<CacheBuffer, CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        #[cfg(feature = "rocksdb-ssd")]
        let value = self
            .rocksdb()?
            .get(key.as_bytes())
            .map_err(|err| CacheError::RocksDb(err.to_string()))?
            .ok_or(CacheError::NotFound)?;
        #[cfg(not(feature = "rocksdb-ssd"))]
        let value = self.records.get(key).ok_or(CacheError::NotFound)?.clone();
        let mut buffer = CacheBuffer::new(value);
        buffer.SetKey(key);
        Ok(buffer)
    }

    fn peek(&self, key: &str) -> bool {
        if !self.initialized {
            return false;
        }
        #[cfg(feature = "rocksdb-ssd")]
        {
            self.rocksdb()
                .and_then(|db| {
                    db.get(key.as_bytes())
                        .map_err(|err| CacheError::RocksDb(err.to_string()))
                })
                .map(|value| value.is_some())
                .unwrap_or(false)
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            self.records.contains_key(key)
        }
    }

    fn delete_buffer(&mut self, buffer: &CacheBuffer) -> Result<(), CacheError> {
        self.delete(buffer.Key())
    }

    fn async_delete<F>(&mut self, buffer: &CacheBuffer, cb: F) -> Result<(), CacheError>
    where
        F: FnOnce(Result<(), CacheError>),
    {
        let result = self.delete_buffer(buffer);
        match result {
            Ok(()) => {
                cb(Ok(()));
                Ok(())
            }
            Err(err) => {
                cb(Err(cache_error_for_callback(&err)));
                Err(err)
            }
        }
    }

    fn delete(&mut self, key: &str) -> Result<(), CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        #[cfg(feature = "rocksdb-ssd")]
        {
            let db = self.rocksdb()?;
            if db
                .get(key.as_bytes())
                .map_err(|err| CacheError::RocksDb(err.to_string()))?
                .is_none()
            {
                return Err(CacheError::NotFound);
            }
            db.delete_opt(key.as_bytes(), &Self::rocksdb_write_options())
                .map_err(|err| CacheError::RocksDb(err.to_string()))?;
            Ok(())
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            self.records.remove(key).ok_or(CacheError::NotFound)?;
            self.journal_change(key, None)
        }
    }

    fn reset(&mut self) -> Result<(), CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        self.records.clear();
        self.recover_finished = false;
        #[cfg(feature = "rocksdb-ssd")]
        {
            let db = self.rocksdb()?;
            let keys = db
                .iterator(rocksdb::IteratorMode::Start)
                .map(|item| item.map(|(key, _)| key.to_vec()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| CacheError::RocksDb(err.to_string()))?;
            if keys.is_empty() {
                return Ok(());
            }
            let mut batch = rocksdb::WriteBatch::default();
            for key in keys {
                batch.delete(key);
            }
            db.write_opt(batch, &Self::rocksdb_write_options())
                .map_err(|err| CacheError::RocksDb(err.to_string()))?;
            Ok(())
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            self.persist_records()
        }
    }

    fn recover_data<C: RecoverDataCallback>(&self, callback: &mut C) -> Result<(), CacheError> {
        if !self.initialized {
            return Err(CacheError::Stopped);
        }
        #[cfg(feature = "rocksdb-ssd")]
        {
            for item in self.rocksdb()?.iterator(rocksdb::IteratorMode::Start) {
                let (key, value) = item.map_err(|err| CacheError::RocksDb(err.to_string()))?;
                let key = str::from_utf8(&key)
                    .map_err(|_| CacheError::CorruptBlock("rocksdb key is not utf8".into()))?;
                let mut buffer = CacheBuffer::new(value.to_vec());
                buffer.set_key(key);
                callback.on_recover_data(key, buffer);
            }
        }
        #[cfg(not(feature = "rocksdb-ssd"))]
        {
            let mut keys = self.records.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                let value = self.records.get(&key).ok_or(CacheError::NotFound)?;
                let mut buffer = CacheBuffer::new(value.clone());
                buffer.set_key(&key);
                callback.on_recover_data(&key, buffer);
            }
        }
        Ok(())
    }

    fn set_capacity(&mut self, capacity: u64) {
        self.capacity = capacity;
    }

    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn is_started(&self) -> bool {
        self.initialized
    }

    fn storage_engine_type(&self) -> StorageEngineKind {
        StorageEngineKind::Ssd
    }
}

#[allow(non_snake_case)]
impl StorageEngineRocksDb {
    pub fn Start(&mut self) -> bool {
        self.start()
    }

    pub fn Stop(&mut self) -> bool {
        self.stop()
    }

    pub fn Put(&mut self, key: &str, value: Vec<u8>) -> Result<CacheBuffer, CacheError> {
        self.put(key, value)
    }

    pub fn AsyncPut<F>(&mut self, buffer: CacheBuffer, cb: F) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>),
    {
        self.async_put(buffer, cb)
    }

    pub fn Get(&self, key: &str) -> Result<CacheBuffer, CacheError> {
        self.get(key)
    }

    pub fn Peek(&self, key: &str) -> bool {
        self.peek(key)
    }

    pub fn Delete(&mut self, key: &str) -> Result<(), CacheError> {
        self.delete(key)
    }

    pub fn DeleteBuffer(&mut self, buffer: &CacheBuffer) -> Result<(), CacheError> {
        self.delete_buffer(buffer)
    }

    pub fn AsyncDelete<F>(&mut self, buffer: &CacheBuffer, cb: F) -> Result<(), CacheError>
    where
        F: FnOnce(Result<(), CacheError>),
    {
        self.async_delete(buffer, cb)
    }

    pub fn Reset(&mut self) -> Result<(), CacheError> {
        self.reset()
    }

    pub fn RecoverData<C: RecoverDataCallback>(&self, callback: &mut C) -> Result<(), CacheError> {
        self.recover_data(callback)
    }

    pub fn SetCapacity(&mut self, capacity: u64) {
        self.set_capacity(capacity);
    }

    pub fn Capacity(&self) -> u64 {
        self.capacity()
    }

    pub fn StorageEngineType(&self) -> StorageEngineKind {
        self.storage_engine_type()
    }

    pub fn SsdBackendName(&self) -> &'static str {
        self.ssd_backend_name()
    }
}

pub type StorageEngineSsd = StorageEngineRocksDb;

#[derive(Debug, Clone)]
/// Several SSD devices behind one engine, one chosen per key by hash.
///
/// Each device is a [`StorageEngineRocksDb`] over its own path. **The capacity is
/// per device, not shared**: setting it applies the same figure to every device,
/// so total capacity is the capacity times the device count -- matching the
/// reference, whose field carries the same caveat.
///
/// Devices can be added and removed while running. Removing one drops the data
/// it held, and because placement is by hash of the key, changing the device set
/// changes where subsequent keys land.
pub struct StorageEngineMultiSsd {
    paths: Vec<String>,
    storages: Vec<StorageEngineRocksDb>,
    capacity: u64,
    initialized: bool,
    ssdcache_type: StorageEngineKind,
}

impl StorageEngineMultiSsd {
    pub fn new(paths: impl IntoIterator<Item = String>, capacity: u64) -> Self {
        Self::with_type(paths, capacity, StorageEngineKind::Ssd)
    }

    pub fn with_paths(paths: impl IntoIterator<Item = PathBuf>, capacity: u64) -> Self {
        Self::new(
            paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string()),
            capacity,
        )
    }

    pub fn with_type(
        paths: impl IntoIterator<Item = String>,
        capacity: u64,
        ssdcache_type: StorageEngineKind,
    ) -> Self {
        Self {
            paths: paths.into_iter().collect(),
            storages: Vec::new(),
            capacity,
            initialized: false,
            ssdcache_type,
        }
    }

    fn init(&mut self) -> bool {
        if self.paths.is_empty() {
            return false;
        }
        self.storages.clear();
        for path in &self.paths {
            let mut storage = self.create_storage_by_device_path(path);
            if !storage.Start() {
                self.storages.clear();
                return false;
            }
            self.storages.push(storage);
        }
        true
    }

    fn create_storage_by_device_path(&self, path: &str) -> StorageEngineRocksDb {
        let mut storage = StorageEngineRocksDb::new(path.to_string());
        if self.capacity != 0 {
            storage.SetCapacity(self.capacity);
        }
        storage
    }

    /// Hash used to spread keys across devices.
    ///
    /// This is the same hash the rest of the crate uses, so the device a key
    /// lands on is reproducible: a data directory written by one process is
    /// read back through the same device selection by another.
    fn hash(key: &str) -> u32 {
        mur_mur_hash2(key.as_bytes())
    }

    /// How many devices keys are spread across. Before start there are no
    /// storages yet, so the configured paths stand in; `init` rebuilds the
    /// storages from the paths one for one, so the two agree once started.
    fn shard_count(&self) -> usize {
        if self.initialized && !self.storages.is_empty() {
            self.storages.len()
        } else {
            self.paths.len()
        }
    }

    fn storage_index(&self, key: &str) -> Result<usize, CacheError> {
        if !self.initialized || self.storages.is_empty() {
            return Err(CacheError::Stopped);
        }
        Ok(Self::hash(key) as usize % self.storages.len())
    }

    pub fn add_device(&mut self, path: impl Into<String>) -> bool {
        let path = path.into();
        if self.paths.iter().any(|candidate| candidate == &path) {
            return false;
        }
        let mut storage = self.create_storage_by_device_path(&path);
        if self.initialized && !storage.Start() {
            return false;
        }
        self.paths.push(path);
        if self.initialized {
            self.storages.push(storage);
        }
        true
    }

    pub fn remove_device(&mut self, path: &str) -> bool {
        let Some(index) = self.paths.iter().position(|candidate| candidate == path) else {
            return false;
        };
        self.paths.remove(index);
        if self.initialized && index < self.storages.len() {
            self.storages.remove(index);
        }
        true
    }

    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    pub fn storage_count(&self) -> usize {
        self.storages.len()
    }

    /// Path of the device that holds `key`, using the same selection as reads
    /// and writes so the answer matches where the data actually goes.
    pub fn device_for_key(&self, key: &str) -> Option<&str> {
        let count = self.shard_count();
        if count == 0 {
            return None;
        }
        let index = Self::hash(key) as usize % count;
        self.paths.get(index).map(String::as_str)
    }

    pub fn ssdcache_type(&self) -> StorageEngineKind {
        self.ssdcache_type
    }

    #[cfg(feature = "rocksdb-ssd")]
    pub fn recover_view_data<C>(&mut self, callback: &mut C) -> Result<(), CacheError>
    where
        C: FnMut(&str, StringViewBuffer),
    {
        if !self.initialized || self.storages.is_empty() {
            return Err(CacheError::Stopped);
        }
        for storage in &mut self.storages {
            storage.recover_view_data(callback)?;
        }
        Ok(())
    }

    #[allow(non_snake_case)]
    pub fn AddDevice(&mut self, path: &str) -> bool {
        self.add_device(path)
    }

    #[allow(non_snake_case)]
    pub fn RemoveDevice(&mut self, path: &str) -> bool {
        self.remove_device(path)
    }

    pub fn path_count(&self) -> usize {
        self.paths.len()
    }

    #[allow(non_snake_case)]
    pub fn PathCount(&self) -> usize {
        self.path_count()
    }

    #[allow(non_snake_case)]
    pub fn StorageCount(&self) -> usize {
        self.storage_count()
    }
}

impl StorageEngineApi for StorageEngineMultiSsd {
    fn start(&mut self) -> bool {
        if self.initialized {
            return true;
        }
        if !self.init() {
            return false;
        }
        self.initialized = true;
        true
    }

    fn stop(&mut self) -> bool {
        for storage in &mut self.storages {
            storage.Stop();
        }
        self.initialized = false;
        true
    }

    fn put(&mut self, key: &str, value: Vec<u8>) -> Result<CacheBuffer, CacheError> {
        let index = self.storage_index(key)?;
        self.storages[index].Put(key, value)
    }

    fn async_put<F>(&mut self, buffer: CacheBuffer, cb: F) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>),
    {
        let key = buffer.key().to_string();
        let value = buffer.to_vec();
        let result = self.put(&key, value);
        match result {
            Ok(buf) => {
                let mut cloned = CacheBuffer::new(buf.to_vec());
                cloned.set_key(buf.key());
                cb(Ok(cloned));
                Ok(buf)
            }
            Err(err) => {
                cb(Err(cache_error_for_callback(&err)));
                Err(err)
            }
        }
    }

    fn get(&self, key: &str) -> Result<CacheBuffer, CacheError> {
        let index = self.storage_index(key)?;
        self.storages[index].Get(key)
    }

    fn peek(&self, key: &str) -> bool {
        let Ok(index) = self.storage_index(key) else {
            return false;
        };
        self.storages[index].Peek(key)
    }

    fn delete_buffer(&mut self, buffer: &CacheBuffer) -> Result<(), CacheError> {
        self.delete(buffer.key())
    }

    fn async_delete<F>(&mut self, buffer: &CacheBuffer, cb: F) -> Result<(), CacheError>
    where
        F: FnOnce(Result<(), CacheError>),
    {
        let result = self.delete_buffer(buffer);
        match result {
            Ok(()) => {
                cb(Ok(()));
                Ok(())
            }
            Err(err) => {
                cb(Err(cache_error_for_callback(&err)));
                Err(err)
            }
        }
    }

    fn delete(&mut self, key: &str) -> Result<(), CacheError> {
        let index = self.storage_index(key)?;
        self.storages[index].Delete(key)
    }

    fn reset(&mut self) -> Result<(), CacheError> {
        if !self.initialized || self.storages.is_empty() {
            return Err(CacheError::Stopped);
        }
        for storage in &mut self.storages {
            storage.Reset()?;
        }
        Ok(())
    }

    fn recover_data<C: RecoverDataCallback>(&self, callback: &mut C) -> Result<(), CacheError> {
        if !self.initialized || self.storages.is_empty() {
            return Err(CacheError::Stopped);
        }
        for storage in &self.storages {
            storage.RecoverData(callback)?;
        }
        Ok(())
    }

    fn set_capacity(&mut self, capacity: u64) {
        self.capacity = capacity;
        for storage in &mut self.storages {
            storage.SetCapacity(capacity);
        }
    }

    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn is_started(&self) -> bool {
        self.initialized
    }

    fn storage_engine_type(&self) -> StorageEngineKind {
        StorageEngineKind::MultiSsd
    }
}

#[allow(non_snake_case)]
impl StorageEngineMultiSsd {
    pub fn Start(&mut self) -> bool {
        self.start()
    }

    pub fn Stop(&mut self) -> bool {
        self.stop()
    }

    pub fn Put(&mut self, key: &str, value: Vec<u8>) -> Result<CacheBuffer, CacheError> {
        self.put(key, value)
    }

    pub fn AsyncPut<F>(&mut self, buffer: CacheBuffer, cb: F) -> Result<CacheBuffer, CacheError>
    where
        F: FnOnce(Result<CacheBuffer, CacheError>),
    {
        self.async_put(buffer, cb)
    }

    pub fn Get(&self, key: &str) -> Result<CacheBuffer, CacheError> {
        self.get(key)
    }

    pub fn Peek(&self, key: &str) -> bool {
        self.peek(key)
    }

    pub fn DeleteBuffer(&mut self, buffer: &CacheBuffer) -> Result<(), CacheError> {
        self.delete_buffer(buffer)
    }

    pub fn AsyncDelete<F>(&mut self, buffer: &CacheBuffer, cb: F) -> Result<(), CacheError>
    where
        F: FnOnce(Result<(), CacheError>),
    {
        self.async_delete(buffer, cb)
    }

    pub fn Delete(&mut self, key: &str) -> Result<(), CacheError> {
        self.delete(key)
    }

    pub fn Reset(&mut self) -> Result<(), CacheError> {
        self.reset()
    }

    pub fn RecoverData<C: RecoverDataCallback>(&self, callback: &mut C) -> Result<(), CacheError> {
        self.recover_data(callback)
    }

    pub fn SetCapacity(&mut self, capacity: u64) {
        self.set_capacity(capacity);
    }

    pub fn Capacity(&self) -> u64 {
        self.capacity()
    }

    pub fn StorageEngineType(&self) -> StorageEngineKind {
        self.storage_engine_type()
    }
}

fn cache_error_for_callback(err: &CacheError) -> CacheError {
    match err {
        CacheError::Io(io_err) => CacheError::CorruptBlock(io_err.to_string()),
        CacheError::Stopped => CacheError::Stopped,
        CacheError::AlreadyStarted => CacheError::AlreadyStarted,
        CacheError::NotFound => CacheError::NotFound,
        CacheError::ReplaceMismatch => CacheError::ReplaceMismatch,
        CacheError::UnsupportedTier(tier) => CacheError::UnsupportedTier(*tier),
        CacheError::UnsupportedInstance(instance) => CacheError::UnsupportedInstance(*instance),
        CacheError::CorruptBlock(message) => CacheError::CorruptBlock(message.clone()),
        CacheError::UnsupportedCodec(codec) => CacheError::UnsupportedCodec(*codec),
        CacheError::CapacityExceeded => CacheError::CapacityExceeded,
        CacheError::InvalidConfig(message) => CacheError::InvalidConfig(message.clone()),
        CacheError::RocksDb(message) => CacheError::RocksDb(message.clone()),
    }
}

type AsyncWriteResult = Result<AllocatorAddress, CacheError>;
type AsyncWriteFunc =
    Box<dyn FnOnce(&mut SimpleLogBasedMemoryAllocator) -> AsyncWriteResult + Send + 'static>;
type AsyncWriteCallback = Box<
    dyn FnOnce(AsyncWriteResult, &SimpleLogBasedMemoryAllocator) -> Result<CacheBuffer, CacheError>
        + Send
        + 'static,
>;

pub struct AsyncWriteTask {
    write_func: Option<AsyncWriteFunc>,
    callback_func: Option<AsyncWriteCallback>,
    addr: Option<AllocatorAddress>,
}

impl AsyncWriteTask {
    pub fn new<W, C>(write_func: W, callback_func: C) -> Self
    where
        W: FnOnce(&mut SimpleLogBasedMemoryAllocator) -> AsyncWriteResult + Send + 'static,
        C: FnOnce(
                AsyncWriteResult,
                &SimpleLogBasedMemoryAllocator,
            ) -> Result<CacheBuffer, CacheError>
            + Send
            + 'static,
    {
        Self {
            write_func: Some(Box::new(write_func)),
            callback_func: Some(Box::new(callback_func)),
            addr: None,
        }
    }

    pub fn with_addr<W, C>(write_func: W, callback_func: C, addr: AllocatorAddress) -> Self
    where
        W: FnOnce(&mut SimpleLogBasedMemoryAllocator) -> AsyncWriteResult + Send + 'static,
        C: FnOnce(
                AsyncWriteResult,
                &SimpleLogBasedMemoryAllocator,
            ) -> Result<CacheBuffer, CacheError>
            + Send
            + 'static,
    {
        Self {
            write_func: Some(Box::new(write_func)),
            callback_func: Some(Box::new(callback_func)),
            addr: Some(addr),
        }
    }

    pub fn addr(&self) -> Option<AllocatorAddress> {
        self.addr
    }

    #[allow(non_snake_case)]
    pub fn Addr(&self) -> Option<AllocatorAddress> {
        self.addr()
    }
}

pub struct AsyncWriter {
    allocator: SimpleLogBasedMemoryAllocator,
    stopped: bool,
    in_flight_write_count: u64,
    in_flight_callback_count: u64,
    completed_write_count: u64,
    completed_callback_count: u64,
}

impl AsyncWriter {
    pub fn new(allocator: SimpleLogBasedMemoryAllocator) -> Self {
        Self {
            allocator,
            stopped: false,
            in_flight_write_count: 0,
            in_flight_callback_count: 0,
            completed_write_count: 0,
            completed_callback_count: 0,
        }
    }

    pub fn async_write(&mut self, mut task: AsyncWriteTask) -> Result<CacheBuffer, CacheError> {
        if self.stopped {
            return Err(CacheError::Stopped);
        }
        let write = task
            .write_func
            .take()
            .ok_or_else(|| CacheError::CorruptBlock("missing async write function".to_string()))?;
        let callback = task
            .callback_func
            .take()
            .ok_or_else(|| CacheError::CorruptBlock("missing async write callback".to_string()))?;

        self.in_flight_write_count = self.in_flight_write_count.saturating_add(1);
        let write_result = write(&mut self.allocator);
        self.in_flight_write_count = self.in_flight_write_count.saturating_sub(1);
        self.completed_write_count = self.completed_write_count.saturating_add(1);

        self.in_flight_callback_count = self.in_flight_callback_count.saturating_add(1);
        let callback_result = callback(write_result, &self.allocator);
        self.in_flight_callback_count = self.in_flight_callback_count.saturating_sub(1);
        self.completed_callback_count = self.completed_callback_count.saturating_add(1);
        callback_result
    }

    // Completion barrier: `in_flight_write_count`/`in_flight_callback_count` are maintained by the write
    // executor's submit/complete accounting, not mutated in this spin body.
    #[allow(clippy::while_immutable_condition)]
    pub fn stop(&mut self) {
        while self.in_flight_write_count != 0 || self.in_flight_callback_count != 0 {
            std::thread::yield_now();
        }
        self.stopped = true;
    }

    #[allow(clippy::while_immutable_condition)]
    pub fn test_join_write_executor(&mut self) {
        while self.in_flight_write_count != 0 {
            std::thread::yield_now();
        }
    }

    pub fn allocator(&self) -> &SimpleLogBasedMemoryAllocator {
        &self.allocator
    }

    pub fn allocator_mut(&mut self) -> &mut SimpleLogBasedMemoryAllocator {
        &mut self.allocator
    }

    pub fn in_flight_writes(&self) -> u64 {
        self.in_flight_write_count
    }

    pub fn in_flight_callbacks(&self) -> u64 {
        self.in_flight_callback_count
    }

    pub fn completed_writes(&self) -> u64 {
        self.completed_write_count
    }

    pub fn completed_callbacks(&self) -> u64 {
        self.completed_callback_count
    }

    #[allow(non_snake_case)]
    pub fn AsyncWrite(&mut self, task: AsyncWriteTask) -> Result<CacheBuffer, CacheError> {
        self.async_write(task)
    }

    #[allow(non_snake_case)]
    pub fn Stop(&mut self) {
        self.stop();
    }

    #[allow(non_snake_case)]
    pub fn TEST_JoinWriteExecutor(&mut self) {
        self.test_join_write_executor();
    }

    #[allow(non_snake_case)]
    pub fn FlyWriteNum(&self) -> u64 {
        self.in_flight_writes()
    }

    #[allow(non_snake_case)]
    pub fn FlyCbNum(&self) -> u64 {
        self.in_flight_callbacks()
    }
}

pub struct PmemDispatcher {
    allocator_kind: AllocatorKind,
    writers: Vec<AsyncWriter>,
    current_numa: usize,
    stopped: bool,
}

impl PmemDispatcher {
    pub fn new(numa_count: usize, allocator_capacity: usize) -> Self {
        let numa_count = numa_count.max(1);
        let writers = (0..numa_count)
            .map(|numa_id| {
                let base_ptr = ((numa_id + 1) as AllocatorAddress) << 48;
                AsyncWriter::new(SimpleLogBasedMemoryAllocator::with_capacity_and_base(
                    allocator_capacity,
                    base_ptr,
                ))
            })
            .collect();
        Self {
            allocator_kind: AllocatorKind::LogBasedAllocator,
            writers,
            current_numa: 0,
            stopped: true,
        }
    }

    pub fn from_allocators(allocators: Vec<SimpleLogBasedMemoryAllocator>) -> Self {
        let writers = allocators.into_iter().map(AsyncWriter::new).collect();
        Self {
            allocator_kind: AllocatorKind::LogBasedAllocator,
            writers,
            current_numa: 0,
            stopped: true,
        }
    }

    pub fn start(&mut self) -> bool {
        self.stopped = false;
        true
    }

    pub fn stop(&mut self) -> bool {
        self.stopped = true;
        for writer in &mut self.writers {
            writer.Stop();
        }
        true
    }

    pub fn push_task(&mut self, task: AsyncWriteTask) -> Result<CacheBuffer, CacheError> {
        if self.stopped {
            return Err(CacheError::Stopped);
        }
        if self.writers.is_empty() {
            return Err(CacheError::CorruptBlock(
                "pmem dispatcher has no writers".to_string(),
            ));
        }
        let numa_id = match task.addr() {
            Some(addr) => self
                .get_numa_id_by_pmem_addr(addr)
                .ok_or_else(|| CacheError::CorruptBlock(format!("invalid pmem addr {addr}")))?,
            None => self.next_round_robin_numa(),
        };
        self.writers[numa_id].AsyncWrite(task)
    }

    fn next_round_robin_numa(&mut self) -> usize {
        let numa_id = self.current_numa % self.writers.len();
        self.current_numa = self.current_numa.wrapping_add(1);
        numa_id
    }

    pub fn get_numa_id_by_pmem_addr(&self, addr: AllocatorAddress) -> Option<usize> {
        self.writers
            .iter()
            .position(|writer| writer.allocator().Contains(addr))
    }

    pub fn get_allocator(
        &mut self,
        addr: Option<AllocatorAddress>,
    ) -> Option<&mut SimpleLogBasedMemoryAllocator> {
        if self.writers.is_empty() {
            return None;
        }
        let numa_id = match addr {
            Some(addr) => self.get_numa_id_by_pmem_addr(addr)?,
            None => self.next_round_robin_numa(),
        };
        Some(self.writers[numa_id].allocator_mut())
    }

    pub fn allocators(&self) -> Vec<&SimpleLogBasedMemoryAllocator> {
        self.writers
            .iter()
            .map(|writer| writer.allocator())
            .collect()
    }

    pub fn writers(&self) -> &[AsyncWriter] {
        &self.writers
    }

    pub fn test_get_allocator(&self, numa_id: usize) -> Option<&SimpleLogBasedMemoryAllocator> {
        self.writers.get(numa_id).map(|writer| writer.allocator())
    }

    pub fn test_get_writer(&self, numa_id: usize) -> Option<&AsyncWriter> {
        self.writers.get(numa_id)
    }

    pub fn test_join_pmem_write_executor(&mut self) {
        for writer in &mut self.writers {
            writer.TEST_JoinWriteExecutor();
        }
    }

    pub fn test_put_to_numa(
        &mut self,
        numa_id: usize,
        key: impl Into<String>,
        value: impl Into<Vec<u8>>,
    ) -> Result<CacheBuffer, CacheError> {
        if self.stopped {
            return Err(CacheError::Stopped);
        }
        let key = key.into();
        let value = value.into();
        let write_value = value.clone();
        let task = AsyncWriteTask::new(
            move |allocator| {
                let ptr = allocator.Allocate(write_value.len())?;
                allocator.write(ptr, &write_value)?;
                allocator.Seal(ptr)?;
                Ok(ptr)
            },
            move |write_result, allocator| {
                let ptr = write_result?;
                let mut buffer = CacheBuffer::new(allocator.read(ptr)?.to_vec());
                buffer.SetKey(&key);
                Ok(buffer)
            },
        );
        self.writers
            .get_mut(numa_id)
            .ok_or_else(|| CacheError::CorruptBlock(format!("invalid numa id {numa_id}")))?
            .AsyncWrite(task)
    }

    pub fn numa_count(&self) -> usize {
        self.writers.len()
    }

    pub fn allocator_count(&self) -> usize {
        self.writers.len()
    }

    pub fn writer_count(&self) -> usize {
        self.writers.len()
    }

    pub fn allocator_type(&self) -> AllocatorKind {
        self.allocator_kind
    }

    #[allow(non_snake_case)]
    pub fn Start(&mut self) -> bool {
        self.start()
    }

    #[allow(non_snake_case)]
    pub fn Stop(&mut self) -> bool {
        self.stop()
    }

    #[allow(non_snake_case)]
    pub fn PushTask(&mut self, task: AsyncWriteTask) -> Result<CacheBuffer, CacheError> {
        self.push_task(task)
    }

    #[allow(non_snake_case)]
    pub fn GetAllocator(
        &mut self,
        addr: Option<AllocatorAddress>,
    ) -> Option<&mut SimpleLogBasedMemoryAllocator> {
        self.get_allocator(addr)
    }

    #[allow(non_snake_case)]
    pub fn GetAllocators(&self) -> Vec<&SimpleLogBasedMemoryAllocator> {
        self.allocators()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetAllocator(&self, numa_id: usize) -> Option<&SimpleLogBasedMemoryAllocator> {
        self.test_get_allocator(numa_id)
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetWriter(&self, numa_id: usize) -> Option<&AsyncWriter> {
        self.test_get_writer(numa_id)
    }

    #[allow(non_snake_case)]
    pub fn TEST_JoinPmemWriteExecutor(&mut self) {
        self.test_join_pmem_write_executor();
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringViewBuffer {
    key: String,
    size: usize,
}

impl StringViewBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            key: String::new(),
            size,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn set_key(&mut self, key: impl Into<String>) {
        self.key = key.into();
    }

    pub fn data(&self) -> Option<&[u8]> {
        None
    }

    pub fn data_ptr(&self) -> *const u8 {
        std::ptr::null()
    }

    pub fn value(&self) -> Option<&[u8]> {
        self.data()
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn into_cache_buffer_with_value(self, value: impl Into<Vec<u8>>) -> CacheBuffer {
        let mut buffer = CacheBuffer::new(value.into());
        buffer.set_key(self.key);
        buffer
    }

    #[allow(non_snake_case)]
    pub fn Key(&self) -> &str {
        self.key()
    }

    #[allow(non_snake_case)]
    pub fn SetKey(&mut self, key: impl Into<String>) {
        self.set_key(key);
    }

    #[allow(non_snake_case)]
    pub fn Data(&self) -> Option<&[u8]> {
        self.data()
    }

    #[allow(non_snake_case)]
    pub fn DataPtr(&self) -> *const u8 {
        self.data_ptr()
    }

    #[allow(non_snake_case)]
    pub fn Value(&self) -> Option<&[u8]> {
        self.value()
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }
}

