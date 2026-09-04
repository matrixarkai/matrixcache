// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumaTopology {
    num_cores: usize,
    max_num_cores: usize,
    max_num_numa_nodes: usize,
    core_to_numa_node: Vec<usize>,
    numa_node_to_cores: Vec<Vec<usize>>,
    numa_node_core_index: Vec<usize>,
}

impl NumaTopology {
    fn detect() -> Self {
        let cores = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .max(1);
        let core_to_numa_node = vec![0; cores];
        let numa_node_to_cores = vec![(0..cores).collect::<Vec<_>>()];
        let numa_node_core_index = (0..cores).collect::<Vec<_>>();
        Self {
            num_cores: cores,
            max_num_cores: cores,
            max_num_numa_nodes: 1,
            core_to_numa_node,
            numa_node_to_cores,
            numa_node_core_index,
        }
    }
}

fn numa_topology() -> &'static NumaTopology {
    static TOPOLOGY: OnceLock<NumaTopology> = OnceLock::new();
    TOPOLOGY.get_or_init(NumaTopology::detect)
}

pub struct NumaInfo;

impl NumaInfo {
    pub fn init() {
        let _ = numa_topology();
    }

    pub fn num_all_cores() -> usize {
        numa_topology().num_cores
    }

    pub fn num_online_cores() -> usize {
        numa_topology().max_num_cores
    }

    pub fn current_cpu_core() -> usize {
        0
    }

    pub fn max_num_numa_nodes() -> usize {
        numa_topology().max_num_numa_nodes
    }

    pub fn get_numa_node_of_cpu_core(core: usize) -> usize {
        let topology = numa_topology();
        topology.core_to_numa_node.get(core).copied().unwrap_or(0)
    }

    pub fn get_cpu_cores_of_numa_node(node: usize) -> Vec<usize> {
        numa_topology()
            .numa_node_to_cores
            .get(node)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_cpu_cores_of_same_numa_node(core: usize) -> Vec<usize> {
        Self::get_cpu_cores_of_numa_node(Self::get_numa_node_of_cpu_core(core))
    }

    pub fn get_numa_node_core_index(core: usize) -> usize {
        numa_topology()
            .numa_node_core_index
            .get(core)
            .copied()
            .unwrap_or(0)
    }

    pub fn bind_thread_to_cpu_core(core: usize) -> Result<(), CacheError> {
        if core < Self::num_online_cores() {
            Ok(())
        } else {
            Err(CacheError::CorruptBlock(format!(
                "cpu core {core} is outside online core range"
            )))
        }
    }

    #[allow(non_snake_case)]
    pub fn Init() {
        Self::init();
    }

    #[allow(non_snake_case)]
    pub fn GetNumAllCores() -> usize {
        Self::num_all_cores()
    }

    #[allow(non_snake_case)]
    pub fn GetNumOnlineCores() -> usize {
        Self::num_online_cores()
    }

    #[allow(non_snake_case)]
    pub fn GetCurrentCpuCore() -> usize {
        Self::current_cpu_core()
    }

    #[allow(non_snake_case)]
    pub fn GetMaxNumNumaNodes() -> usize {
        Self::max_num_numa_nodes()
    }

    #[allow(non_snake_case)]
    pub fn GetNumaNodeOfCpuCore(core: usize) -> usize {
        Self::get_numa_node_of_cpu_core(core)
    }

    #[allow(non_snake_case)]
    pub fn GetCpuCoresOfNumaNode(node: usize) -> Vec<usize> {
        Self::get_cpu_cores_of_numa_node(node)
    }

    #[allow(non_snake_case)]
    pub fn GetCpuCoresOfSameNumaNode(core: usize) -> Vec<usize> {
        Self::get_cpu_cores_of_same_numa_node(core)
    }

    #[allow(non_snake_case)]
    pub fn GetNumaNodeCoreIdx(core: usize) -> usize {
        Self::get_numa_node_core_index(core)
    }

    #[allow(non_snake_case)]
    pub fn BindThreadToCpuCore(core: usize) -> Result<(), CacheError> {
        Self::bind_thread_to_cpu_core(core)
    }
}

/// A key, in the four parts this cache indexes by.
///
/// `shard_id` selects the shard, and `namespace`, `record_key` and `selector`
/// identify the value within it. The constructors build the common shapes --
/// [`CacheKey::string`] for a plain key, [`CacheKey::page`] and
/// [`CacheKey::page_with_slot`] for paged records, [`CacheKey::feature_query`]
/// and [`CacheKey::set_members`] for the query forms.
///
/// Not to be confused with `PolicyKey`, which is the plain `String` the
/// replacement policies index by. The two are not interchangeable.
///
/// # Examples
///
/// ```
/// use matrixcache::CacheKey;
///
/// let plain = CacheKey::string(0, "greeting");
/// assert_eq!(plain, CacheKey::string(0, "greeting"));
///
/// // The same record key in a different shard is a different key.
/// assert_ne!(plain, CacheKey::string(1, "greeting"));
///
/// // Paged records carry their segment, offset and length.
/// let page = CacheKey::page(0, 7, 4096, 512);
/// assert_ne!(page, plain);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    pub shard_id: ShardId,
    pub record_key: String,
    /// Always one of a handful of literals in practice -- `page`, `hash`, `string`, `set`,
    /// `feature`. A `Cow` holds those without allocating, and still holds an owned String for the
    /// one caller that produces one: `decode_line`, reading a namespace back out of a manifest.
    ///
    /// Serde writes a `Cow<str>` exactly as it writes a `String`, and `Ord`/`Hash`/`Eq` work on the
    /// `str` either way, so the manifest format, the map lookups and the ordering are unchanged.
    pub namespace: std::borrow::Cow<'static, str>,
    pub selector: String,
}

