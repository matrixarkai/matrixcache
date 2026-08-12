// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachePressureValidationReport {
    pub iterations: usize,
    pub memory_admitted: u64,
    pub pmem_admitted: u64,
    pub ssd_admitted: u64,
    pub rejected: u64,
    pub observed_evictions: u64,
    pub observed_disk_refills: u64,
    #[serde(default)]
    pub observed_ssd_evictions: u64,
    #[serde(default)]
    pub observed_hotness_promotions: u64,
    pub passed: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheReplacementPolicySoakReport {
    pub iterations: usize,
    pub hot_key_count: usize,
    pub cold_key_count: usize,
    pub hot_memory_survivors: usize,
    pub cold_memory_survivors: usize,
    pub pinned_memory_survived: bool,
    pub restart_disk_refill_ready: bool,
    pub observed_evictions: u64,
    pub observed_pinned_skips: u64,
    pub observed_disk_refills: u64,
    #[serde(default)]
    pub observed_async_writeback_backpressure: u64,
    #[serde(default)]
    pub async_writeback_max_queue_depth: u64,
    #[serde(default)]
    pub async_writeback_max_queue_bytes: u64,
    pub get_latency_samples: u64,
    pub put_latency_samples: u64,
    #[serde(default)]
    pub read_through_latency_samples: u64,
    #[serde(default)]
    pub refill_latency_samples: u64,
    #[serde(default)]
    pub writeback_latency_samples: u64,
    #[serde(default)]
    pub eviction_latency_samples: u64,
    #[serde(default)]
    pub compaction_latency_samples: u64,
    #[serde(default)]
    pub read_through_latency_bucketed: bool,
    #[serde(default)]
    pub refill_latency_bucketed: bool,
    #[serde(default)]
    pub writeback_latency_bucketed: bool,
    #[serde(default)]
    pub eviction_latency_bucketed: bool,
    #[serde(default)]
    pub compaction_latency_bucketed: bool,
    pub passed: bool,
    pub reasons: Vec<String>,
}

pub fn validate_cache_pressure_policy(
    policy: CacheTieringPolicy,
    requests: &[CacheAdmissionRequest],
    stats: CacheStats,
) -> CachePressureValidationReport {
    let mut report = CachePressureValidationReport {
        iterations: requests.len(),
        observed_evictions: stats.memory_evictions,
        observed_disk_refills: stats.disk_hits,
        observed_ssd_evictions: stats.ssd_evictions,
        observed_hotness_promotions: stats.hotness_promotions,
        ..CachePressureValidationReport::default()
    };
    for request in requests {
        match policy.decide(request).tier {
            CacheTier::Memory => report.memory_admitted += 1,
            CacheTier::Pmem => report.pmem_admitted += 1,
            CacheTier::Ssd => report.ssd_admitted += 1,
            CacheTier::Reject => report.rejected += 1,
        }
    }
    if report.memory_admitted == 0 {
        report.reasons.push("missing_memory_admission".to_string());
    }
    if report.ssd_admitted == 0 {
        report.reasons.push("missing_ssd_admission".to_string());
    }
    if policy.pmem_capacity_bytes > 0 && report.pmem_admitted == 0 {
        report.reasons.push("missing_pmem_admission".to_string());
    }
    if report.rejected == 0 {
        report.reasons.push("missing_rejection_case".to_string());
    }
    if stats.memory_evictions == 0 {
        report
            .reasons
            .push("missing_eviction_observation".to_string());
    }
    if stats.disk_hits == 0 {
        report
            .reasons
            .push("missing_disk_refill_observation".to_string());
    }
    report.passed = report.reasons.is_empty();
    report
}

