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

/// How values are encoded before they are stored.
///
/// **The default is `Zstd { level: 1 }`, not `None`.** Either way the codec
/// applies only to values of at least `min_compress_bytes`; smaller values are
/// stored raw, and a value that fails to compress is also stored raw rather
/// than growing.
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

/// The replacement policies this crate actually implements.
///
/// Distinct from [`ReplacementPolicyKind`], which is the wider vocabulary a
/// configuration file may use. Converting from that one is lossy on purpose:
/// its `Lru` and its `MaxCode` sentinel both arrive here as
/// `WeightedHotnessLru`, because plain LRU is not implemented separately.
///
/// Defaults to `WeightedHotnessLru`.
/// Which entry a tier gives up when it needs room.
///
/// `Slru` selects the same eviction as `WeightedHotnessLru` on a
/// [`MultiLayerCache`] tier: the two share a branch in every tier's victim
/// selection, and a scan-resistance run gives them identical hit rates and
/// identical eviction counts at every insertion point. The segmented policy
/// itself exists, as [`ReplacementSlru`], but nothing connects it to tier
/// eviction yet.
///
/// It is kept as a distinct value rather than removed, because a
/// configuration naming it should keep working; [`CacheOptions::validate`]
/// reports that it resolves to the weighted policy, so asking for it is not
/// silently answered with something else.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheReplacementPolicy {
    Fifo,
    Slru,
    #[default]
    WeightedHotnessLru,
}

impl CacheReplacementPolicy {
    /// The policy this name asks for, or `None` if no policy has it.
    ///
    /// The only list of names. [`Self::from_config_name`] is written in terms
    /// of this one, so a name cannot be accepted by a check and then resolve
    /// to something else -- which is the failure that matters, because it is
    /// silent.
    pub fn try_from_config_name(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("fifo") {
            Some(Self::Fifo)
        } else if value.eq_ignore_ascii_case("slru") {
            Some(Self::Slru)
        } else if value.eq_ignore_ascii_case("weightedhotnesslru")
            // Plain LRU is not a separate policy here; the weighted one is
            // what a configuration asking for LRU gets, and has always got.
            // Named rather than left to the fallback, so asking for it is not
            // reported as a name nobody recognises.
            || value.eq_ignore_ascii_case("lru")
        {
            Some(Self::WeightedHotnessLru)
        } else {
            None
        }
    }

    /// Every name a configuration may use, for saying so in an error.
    pub fn config_names() -> Vec<&'static str> {
        [Self::Fifo, Self::Slru, Self::WeightedHotnessLru]
            .into_iter()
            .map(Self::as_config_name)
            .collect()
    }

    /// The policy this name asks for, falling back to the default.
    ///
    /// Kept infallible because a cache built from a configuration file should
    /// start rather than refuse. [`CacheOptions::validate`] is what says the
    /// name was not recognised.
    pub fn from_config_name(value: &str) -> Self {
        Self::try_from_config_name(value).unwrap_or_default()
    }

    pub fn as_config_name(self) -> &'static str {
        match self {
            Self::Fifo => "FIFO",
            Self::Slru => "SLRU",
            Self::WeightedHotnessLru => "WeightedHotnessLru",
        }
    }
}

pub type PolicyKey = String;

/// Sentinel for "no node" in the intrusive key lists below.
const KEY_LIST_NIL: u32 = u32::MAX;

/// One slot in a [`KeyArena`]. A node is linked into at most one [`KeyList`] at
/// a time and carries its key, so eviction can walk from a list end back into
/// the owning index.
#[derive(Debug, Clone)]
struct KeyListNode {
    key: PolicyKey,
    prev: u32,
    next: u32,
}

/// Backing storage for [`KeyList`]. Released slots are recycled, so a policy
/// that churns keys does not grow the arena without bound.
#[derive(Debug, Clone, Default)]
struct KeyArena {
    nodes: Vec<KeyListNode>,
    free: Vec<u32>,
}

/// An intrusive doubly-linked list of keys held in a [`KeyArena`].
///
/// Unlike a `VecDeque` of keys, this unlinks or re-fronts an arbitrary element
/// in O(1), which is what keeps policy lookups and deletes independent of how
/// many entries are cached. `used` is a byte counter for policies that budget
/// their lists by size; policies that only care about order leave it at zero.
#[derive(Debug, Clone)]
struct KeyList {
    head: u32,
    tail: u32,
    used: usize,
    len: usize,
}

impl KeyList {
    fn new() -> Self {
        Self {
            head: KEY_LIST_NIL,
            tail: KEY_LIST_NIL,
            used: 0,
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.head = KEY_LIST_NIL;
        self.tail = KEY_LIST_NIL;
        self.used = 0;
        self.len = 0;
    }

    fn front(&self) -> Option<u32> {
        (self.head != KEY_LIST_NIL).then_some(self.head)
    }

    fn back(&self) -> Option<u32> {
        (self.tail != KEY_LIST_NIL).then_some(self.tail)
    }
}

impl Default for KeyList {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyArena {
    fn new() -> Self {
        Self::default()
    }

    fn alloc(&mut self, key: &str) -> u32 {
        if let Some(index) = self.free.pop() {
            let node = &mut self.nodes[index as usize];
            node.key.clear();
            node.key.push_str(key);
            node.prev = KEY_LIST_NIL;
            node.next = KEY_LIST_NIL;
            return index;
        }
        self.nodes.push(KeyListNode {
            key: key.to_string(),
            prev: KEY_LIST_NIL,
            next: KEY_LIST_NIL,
        });
        (self.nodes.len() - 1) as u32
    }

    fn release(&mut self, node: u32) {
        let slot = &mut self.nodes[node as usize];
        slot.key.clear();
        slot.prev = KEY_LIST_NIL;
        slot.next = KEY_LIST_NIL;
        self.free.push(node);
    }

    fn key(&self, node: u32) -> &str {
        &self.nodes[node as usize].key
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.free.clear();
    }

    fn link_front(&mut self, list: &mut KeyList, node: u32) {
        let old_head = list.head;
        self.nodes[node as usize].prev = KEY_LIST_NIL;
        self.nodes[node as usize].next = old_head;
        if old_head == KEY_LIST_NIL {
            list.tail = node;
        } else {
            self.nodes[old_head as usize].prev = node;
        }
        list.head = node;
        list.len = list.len.saturating_add(1);
    }

    fn link_back(&mut self, list: &mut KeyList, node: u32) {
        let old_tail = list.tail;
        self.nodes[node as usize].next = KEY_LIST_NIL;
        self.nodes[node as usize].prev = old_tail;
        if old_tail == KEY_LIST_NIL {
            list.head = node;
        } else {
            self.nodes[old_tail as usize].next = node;
        }
        list.tail = node;
        list.len = list.len.saturating_add(1);
    }