/// `segment-00000000000000000008`, built directly.
///
/// `format!` allocates twice here: once for the returned String and once inside the padded-integer
/// path. Writing into a String sized up front is one allocation for the same bytes. This runs on
/// every page cache get and put, so the second allocation is pure overhead on the hot read path.
fn segment_record_key(page_segment_id: u64) -> String {
    use std::fmt::Write as _;
    let mut key = String::with_capacity(SEGMENT_PREFIX.len() + 20);
    key.push_str(SEGMENT_PREFIX);
    let _ = write!(key, "{page_segment_id:020}");
    key
}

const SEGMENT_PREFIX: &str = "segment-";

/// The page selector, built directly for the same reason as [`segment_record_key`].
///
/// Four shapes, one per combination of routing slot and generation, kept byte for byte as the
/// `format!` calls they replace -- these strings are hashed and compared against keys already
/// written, so a changed byte is a silent cache miss, not a test failure.
fn page_selector(
    routing_slot: Option<u32>,
    generation: Option<u64>,
    offset: u64,
    length: u64,
) -> String {
    use std::fmt::Write as _;
    // "slot-" + u32 + ":gen-" + u64 + ":" + u64 + ":" + u64, generously.
    let mut selector = String::with_capacity(64);
    if let Some(slot) = routing_slot {
        selector.push_str("slot-");
        let _ = write!(selector, "{slot}:");
    }
    if let Some(generation) = generation {
        selector.push_str("gen-");
        let _ = write!(selector, "{generation}:");
    }
    let _ = write!(selector, "{offset}:{length}");
    selector
}

impl CacheKey {
    pub fn string(shard_id: ShardId, key: &str) -> Self {
        Self {
            shard_id,
            record_key: key.to_string(),
            namespace: std::borrow::Cow::Borrowed("string"),
            selector: "value".to_string(),
        }
    }

    pub fn hash(shard_id: ShardId, key: &str, field: &str) -> Self {
        Self {
            shard_id,
            record_key: key.to_string(),
            namespace: std::borrow::Cow::Borrowed("hash"),
            selector: field.to_string(),
        }
    }

    pub fn set_members(shard_id: ShardId, key: &str) -> Self {
        Self {
            shard_id,
            record_key: key.to_string(),
            namespace: std::borrow::Cow::Borrowed("set"),
            selector: "members".to_string(),
        }
    }

    pub fn feature_query(
        shard_id: ShardId,
        key: &str,
        start_ms: u64,
        end_ms: u64,
        count: Option<usize>,
    ) -> Self {
        Self {
            shard_id,
            record_key: key.to_string(),
            namespace: std::borrow::Cow::Borrowed("feature"),
            selector: format!("{start_ms}:{end_ms}:{}", count.unwrap_or(5000)),
        }
    }

    pub fn page(shard_id: ShardId, page_segment_id: u64, offset: u64, length: u64) -> Self {
        Self {
            shard_id,
            record_key: segment_record_key(page_segment_id),
            namespace: std::borrow::Cow::Borrowed("page"),
            selector: page_selector(None, None, offset, length),
        }
    }

    pub fn page_with_slot(
        shard_id: ShardId,
        page_segment_id: u64,
        offset: u64,
        length: u64,
        routing_slot: Option<u32>,
    ) -> Self {
        let selector = page_selector(routing_slot, None, offset, length);
        Self {
            shard_id,
            record_key: segment_record_key(page_segment_id),
            namespace: std::borrow::Cow::Borrowed("page"),
            selector,
        }
    }