fn latency_bucket_count(
    le_10us: u64,
    le_100us: u64,
    le_1ms: u64,
    le_10ms: u64,
    gt_10ms: u64,
) -> u64 {
    le_10us
        .saturating_add(le_100us)
        .saturating_add(le_1ms)
        .saturating_add(le_10ms)
        .saturating_add(gt_10ms)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheCompression {
    None,
    Zstd { level: i32 },
}

impl Default for CacheCompression {
    fn default() -> Self {
        Self::Zstd { level: 1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheBlockOptions {
    pub compression: CacheCompression,
    pub min_compress_bytes: usize,
}

impl Default for CacheBlockOptions {
    fn default() -> Self {
        Self {
            compression: CacheCompression::default(),
            min_compress_bytes: 128,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheGcReport {
    pub shard_id: ShardId,
    pub memory_entries_removed: usize,
    pub disk_bytes_removed: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntryInfo {
    pub shard_id: ShardId,
    pub namespace: String,
    pub record_key: String,
    pub selector: String,
    pub memory_bytes: u64,
    #[serde(default)]
    pub pmem_bytes: u64,
    pub disk_bytes: u64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub block_kind: Option<CacheBlockKind>,
    #[serde(default)]
    pub routing_slot: Option<u32>,
    #[serde(default)]
    pub hotness: u32,
    #[serde(default)]
    pub hits: u64,
    #[serde(default)]
    pub last_access_epoch: u64,
    #[serde(default)]
    pub admission_reason: Option<CacheAdmissionReason>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEvictionReport {
    pub memory_evictions: u64,
    pub memory_capacity_evictions: u64,
    pub memory_cold_evictions: u64,
    pub memory_low_hit_evictions: u64,
    pub memory_stale_evictions: u64,
    pub memory_pinned_skips: u64,
    #[serde(default)]
    pub pmem_evictions: u64,
    #[serde(default)]
    pub pmem_capacity_evictions: u64,
    #[serde(default)]
    pub pmem_pinned_skips: u64,
    pub ssd_evictions: u64,
    pub ssd_capacity_evictions: u64,
    pub ssd_cold_evictions: u64,
    pub ssd_low_hit_evictions: u64,
    pub ssd_stale_evictions: u64,
    pub ssd_pinned_skips: u64,
    pub sampled_eviction_groups: u64,
    pub memory_slot_evictions: u64,
    pub ssd_slot_evictions: u64,
    pub replacement_policy: CacheReplacementPolicy,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheWritebackBackpressureReport {
    pub ssd_write_through_enabled: bool,
    pub write_through_admissions: u64,
    pub ssd_admission_rejections: u64,
    pub ssd_evictions: u64,
    pub ssd_oversize_rejections: u64,
    pub backpressure_events: u64,
    pub bounded_queue_ready: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheLatencyMetricsReport {
    pub get_count: u64,
    pub get_avg_us: u64,
    pub get_max_us: u64,
    pub put_count: u64,
    pub put_avg_us: u64,
    pub put_max_us: u64,
    pub histogram_ready: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheReplacementPolicy {
    Fifo,
    Slru,
    #[default]
    WeightedHotnessLru,
}

impl CacheReplacementPolicy {
    pub fn from_cxx_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("fifo") {
            Self::Fifo
        } else if value.eq_ignore_ascii_case("slru") {
            Self::Slru
        } else {
            Self::WeightedHotnessLru
        }
    }

    pub fn as_cxx_name(self) -> &'static str {
        match self {
            Self::Fifo => "FIFO",
            Self::Slru => "SLRU",
            Self::WeightedHotnessLru => "WeightedHotnessLru",
        }
    }
}

pub type CacheKeyType = String;

#[derive(Debug, Clone)]
pub struct BaseLRUList {
    capacity: usize,
    list: VecDeque<CacheKeyType>,
    index: HashMap<CacheKeyType, ()>,
}

impl BaseLRUList {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            list: VecDeque::new(),
            index: HashMap::new(),
        }
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn size(&self) -> usize {
        self.index.len()
    }

    pub fn put(&mut self, key: CacheKeyType) {
        self.delete(&key);
        self.list.push_front(key.clone());
        self.index.insert(key, ());
    }

    pub fn get(&mut self, key: &str) -> bool {
        if !self.index.contains_key(key) {
            return false;
        }
        let key = key.to_string();
        self.list.retain(|candidate| candidate != &key);
        self.list.push_front(key);
        true
    }

    pub fn delete(&mut self, key: &str) -> bool {
        if self.index.remove(key).is_some() {
            self.list.retain(|candidate| candidate != key);
            return true;
        }
        false
    }

    pub fn get_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.list.iter().rev().take(size).cloned().collect()
    }

    pub fn evict(&mut self) -> Vec<CacheKeyType> {
        let mut evicted = Vec::new();
        while self.index.len() > self.capacity {
            evicted.extend(self.evict_one());
        }
        evicted
    }

    pub fn evict_one(&mut self) -> Vec<CacheKeyType> {
        if let Some(key) = self.list.pop_back() {
            self.index.remove(&key);
            return vec![key];
        }
        Vec::new()
    }

    pub fn reset(&mut self) {
        self.list.clear();
        self.index.clear();
    }
}

#[allow(non_snake_case)]
impl BaseLRUList {
    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn Capacity(&self) -> usize {
        self.capacity()
    }

    pub fn Size(&self) -> usize {
        self.size()
    }

    pub fn Put(&mut self, key: CacheKeyType) {
        self.put(key);
    }

    pub fn Get(&mut self, key: &str) -> bool {
        self.get(key)
    }

    pub fn Delete(&mut self, key: &str) -> bool {
        self.delete(key)
    }

    pub fn GetTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_tail(size)
    }

    pub fn Evict(&mut self) -> Vec<CacheKeyType> {
        self.evict()
    }

    pub fn EvictOne(&mut self) -> Vec<CacheKeyType> {
        self.evict_one()
    }

    pub fn Reset(&mut self) {
        self.reset();
    }
}

impl Default for BaseLRUList {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GhostLRUPopResult {
    pub item: CacheKeyType,
    pub is_ghost: bool,
}

#[derive(Debug, Clone)]
pub struct GhostLRUList {
    data_list: BaseLRUList,
    ghost_list: BaseLRUList,
}

impl GhostLRUList {
    pub fn new(capacity: usize) -> Self {
        Self {
            data_list: BaseLRUList::new(capacity),
            ghost_list: BaseLRUList::new(capacity),
        }
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.data_list.set_capacity(capacity);
        self.ghost_list.set_capacity(capacity);
    }

    pub fn capacity(&self) -> usize {
        self.data_list.capacity()
    }

    pub fn ghost_capacity(&self) -> usize {
        self.ghost_list.capacity()
    }

    pub fn size(&self) -> usize {
        self.data_list.size()
    }

    pub fn ghost_size(&self) -> usize {
        self.ghost_list.size()
    }

    pub fn total_size(&self) -> usize {
        self.size().saturating_add(self.ghost_size())
    }

    pub fn put(&mut self, key: CacheKeyType) {
        self.data_list.put(key);
    }

    pub fn put_ghost(&mut self, key: CacheKeyType) {
        self.ghost_list.put(key);
    }

    pub fn get(&mut self, key: &str) -> bool {
        if self.data_list.get(key) {
            return true;
        }
        self.ghost_list.get(key);
        false
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let deleted_data = self.data_list.delete(key);
        let deleted_ghost = self.ghost_list.delete(key);
        deleted_data || deleted_ghost
    }

    pub fn downgrade(&mut self) {
        for key in self.data_list.evict_one() {
            self.ghost_list.put(key);
        }
    }

    pub fn evict_one_data(&mut self) -> Vec<CacheKeyType> {
        self.data_list.evict_one()
    }

    pub fn evict_one_ghost(&mut self) -> Vec<CacheKeyType> {
        self.ghost_list.evict_one()
    }

    pub fn get_data_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.data_list.get_tail(size)
    }

    pub fn get_ghost_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.ghost_list.get_tail(size)
    }

    pub fn evict(&mut self) -> Vec<CacheKeyType> {
        self.ghost_list.evict();
        self.data_list.evict()
    }

    pub fn pop(&mut self, key: &str) -> GhostLRUPopResult {
        if self.data_list.delete(key) {
            return GhostLRUPopResult {
                item: key.to_string(),
                is_ghost: false,
            };
        }
        if self.ghost_list.delete(key) {
            return GhostLRUPopResult {
                item: key.to_string(),
                is_ghost: true,
            };
        }
        GhostLRUPopResult::default()
    }

    pub fn reset(&mut self) {
        self.data_list.reset();
        self.ghost_list.reset();
    }
}

#[allow(non_snake_case)]
impl GhostLRUList {
    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn Capacity(&self) -> usize {
        self.capacity()
    }

    pub fn GhostCapacity(&self) -> usize {
        self.ghost_capacity()
    }

    pub fn Size(&self) -> usize {
        self.size()
    }

    pub fn GhostSize(&self) -> usize {
        self.ghost_size()
    }

    pub fn TotalSize(&self) -> usize {
        self.total_size()
    }

    pub fn Put(&mut self, key: CacheKeyType) {
        self.put(key);
    }

    pub fn PutGhost(&mut self, key: CacheKeyType) {
        self.put_ghost(key);
    }

    pub fn Get(&mut self, key: &str) -> bool {
        self.get(key)
    }

    pub fn Delete(&mut self, key: &str) -> bool {
        self.delete(key)
    }

    pub fn Downgrade(&mut self) {
        self.downgrade();
    }

    pub fn EvictOneData(&mut self) -> Vec<CacheKeyType> {
        self.evict_one_data()
    }

    pub fn EvictOneGhost(&mut self) -> Vec<CacheKeyType> {
        self.evict_one_ghost()
    }

    pub fn GetDataTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_data_tail(size)
    }

    pub fn GetGhostTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_ghost_tail(size)
    }

    pub fn Evict(&mut self) -> Vec<CacheKeyType> {
        self.evict()
    }

    pub fn Pop(&mut self, key: &str) -> GhostLRUPopResult {
        self.pop(key)
    }

    pub fn Reset(&mut self) {
        self.reset();
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArcPopResult {
    pub item: CacheKeyType,
    pub is_active: bool,
    pub is_ghost: bool,
}

#[derive(Debug, Clone)]
pub struct ArcList {
    capacity: usize,
    fetch_list: GhostLRUList,
    active_list: GhostLRUList,
}

impl ArcList {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            fetch_list: GhostLRUList::new(capacity / 2),
            active_list: GhostLRUList::new(capacity.saturating_sub(capacity / 2)),
        }
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
    }

    pub fn size(&self) -> usize {
        self.fetch_list
            .size()
            .saturating_add(self.active_list.size())
    }

    pub fn ghost_size(&self) -> usize {
        self.fetch_list
            .ghost_size()
            .saturating_add(self.active_list.ghost_size())
    }

    pub fn total_size(&self) -> usize {
        self.size().saturating_add(self.ghost_size())
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn fetch_capacity(&self) -> usize {
        self.fetch_list.capacity()
    }

    pub fn active_capacity(&self) -> usize {
        self.active_list.capacity()
    }

    pub fn data_full(&self) -> bool {
        self.size() >= self.capacity()
    }

    pub fn reset(&mut self) {
        self.fetch_list.reset();
        self.active_list.reset();
    }

    pub fn put(&mut self, key: CacheKeyType) {
        self.access(key);
    }

    pub fn get(&mut self, key: &str) -> bool {
        self.access(key.to_string())
    }

    pub fn evict(&mut self) -> Vec<CacheKeyType> {
        let mut evicted = self.fetch_list.evict();
        evicted.extend(self.active_list.evict());
        evicted
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.fetch_list.delete(key) || self.active_list.delete(key)
    }

    pub fn get_active_data_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.active_list.get_data_tail(size)
    }

    pub fn get_fetch_data_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.fetch_list.get_data_tail(size)
    }

    pub fn get_active_ghost_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.active_list.get_ghost_tail(size)
    }

    pub fn get_fetch_ghost_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.fetch_list.get_ghost_tail(size)
    }

    fn access(&mut self, key: CacheKeyType) -> bool {
        let result = self.pop(&key);
        if result.item.is_empty() {
            if self.fetch_list.total_size() >= self.capacity() {
                if self.fetch_list.size() < self.capacity() {
                    self.fetch_list.evict_one_ghost();
                    self.replace(false);
                } else {
                    self.fetch_list.evict_one_data();
                }
            } else if self.total_size() >= self.capacity() {
                if self.total_size() >= self.capacity().saturating_mul(2) {
                    self.active_list.evict_one_ghost();
                }
                self.replace(false);
            }
            self.fetch_list.put(key);
            false
        } else if result.is_ghost {
            if result.is_active {
                let c = self.capacity();
                let mut p = self.fetch_list.capacity();
                let b1 = self.fetch_list.ghost_size();
                let b2 = self.active_list.ghost_size().saturating_add(1);
                let delta = std::cmp::max(1, b1 / b2.max(1));
                p = p.saturating_sub(delta);
                self.fetch_list.set_capacity(p);
                self.active_list.set_capacity(c.saturating_sub(p));
                self.replace(true);
                self.active_list.put(key);
            } else {
                let c = self.capacity();
                let mut p = self.fetch_list.capacity();
                let b1 = self.fetch_list.ghost_size().saturating_add(1);
                let b2 = self.active_list.ghost_size();
                let delta = std::cmp::max(1, b2 / b1.max(1));
                p = std::cmp::min(p.saturating_add(delta), c);
                self.fetch_list.set_capacity(p);
                self.active_list.set_capacity(c.saturating_sub(p));
                self.replace(false);
                self.active_list.put(key);
            }
            false
        } else {
            self.active_list.put(key);
            true
        }
    }

    fn replace(&mut self, key_in_active_ghost: bool) {
        if self.fetch_list.size() != 0
            && (self.fetch_list.size() > self.fetch_list.capacity()
                || (key_in_active_ghost && self.fetch_list.size() == self.fetch_list.capacity()))
        {
            self.fetch_list.downgrade();
        } else {
            self.active_list.downgrade();
        }
    }

    fn pop(&mut self, key: &str) -> ArcPopResult {
        let fetch = self.fetch_list.pop(key);
        if !fetch.item.is_empty() {
            return ArcPopResult {
                item: fetch.item,
                is_active: false,
                is_ghost: fetch.is_ghost,
            };
        }
        let active = self.active_list.pop(key);
        if !active.item.is_empty() {
            return ArcPopResult {
                item: active.item,
                is_active: true,
                is_ghost: active.is_ghost,
            };
        }
        ArcPopResult::default()
    }
}

#[allow(non_snake_case)]
impl ArcList {
    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn Size(&self) -> usize {
        self.size()
    }

    pub fn GhostSize(&self) -> usize {
        self.ghost_size()
    }

    pub fn TotalSize(&self) -> usize {
        self.total_size()
    }

    pub fn Capacity(&self) -> usize {
        self.capacity()
    }

    pub fn FetchCapacity(&self) -> usize {
        self.fetch_capacity()
    }

    pub fn ActiveCapacity(&self) -> usize {
        self.active_capacity()
    }

    pub fn DataFull(&self) -> bool {
        self.data_full()
    }

    pub fn Reset(&mut self) {
        self.reset();
    }

    pub fn Put(&mut self, key: CacheKeyType) {
        self.put(key);
    }

    pub fn Get(&mut self, key: &str) -> bool {
        self.get(key)
    }

    pub fn Evict(&mut self) -> Vec<CacheKeyType> {
        self.evict()
    }

    pub fn Delete(&mut self, key: &str) -> bool {
        self.delete(key)
    }

    pub fn GetActiveDataTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_active_data_tail(size)
    }

    pub fn GetFetchDataTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_fetch_data_tail(size)
    }

    pub fn GetActiveGhostTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_active_ghost_tail(size)
    }

    pub fn GetFetchGhostTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_fetch_ghost_tail(size)
    }
}

