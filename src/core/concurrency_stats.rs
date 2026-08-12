// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumaTopology {
    num_cores: usize,
    max_num_cores: usize,
    max_num_numa_nodes: usize,
    core_to_numa_node: Vec<usize>,
    numa_node_to_cores: Vec<Vec<usize>>,
    numa_node_core_idx: Vec<usize>,
}

impl NumaTopology {
    fn detect() -> Self {
        let cores = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .max(1);
        let core_to_numa_node = vec![0; cores];
        let numa_node_to_cores = vec![(0..cores).collect::<Vec<_>>()];
        let numa_node_core_idx = (0..cores).collect::<Vec<_>>();
        Self {
            num_cores: cores,
            max_num_cores: cores,
            max_num_numa_nodes: 1,
            core_to_numa_node,
            numa_node_to_cores,
            numa_node_core_idx,
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

    pub fn get_num_all_cores() -> usize {
        numa_topology().num_cores
    }

    pub fn get_num_online_cores() -> usize {
        numa_topology().max_num_cores
    }

    pub fn get_current_cpu_core() -> usize {
        0
    }

    pub fn get_max_num_numa_nodes() -> usize {
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

    pub fn get_numa_node_core_idx(core: usize) -> usize {
        numa_topology()
            .numa_node_core_idx
            .get(core)
            .copied()
            .unwrap_or(0)
    }

    pub fn bind_thread_to_cpu_core(core: usize) -> Result<(), CacheError> {
        if core < Self::get_num_online_cores() {
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
        Self::get_num_all_cores()
    }

    #[allow(non_snake_case)]
    pub fn GetNumOnlineCores() -> usize {
        Self::get_num_online_cores()
    }

    #[allow(non_snake_case)]
    pub fn GetCurrentCpuCore() -> usize {
        Self::get_current_cpu_core()
    }

    #[allow(non_snake_case)]
    pub fn GetMaxNumNumaNodes() -> usize {
        Self::get_max_num_numa_nodes()
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
        Self::get_numa_node_core_idx(core)
    }

    #[allow(non_snake_case)]
    pub fn BindThreadToCpuCore(core: usize) -> Result<(), CacheError> {
        Self::bind_thread_to_cpu_core(core)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    pub shard_id: ShardId,
    pub record_key: String,
    pub namespace: String,
    pub selector: String,
}

impl CacheKey {
    pub fn string(shard_id: ShardId, key: &str) -> Self {
        Self {
            shard_id,
            record_key: key.to_string(),
            namespace: "string".to_string(),
            selector: "value".to_string(),
        }
    }

    pub fn hash(shard_id: ShardId, key: &str, field: &str) -> Self {
        Self {
            shard_id,
            record_key: key.to_string(),
            namespace: "hash".to_string(),
            selector: field.to_string(),
        }
    }

    pub fn set_members(shard_id: ShardId, key: &str) -> Self {
        Self {
            shard_id,
            record_key: key.to_string(),
            namespace: "set".to_string(),
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
            namespace: "feature".to_string(),
            selector: format!("{start_ms}:{end_ms}:{}", count.unwrap_or(5000)),
        }
    }

    pub fn page(shard_id: ShardId, page_segment_id: u64, offset: u64, length: u64) -> Self {
        Self {
            shard_id,
            record_key: format!("segment-{page_segment_id:020}"),
            namespace: "page".to_string(),
            selector: format!("{offset}:{length}"),
        }
    }

    pub fn page_with_slot(
        shard_id: ShardId,
        page_segment_id: u64,
        offset: u64,
        length: u64,
        routing_slot: Option<u32>,
    ) -> Self {
        let selector = match routing_slot {
            Some(slot) => format!("slot-{slot}:{offset}:{length}"),
            None => format!("{offset}:{length}"),
        };
        Self {
            shard_id,
            record_key: format!("segment-{page_segment_id:020}"),
            namespace: "page".to_string(),
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
            Some(slot) => format!("slot-{slot}:gen-{generation}:{offset}:{length}"),
            None => format!("gen-{generation}:{offset}:{length}"),
        };
        Self {
            shard_id,
            record_key: format!("segment-{page_segment_id:020}"),
            namespace: "page".to_string(),
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

    pub fn map_trylock(&self, _key: &K) -> bool {
        true
    }

    pub fn map_lock(&self, _key: &K) {}

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
    #[serde(default)]
    pub ssd_write_through_admissions: u64,
    #[serde(default)]
    pub hotness_promotions: u64,
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