    fn unlink(&mut self, list: &mut KeyList, node: u32) {
        let prev = self.nodes[node as usize].prev;
        let next = self.nodes[node as usize].next;
        if prev == KEY_LIST_NIL {
            list.head = next;
        } else {
            self.nodes[prev as usize].next = next;
        }
        if next == KEY_LIST_NIL {
            list.tail = prev;
        } else {
            self.nodes[next as usize].prev = prev;
        }
        self.nodes[node as usize].prev = KEY_LIST_NIL;
        self.nodes[node as usize].next = KEY_LIST_NIL;
        list.len = list.len.saturating_sub(1);
    }

    /// Collect up to `size` keys walking from the tail toward the head.
    fn collect_from_tail(&self, list: &KeyList, size: usize) -> Vec<PolicyKey> {
        let mut collected = Vec::new();
        let mut cursor = list.tail;
        while cursor != KEY_LIST_NIL && collected.len() < size {
            collected.push(self.nodes[cursor as usize].key.clone());
            cursor = self.nodes[cursor as usize].prev;
        }
        collected
    }
}

#[derive(Debug, Clone)]
pub struct BaseLruList {
    capacity: usize,
    arena: KeyArena,
    list: KeyList,
    index: HashMap<PolicyKey, u32>,
}

impl BaseLruList {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            arena: KeyArena::new(),
            list: KeyList::new(),
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

    pub fn put(&mut self, key: PolicyKey) {
        if let Some(&node) = self.index.get(&key) {
            self.arena.unlink(&mut self.list, node);
            self.arena.link_front(&mut self.list, node);
            return;
        }
        let node = self.arena.alloc(&key);
        self.arena.link_front(&mut self.list, node);
        self.index.insert(key, node);
    }

    pub fn get(&mut self, key: &str) -> bool {
        let Some(&node) = self.index.get(key) else {
            return false;
        };
        self.arena.unlink(&mut self.list, node);
        self.arena.link_front(&mut self.list, node);
        true
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let Some(node) = self.index.remove(key) else {
            return false;
        };
        self.arena.unlink(&mut self.list, node);
        self.arena.release(node);
        true
    }

    pub fn get_tail(&self, size: usize) -> Vec<PolicyKey> {
        self.arena.collect_from_tail(&self.list, size)
    }

    pub fn evict(&mut self) -> Vec<PolicyKey> {
        let mut evicted = Vec::new();
        while self.index.len() > self.capacity {
            let popped = self.evict_one();
            if popped.is_empty() {
                break;
            }
            evicted.extend(popped);
        }
        evicted
    }

    pub fn evict_one(&mut self) -> Vec<PolicyKey> {
        let Some(node) = self.list.back() else {
            return Vec::new();
        };
        let key = self.arena.key(node).to_string();
        self.arena.unlink(&mut self.list, node);
        self.arena.release(node);
        self.index.remove(&key);
        vec![key]
    }

    pub fn reset(&mut self) {
        self.list.clear();
        self.arena.clear();
        self.index.clear();
    }
}

#[allow(non_snake_case)]
impl BaseLruList {
    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn Capacity(&self) -> usize {
        self.capacity()
    }

    pub fn Size(&self) -> usize {
        self.size()
    }

    pub fn Put(&mut self, key: PolicyKey) {
        self.put(key);
    }

    pub fn Get(&mut self, key: &str) -> bool {
        self.get(key)
    }

    pub fn Delete(&mut self, key: &str) -> bool {
        self.delete(key)
    }

    pub fn GetTail(&self, size: usize) -> Vec<PolicyKey> {
        self.get_tail(size)
    }

    pub fn Evict(&mut self) -> Vec<PolicyKey> {
        self.evict()
    }

    pub fn EvictOne(&mut self) -> Vec<PolicyKey> {
        self.evict_one()
    }