#[derive(Debug, Clone)]
pub struct ReplacementArc {
    item_capacity: usize,
    initialized: bool,
    arc_list: ArcList,
}

impl ReplacementArc {
    pub fn new(item_capacity: usize) -> Self {
        Self {
            item_capacity,
            initialized: false,
            arc_list: ArcList::new(item_capacity),
        }
    }

    pub fn init(&mut self) -> Result<(), CacheError> {
        self.initialized = true;
        self.arc_list.set_capacity(self.item_capacity);
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), CacheError> {
        self.initialized = false;
        self.arc_list.reset();
        Ok(())
    }

    pub fn item_capacity(&self) -> usize {
        self.item_capacity
    }

    pub fn set_item_capacity(&mut self, new_item_capacity: usize) {
        self.item_capacity = new_item_capacity;
        self.arc_list.set_capacity(self.item_capacity);
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn put(&mut self, key: CacheKeyType) {
        if !key.is_empty() {
            self.arc_list.put(key);
        }
    }

    pub fn get(&mut self, key: &str) -> bool {
        self.arc_list.get(key)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.arc_list.delete(key)
    }

    pub fn get_active_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.arc_list.get_active_data_tail(size)
    }

    pub fn get_fetch_tail(&self, size: usize) -> Vec<CacheKeyType> {
        self.arc_list.get_fetch_data_tail(size)
    }
}