    pub fn page_with_slot_generation(
        shard_id: ShardId,
        page_segment_id: u64,
        offset: u64,
        length: u64,
        routing_slot: Option<u32>,
        generation: Option<u64>,
    ) -> Self {
        let Some(generation) = generation else {
            return Self::page_with_slot(shard_id, page_segment_id, offset, length, routing_slot);
        };
        let selector = match routing_slot {
            Some(slot) => page_selector(Some(slot), Some(generation), offset, length),
            None => page_selector(None, Some(generation), offset, length),
        };
        Self {
            shard_id,
            record_key: segment_record_key(page_segment_id),
            namespace: std::borrow::Cow::Borrowed("page"),
            selector,
        }
    }

    fn disk_name(&self) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        format!("{:016x}.cache_block", hasher.finish())
    }

    fn logical_size(&self) -> usize {
        std::mem::size_of::<ShardId>()
            .saturating_add(self.record_key.len())
            .saturating_add(self.namespace.len())
            .saturating_add(self.selector.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcurrentHashMapEntry<K, V> {
    pub key: K,
    pub value: V,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcurrentHashMapInsertResult<K, V> {
    pub first: ConcurrentHashMapEntry<K, V>,
    pub second: bool,
}

/// A map that serialises each operation behind a single lock.
///
/// Every method takes the internal `RwLock`, so an individual insert, lookup or
/// erase is atomic. A *sequence* of them is not — see
/// [`map_trylock`](Self::map_trylock), which despite its name cannot give you
/// one.
///
/// This is a `std::HashMap` behind a lock rather than a port of the reference's
/// vendored striped map, which is deliberately not carried over.
#[derive(Debug, Clone)]
pub struct ConcurrentHashMap<K, V> {
    inner: Arc<RwLock<HashMap<K, V>>>,
    max_size: usize,
}

impl<K, V> ConcurrentHashMap<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone + PartialEq,
{
    pub fn new(size: usize, max_size: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::with_capacity(size))),
            max_size,
        }
    }

    pub fn with_capacity(size: usize) -> Self {
        Self::new(size, 0)
    }

    pub fn empty(&self) -> bool {
        self.inner
            .read()
            .expect("cache map lock poisoned")
            .is_empty()
    }

    pub fn size(&self) -> usize {
        self.inner.read().expect("cache map lock poisoned").len()
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }

    pub fn clear(&self) {
        self.inner.write().expect("cache map lock poisoned").clear();
    }

    pub fn reserve(&self, additional: usize) {
        self.inner
            .write()
            .expect("cache map lock poisoned")
            .reserve(additional);
    }

    pub fn find(&self, key: &K) -> Option<ConcurrentHashMapEntry<K, V>> {
        self.inner
            .read()
            .expect("cache map lock poisoned")
            .get(key)
            .cloned()
            .map(|value| ConcurrentHashMapEntry {
                key: key.clone(),
                value,
            })
    }

    pub fn at(&self, key: &K) -> V
    where
        V: Default,
    {
        self.inner
            .read()
            .expect("cache map lock poisoned")
            .get(key)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_or_default(&self, key: &K) -> V
    where
        V: Default,
    {
        self.at(key)
    }

    pub fn cbegin(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        self.entries()
    }

    pub fn cend(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        Vec::new()
    }

    pub fn begin(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        self.entries()
    }

    pub fn end(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        Vec::new()
    }

    pub fn entries(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        self.inner
            .read()
            .expect("cache map lock poisoned")
            .iter()
            .map(|(key, value)| ConcurrentHashMapEntry {
                key: key.clone(),
                value: value.clone(),
            })
            .collect()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.inner
            .read()
            .expect("cache map lock poisoned")
            .contains_key(key)
    }

    pub fn insert_entry(
        &self,
        key: K,
        value: V,
    ) -> Result<ConcurrentHashMapInsertResult<K, V>, CacheError> {
        let mut inner = self.inner.write().expect("cache map lock poisoned");
        if let Some(existing) = inner.get(&key).cloned() {
            return Ok(ConcurrentHashMapInsertResult {
                first: ConcurrentHashMapEntry {
                    key,
                    value: existing,
                },
                second: false,
            });
        }
        if self.max_size != 0 && inner.len() >= self.max_size {
            return Err(CacheError::CapacityExceeded);
        }
        inner.insert(key.clone(), value.clone());
        Ok(ConcurrentHashMapInsertResult {
            first: ConcurrentHashMapEntry { key, value },
            second: true,
        })
    }

    pub fn insert(&self, key: K, value: V) -> Result<bool, CacheError> {
        self.insert_entry(key, value).map(|result| result.second)
    }

    pub fn try_emplace(&self, key: K, value: V) -> Result<bool, CacheError> {
        self.insert(key, value)
    }

    pub fn emplace(&self, key: K, value: V) -> Result<bool, CacheError> {
        self.insert(key, value)
    }

    pub fn insert_or_assign_entry(
        &self,
        key: K,
        value: V,
    ) -> Result<ConcurrentHashMapInsertResult<K, V>, CacheError> {
        let mut inner = self.inner.write().expect("cache map lock poisoned");
        if let Some(old_value) = inner.insert(key.clone(), value.clone()) {
            Ok(ConcurrentHashMapInsertResult {
                first: ConcurrentHashMapEntry {
                    key,
                    value: old_value,
                },
                second: false,
            })
        } else {
            if self.max_size != 0 && inner.len() > self.max_size {
                inner.remove(&key);
                return Err(CacheError::CapacityExceeded);
            }
            Ok(ConcurrentHashMapInsertResult {
                first: ConcurrentHashMapEntry { key, value },
                second: true,
            })
        }
    }

    pub fn insert_or_assign(&self, key: K, value: V) -> Result<bool, CacheError> {
        self.insert_or_assign_entry(key, value)
            .map(|result| result.second)
    }

    pub fn assign(&self, key: K, value: V) -> Option<ConcurrentHashMapEntry<K, V>> {
        let mut inner = self.inner.write().expect("cache map lock poisoned");
        if inner.contains_key(&key) {
            inner.insert(key.clone(), value.clone());
            Some(ConcurrentHashMapEntry { key, value })
        } else {
            None
        }
    }

    pub fn assign_if_equal(
        &self,
        key: K,
        expected: &V,
        value: V,
    ) -> Option<ConcurrentHashMapEntry<K, V>> {
        let mut inner = self.inner.write().expect("cache map lock poisoned");
        match inner.get(&key) {
            Some(current) if current == expected => {
                inner.insert(key.clone(), value.clone());
                Some(ConcurrentHashMapEntry { key, value })
            }
            _ => None,
        }
    }

    pub fn erase(&self, key: &K) -> usize {
        usize::from(
            self.inner
                .write()
                .expect("cache map lock poisoned")
                .remove(key)
                .is_some(),
        )
    }

    pub fn erase_if_equal(&self, key: &K, expected: &V) -> usize {
        self.erase_key_if(key, |value| value == expected)
    }

    pub fn erase_key_if<F>(&self, key: &K, predicate: F) -> usize
    where
        F: FnOnce(&V) -> bool,
    {
        let mut inner = self.inner.write().expect("cache map lock poisoned");
        if inner.get(key).is_some_and(predicate) {
            inner.remove(key);
            1
        } else {
            0
        }
    }

    pub fn erase_entry(&self, entry: &ConcurrentHashMapEntry<K, V>) -> usize {
        self.erase_if_equal(&entry.key, &entry.value)
    }

    pub fn erase_entries_if<F>(&self, mut predicate: F) -> usize
    where
        F: FnMut(&ConcurrentHashMapEntry<K, V>) -> bool,
    {
        let mut inner = self.inner.write().expect("cache map lock poisoned");
        let keys = inner
            .iter()
            .filter_map(|(key, value)| {
                let entry = ConcurrentHashMapEntry {
                    key: key.clone(),
                    value: value.clone(),
                };
                predicate(&entry).then_some(key.clone())
            })
            .collect::<Vec<_>>();
        let removed = keys.len();
        for key in keys {
            inner.remove(&key);
        }
        removed
    }

    /// Always returns `true`, and takes no lock.
    ///
    /// **This is not a lock and must not be used as one.** It exists to keep the
    /// shape of the reference's per-key locking API, which this crate does not
    /// port — the map serialises each operation internally instead, so there is
    /// no per-key lock to acquire.
    ///
    /// The danger is that it looks like it works. Guarding a read-modify-write
    /// with it:
    ///
    /// ```ignore
    /// if map.map_trylock(&key) {          // always true
    ///     let value = map.get(&key);      // another thread may interleave here
    ///     map.insert(key, value + 1);     // ...and this update can be lost
    ///     map.map_unlock(&key);           // no-op
    /// }
    /// ```
    ///
    /// compiles, runs, and provides no exclusion whatsoever. For a sequence that
    /// must be atomic, hold your own lock around the map.
    pub fn map_trylock(&self, _key: &K) -> bool {
        true
    }

    /// Does nothing. See [`map_trylock`](Self::map_trylock).
    pub fn map_lock(&self, _key: &K) {}

    /// Does nothing. See [`map_trylock`](Self::map_trylock).
    pub fn map_unlock(&self, _key: &K) {}

    #[allow(non_snake_case)]
    pub fn Empty(&self) -> bool {
        self.empty()
    }

    #[allow(non_snake_case)]
    pub fn Size(&self) -> usize {
        self.size()
    }

    #[allow(non_snake_case)]
    pub fn MaxSize(&self) -> usize {
        self.max_size()
    }

    #[allow(non_snake_case)]
    pub fn Clear(&self) {
        self.clear();
    }

    #[allow(non_snake_case)]
    pub fn Reserve(&self, additional: usize) {
        self.reserve(additional);
    }

    #[allow(non_snake_case)]
    pub fn Find(&self, key: &K) -> Option<ConcurrentHashMapEntry<K, V>> {
        self.find(key)
    }

    #[allow(non_snake_case)]
    pub fn At(&self, key: &K) -> V
    where
        V: Default,
    {
        self.at(key)
    }

    #[allow(non_snake_case)]
    pub fn GetOrDefault(&self, key: &K) -> V
    where
        V: Default,
    {
        self.get_or_default(key)
    }

    #[allow(non_snake_case)]
    pub fn CBegin(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        self.cbegin()
    }

    #[allow(non_snake_case)]
    pub fn CEnd(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        self.cend()
    }

    #[allow(non_snake_case)]
    pub fn Begin(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        self.begin()
    }

    #[allow(non_snake_case)]
    pub fn End(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        self.end()
    }

    #[allow(non_snake_case)]
    pub fn Entries(&self) -> Vec<ConcurrentHashMapEntry<K, V>> {
        self.entries()
    }

    #[allow(non_snake_case)]
    pub fn Insert(&self, key: K, value: V) -> Result<bool, CacheError> {
        self.insert(key, value)
    }

    #[allow(non_snake_case)]
    pub fn InsertEntry(
        &self,
        key: K,
        value: V,
    ) -> Result<ConcurrentHashMapInsertResult<K, V>, CacheError> {
        self.insert_entry(key, value)
    }

    #[allow(non_snake_case)]
    pub fn TryEmplace(&self, key: K, value: V) -> Result<bool, CacheError> {
        self.try_emplace(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Emplace(&self, key: K, value: V) -> Result<bool, CacheError> {
        self.emplace(key, value)
    }

    #[allow(non_snake_case)]
    pub fn InsertOrAssign(&self, key: K, value: V) -> Result<bool, CacheError> {
        self.insert_or_assign(key, value)
    }

    #[allow(non_snake_case)]
    pub fn InsertOrAssignEntry(
        &self,
        key: K,
        value: V,
    ) -> Result<ConcurrentHashMapInsertResult<K, V>, CacheError> {
        self.insert_or_assign_entry(key, value)
    }

    #[allow(non_snake_case)]
    pub fn Assign(&self, key: K, value: V) -> Option<ConcurrentHashMapEntry<K, V>> {
        self.assign(key, value)
    }

    #[allow(non_snake_case)]
    pub fn AssignIfEqual(
        &self,
        key: K,
        expected: &V,
        value: V,
    ) -> Option<ConcurrentHashMapEntry<K, V>> {
        self.assign_if_equal(key, expected, value)
    }

    #[allow(non_snake_case)]
    pub fn Erase(&self, key: &K) -> usize {
        self.erase(key)
    }

    #[allow(non_snake_case)]
    pub fn EraseIfEqual(&self, key: &K, expected: &V) -> usize {
        self.erase_if_equal(key, expected)
    }

    #[allow(non_snake_case)]
    pub fn EraseKeyIf<F>(&self, key: &K, predicate: F) -> usize
    where
        F: FnOnce(&V) -> bool,
    {
        self.erase_key_if(key, predicate)
    }

    #[allow(non_snake_case)]
    pub fn EraseEntry(&self, entry: &ConcurrentHashMapEntry<K, V>) -> usize {
        self.erase_entry(entry)
    }

    #[allow(non_snake_case)]
    pub fn EraseEntriesIf<F>(&self, predicate: F) -> usize
    where
        F: FnMut(&ConcurrentHashMapEntry<K, V>) -> bool,
    {
        self.erase_entries_if(predicate)
    }

    #[allow(non_snake_case)]
    pub fn MapTryLock(&self, key: &K) -> bool {
        self.map_trylock(key)
    }

    #[allow(non_snake_case)]
    pub fn MapLock(&self, key: &K) {
        self.map_lock(key);
    }

    #[allow(non_snake_case)]
    pub fn MapUnlock(&self, key: &K) {
        self.map_unlock(key);
    }
}

impl<K, V> Default for ConcurrentHashMap<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone + PartialEq,
{
    fn default() -> Self {
        Self::new(8, 0)
    }
}

#[derive(Debug, Clone)]
pub struct HistStats {
    buckets: Vec<u64>,
    large_nums: Vec<usize>,
    total_count: u64,
}

impl HistStats {
    pub fn new() -> Self {
        Self {
            buckets: vec![0; 10 << 20],
            large_nums: Vec::new(),
            total_count: 0,
        }
    }

    pub fn with_bucket_size(bucket_size: usize) -> Self {
        Self {
            buckets: vec![0; bucket_size],
            large_nums: Vec::new(),
            total_count: 0,
        }
    }

    pub fn append(&mut self, unit: usize) {
        if unit < self.buckets.len() {
            self.buckets[unit] = self.buckets[unit].saturating_add(1);
        } else {
            self.large_nums.push(unit);
        }
        self.total_count = self.total_count.saturating_add(1);
    }

    pub fn get_result(&self, percentiles: &[f64]) -> Vec<usize> {
        debug_assert!(percentiles.windows(2).all(|pair| pair[0] <= pair[1]));
        let total = self.count().max(1) as f64;
        let mut result = vec![0; percentiles.len() + 2];
        let mut percentile_idx = 0usize;
        let mut accumulated = 0u64;
        let mut total_units = 0usize;
        let mut max = 0usize;

        for (unit, count) in self.buckets.iter().copied().enumerate() {
            if count == 0 {
                continue;
            }
            accumulated = accumulated.saturating_add(count);
            total_units = total_units.saturating_add(unit.saturating_mul(count as usize));
            while percentile_idx < percentiles.len()
                && (accumulated as f64 / total) >= percentiles[percentile_idx]
            {
                result[percentile_idx] = unit;
                percentile_idx += 1;
            }
            max = unit;
        }

        let mut large_nums = self.large_nums.clone();
        large_nums.sort_unstable();
        for unit in large_nums {
            accumulated = accumulated.saturating_add(1);
            total_units = total_units.saturating_add(unit);
            while percentile_idx < percentiles.len()
                && (accumulated as f64 / total) >= percentiles[percentile_idx]
            {
                result[percentile_idx] = unit;
                percentile_idx += 1;
            }
            max = unit;
        }

        let avg = total_units / accumulated.max(1) as usize;
        let avg_idx = percentiles.len();
        result[avg_idx] = avg;
        result[avg_idx + 1] = max;
        result
    }

    pub fn merge(&mut self, another: &HistStats) {
        if another.buckets.len() > self.buckets.len() {
            self.buckets.resize(another.buckets.len(), 0);
        }
        for (idx, count) in another.buckets.iter().copied().enumerate() {
            self.buckets[idx] = self.buckets[idx].saturating_add(count);
        }
        self.large_nums.extend_from_slice(&another.large_nums);
        self.total_count = self.total_count.saturating_add(another.count());
    }

    pub fn count(&self) -> u64 {
        self.total_count
    }

    pub fn reset(&mut self) {
        self.buckets.fill(0);
        self.large_nums.clear();
        if self.large_nums.capacity() > (10 << 10) {
            self.large_nums.shrink_to_fit();
        }
        self.total_count = 0;
    }

    pub fn result_string(&self, unit_suffix: &str) -> String {
        let result = self.get_result(&[0.50, 0.90, 0.95, 0.99, 0.999]);
        format!(
            "P50:{}{} P90:{}{} P95:{}{} P99:{}{} P999:{}{} Avg:{}{} Max:{}{}",
            result[0],
            unit_suffix,
            result[1],
            unit_suffix,
            result[2],
            unit_suffix,
            result[3],
            unit_suffix,
            result[4],
            unit_suffix,
            result[5],
            unit_suffix,
            result[6],
            unit_suffix
        )
    }

    #[allow(non_snake_case)]
    pub fn Append(&mut self, unit: usize) {
        self.append(unit);
    }

    #[allow(non_snake_case)]
    pub fn GetResult(&self, percentiles: &[f64]) -> Vec<usize> {
        self.get_result(percentiles)
    }

    #[allow(non_snake_case)]
    pub fn Merge(&mut self, another: &HistStats) {
        self.merge(another);
    }

    #[allow(non_snake_case)]
    pub fn Count(&self) -> u64 {
        self.count()
    }

    #[allow(non_snake_case)]
    pub fn Reset(&mut self) {
        self.reset();
    }

    #[allow(non_snake_case)]
    pub fn ResultString(&self, unit_suffix: &str) -> String {
        self.result_string(unit_suffix)
    }
}

impl Default for HistStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Counters for one cache, sampled rather than live.
///
/// Hits are split by the tier that served them, so `memory_hits`, `pmem_hits`
/// and `disk_hits` sum to the total hit count and `misses` is separate. The
/// `*_fills` count entries written *into* a tier and the `*_evictions` count
/// entries dropped from one, which is what makes a promotion visible: it appears
/// as a fill in the upper tier without a corresponding miss.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheStats {
    pub memory_hits: u64,
    pub disk_hits: u64,
    pub misses: u64,
    pub puts: u64,
    pub invalidations: u64,
    pub memory_evictions: u64,
    #[serde(default)]
    pub pmem_hits: u64,
    #[serde(default)]
    pub pmem_fills: u64,
    #[serde(default)]
    pub pmem_evictions: u64,
    #[serde(default)]
    pub pmem_admission_accepted: u64,
    #[serde(default)]
    pub pmem_admission_rejected: u64,
    #[serde(default)]
    pub pmem_eviction_capacity: u64,
    #[serde(default)]
    pub pmem_eviction_pinned_skips: u64,
    #[serde(default)]
    pub memory_admission_accepted: u64,
    #[serde(default)]
    pub memory_admission_rejected: u64,
    #[serde(default)]
    pub memory_fills: u64,
    #[serde(default)]
    pub disk_fills: u64,
    #[serde(default)]
    pub ssd_admission_accepted: u64,
    #[serde(default)]
    pub ssd_admission_rejected: u64,
    #[serde(default)]
    pub ssd_evictions: u64,
    #[serde(default)]
    pub ssd_eviction_capacity: u64,
    #[serde(default)]
    pub ssd_eviction_pinned_skips: u64,
    #[serde(default)]
    pub ssd_oversize_rejections: u64,
    /// Bytes handed to the SSD tier's store, counting every physical write:
    /// admissions, reclaim rewrites and recovery alike. This is the number
    /// that wears the drive.
    pub ssd_bytes_written: u64,
    /// Admissions the write budget turned away to stay inside its target rate.
    pub ssd_write_budget_rejections: u64,
    /// Share of keys the write budget is currently admitting, out of 10000.
    /// Sits at 10000 when no target is set.
    pub ssd_write_budget_share: u64,
    /// Bytes per second the SSD write budget measured over its last window.
    /// Zero when no budget is set, or before a window has closed.
    pub ssd_write_budget_observed_bytes_per_sec: u64,
    /// Bytes per second the SSD write budget is aiming at. Zero when uncapped.
    pub ssd_write_budget_target_bytes_per_sec: u64,
    /// Copies dropped from a tier because a newer write for the same key was
    /// admitted to a different one. A rising count is ordinary for a hot key
    /// being rewritten; it is not an eviction.
    pub stale_tier_copies_dropped: u64,
    /// Demotions declined because the entry had already expired, and writing
    /// it to a slower tier would have spent a write on a value no read can be
    /// given.
    pub expired_demotions_skipped: u64,
    /// Reads that found an entry which had passed its time to live. Counted as
    /// misses too, because that is what the caller was served.
    pub expired_reads: u64,
    /// Entries removed for having passed their time to live, by a read that
    /// found one or by a sweep.
    pub expired_removals: u64,
    /// Evictions that took an entry which had already expired. These are free:
    /// the entry could not have been served again anyway, so a rising share
    /// here means eviction is reclaiming without costing future hits.
    pub eviction_expired: u64,
    /// Reclaims of an expired entry whose durable copy could not be deleted.
    ///
    /// The entry is gone from memory either way, so a read will not serve it
    /// now — but the copy left on the device is what a restart recovers, and
    /// it comes back without the metadata that carried its life. Any value
    /// above zero means expiry is not actually reclaiming, and the error that
    /// says so is otherwise discarded.
    pub expired_delete_failures: u64,
    #[serde(default)]
    pub ssd_write_through_admissions: u64,
    #[serde(default)]
    pub hotness_promotions: u64,
    /// Hits that had to take the cache exclusively to move their entry in the
    /// access orders. The rest were served entirely under the shared lock, so
    /// this against the hit counts is how much of the read path is actually
    /// concurrent.
    #[serde(default)]
    pub access_order_refreshes: u64,
    #[serde(default)]
    pub refill_failures: u64,
    #[serde(default)]
    pub eviction_capacity: u64,
    #[serde(default)]
    pub eviction_oversize: u64,
    #[serde(default)]
    pub eviction_cold: u64,
    #[serde(default)]
    pub eviction_low_hit: u64,
    #[serde(default)]
    pub eviction_stale: u64,
    #[serde(default)]
    pub pinned_entries: u64,
    #[serde(default)]
    pub pinned_bytes: u64,
    #[serde(default)]
    pub pin_operations: u64,
    #[serde(default)]
    pub unpin_operations: u64,
    #[serde(default)]
    pub insert_pinned_operations: u64,
    #[serde(default)]
    pub eviction_pinned_skips: u64,
    #[serde(default)]
    pub zero_copy_handle_hits: u64,
    #[serde(default)]
    pub zero_copy_handle_misses: u64,
    #[serde(default)]
    pub async_writeback_enqueued: u64,
    #[serde(default)]
    pub async_writeback_drained: u64,
    #[serde(default)]
    pub async_writeback_backpressure_rejections: u64,
    #[serde(default)]
    pub writeback_backpressure_events: u64,
    #[serde(default)]
    pub async_writeback_queue_depth: u64,
    #[serde(default)]
    pub async_writeback_queue_bytes: u64,
    #[serde(default)]
    pub async_writeback_max_queue_depth: u64,
    #[serde(default)]
    pub async_writeback_max_queue_bytes: u64,
    #[serde(default)]
    pub get_latency_samples: u64,
    #[serde(default)]
    pub put_latency_samples: u64,
    #[serde(default)]
    pub get_latency_total_micros: u64,
    #[serde(default)]
    pub put_latency_total_micros: u64,
    #[serde(default)]
    pub get_latency_max_micros: u64,
    #[serde(default)]
    pub put_latency_max_micros: u64,
    #[serde(default)]
    pub get_latency_le_10us: u64,
    #[serde(default)]
    pub get_latency_le_100us: u64,
    #[serde(default)]
    pub get_latency_le_1ms: u64,
    #[serde(default)]
    pub get_latency_le_10ms: u64,
    #[serde(default)]
    pub get_latency_gt_10ms: u64,
    #[serde(default)]
    pub put_latency_le_10us: u64,
    #[serde(default)]
    pub put_latency_le_100us: u64,
    #[serde(default)]
    pub put_latency_le_1ms: u64,
    #[serde(default)]
    pub put_latency_le_10ms: u64,
    #[serde(default)]
    pub put_latency_gt_10ms: u64,
    #[serde(default)]
    pub read_through_latency_samples: u64,
    #[serde(default)]
    pub read_through_latency_total_micros: u64,
    #[serde(default)]
    pub read_through_latency_le_10us: u64,
    #[serde(default)]
    pub read_through_latency_le_100us: u64,
    #[serde(default)]
    pub read_through_latency_le_1ms: u64,
    #[serde(default)]
    pub read_through_latency_le_10ms: u64,
    #[serde(default)]
    pub read_through_latency_gt_10ms: u64,
    #[serde(default)]
    pub refill_latency_samples: u64,
    #[serde(default)]
    pub refill_latency_total_micros: u64,
    #[serde(default)]
    pub refill_latency_le_10us: u64,
    #[serde(default)]
    pub refill_latency_le_100us: u64,
    #[serde(default)]
    pub refill_latency_le_1ms: u64,
    #[serde(default)]
    pub refill_latency_le_10ms: u64,
    #[serde(default)]
    pub refill_latency_gt_10ms: u64,
    #[serde(default)]
    pub writeback_latency_samples: u64,
    #[serde(default)]
    pub writeback_latency_total_micros: u64,
    #[serde(default)]
    pub writeback_latency_le_10us: u64,
    #[serde(default)]
    pub writeback_latency_le_100us: u64,
    #[serde(default)]
    pub writeback_latency_le_1ms: u64,
    #[serde(default)]
    pub writeback_latency_le_10ms: u64,
    #[serde(default)]
    pub writeback_latency_gt_10ms: u64,
    #[serde(default)]
    pub eviction_latency_samples: u64,
    #[serde(default)]
    pub eviction_latency_total_micros: u64,
    #[serde(default)]
    pub eviction_latency_le_10us: u64,
    #[serde(default)]
    pub eviction_latency_le_100us: u64,
    #[serde(default)]
    pub eviction_latency_le_1ms: u64,
    #[serde(default)]
    pub eviction_latency_le_10ms: u64,
    #[serde(default)]
    pub eviction_latency_gt_10ms: u64,
    #[serde(default)]
    pub compaction_latency_samples: u64,
    #[serde(default)]
    pub compaction_latency_total_micros: u64,
    #[serde(default)]
    pub compaction_latency_le_10us: u64,
    #[serde(default)]
    pub compaction_latency_le_100us: u64,
    #[serde(default)]
    pub compaction_latency_le_1ms: u64,
    #[serde(default)]
    pub compaction_latency_le_10ms: u64,
    #[serde(default)]
    pub compaction_latency_gt_10ms: u64,
    #[serde(default)]
    pub eviction_sampled_groups: u64,
    #[serde(default)]
    pub memory_slot_evictions: u64,
    #[serde(default)]
    pub ssd_slot_evictions: u64,
    #[serde(default)]
    pub ssd_eviction_cold: u64,
    #[serde(default)]
    pub ssd_eviction_low_hit: u64,
    #[serde(default)]
    pub ssd_eviction_stale: u64,
    pub compressed_puts: u64,
    pub compressed_hits: u64,
    pub compression_bytes_saved: u64,
    #[serde(default)]
    pub get_latency_count: u64,
    #[serde(default)]
    pub get_latency_total_us: u64,
    #[serde(default)]
    pub get_latency_max_us: u64,
    #[serde(default)]
    pub put_latency_count: u64,
    #[serde(default)]
    pub put_latency_total_us: u64,
    #[serde(default)]
    pub put_latency_max_us: u64,
    pub memory_bytes: u64,
    #[serde(default)]
    pub pmem_bytes: u64,
    pub disk_bytes: u64,
}