    pub fn Reset(&mut self) {
        self.reset();
    }
}

impl Default for BaseLruList {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GhostLruPopResult {
    pub item: PolicyKey,
    pub is_ghost: bool,
}

#[derive(Debug, Clone)]
pub struct GhostLruList {
    data_list: BaseLruList,
    ghost_list: BaseLruList,
}

impl GhostLruList {
    pub fn new(capacity: usize) -> Self {
        Self {
            data_list: BaseLruList::new(capacity),
            ghost_list: BaseLruList::new(capacity),
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

    pub fn put(&mut self, key: PolicyKey) {
        self.data_list.put(key);
    }

    pub fn put_ghost(&mut self, key: PolicyKey) {
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

    pub fn evict_one_data(&mut self) -> Vec<PolicyKey> {
        self.data_list.evict_one()
    }

    pub fn evict_one_ghost(&mut self) -> Vec<PolicyKey> {
        self.ghost_list.evict_one()
    }

    pub fn get_data_tail(&self, size: usize) -> Vec<PolicyKey> {
        self.data_list.get_tail(size)
    }

    pub fn get_ghost_tail(&self, size: usize) -> Vec<PolicyKey> {
        self.ghost_list.get_tail(size)
    }

    pub fn evict(&mut self) -> Vec<PolicyKey> {
        self.ghost_list.evict();
        self.data_list.evict()
    }

    pub fn pop(&mut self, key: &str) -> GhostLruPopResult {
        if self.data_list.delete(key) {
            return GhostLruPopResult {
                item: key.to_string(),
                is_ghost: false,
            };
        }
        if self.ghost_list.delete(key) {
            return GhostLruPopResult {
                item: key.to_string(),
                is_ghost: true,
            };
        }
        GhostLruPopResult::default()
    }

    pub fn reset(&mut self) {
        self.data_list.reset();
        self.ghost_list.reset();
    }
}

#[allow(non_snake_case)]
impl GhostLruList {
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

    pub fn Put(&mut self, key: PolicyKey) {
        self.put(key);
    }

    pub fn PutGhost(&mut self, key: PolicyKey) {
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

    pub fn EvictOneData(&mut self) -> Vec<PolicyKey> {
        self.evict_one_data()
    }

    pub fn EvictOneGhost(&mut self) -> Vec<PolicyKey> {
        self.evict_one_ghost()
    }

    pub fn GetDataTail(&self, size: usize) -> Vec<PolicyKey> {
        self.get_data_tail(size)
    }

    pub fn GetGhostTail(&self, size: usize) -> Vec<PolicyKey> {
        self.get_ghost_tail(size)
    }

    pub fn Evict(&mut self) -> Vec<PolicyKey> {
        self.evict()
    }

    pub fn Pop(&mut self, key: &str) -> GhostLruPopResult {
        self.pop(key)
    }

    pub fn Reset(&mut self) {
        self.reset();
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArcPopResult {
    pub item: PolicyKey,
    pub is_active: bool,
    pub is_ghost: bool,
}

#[derive(Debug, Clone)]
pub struct ArcList {
    capacity: usize,
    fetch_list: GhostLruList,
    active_list: GhostLruList,
}

impl ArcList {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            fetch_list: GhostLruList::new(capacity / 2),
            active_list: GhostLruList::new(capacity.saturating_sub(capacity / 2)),
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

    pub fn put(&mut self, key: PolicyKey) {
        self.access(key);
    }

    pub fn get(&mut self, key: &str) -> bool {
        self.access(key.to_string())
    }

    pub fn evict(&mut self) -> Vec<PolicyKey> {
        let mut evicted = self.fetch_list.evict();
        evicted.extend(self.active_list.evict());
        evicted
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.fetch_list.delete(key) || self.active_list.delete(key)
    }

    pub fn get_active_data_tail(&self, size: usize) -> Vec<PolicyKey> {
        self.active_list.get_data_tail(size)
    }

    pub fn get_fetch_data_tail(&self, size: usize) -> Vec<PolicyKey> {
        self.fetch_list.get_data_tail(size)
    }

    pub fn get_active_ghost_tail(&self, size: usize) -> Vec<PolicyKey> {
        self.active_list.get_ghost_tail(size)
    }

    pub fn get_fetch_ghost_tail(&self, size: usize) -> Vec<PolicyKey> {
        self.fetch_list.get_ghost_tail(size)
    }

    fn access(&mut self, key: PolicyKey) -> bool {
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

    pub fn Put(&mut self, key: PolicyKey) {
        self.put(key);
    }

    pub fn Get(&mut self, key: &str) -> bool {
        self.get(key)
    }

    pub fn Evict(&mut self) -> Vec<PolicyKey> {
        self.evict()
    }

    pub fn Delete(&mut self, key: &str) -> bool {
        self.delete(key)
    }

    pub fn GetActiveDataTail(&self, size: usize) -> Vec<PolicyKey> {
        self.get_active_data_tail(size)
    }

    pub fn GetFetchDataTail(&self, size: usize) -> Vec<PolicyKey> {
        self.get_fetch_data_tail(size)
    }

    pub fn GetActiveGhostTail(&self, size: usize) -> Vec<PolicyKey> {
        self.get_active_ghost_tail(size)
    }

    pub fn GetFetchGhostTail(&self, size: usize) -> Vec<PolicyKey> {
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

    /// Drop every tracked key and put this policy back in the uninitialized
    /// state.
    ///
    /// Unlike the Fifo and SLRU policies, resetting this one *does* require
    /// another `init()` before it will accept keys. That asymmetry is
    /// intentional and load-bearing for callers that drive the adaptive
    /// lists directly; do not "harmonize" it away.
    /// Clear every tracked key and mark the policy uninitialized.
    ///
    /// Clearing the flag matches the reference, whose `Reset` clears it for this
    /// policy and *not* for [`ReplacementSlru`] or [`ReplacementFifo`]. The
    /// asymmetry is deliberate; do not harmonize it away.
    ///
    /// Note what it does **not** mean here. Unlike those two, this policy's
    /// [`put`](Self::put), [`get`](Self::get) and [`delete`](Self::delete) never
    /// consult the flag, so an "uninitialized" `ReplacementArc` keeps working
    /// normally. [`is_initialized`](Self::is_initialized) reports lifecycle
    /// state on this policy, not whether a `put` will land.
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

    pub fn put(&mut self, key: PolicyKey) {
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

    pub fn get_active_tail(&self, size: usize) -> Vec<PolicyKey> {
        self.arc_list.get_active_data_tail(size)
    }

    pub fn get_fetch_tail(&self, size: usize) -> Vec<PolicyKey> {
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

    pub fn Put(&mut self, key: PolicyKey) {
        self.put(key);
    }

    pub fn Get(&mut self, key: &str) -> bool {
        self.get(key)
    }

    pub fn Delete(&mut self, key: &str) -> bool {
        self.delete(key)
    }

    pub fn GetActiveTail(&self, size: usize) -> Vec<PolicyKey> {
        self.get_active_tail(size)
    }

    pub fn GetFetchTail(&self, size: usize) -> Vec<PolicyKey> {
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

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        assert!(capacity > 0, "replacement policy capacity must be positive");
        self.capacity = capacity;
    }

    pub fn used_space(&self) -> usize {
        self.used
    }

    pub fn free_space(&self) -> usize {
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
        self.capacity()
    }

    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn GetUsedSpace(&self) -> usize {
        self.used_space()
    }

    pub fn GetFreeSpace(&self) -> usize {
        self.free_space()
    }
}

pub type ReplacementPolicy = ReplacementPolicyBase;

#[derive(Debug)]
struct FifoEntry {
    buffer: CacheBuffer,
    node: u32,
}

pub struct ReplacementFifo {
    base: ReplacementPolicyBase,
    index: HashMap<String, FifoEntry>,
    arena: KeyArena,
    queue: KeyList,
    mem_eviction_func: Option<MemEvictionHandler>,
}

impl ReplacementFifo {
    pub fn new(capacity: usize) -> Self {
        Self {
            base: ReplacementPolicyBase::new(capacity),
            index: HashMap::new(),
            arena: KeyArena::new(),
            queue: KeyList::new(),
            mem_eviction_func: None,
        }
    }

    pub fn init(&mut self) -> Result<(), CacheError> {
        self.base.init()
    }

    /// Drop every tracked buffer and return the policy to an empty state.
    ///
    /// A successful reset leaves the policy *initialized and usable*: it
    /// empties the index, not the policy's lifecycle. Clearing the
    /// initialized flag here would make every later `put` return "no
    /// evictions" while silently discarding the buffer, turning a reset
    /// cache into a black hole that never reports an error.
    pub fn reset(&mut self) -> Result<(), CacheError> {
        self.index.clear();
        self.queue.clear();
        self.arena.clear();
        self.base.used = 0;
        Ok(())
    }

    /// Track `buffer`, evicting from the front of the queue if that pushes the
    /// policy over capacity.
    ///
    /// Overwriting a key that is already tracked keeps its original queue
    /// position: first-in-first-out orders by when a key first entered the
    /// cache, not by when it was last written. Only the byte accounting moves.
    /// # Lifecycle
    ///
    /// A policy from `new` is **not initialized**, and an uninitialized policy
    /// discards what it is given: this returns an empty vector and the buffer is
    /// dropped, with no error. Call [`init`](Self::init) first.
    /// `is_initialized` on the policy base is the only way to tell.
    pub fn put(&mut self, buffer: CacheBuffer) -> Vec<CacheBuffer> {
        if !self.base.initialized {
            return Vec::new();
        }
        let key = buffer.key().to_string();
        if key.is_empty() {
            return Vec::new();
        }
        let space = cache_buffer_space(&buffer);
        if let Some(entry) = self.index.get_mut(&key) {
            let old_space = cache_buffer_space(&entry.buffer);
            entry.buffer = buffer;
            self.base.used = self
                .base
                .used
                .saturating_sub(old_space)
                .saturating_add(space);
        } else {
            let node = self.arena.alloc(&key);
            self.arena.link_back(&mut self.queue, node);
            self.base.used = self.base.used.saturating_add(space);
            self.index.insert(key, FifoEntry { buffer, node });
        }
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
        let old_space = cache_buffer_space(&entry.buffer);
        let new_space = cache_buffer_space(&buffer);
        entry.buffer = buffer;
        self.base.used = self
            .base
            .used
            .saturating_sub(old_space)
            .saturating_add(new_space);
        let _ = self.evict_to_capacity();
        Ok(())
    }

    /// A read never changes eviction order under this policy, so `get` and
    /// `peek` are the same lookup.
    pub fn get(&self, key: &str) -> Option<CacheBuffer> {
        self.index
            .get(key)
            .map(|entry| clone_cache_buffer(&entry.buffer))
    }

    pub fn peek(&self, key: &str) -> Option<CacheBuffer> {
        self.get(key)
    }

    pub fn delete(&mut self, key: &str) -> Option<CacheBuffer> {
        let entry = self.index.remove(key)?;
        self.base.used = self
            .base
            .used
            .saturating_sub(cache_buffer_space(&entry.buffer));
        self.arena.unlink(&mut self.queue, entry.node);
        self.arena.release(entry.node);
        Some(entry.buffer)
    }

    pub fn capacity(&self) -> usize {
        self.base.capacity()
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.base.set_capacity(capacity);
        let _ = self.evict_to_capacity();
    }

    pub fn used_space(&self) -> usize {
        self.base.used_space()
    }

    pub fn free_space(&self) -> usize {
        self.base.free_space()
    }

    pub fn item_count(&self) -> usize {
        self.index.len()
    }

    /// Entries currently queued. Equal to [`ReplacementFifo::item_count`];
    /// the queue holds no tombstones for deleted keys.
    pub fn queue_len(&self) -> usize {
        self.queue.len
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
            let Some(node) = self.queue.front() else {
                break;
            };
            let key = self.arena.key(node).to_string();
            self.arena.unlink(&mut self.queue, node);
            self.arena.release(node);
            let Some(entry) = self.index.remove(&key) else {
                continue;
            };
            self.base.used = self
                .base
                .used
                .saturating_sub(cache_buffer_space(&entry.buffer));
            if let Some(handler) = &self.mem_eviction_func {
                handler(clone_cache_buffer(&entry.buffer));
            }
            evicted.push(entry.buffer);
        }
        evicted
    }
}

#[allow(non_snake_case)]
impl ReplacementFifo {
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
        self.capacity()
    }

    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn GetUsedSpace(&self) -> usize {
        self.used_space()
    }

    pub fn GetFreeSpace(&self) -> usize {
        self.free_space()
    }

    pub fn GetItemNum(&self) -> usize {
        self.item_count()
    }

    pub fn RegisterMemEvictionHandler<F>(&mut self, func: F)
    where
        F: Fn(CacheBuffer) + Send + Sync + 'static,
    {
        self.register_mem_eviction_handler(func);
    }
}

/// Default number of hash-partitioned SLRU segments.
pub const SLRU_DEFAULT_NUM_SEGMENTS: usize = 256;
/// Share of a segment byte budget the hot list is allowed to hold, in percent.
pub const SLRU_DEFAULT_HOT_LRU_PCT: u32 = 20;
/// Share of a segment byte budget the warm list is allowed to hold, in percent.
pub const SLRU_DEFAULT_WARM_LRU_PCT: u32 = 40;

const SLRU_LIST_COUNT: usize = 3;
/// Upper bound on the configurable segment count, so that rounding the request
/// up to a power of two can never overflow.
const SLRU_MAX_NUM_SEGMENTS: usize = 1 << 20;

/// Resolve the effective segment count: the request is rounded up to a power of
/// two so that segment selection can mask instead of divide, and a capacity
/// smaller than the segment count collapses to a single segment (otherwise each
/// segment would get a zero byte budget and evict every insert immediately).
fn resolve_slru_num_segments(capacity: usize, requested: usize) -> usize {
    let requested = requested.clamp(1, SLRU_MAX_NUM_SEGMENTS).next_power_of_two();
    if capacity < requested {
        1
    } else {
        requested
    }
}

#[derive(Debug)]
struct SlruEntry {
    buffer: CacheBuffer,
    flag: u16,
    lru: u16,
    segment: u32,
    node: u32,
}

/// A hash-partitioned shard: three Lru lists plus the shard byte accounting.
#[derive(Debug)]
struct SlruSegment {
    lists: [KeyList; SLRU_LIST_COUNT],
    used: usize,
}

impl SlruSegment {
    fn new() -> Self {
        Self {
            lists: [KeyList::new(), KeyList::new(), KeyList::new()],
            used: 0,
        }
    }
}

/// What the tail of a list should become, decided while the index entry is
/// borrowed and applied afterwards against the lists.
enum SlruTailAction {
    /// The list node has no index entry left; drop the node.
    Orphan,
    /// Move the node to the front of `.0`, whose payload occupies `.1` bytes.
    MoveTo(u16, usize),
    /// Evict the entry outright.
    Evict,
}

/// Outcome of one background-maintainer step over a cold list tail.
enum SlruMaintainerStep {
    /// Nothing left to do in this list.
    Stop,
    /// The tail was reshuffled into another list.
    Moved,
    /// The tail was evicted.
    Evicted(CacheBuffer),
}

/// Segmented Lru replacement policy.
///
/// The cache is hash-partitioned into [`ReplacementSlru::num_segments`] shards,
/// each holding an independent hot/warm/cold Lru triple with its own byte
/// budget of `capacity / num_segments`. Inserts, lookups and eviction only ever
/// touch the shard owning the key, so eviction scans a small shard-local list
/// instead of one global list, and no single hot list can consume the whole
/// budget.
///
/// A background maintainer keeps each shard's hot and warm lists within their
/// configured share of the shard budget ([`SLRU_DEFAULT_HOT_LRU_PCT`] and
/// [`SLRU_DEFAULT_WARM_LRU_PCT`]), demoting entries that were not touched twice
/// and promoting the ones that were. Because this policy is driven through
/// `&mut self` rather than by its own thread, the maintainer runs as an
/// explicit pass: [`ReplacementSlru::run_lru_maintainer_pass`] sweeps every
/// shard, and each `put` maintains just the shard it touched. Passes are
/// disabled by [`ReplacementSlru::test_config_lru_maintainer`].
pub struct ReplacementSlru {
    base: ReplacementPolicyBase,
    index: HashMap<String, SlruEntry>,
    segments: Vec<SlruSegment>,
    arena: KeyArena,
    num_segments: usize,
    bytes_each_segment: usize,
    hot_lru_pct: u32,
    warm_lru_pct: u32,
    lru_maintainer_enabled: bool,
    mem_eviction_func: Option<MemEvictionHandler>,
}

impl ReplacementSlru {
    pub fn new(capacity: usize) -> Self {
        Self::with_num_segments(capacity, SLRU_DEFAULT_NUM_SEGMENTS)
    }

    /// Build a policy with an explicit shard count. The request is rounded up to
    /// a power of two, and collapses to a single shard when `capacity` is
    /// smaller than the shard count.
    pub fn with_num_segments(capacity: usize, num_segments: usize) -> Self {
        let base = ReplacementPolicyBase::new(capacity);
        let num_segments = resolve_slru_num_segments(capacity, num_segments);
        let mut segments = Vec::with_capacity(num_segments);
        for _ in 0..num_segments {
            segments.push(SlruSegment::new());
        }
        Self {
            base,
            index: HashMap::new(),
            segments,
            arena: KeyArena::new(),
            num_segments,
            bytes_each_segment: capacity / num_segments,
            hot_lru_pct: SLRU_DEFAULT_HOT_LRU_PCT,
            warm_lru_pct: SLRU_DEFAULT_WARM_LRU_PCT,
            lru_maintainer_enabled: true,
            mem_eviction_func: None,
        }
    }

    pub fn init(&mut self) -> Result<(), CacheError> {
        self.base.init()
    }

    /// Drop every tracked buffer and return the policy to an empty state.
    ///
    /// As with the other policies, a successful reset leaves this one
    /// initialized and ready to accept buffers again; see the note on
    /// `ReplacementFifo::reset`.
    pub fn reset(&mut self) -> Result<(), CacheError> {
        self.index.clear();
        for segment in &mut self.segments {
            segment.used = 0;
            for list in &mut segment.lists {
                list.clear();
            }
        }
        self.arena.clear();
        self.base.used = 0;
        Ok(())
    }

    /// # Lifecycle
    ///
    /// A policy from `new` is **not initialized**, and an uninitialized policy
    /// discards what it is given: this returns an empty vector and the buffer is
    /// dropped, with no error. Call [`init`](Self::init) first.
    /// `is_initialized` on the policy base is the only way to tell.
    pub fn put(&mut self, buffer: CacheBuffer) -> Vec<CacheBuffer> {
        if !self.base.initialized {
            return Vec::new();
        }
        let key = buffer.key().to_string();
        if key.is_empty() {
            return Vec::new();
        }
        let segment = self.pick_segment(&key);
        if let Some(old) = self.index.remove(&key) {
            let space = cache_buffer_space(&old.buffer);
            self.detach(old.segment, old.lru, old.node, space);
            self.release_node(old.node);
        }
        let space = cache_buffer_space(&buffer);
        let node = self.alloc_node(&key);
        self.attach_front(segment, HOT_LRU, node, space);
        self.index.insert(
            key,
            SlruEntry {
                buffer,
                flag: BUFFER_INIT,
                lru: HOT_LRU,
                segment,
                node,
            },
        );

        let mut evicted = Vec::new();
        if self.lru_maintainer_enabled {
            self.maintain_segment(segment, &mut evicted);
        }
        self.evict_segment(segment, &mut evicted);
        evicted
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
        let old_space = cache_buffer_space(&entry.buffer);
        let new_space = cache_buffer_space(&buffer);
        entry.buffer = buffer;
        let segment = entry.segment;
        let lru = entry.lru;

        let shard = &mut self.segments[segment as usize];
        let list = &mut shard.lists[lru as usize];
        list.used = list.used.saturating_sub(old_space).saturating_add(new_space);
        shard.used = shard.used.saturating_sub(old_space).saturating_add(new_space);

        let mut evicted = Vec::new();
        self.evict_segment(segment, &mut evicted);
        Ok(())
    }

    /// Look up `key` and record the access.
    ///
    /// Only the access flag moves: an entry is not re-fronted in its list on a
    /// read. List position is decided by the maintainer and by eviction, which
    /// is what keeps this a segmented policy rather than a plain Lru — reading
    /// a key once does not jump it ahead of one that was read twice.
    pub fn get(&mut self, key: &str) -> Option<CacheBuffer> {
        let entry = self.index.get_mut(key)?;
        entry.flag = if entry.flag == BUFFER_INIT {
            BUFFER_FETCHED
        } else {
            BUFFER_ACTIVE
        };
        Some(clone_cache_buffer(&entry.buffer))
    }

    pub fn peek(&self, key: &str) -> Option<CacheBuffer> {
        self.index
            .get(key)
            .map(|entry| clone_cache_buffer(&entry.buffer))
    }

    pub fn delete(&mut self, key: &str) -> Option<CacheBuffer> {
        let entry = self.index.remove(key)?;
        let space = cache_buffer_space(&entry.buffer);
        self.detach(entry.segment, entry.lru, entry.node, space);
        self.release_node(entry.node);
        Some(entry.buffer)
    }

    pub fn capacity(&self) -> usize {
        self.base.capacity()
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.base.set_capacity(capacity);
        self.update_segment_byte_limit();
        let mut evicted = Vec::new();
        for segment in 0..self.num_segments as u32 {
            self.evict_segment(segment, &mut evicted);
        }
    }

    pub fn used_space(&self) -> usize {
        self.segments.iter().map(|segment| segment.used).sum()
    }

    pub fn free_space(&self) -> usize {
        self.capacity().saturating_sub(self.used_space())
    }

    pub fn item_count(&self) -> usize {
        self.index.len()
    }

    /// Number of hash-partitioned shards.
    pub fn num_segments(&self) -> usize {
        self.num_segments
    }

    /// Byte budget of a single shard, i.e. `capacity / num_segments`.
    pub fn segment_byte_limit(&self) -> usize {
        self.bytes_each_segment
    }

    /// Bytes currently held by one shard across its hot, warm and cold lists.
    pub fn segment_used_size(&self, segment_id: usize) -> usize {
        self.segments
            .get(segment_id)
            .map(|segment| segment.used)
            .unwrap_or(0)
    }

    /// Bytes currently held by one hot/warm/cold list of one shard.
    pub fn list_used_size(&self, segment_id: usize, lru: u16) -> usize {
        self.segments
            .get(segment_id)
            .and_then(|segment| segment.lists.get(lru as usize))
            .map(|list| list.used)
            .unwrap_or(0)
    }

    /// Entry count of one hot/warm/cold list of one shard.
    pub fn list_item_count(&self, segment_id: usize, lru: u16) -> usize {
        self.segments
            .get(segment_id)
            .and_then(|segment| segment.lists.get(lru as usize))
            .map(|list| list.len)
            .unwrap_or(0)
    }

    /// Shard that owns `key`.
    pub fn segment_for_key(&self, key: &str) -> usize {
        self.pick_segment(key) as usize
    }

    pub fn hot_lru_pct(&self) -> u32 {
        self.hot_lru_pct
    }

    /// Set the share of a shard budget the hot list may hold, in percent.
    pub fn set_hot_lru_pct(&mut self, pct: u32) {
        self.hot_lru_pct = pct.min(100);
    }

    pub fn warm_lru_pct(&self) -> u32 {
        self.warm_lru_pct
    }

    /// Set the share of a shard budget the warm list may hold, in percent.
    pub fn set_warm_lru_pct(&mut self, pct: u32) {
        self.warm_lru_pct = pct.min(100);
    }

    /// Run one maintainer sweep across every shard, the work a background
    /// maintainer thread would do on its interval. Returns the buffers evicted
    /// by the sweep. Does nothing while the maintainer is disabled.
    pub fn run_lru_maintainer_pass(&mut self) -> Vec<CacheBuffer> {
        let mut evicted = Vec::new();
        if !self.base.initialized || !self.lru_maintainer_enabled {
            return evicted;
        }
        for segment in 0..self.num_segments as u32 {
            self.maintain_segment(segment, &mut evicted);
        }
        evicted
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

    fn pick_segment(&self, key: &str) -> u32 {
        if self.num_segments <= 1 {
            return 0;
        }
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() & (self.num_segments as u64 - 1)) as u32
    }

    fn update_segment_byte_limit(&mut self) {
        self.bytes_each_segment = self.base.capacity() / self.num_segments;
    }

    fn alloc_node(&mut self, key: &str) -> u32 {
        self.arena.alloc(key)
    }

    fn release_node(&mut self, node: u32) {
        self.arena.release(node);
    }

    fn attach_front(&mut self, segment: u32, lru: u16, node: u32, space: usize) {
        let shard = &mut self.segments[segment as usize];
        self.arena.link_front(&mut shard.lists[lru as usize], node);
        let list = &mut shard.lists[lru as usize];
        list.used = list.used.saturating_add(space);
        shard.used = shard.used.saturating_add(space);
    }

    fn detach(&mut self, segment: u32, lru: u16, node: u32, space: usize) {
        let shard = &mut self.segments[segment as usize];
        self.arena.unlink(&mut shard.lists[lru as usize], node);
        let list = &mut shard.lists[lru as usize];
        list.used = list.used.saturating_sub(space);
        shard.used = shard.used.saturating_sub(space);
    }

    /// Move `node` to the front of `to`. `from == to` re-fronts it in place.
    /// The shard total is unchanged: the payload stays in the same shard.
    fn move_node(&mut self, segment: u32, from: u16, to: u16, node: u32, space: usize) {
        let shard = &mut self.segments[segment as usize];
        self.arena.unlink(&mut shard.lists[from as usize], node);
        shard.lists[from as usize].used = shard.lists[from as usize].used.saturating_sub(space);
        self.arena.link_front(&mut shard.lists[to as usize], node);
        shard.lists[to as usize].used = shard.lists[to as usize].used.saturating_add(space);
    }

    fn list_tail(&self, segment: u32, lru: u16) -> Option<u32> {
        let tail = self.segments[segment as usize].lists[lru as usize].tail;
        if tail == KEY_LIST_NIL {
            None
        } else {
            Some(tail)
        }
    }

    fn node_key(&self, node: u32) -> String {
        self.arena.key(node).to_string()
    }

    fn segment_len(&self, segment: u32) -> usize {
        self.segments[segment as usize]
            .lists
            .iter()
            .map(|list| list.len)
            .sum()
    }

    /// Bound on how many reshuffles one drain loop may perform. Reshuffles move
    /// entries between lists without freeing bytes, so a loop that only ever
    /// reshuffles needs a stop condition; four passes over the shard is more
    /// than enough for every entry to be demoted and then evicted.
    fn drain_budget(&self, segment: u32) -> usize {
        self.segment_len(segment).saturating_mul(4).saturating_add(4)
    }

    fn drop_orphan_node(&mut self, segment: u32, lru: u16, node: u32) {
        self.detach(segment, lru, node, 0);
        self.release_node(node);
    }

    fn evict_segment(&mut self, segment: u32, evicted: &mut Vec<CacheBuffer>) {
        let budget = self.drain_budget(segment);
        let mut attempts = 0usize;
        while self.segments[segment as usize].used > self.bytes_each_segment && attempts <= budget {
            attempts = attempts.saturating_add(1);
            if let Some(buffer) = self.try_evict_or_shuffle_cold(segment) {
                evicted.push(buffer);
                continue;
            }
            if self.shuffle_hot_tail(segment) {
                continue;
            }
            if self.shuffle_warm_tail(segment) {
                continue;
            }
            if let Some(buffer) = self.force_evict_any_tail(segment) {
                evicted.push(buffer);
            } else {
                break;
            }
        }
    }

    fn try_evict_or_shuffle_cold(&mut self, segment: u32) -> Option<CacheBuffer> {
        let node = self.list_tail(segment, COLD_LRU)?;
        let key = self.node_key(node);
        let action = match self.index.get_mut(&key) {
            None => SlruTailAction::Orphan,
            Some(entry) => {
                if entry.flag >= BUFFER_FETCHED {
                    entry.flag = BUFFER_ACTIVE;
                    entry.lru = WARM_LRU;
                    SlruTailAction::MoveTo(WARM_LRU, cache_buffer_space(&entry.buffer))
                } else {
                    SlruTailAction::Evict
                }
            }
        };
        match action {
            SlruTailAction::Orphan => {
                self.drop_orphan_node(segment, COLD_LRU, node);
                None
            }
            SlruTailAction::MoveTo(to, space) => {
                self.move_node(segment, COLD_LRU, to, node, space);
                None
            }
            SlruTailAction::Evict => self.evict_key(&key),
        }
    }

    fn shuffle_hot_tail(&mut self, segment: u32) -> bool {
        let Some(node) = self.list_tail(segment, HOT_LRU) else {
            return false;
        };
        let key = self.node_key(node);
        let action = match self.index.get_mut(&key) {
            None => SlruTailAction::Orphan,
            Some(entry) => {
                let space = cache_buffer_space(&entry.buffer);
                if entry.flag >= BUFFER_FETCHED {
                    entry.flag = BUFFER_ACTIVE;
                    entry.lru = WARM_LRU;
                    SlruTailAction::MoveTo(WARM_LRU, space)
                } else {
                    entry.lru = COLD_LRU;
                    SlruTailAction::MoveTo(COLD_LRU, space)
                }
            }
        };
        match action {
            SlruTailAction::MoveTo(to, space) => self.move_node(segment, HOT_LRU, to, node, space),
            _ => self.drop_orphan_node(segment, HOT_LRU, node),
        }
        true
    }

    fn shuffle_warm_tail(&mut self, segment: u32) -> bool {
        let Some(node) = self.list_tail(segment, WARM_LRU) else {
            return false;
        };
        let key = self.node_key(node);
        let action = match self.index.get_mut(&key) {
            None => SlruTailAction::Orphan,
            Some(entry) => {
                let space = cache_buffer_space(&entry.buffer);
                if entry.flag >= BUFFER_ACTIVE {
                    entry.flag = BUFFER_FETCHED;
                    SlruTailAction::MoveTo(WARM_LRU, space)
                } else {
                    entry.lru = COLD_LRU;
                    SlruTailAction::MoveTo(COLD_LRU, space)
                }
            }
        };
        match action {
            SlruTailAction::MoveTo(to, space) => self.move_node(segment, WARM_LRU, to, node, space),
            _ => self.drop_orphan_node(segment, WARM_LRU, node),
        }
        true
    }

    fn force_evict_any_tail(&mut self, segment: u32) -> Option<CacheBuffer> {
        for lru in [COLD_LRU, HOT_LRU, WARM_LRU] {
            let Some(node) = self.list_tail(segment, lru) else {
                continue;
            };
            let key = self.node_key(node);
            if let Some(buffer) = self.evict_key(&key) {
                return Some(buffer);
            }
            self.drop_orphan_node(segment, lru, node);
        }
        None
    }

    fn evict_key(&mut self, key: &str) -> Option<CacheBuffer> {
        let entry = self.index.remove(key)?;
        let space = cache_buffer_space(&entry.buffer);
        self.detach(entry.segment, entry.lru, entry.node, space);
        self.release_node(entry.node);
        if let Some(handler) = &self.mem_eviction_func {
            handler(clone_cache_buffer(&entry.buffer));
        }
        Some(entry.buffer)
    }

    fn hot_byte_limit(&self) -> usize {
        self.bytes_each_segment
            .saturating_mul(self.hot_lru_pct as usize)
            / 100
    }

    fn warm_byte_limit(&self) -> usize {
        self.bytes_each_segment
            .saturating_mul(self.warm_lru_pct as usize)
            / 100
    }

    /// One maintainer pass over a single shard: trim the hot list to its share
    /// of the shard budget, then the warm list to its share, then enforce the
    /// whole shard budget from the cold list.
    fn maintain_segment(&mut self, segment: u32, evicted: &mut Vec<CacheBuffer>) {
        let budget = self.drain_budget(segment);

        let hot_limit = self.hot_byte_limit();
        let mut attempts = 0usize;
        while self.segments[segment as usize].lists[HOT_LRU as usize].used > hot_limit
            && attempts <= budget
        {
            attempts = attempts.saturating_add(1);
            if !self.maintain_demote_tail(segment, HOT_LRU) {
                break;
            }
        }

        let warm_limit = self.warm_byte_limit();
        attempts = 0;
        while self.segments[segment as usize].lists[WARM_LRU as usize].used > warm_limit
            && attempts <= budget
        {
            attempts = attempts.saturating_add(1);
            if !self.maintain_demote_tail(segment, WARM_LRU) {
                break;
            }
        }

        attempts = 0;
        while self.segments[segment as usize].used > self.bytes_each_segment && attempts <= budget {
            attempts = attempts.saturating_add(1);
            match self.maintain_cold_tail(segment) {
                SlruMaintainerStep::Stop => break,
                SlruMaintainerStep::Moved => {}
                SlruMaintainerStep::Evicted(buffer) => evicted.push(buffer),
            }
        }
    }

    /// Maintainer step for a hot or warm list tail: an entry touched twice is
    /// promoted to the front of the warm list, anything else is demoted to the
    /// cold list. Either way the access flag is reset, so an entry that keeps
    /// cycling through the warm list without being touched again drains to cold
    /// on the next pass.
    fn maintain_demote_tail(&mut self, segment: u32, from: u16) -> bool {
        let Some(node) = self.list_tail(segment, from) else {
            return false;
        };
        let key = self.node_key(node);
        let action = match self.index.get_mut(&key) {
            None => SlruTailAction::Orphan,
            Some(entry) => {
                let space = cache_buffer_space(&entry.buffer);
                let target = if entry.flag == BUFFER_ACTIVE {
                    WARM_LRU
                } else {
                    COLD_LRU
                };
                entry.flag = BUFFER_INIT;
                entry.lru = target;
                SlruTailAction::MoveTo(target, space)
            }
        };
        match action {
            SlruTailAction::MoveTo(to, space) => self.move_node(segment, from, to, node, space),
            _ => self.drop_orphan_node(segment, from, node),
        }
        true
    }

    fn maintain_cold_tail(&mut self, segment: u32) -> SlruMaintainerStep {
        let Some(node) = self.list_tail(segment, COLD_LRU) else {
            return SlruMaintainerStep::Stop;
        };
        let key = self.node_key(node);
        let action = match self.index.get_mut(&key) {
            None => SlruTailAction::Orphan,
            Some(entry) => {
                if entry.flag == BUFFER_ACTIVE {
                    let space = cache_buffer_space(&entry.buffer);
                    entry.flag = BUFFER_INIT;
                    entry.lru = WARM_LRU;
                    SlruTailAction::MoveTo(WARM_LRU, space)
                } else {
                    SlruTailAction::Evict
                }
            }
        };
        match action {
            SlruTailAction::Orphan => {
                self.drop_orphan_node(segment, COLD_LRU, node);
                SlruMaintainerStep::Moved
            }
            SlruTailAction::MoveTo(to, space) => {
                self.move_node(segment, COLD_LRU, to, node, space);
                SlruMaintainerStep::Moved
            }
            SlruTailAction::Evict => match self.evict_key(&key) {
                Some(buffer) => SlruMaintainerStep::Evicted(buffer),
                None => SlruMaintainerStep::Stop,
            },
        }
    }
}

#[allow(non_snake_case)]
impl ReplacementSlru {
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
        self.capacity()
    }

    pub fn SetCapacity(&mut self, capacity: usize) {
        self.set_capacity(capacity);
    }

    pub fn GetUsedSpace(&self) -> usize {
        self.used_space()
    }

    pub fn GetFreeSpace(&self) -> usize {
        self.free_space()
    }

    pub fn GetItemNum(&self) -> usize {
        self.item_count()
    }

    pub fn GetSegmentUsedSize(&self, segment_id: usize) -> usize {
        self.segment_used_size(segment_id)
    }

    pub fn GetSegmentByteLimit(&self) -> usize {
        self.segment_byte_limit()
    }

    pub fn GetListUsedSize(&self, segment_id: usize, lru: u16) -> usize {
        self.list_used_size(segment_id, lru)
    }

    pub fn PickSegment(&self, key: &str) -> usize {
        self.segment_for_key(key)
    }

    pub fn LRUMaintainerTask(&mut self) -> Vec<CacheBuffer> {
        self.run_lru_maintainer_pass()
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

/// A segmented Lru that can be shared across threads, holding one lock per
/// segment instead of one lock over the whole policy.
///
/// [`ReplacementSlru`] partitions its lists into segments, but it is driven
/// through `&mut self`, so a caller that shares it has to wrap the whole policy
/// in one lock and the partitioning buys nothing under concurrency. This type
/// keeps the same partitioning and gives each segment its own lock, so
/// operations on keys that hash to different segments do not wait on each
/// other. Each segment is an independent [`ReplacementSlru`] holding
/// `capacity / num_segments` bytes in a single internal segment, which is the
/// same layout, just reached through a per-segment lock.
///
/// The trade-off is unchanged from the single-threaded form: hash partitioning
/// is slightly less hit-rate-optimal than one global list, because a segment
/// can evict an entry that is globally warmer than one another segment keeps.
pub struct ConcurrentReplacementSlru {
    segments: Vec<Mutex<ReplacementSlru>>,
    num_segments: usize,
    capacity: usize,
    bytes_each_segment: usize,
}

impl ConcurrentReplacementSlru {
    pub fn new(capacity: usize) -> Self {
        Self::with_num_segments(capacity, SLRU_DEFAULT_NUM_SEGMENTS)
    }

    /// Build a policy with an explicit segment count. The request is rounded up
    /// to a power of two, and collapses to a single segment when `capacity` is
    /// smaller than the segment count.
    pub fn with_num_segments(capacity: usize, num_segments: usize) -> Self {
        let num_segments = resolve_slru_num_segments(capacity, num_segments);
        let bytes_each_segment = capacity / num_segments;
        let mut segments = Vec::with_capacity(num_segments);
        for _ in 0..num_segments {
            // Each segment is a whole policy holding one internal segment, so
            // its byte budget is exactly this segment share.
            segments.push(Mutex::new(ReplacementSlru::with_num_segments(
                bytes_each_segment,
                1,
            )));
        }
        Self {
            segments,
            num_segments,
            capacity,
            bytes_each_segment,
        }
    }

    pub fn init(&self) -> Result<(), CacheError> {
        for segment in &self.segments {
            self.lock(segment).init()?;
        }
        Ok(())
    }

    pub fn reset(&self) -> Result<(), CacheError> {
        for segment in &self.segments {
            self.lock(segment).reset()?;
        }
        Ok(())
    }

    pub fn put(&self, buffer: CacheBuffer) -> Vec<CacheBuffer> {
        let index = self.segment_for_key(buffer.key());
        self.lock(&self.segments[index]).put(buffer)
    }

    pub fn get(&self, key: &str) -> Option<CacheBuffer> {
        let index = self.segment_for_key(key);
        self.lock(&self.segments[index]).get(key)
    }

    pub fn peek(&self, key: &str) -> Option<CacheBuffer> {
        let index = self.segment_for_key(key);
        self.lock(&self.segments[index]).peek(key)
    }

    pub fn delete(&self, key: &str) -> Option<CacheBuffer> {
        let index = self.segment_for_key(key);
        self.lock(&self.segments[index]).delete(key)
    }

    pub fn update_cache_buffer(
        &self,
        key: &str,
        old_data: &[u8],
        buffer: CacheBuffer,
    ) -> Result<(), CacheError> {
        let index = self.segment_for_key(key);
        self.lock(&self.segments[index]).update_cache_buffer(key, old_data, buffer)
    }

    /// Run one maintainer sweep over every segment, taking each segment lock in
    /// turn rather than holding one lock across the whole sweep.
    pub fn run_lru_maintainer_pass(&self) -> Vec<CacheBuffer> {
        let mut evicted = Vec::new();
        for segment in &self.segments {
            evicted.extend(self.lock(segment).run_lru_maintainer_pass());
        }
        evicted
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn used_space(&self) -> usize {
        self.segments
            .iter()
            .map(|segment| self.lock(segment).used_space())
            .sum()
    }

    pub fn free_space(&self) -> usize {
        self.capacity.saturating_sub(self.used_space())
    }

    pub fn item_count(&self) -> usize {
        self.segments
            .iter()
            .map(|segment| self.lock(segment).item_count())
            .sum()
    }

    pub fn num_segments(&self) -> usize {
        self.num_segments
    }

    pub fn segment_byte_limit(&self) -> usize {
        self.bytes_each_segment
    }

    pub fn segment_used_size(&self, segment_id: usize) -> usize {
        self.segments
            .get(segment_id)
            .map(|segment| self.lock(segment).used_space())
            .unwrap_or(0)
    }

    pub fn segment_item_count(&self, segment_id: usize) -> usize {
        self.segments
            .get(segment_id)
            .map(|segment| self.lock(segment).item_count())
            .unwrap_or(0)
    }

    /// Segment that owns `key`. Uses the same hash and mask as the
    /// single-threaded form, so a key lands in the same relative segment.
    pub fn segment_for_key(&self, key: &str) -> usize {
        if self.num_segments <= 1 {
            return 0;
        }
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() & (self.num_segments as u64 - 1)) as usize
    }

    pub fn set_hot_lru_pct(&self, pct: u32) {
        for segment in &self.segments {
            self.lock(segment).set_hot_lru_pct(pct);
        }
    }

    pub fn set_warm_lru_pct(&self, pct: u32) {
        for segment in &self.segments {
            self.lock(segment).set_warm_lru_pct(pct);
        }
    }

    pub fn test_config_lru_maintainer(&self, status: bool) {
        for segment in &self.segments {
            self.lock(segment).test_config_lru_maintainer(status);
        }
    }

    /// Register an eviction handler on every segment. The handler is shared, so
    /// it is called from whichever thread drives the eviction and must be
    /// `Send + Sync`.
    pub fn register_mem_eviction_handler<F>(&self, func: F)
    where
        F: Fn(CacheBuffer) + Send + Sync + 'static,
    {
        let shared = Arc::new(func);
        for segment in &self.segments {
            let handler = Arc::clone(&shared);
            self.lock(segment).register_mem_eviction_handler(move |buffer| handler(buffer));
        }
    }

    /// A poisoned segment lock means another thread panicked mid-update, which
    /// would leave that segment inconsistent. Recovering the guard keeps the
    /// remaining segments usable rather than cascading the panic across every
    /// caller.
    fn lock<'a>(&self, segment: &'a Mutex<ReplacementSlru>) -> std::sync::MutexGuard<'a, ReplacementSlru> {
        segment.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[allow(non_snake_case)]
impl ConcurrentReplacementSlru {
    pub fn Init(&self) -> Result<(), CacheError> {
        self.init()
    }

    pub fn Reset(&self) -> Result<(), CacheError> {
        self.reset()
    }

    pub fn Put(&self, buffer: CacheBuffer) -> Vec<CacheBuffer> {
        self.put(buffer)
    }

    pub fn Get(&self, key: &str) -> Option<CacheBuffer> {
        self.get(key)
    }

    pub fn Peek(&self, key: &str) -> Option<CacheBuffer> {
        self.peek(key)
    }

    pub fn Delete(&self, key: &str) -> Option<CacheBuffer> {
        self.delete(key)
    }

    pub fn GetCapacity(&self) -> usize {
        self.capacity()
    }

    pub fn GetUsedSpace(&self) -> usize {
        self.used_space()
    }

    pub fn GetFreeSpace(&self) -> usize {
        self.free_space()
    }

    pub fn GetItemNum(&self) -> usize {
        self.item_count()
    }

    pub fn GetSegmentUsedSize(&self, segment_id: usize) -> usize {
        self.segment_used_size(segment_id)
    }

    pub fn GetSegmentByteLimit(&self) -> usize {
        self.segment_byte_limit()
    }

    pub fn PickSegment(&self, key: &str) -> usize {
        self.segment_for_key(key)
    }

    pub fn LRUMaintainerTask(&self) -> Vec<CacheBuffer> {
        self.run_lru_maintainer_pass()
    }
}