#[allow(non_snake_case)]
impl ReplacementArc {
    pub fn Init(&mut self) -> Result<(), CacheError> {
        self.init()
    }

    pub fn Reset(&mut self) -> Result<(), CacheError> {
        self.reset()
    }

    pub fn GetItemCapacity(&self) -> usize {
        self.item_capacity()
    }

    pub fn SetItemCapacity(&mut self, new_item_capacity: usize) {
        self.set_item_capacity(new_item_capacity);
    }

    pub fn Put(&mut self, key: CacheKeyType) {
        self.put(key);
    }

    pub fn Get(&mut self, key: &str) -> bool {
        self.get(key)
    }

    pub fn Delete(&mut self, key: &str) -> bool {
        self.delete(key)
    }

    pub fn GetActiveTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_active_tail(size)
    }

    pub fn GetFetchTail(&self, size: usize) -> Vec<CacheKeyType> {
        self.get_fetch_tail(size)
    }
}

pub const BUFFER_INIT: u16 = 4;
pub const BUFFER_FETCHED: u16 = 8;
pub const BUFFER_ACTIVE: u16 = 16;

pub const HOT_LRU: u16 = 0;
pub const WARM_LRU: u16 = 1;
pub const COLD_LRU: u16 = 2;
pub const INVALID_LRU: u16 = u16::MAX;

type MemEvictionHandler = Arc<dyn Fn(CacheBuffer) + Send + Sync>;

fn cache_buffer_space(buffer: &CacheBuffer) -> usize {
    buffer.key().len().saturating_add(buffer.size())
}

fn clone_cache_buffer(buffer: &CacheBuffer) -> CacheBuffer {
    CacheBuffer {
        key: buffer.key().to_string(),
        value: Arc::clone(&buffer.value),
        logical_size: buffer.logical_size,
        tier: buffer.tier,
        cache: None,
        handle: None,
    }
}

#[derive(Debug, Clone)]
pub struct ReplacementPolicyBase {
    capacity: usize,
    used: usize,
    initialized: bool,
}

impl ReplacementPolicyBase {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "replacement policy capacity must be positive");
        Self {
            capacity,
            used: 0,
            initialized: false,
        }
    }

    pub fn init(&mut self) -> Result<(), CacheError> {
        self.initialized = true;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), CacheError> {
        self.used = 0;
        self.initialized = false;
        Ok(())
    }

    pub fn get_capacity(&self) -> usize {
        self.capacity
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        assert!(capacity > 0, "replacement policy capacity must be positive");
        self.capacity = capacity;
    }

    pub fn get_used_space(&self) -> usize {
        self.used
    }

    pub fn get_free_space(&self) -> usize {
        self.capacity.saturating_sub(self.used)
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

#[allow(non_snake_case)]
impl ReplacementPolicyBase {
    pub fn Init(&mut self) -> Result<(), CacheError> {
        self.init()
    }

    pub fn Reset(&mut self) -> Result<(), CacheError> {
        self.reset()
    }

    pub fn GetCapacity(&self) -> usize {
        self.get_capacity()
    }

    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn GetUsedSpace(&self) -> usize {
        self.get_used_space()
    }

    pub fn GetFreeSpace(&self) -> usize {
        self.get_free_space()
    }
}

pub type ReplacementPolicy = ReplacementPolicyBase;

pub struct ReplacementFIFO {
    base: ReplacementPolicyBase,
    index: HashMap<String, CacheBuffer>,
    queue: VecDeque<String>,
    mem_eviction_func: Option<MemEvictionHandler>,
}

impl ReplacementFIFO {
    pub fn new(capacity: usize) -> Self {
        Self {
            base: ReplacementPolicyBase::new(capacity),
            index: HashMap::new(),
            queue: VecDeque::new(),
            mem_eviction_func: None,
        }
    }

    pub fn init(&mut self) -> Result<(), CacheError> {
        self.base.init()
    }

    pub fn reset(&mut self) -> Result<(), CacheError> {
        self.index.clear();
        self.queue.clear();
        self.base.used = 0;
        self.base.initialized = false;
        Ok(())
    }

    pub fn put(&mut self, buffer: CacheBuffer) -> Vec<CacheBuffer> {
        if !self.base.initialized {
            return Vec::new();
        }
        let key = buffer.key().to_string();
        if key.is_empty() {
            return Vec::new();
        }
        if let Some(old) = self.index.remove(&key) {
            self.base.used = self.base.used.saturating_sub(cache_buffer_space(&old));
            self.queue.retain(|candidate| candidate != &key);
        }
        self.base.used = self.base.used.saturating_add(cache_buffer_space(&buffer));
        self.queue.push_back(key.clone());
        self.index.insert(key, buffer);
        self.evict_to_capacity()
    }

    pub fn update_cache_buffer(
        &mut self,
        key: &str,
        old_data: &[u8],
        buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        let old = self.index.get(key).ok_or(CacheError::NotFound)?;
        if old.data() != old_data {
            return Err(CacheError::ReplaceMismatch);
        }
        self.base.used = self.base.used.saturating_sub(cache_buffer_space(old));
        self.base.used = self.base.used.saturating_add(cache_buffer_space(&buffer));
        self.index.insert(key.to_string(), buffer);
        let _ = self.evict_to_capacity();
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<CacheBuffer> {
        self.index.get(key).map(clone_cache_buffer)
    }

    pub fn peek(&self, key: &str) -> Option<CacheBuffer> {
        self.get(key)
    }

    pub fn delete(&mut self, key: &str) -> Option<CacheBuffer> {
        let buffer = self.index.remove(key)?;
        self.base.used = self.base.used.saturating_sub(cache_buffer_space(&buffer));
        self.queue.retain(|candidate| candidate != key);
        Some(buffer)
    }

    pub fn get_capacity(&self) -> usize {
        self.base.get_capacity()
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.base.set_capacity(capacity);
        let _ = self.evict_to_capacity();
    }

    pub fn get_used_space(&self) -> usize {
        self.base.get_used_space()
    }

    pub fn get_free_space(&self) -> usize {
        self.base.get_free_space()
    }

    pub fn get_item_num(&self) -> usize {
        self.index.len()
    }

    pub fn register_mem_eviction_handler<F>(&mut self, func: F)
    where
        F: Fn(CacheBuffer) + Send + Sync + 'static,
    {
        self.mem_eviction_func = Some(Arc::new(func));
    }

    fn evict_to_capacity(&mut self) -> Vec<CacheBuffer> {
        let mut evicted = Vec::new();
        while self.base.used > self.base.capacity {
            let Some(key) = self.queue.pop_front() else {
                break;
            };
            let Some(buffer) = self.index.remove(&key) else {
                continue;
            };
            self.base.used = self.base.used.saturating_sub(cache_buffer_space(&buffer));
            if let Some(handler) = &self.mem_eviction_func {
                handler(clone_cache_buffer(&buffer));
            }
            evicted.push(buffer);
        }
        evicted
    }
}

#[allow(non_snake_case)]
impl ReplacementFIFO {
    pub fn Init(&mut self) -> Result<(), CacheError> {
        self.init()
    }

    pub fn Reset(&mut self) -> Result<(), CacheError> {
        self.reset()
    }

    pub fn Put(&mut self, buffer: CacheBuffer) -> Vec<CacheBuffer> {
        self.put(buffer)
    }

    pub fn UpdateCacheBuffer(
        &mut self,
        key: &str,
        old_data: &[u8],
        buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        self.update_cache_buffer(key, old_data, buffer)
    }

    pub fn Get(&self, key: &str) -> Option<CacheBuffer> {
        self.get(key)
    }

    pub fn Peek(&self, key: &str) -> Option<CacheBuffer> {
        self.peek(key)
    }

    pub fn Delete(&mut self, key: &str) -> Option<CacheBuffer> {
        self.delete(key)
    }

    pub fn GetCapacity(&self) -> usize {
        self.get_capacity()
    }

    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn GetUsedSpace(&self) -> usize {
        self.get_used_space()
    }

    pub fn GetFreeSpace(&self) -> usize {
        self.get_free_space()
    }

    pub fn GetItemNum(&self) -> usize {
        self.get_item_num()
    }

    pub fn RegisterMemEvictionHandler<F>(&mut self, func: F)
    where
        F: Fn(CacheBuffer) + Send + Sync + 'static,
    {
        self.register_mem_eviction_handler(func);
    }
}

#[derive(Debug)]
struct SlruEntry {
    buffer: CacheBuffer,
    flag: u16,
    lru: u16,
}

pub struct ReplacementSLRU {
    base: ReplacementPolicyBase,
    index: HashMap<String, SlruEntry>,
    hot: VecDeque<String>,
    warm: VecDeque<String>,
    cold: VecDeque<String>,
    lru_maintainer_enabled: bool,
    mem_eviction_func: Option<MemEvictionHandler>,
}

impl ReplacementSLRU {
    pub fn new(capacity: usize) -> Self {
        Self {
            base: ReplacementPolicyBase::new(capacity),
            index: HashMap::new(),
            hot: VecDeque::new(),
            warm: VecDeque::new(),
            cold: VecDeque::new(),
            lru_maintainer_enabled: true,
            mem_eviction_func: None,
        }
    }

    pub fn init(&mut self) -> Result<(), CacheError> {
        self.base.init()
    }

    pub fn reset(&mut self) -> Result<(), CacheError> {
        self.index.clear();
        self.hot.clear();
        self.warm.clear();
        self.cold.clear();
        self.base.used = 0;
        self.base.initialized = false;
        Ok(())
    }

    pub fn put(&mut self, buffer: CacheBuffer) -> Vec<CacheBuffer> {
        if !self.base.initialized {
            return Vec::new();
        }
        let key = buffer.key().to_string();
        if key.is_empty() {
            return Vec::new();
        }
        if let Some(old) = self.index.remove(&key) {
            self.base.used = self
                .base
                .used
                .saturating_sub(cache_buffer_space(&old.buffer));
            self.remove_from_lists(&key);
        }
        self.base.used = self.base.used.saturating_add(cache_buffer_space(&buffer));
        self.hot.push_front(key.clone());
        self.index.insert(
            key,
            SlruEntry {
                buffer,
                flag: BUFFER_INIT,
                lru: HOT_LRU,
            },
        );
        self.evict_to_capacity()
    }

    pub fn update_cache_buffer(
        &mut self,
        key: &str,
        old_data: &[u8],
        buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        let entry = self.index.get_mut(key).ok_or(CacheError::NotFound)?;
        if entry.buffer.data() != old_data {
            return Err(CacheError::ReplaceMismatch);
        }
        self.base.used = self
            .base
            .used
            .saturating_sub(cache_buffer_space(&entry.buffer));
        self.base.used = self.base.used.saturating_add(cache_buffer_space(&buffer));
        entry.buffer = buffer;
        let _ = self.evict_to_capacity();
        Ok(())
    }

    pub fn get(&mut self, key: &str) -> Option<CacheBuffer> {
        self.touch(key);
        self.index
            .get(key)
            .map(|entry| clone_cache_buffer(&entry.buffer))
    }

    pub fn peek(&self, key: &str) -> Option<CacheBuffer> {
        self.index
            .get(key)
            .map(|entry| clone_cache_buffer(&entry.buffer))
    }

    pub fn delete(&mut self, key: &str) -> Option<CacheBuffer> {
        let entry = self.index.remove(key)?;
        self.base.used = self
            .base
            .used
            .saturating_sub(cache_buffer_space(&entry.buffer));
        self.remove_from_lists(key);
        Some(entry.buffer)
    }

    pub fn get_capacity(&self) -> usize {
        self.base.get_capacity()
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.base.set_capacity(capacity);
        let _ = self.evict_to_capacity();
    }

    pub fn get_used_space(&self) -> usize {
        self.base.get_used_space()
    }

    pub fn get_free_space(&self) -> usize {
        self.base.get_free_space()
    }

    pub fn get_item_num(&self) -> usize {
        self.index.len()
    }

    pub fn register_mem_eviction_handler<F>(&mut self, func: F)
    where
        F: Fn(CacheBuffer) + Send + Sync + 'static,
    {
        self.mem_eviction_func = Some(Arc::new(func));
    }

    pub fn test_check_lru_pos(&self, key: &str) -> u16 {
        self.index
            .get(key)
            .map(|entry| entry.lru)
            .unwrap_or(INVALID_LRU)
    }

    pub fn test_check_buffer_flag(&self, key: &str) -> u16 {
        self.index.get(key).map(|entry| entry.flag).unwrap_or(0)
    }

    pub fn test_config_lru_maintainer(&mut self, status: bool) {
        self.lru_maintainer_enabled = status;
    }

    pub fn test_wait_for_lru_maintainer(&self) {}

    pub fn test_notify_maintainer_move_complete(&self) {}

    fn touch(&mut self, key: &str) {
        let Some(lru) = self.index.get_mut(key).map(|entry| {
            entry.flag = if entry.flag == BUFFER_INIT {
                BUFFER_FETCHED
            } else {
                BUFFER_ACTIVE
            };
            entry.lru
        }) else {
            return;
        };
        self.move_to_front(key, lru);
    }

    fn evict_to_capacity(&mut self) -> Vec<CacheBuffer> {
        let mut evicted = Vec::new();
        let mut attempts = 0usize;
        while self.base.used > self.base.capacity && attempts <= self.index.len().saturating_mul(4)
        {
            attempts = attempts.saturating_add(1);
            if let Some(buffer) = self.try_evict_or_shuffle_cold() {
                evicted.push(buffer);
                continue;
            }
            if self.shuffle_hot_tail() {
                continue;
            }
            if self.shuffle_warm_tail() {
                continue;
            }
            if let Some(buffer) = self.force_evict_any_tail() {
                evicted.push(buffer);
            } else {
                break;
            }
        }
        evicted
    }

    fn try_evict_or_shuffle_cold(&mut self) -> Option<CacheBuffer> {
        let key = self.cold.pop_back()?;
        let entry = self.index.get_mut(&key)?;
        if entry.flag >= BUFFER_FETCHED {
            entry.flag = BUFFER_ACTIVE;
            entry.lru = WARM_LRU;
            self.warm.push_front(key);
            return None;
        }
        self.evict_key(&key)
    }

    fn shuffle_hot_tail(&mut self) -> bool {
        let Some(key) = self.hot.pop_back() else {
            return false;
        };
        let Some(entry) = self.index.get_mut(&key) else {
            return true;
        };
        if entry.flag >= BUFFER_FETCHED {
            entry.flag = BUFFER_ACTIVE;
            entry.lru = WARM_LRU;
            self.warm.push_front(key);
        } else {
            entry.lru = COLD_LRU;
            self.cold.push_front(key);
        }
        true
    }

    fn shuffle_warm_tail(&mut self) -> bool {
        let Some(key) = self.warm.pop_back() else {
            return false;
        };
        let Some(entry) = self.index.get_mut(&key) else {
            return true;
        };
        if entry.flag >= BUFFER_ACTIVE {
            entry.flag = BUFFER_FETCHED;
            self.warm.push_front(key);
        } else {
            entry.lru = COLD_LRU;
            self.cold.push_front(key);
        }
        true
    }

    fn force_evict_any_tail(&mut self) -> Option<CacheBuffer> {
        for lru in [COLD_LRU, HOT_LRU, WARM_LRU] {
            let key = match lru {
                HOT_LRU => self.hot.pop_back(),
                WARM_LRU => self.warm.pop_back(),
                COLD_LRU => self.cold.pop_back(),
                _ => None,
            };
            if let Some(key) = key {
                if let Some(buffer) = self.evict_key(&key) {
                    return Some(buffer);
                }
            }
        }
        None
    }

    fn evict_key(&mut self, key: &str) -> Option<CacheBuffer> {
        let entry = self.index.remove(key)?;
        self.base.used = self
            .base
            .used
            .saturating_sub(cache_buffer_space(&entry.buffer));
        if let Some(handler) = &self.mem_eviction_func {
            handler(clone_cache_buffer(&entry.buffer));
        }
        Some(entry.buffer)
    }

    fn remove_from_lists(&mut self, key: &str) {
        self.hot.retain(|candidate| candidate != key);
        self.warm.retain(|candidate| candidate != key);
        self.cold.retain(|candidate| candidate != key);
    }

    fn move_to_front(&mut self, key: &str, lru: u16) {
        match lru {
            HOT_LRU => {
                self.hot.retain(|candidate| candidate != key);
                self.hot.push_front(key.to_string());
            }
            WARM_LRU => {
                self.warm.retain(|candidate| candidate != key);
                self.warm.push_front(key.to_string());
            }
            COLD_LRU => {
                self.cold.retain(|candidate| candidate != key);
                self.cold.push_front(key.to_string());
            }
            _ => {}
        }
    }
}

#[allow(non_snake_case)]
impl ReplacementSLRU {
    pub fn Init(&mut self) -> Result<(), CacheError> {
        self.init()
    }

    pub fn Reset(&mut self) -> Result<(), CacheError> {
        self.reset()
    }

    pub fn Put(&mut self, buffer: CacheBuffer) -> Vec<CacheBuffer> {
        self.put(buffer)
    }

    pub fn UpdateCacheBuffer(
        &mut self,
        key: &str,
        old_data: &[u8],
        buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        self.update_cache_buffer(key, old_data, buffer)
    }

    pub fn Get(&mut self, key: &str) -> Option<CacheBuffer> {
        self.get(key)
    }

    pub fn Peek(&self, key: &str) -> Option<CacheBuffer> {
        self.peek(key)
    }

    pub fn Delete(&mut self, key: &str) -> Option<CacheBuffer> {
        self.delete(key)
    }

    pub fn GetCapacity(&self) -> usize {
        self.get_capacity()
    }

    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn GetUsedSpace(&self) -> usize {
        self.get_used_space()
    }

    pub fn GetFreeSpace(&self) -> usize {
        self.get_free_space()
    }

    pub fn GetItemNum(&self) -> usize {
        self.get_item_num()
    }

    pub fn RegisterMemEvictionHandler<F>(&mut self, func: F)
    where
        F: Fn(CacheBuffer) + Send + Sync + 'static,
    {
        self.register_mem_eviction_handler(func);
    }

    pub fn TEST_CheckLRUPos(&self, key: &str) -> u16 {
        self.test_check_lru_pos(key)
    }

    pub fn TEST_CheckBufferFlag(&self, key: &str) -> u16 {
        self.test_check_buffer_flag(key)
    }

    pub fn TEST_ConfigLRUMaintainer(&mut self, status: bool) {
        self.test_config_lru_maintainer(status);
    }

    pub fn TEST_WaitForLRUMaintainer(&self) {
        self.test_wait_for_lru_maintainer();
    }

    pub fn TEST_NotifyMaintainerMoveComplete(&self) {
        self.test_notify_maintainer_move_complete();
    }
}

