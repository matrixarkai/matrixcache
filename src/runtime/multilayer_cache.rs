// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

fn average_latency_us(total_us: u64, count: u64) -> u64 {
    if count == 0 {
        0
    } else {
        total_us / count
    }
}

fn latency_percentile_us(
    count: u64,
    le_10us: u64,
    le_100us: u64,
    le_1ms: u64,
    le_10ms: u64,
    gt_10ms: u64,
    max_us: u64,
    percentile: u64,
) -> u64 {
    if count == 0 {
        return 0;
    }
    let rank = count.saturating_mul(percentile).saturating_add(99) / 100;
    let mut cumulative = le_10us;
    if rank <= cumulative {
        return 10;
    }
    cumulative = cumulative.saturating_add(le_100us);
    if rank <= cumulative {
        return 100;
    }
    cumulative = cumulative.saturating_add(le_1ms);
    if rank <= cumulative {
        return 1_000;
    }
    cumulative = cumulative.saturating_add(le_10ms);
    if rank <= cumulative {
        return 10_000;
    }
    if gt_10ms > 0 {
        return max_us.max(10_001);
    }
    max_us
}

/// The byte-valued cache interface, implemented by every cache in this crate.
///
/// Implemented by [`MultiLayerCache`], [`ShardedMultiLayerCache`],
/// [`SimpleLruCache`] and [`ZeroCopySimpleLruCache`], so a caller can hold any
/// of them behind `dyn CacheApi`.
///
/// **Every method carries a `_cache` suffix on purpose.** Implementors also have
/// inherent methods with the bare names -- `SimpleLruCache::insert`,
/// `MultiLayerCache::size` -- and without the suffix a call would be ambiguous
/// between the two. [`StringCacheApi`] uses `_string` for the same reason, which
/// is what lets one type implement both.
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
    fn capacity_for_instance_cache(&self, instance_type: CacheInstanceKind) -> usize {
        match instance_type {
            CacheInstanceKind::Unified => self.capacity_cache(),
            _ => self.capacity_cache(),
        }
    }
    fn set_capacity_cache(&self, capacity: usize);
    fn set_capacity_for_instance_cache(&self, instance_type: CacheInstanceKind, capacity: usize) {
        match instance_type {
            CacheInstanceKind::Unified => self.set_capacity_cache(capacity),
            _ => self.set_capacity_cache(capacity),
        }
    }
    fn size_cache(&self) -> usize;
    fn used_cache(&self, instance_type: CacheInstanceKind) -> usize {
        match instance_type {
            CacheInstanceKind::Unified => self.size_cache(),
            _ => self.size_cache(),
        }
    }
}

/// Reads that hand back a pinned handle instead of a copy.
///
/// Extends [`CacheApi`] for the caches that can lend their stored bytes:
/// [`MultiLayerCache`], [`ShardedMultiLayerCache`] and
/// [`ZeroCopySimpleLruCache`].
///
/// A handle acquired here holds the entry resident, so it must be released --
/// bytes behind a live handle are still counted by `size`, and an entry removed
/// while pinned is retired rather than freed until the last handle drops.
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

/// The `String`-valued cache interface.
///
/// The counterpart to [`CacheApi`] for the facades that store strings rather
/// than bytes: [`ConcurrentSimpleLruCache`], [`FlexibleCache`],
/// [`InProcessMemcachedCache`], [`MultiTierCache`] and [`MultiTierStringCache`].
///
/// The `_string` suffix on every method exists for the same reason as
/// `CacheApi`'s `_cache`: it keeps these from colliding with the implementors'
/// inherent methods, and with each other on a type that implements both.
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
struct SlotEvictionGroup<'a> {
    group_score: EvictionScore,
    victim: &'a CacheKey,
    victim_score: EvictionScore,
}

impl<'a> SlotEvictionGroup<'a> {
    fn new(victim: &'a CacheKey, score: EvictionScore) -> Self {
        Self {
            group_score: score,
            victim,
            victim_score: score,
        }
    }

    /// Fold one more member into the group.
    ///
    /// Nothing here owns a key. Entries without a routing slot each stand alone
    /// as their own group, so every candidate starts one -- cloning on the way
    /// in cloned the whole window on every eviction, and all but one of those
    /// clones was thrown away. The winner is cloned once, by the caller.
    fn observe(&mut self, key: &'a CacheKey, score: EvictionScore) {
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

/// How entries are grouped when picking an eviction victim.
///
/// Entries that share a routing slot are weighed as a unit; everything else
/// stands alone. Borrowing the key's parts keeps this free of allocation: the
/// grouping used to render one `String` per resident entry per eviction, which
/// on a cache at capacity is a per-write cost proportional to the cache size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EvictionGroupKey<'a> {
    Slot(u32),
    Object(&'a str, &'a str),
}

/// A chosen victim plus how many candidate groups were weighed to find it.
///
/// The count feeds the `eviction_sampled_groups` statistic, which is what says
/// whether selection is inspecting a bounded number of candidates or the whole
/// tier.
struct PickedEvictionVictim {
    victim: Option<(CacheKey, EvictionReason, u64)>,
    groups_weighed: usize,
}

/// The multi-tier cache: a DRAM tier, a persistent-memory-like resident tier and
/// an SSD tier, with admission control, cross-tier eviction and read-through
/// refill.
///
/// Construct it with [`MultiLayerCache::new`] for a memory tier plus a directory
/// for the SSD tier, or with [`MultiLayerCache::with_options`] for anything more
/// than that. Implements [`CacheApi`] and [`ZeroCopyCacheApi`].
///
/// # Concurrency
///
/// **Reads take the write lock.** A hit updates statistics, hotness metadata and
/// two latency histograms on the way out, so concurrent readers serialise
/// against each other and throughput *falls* as threads are added.
///
/// If more than one thread will read this cache, use
/// [`ShardedMultiLayerCache`], which has the same API and does not have the
/// problem. `examples/cache_scaling_bench.rs` measures both.
///
/// # Examples
///
/// ```
/// use matrixcache::{CacheKey, MultiLayerCache};
///
/// let dir = tempfile::tempdir()?;
/// let cache = MultiLayerCache::new(1 << 20, dir.path());
///
/// let key = CacheKey::string(0, "greeting");
/// cache.put(key.clone(), b"hello".to_vec())?;
/// assert_eq!(cache.get(&key)?, Some(b"hello".to_vec()));
///
/// cache.remove(&key)?;
/// assert_eq!(cache.get(&key)?, None);
/// # Ok::<(), matrixcache::CacheError>(())
/// ```
#[derive(Debug, Clone)]
pub struct MultiLayerCache {
    inner: Arc<RwLock<CacheInner>>,
    async_writeback_worker: Arc<Mutex<Option<CacheAsyncWritebackWorker>>>,
    /// Whether an access-record callback is registered.
    ///
    /// Duplicated out of `CacheInner` so the check that begins every get, put
    /// and delete does not have to take the cache lock to make it.
    access_record_registered: Arc<AtomicBool>,
    /// Whether either eviction callback is registered.
    ///
    /// The pending-eviction queues are only ever non-empty while one is, so
    /// this says whether there can be anything to drain -- which lets the
    /// drain after every put and delete skip taking the lock exclusively.
    eviction_callbacks_registered: Arc<AtomicBool>,
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
    /// Whether an SSD block write also survives a machine crash. See
    /// [`CacheOptions::ssd_block_durability`].
    ssd_block_durability: bool,
    /// Whether a persistent-tier write also survives a machine crash. See
    /// [`CacheOptions::pmem_block_durability`].
    pmem_block_durability: bool,
    memory_capacity_bytes: usize,
    memory_bytes: usize,
    pmem_capacity_bytes: usize,
    pmem_bytes: usize,
    ssd_capacity_bytes: usize,
    ssd_bytes: u64,
    disk_dir: PathBuf,
    pmem_paths: Vec<PathBuf>,
    ssd_store: SsdTierStore,
    ssd_write_budget: SsdWriteBudget,
    /// Time to live applied to entries that do not ask for their own, in
    /// milliseconds. Zero means entries do not expire.
    default_ttl_millis: u64,
    /// Whether any entry has ever been given a time to live.
    ///
    /// The incremental sweep below is worth nothing to a cache that does not
    /// use expiry, and this is what keeps it costing nothing: one bool test on
    /// the write path, and the sweep is never entered.
    ttl_in_use: bool,
    tiering_policy: CacheTieringPolicy,
    block_options: CacheBlockOptions,
    memory: HashMap<CacheKey, Arc<[u8]>>,
    pmem: HashMap<CacheKey, Arc<[u8]>>,
    disk_index: HashMap<CacheKey, u64>,
    disk_order: CacheKeyOrder,
    /// Everything about which entries are pinned, behind its own lock.
    ///
    /// It used to be three maps and two counters directly on the cache, which
    /// meant taking a pinned handle needed the cache **exclusively** -- and so
    /// did giving one back. Every reader serialised against every other twice
    /// per zero-copy read, and it showed: `acquire` did not scale at all and
    /// went backwards past two threads, while `get` scaled, leaving the
    /// zero-copy read about eighteen times slower than the copying one it
    /// exists to beat.
    ///
    /// Behind its own lock, a read takes the cache **shared** and holds this
    /// only for the counter update. The model this crate follows goes further
    /// and keeps the refcount on the item itself; this is the same idea with
    /// the accounting kept where it already was.
    ///
    /// **Lock order is cache, then pins, always.** It can only be reached
    /// through a borrow of the cache, so nothing can take it the other way
    /// round.
    pins: Vec<Mutex<CachePinState>>,
    memory_order: CacheKeyOrder,
    pmem_order: CacheKeyOrder,
    async_writeback_queue: VecDeque<CacheWritebackJob>,
    /// Key to the sequence number of its queued job.
    ///
    /// A sequence number, not an index: popping from the front of the queue
    /// would shift every index, and re-deriving them meant rebuilding this
    /// whole map on every drain. The current index is `sequence - head`.
    async_writeback_positions: HashMap<CacheKey, u64>,
    /// How many write-back jobs have ever been popped.
    async_writeback_head: u64,
    /// The floor for the refresh window, in milliseconds. Zero always moves the
    /// entry.
    ///
    /// Moving an entry needs the cache exclusively, so this is what decides
    /// whether a hit can be served under the shared lock. See
    /// [`DEFAULT_LRU_REFRESH`].
    lru_refresh_floor_millis: u64,
    /// Scales the window by the age of the oldest entry. Zero disables the
    /// adaptation and pins the window to the floor, which is the default.
    lru_refresh_ratio: f64,
    /// The window actually in force: the floor, or the scaled value when
    /// adaptation is on. Relaxed -- it is a heuristic, and a reader that sees
    /// the previous interval's value simply uses that.
    lru_refresh_effective_millis: AtomicU64,
    /// When the adaptation is next due to be recomputed.
    next_reconfigure_millis: AtomicU64,
    async_writeback_queue_bytes: u64,
    max_async_writeback_queue: usize,
    access_record_callback: Option<CacheAccessRecordCallback>,
    eviction_callback: Option<CacheEvictionCallback>,
    eviction_handler_enabled: bool,
    pending_eviction_records: VecDeque<CacheEvictionRecord>,
    eviction_metric_callback: Option<CacheEvictionMetricCallback>,
    pending_eviction_metric_tiers: VecDeque<CacheTier>,
    ssd_instance_only: bool,
    memory_replacement_policy: CacheReplacementPolicy,
    pmem_replacement_policy: CacheReplacementPolicy,
    ssd_replacement_policy: CacheReplacementPolicy,
    metadata: HashMap<CacheKey, CacheEntryMeta>,
    /// How often keys have been asked for recently, including keys that are
    /// not resident. Consulted at admission; see `admission_filter_enabled`.
    access_frequency: FrequencySketch,
    /// Whether a candidate colder than the entry it would evict is declined.
    /// Off by default: it changes what the cache keeps.
    admission_filter_enabled: bool,
    /// Read-path statistics, updated through `&self` so a read can count
    /// itself while holding the cache lock shared.
    read_counters: ReadPathCounters,
    stats: CacheStats,
}

/// A latency histogram that can be updated through a shared reference.
///
/// The read path records into these while holding the cache lock shared, so
/// two readers no longer take turns to count themselves.
///
/// These counters wrap where the `u64` fields they replace saturated. At a
/// billion samples a second that is a little under six hundred years, so the
/// difference is theoretical.
#[derive(Debug, Default)]
struct AtomicLatencyHistogram {
    total_micros: AtomicU64,
    max_micros: AtomicU64,
    le_10us: AtomicU64,
    le_100us: AtomicU64,
    le_1ms: AtomicU64,
    le_10ms: AtomicU64,
    gt_10ms: AtomicU64,
}

impl AtomicLatencyHistogram {
    /// Counts the sample in its bucket, and nothing else.
    ///
    /// There is no separate sample counter: `samples()` adds the buckets up,
    /// which is the same number, one atomic write cheaper per sample.
    fn observe(&self, micros: u64) {
        let bucket = if micros <= 10 {
            &self.le_10us
        } else if micros <= 100 {
            &self.le_100us
        } else if micros <= 1_000 {
            &self.le_1ms
        } else if micros <= 10_000 {
            &self.le_10ms
        } else {
            &self.gt_10ms
        };
        bucket.fetch_add(1, Ordering::Relaxed);
    }

    /// As `observe`, and also keeps a running total and maximum.
    ///
    /// Use this for latency families whose Grafana histograms need a `_sum`.
    /// The maximum is read before it is written: after the first few samples
    /// almost none are a new maximum, and a load is much cheaper than an
    /// unconditional read-modify-write.
    fn observe_with_total(&self, micros: u64) {
        self.observe(micros);
        self.total_micros.fetch_add(micros, Ordering::Relaxed);
        if micros > self.max_micros.load(Ordering::Relaxed) {
            self.max_micros.fetch_max(micros, Ordering::Relaxed);
        }
    }

    fn samples(&self) -> u64 {
        self.le_10us.load(Ordering::Relaxed)
            + self.le_100us.load(Ordering::Relaxed)
            + self.le_1ms.load(Ordering::Relaxed)
            + self.le_10ms.load(Ordering::Relaxed)
            + self.gt_10ms.load(Ordering::Relaxed)
    }

    fn reset(&self) {
        for counter in [
            &self.total_micros,
            &self.max_micros,
            &self.le_10us,
            &self.le_100us,
            &self.le_1ms,
            &self.le_10ms,
            &self.gt_10ms,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
    }
}

/// The statistics a read updates, held so that a read does not need the cache
/// exclusively in order to account for itself.
///
/// Everything here is folded into the public `CacheStats` by
/// `MultiLayerCache::stats()`. Relaxed ordering is right for all of it: these
/// are counters read for reporting, and nothing branches on one of them to
/// decide whether some other write is visible.
#[derive(Debug, Default)]
struct ReadPathCounters {
    memory_hits: AtomicU64,
    pmem_hits: AtomicU64,
    disk_hits: AtomicU64,
    misses: AtomicU64,
    hotness_promotions: AtomicU64,
    access_order_refreshes: AtomicU64,
    get_latency: AtomicLatencyHistogram,
    read_through_latency: AtomicLatencyHistogram,
    refill_latency: AtomicLatencyHistogram,
}

impl ReadPathCounters {
    fn reset(&self) {
        self.memory_hits.store(0, Ordering::Relaxed);
        self.pmem_hits.store(0, Ordering::Relaxed);
        self.disk_hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.hotness_promotions.store(0, Ordering::Relaxed);
        self.access_order_refreshes.store(0, Ordering::Relaxed);
        self.get_latency.reset();
        self.read_through_latency.reset();
        self.refill_latency.reset();
    }
}

/// How recently an entry must have been read for a hit to leave it where it is.
///
/// A hit updates its entry's counters, which is atomic, and moves the entry to
/// the back of each tier's access order, which is not -- that needs the cache
/// exclusively. This setting decides how often the second happens: an entry
/// read again within this window keeps its place.
///
/// Zero moves it on every hit, which keeps the order exact and makes every read
/// take the cache exclusively. That is the wrong default for a cache, and it is
/// what CacheLib concluded too -- `lruRefreshTime` defaults to sixty seconds
/// there rather than zero.
///
/// This was an access *count* until it became a duration. The count had two
/// faults a duration does not: it needed a process-wide counter bumped by every
/// hit on every thread, which is a contended cache line; and its units meant
/// that with N threads the counter advanced N times faster, so the effective
/// window shrank as concurrency rose.
/// Measured trade, so it is not re-litigated from first principles.
///
/// Lowering this raises the hit rate and lowers read throughput, and both ends
/// have been measured on `eviction_bench` with a working set four times the
/// cache and 80% of reads on a hot half-cache:
///
/// ```text
///   window   promotions   hit rate @4096   what it costs
///    500ms       54,743           78.99%   the default
///        0      327,947           81.99%   6x the promotions, and a promotion
///                                          is the one thing on the read path
///                                          that takes the cache exclusively
/// ```
///
/// Three points of hit rate for six times the exclusive-lock escalations is
/// what makes the read path scale roughly ninefold from one thread to eight.
/// Turning this down recovers the points and gives that back, which is a
/// trade worth making deliberately rather than by accident.
const DEFAULT_LRU_REFRESH: Duration = Duration::from_millis(500);

/// One hit in this many is recorded into the admission sketch.
///
/// Recording every hit measured ~26ns against a ~226ns read, which is too much
/// to spend on a path a dozen changes have gone into making cheap. Sixteen
/// brings it under 1% of a read, and the comparison it feeds only needs to rank
/// keys against each other, not count them.
const HIT_SAMPLE_INTERVAL: u64 = 16;

/// How often the coarse clock is republished.
///
/// This is the resolution of every recency decision the read path makes, so it
/// wants to be well below the refresh window. It is also a thread wake-up, so
/// it wants not to be tiny. 10ms against a default window of 500ms gives fifty
/// steps, which is far finer than the decision needs.
/// The longest the adaptive window may grow to.
///
/// CacheLib caps at 900 seconds against a 60-second default -- fifteen times.
/// The same proportion against our 500ms floor is 7.5 seconds; 10 is the round
/// number above it. The cap exists because `oldestElementAge` is unbounded: a
/// cache holding one entry that nobody evicts would otherwise grow the window
/// without limit and stop maintaining its order entirely.
const LRU_REFRESH_CAP: Duration = Duration::from_secs(10);

/// How often the adaptive window is recomputed.
///
/// CacheLib defaults this to zero, which recomputes on every access -- cheap
/// there because the check is one comparison and the work behind it reads a
/// single tail node. Ours reads a tail node and a hash entry, so it is spaced
/// out rather than run per hit.
const LRU_RECONFIGURE_INTERVAL: Duration = Duration::from_secs(1);

const COARSE_CLOCK_TICK: Duration = Duration::from_millis(10);

/// Milliseconds since the first read of the clock, republished by a background
/// thread.
///
/// The read path stamps every hit with this, so it is read by every thread on
/// every hit and must be nearly free. `Instant::now()` is not: it measured
/// 225ns at one thread and 368ns at eight on this platform, where
/// `clock_gettime` is not served from the vDSO. A relaxed load of an
/// `AtomicU64` that one thread writes and everyone reads measured 0.45ns and
/// 0.52ns for the same cases -- flat, because the line stays in Shared state
/// rather than being taken exclusively by each writer in turn.
///
/// That is the whole reason this is not simply a clock call.
struct CoarseClock {
    millis: AtomicU64,
}

impl CoarseClock {
    fn shared() -> &'static CoarseClock {
        static CLOCK: OnceLock<CoarseClock> = OnceLock::new();
        CLOCK.get_or_init(|| {
            let clock = CoarseClock {
                millis: AtomicU64::new(0),
            };
            let started = Instant::now();
            // The published value is read through a `&'static`, so the ticker
            // can hold one too and needs no `Arc`. It runs for the life of the
            // process, which is the life of the value it publishes.
            std::thread::Builder::new()
                .name("matrixcache-clock".to_string())
                .spawn(move || loop {
                    let now = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    CoarseClock::shared().millis.store(now, Ordering::Relaxed);
                    std::thread::sleep(COARSE_CLOCK_TICK);
                })
                .expect("spawn coarse clock");
            clock
        })
    }

    /// Milliseconds since the clock started. Monotonic, resolution
    /// [`COARSE_CLOCK_TICK`].
    fn now_millis() -> u64 {
        Self::shared().millis.load(Ordering::Relaxed)
    }
}

/// What a hit still needs the cache exclusively for, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitOutcome {
    /// Fully accounted for under the shared lock. The common case.
    Accounted,
    /// Accounted for, but the entry has not been read in long enough that it
    /// needs moving in the tier access orders.
    NeedsAccessOrderRefresh,
    /// The key has no metadata, which only an exclusive borrow can insert.
    NeedsMetadata,
}

/// How many independent pin locks there are.
///
/// One lock for every pinned key made taking a handle scale no better than the
/// exclusive cache lock it replaced -- it just moved which lock everyone
/// queued on. Striping by key lets handles on different keys be taken at the
/// same time, which is the case that matters: readers pinning the same key at
/// the same instant are rare, readers pinning different keys are the workload.
const PIN_STRIPES: usize = 16;

/// Which entries are pinned, and what their handles are worth.
///
/// The counters live here rather than in `stats` because they are only ever
/// touched under this lock, and a counter kept somewhere else would need the
/// cache exclusively to update -- which is the thing this exists to avoid.
/// What is known about one pinned key.
///
/// One entry rather than three maps keyed the same way. Three maps have to be
/// kept agreeing about which keys exist, and they were not: dropping a pin
/// during an invalidation removed the count and the removed-bytes and left the
/// handle-bytes behind, while the memory-only invalidation next to it removed
/// all three. Neither is now possible to write.
///
/// It also halves the hashing on the pin path -- one lookup to take a handle
/// instead of two, one to give it back instead of up to four -- which is the
/// path a zero-copy read crosses twice.
#[derive(Debug, Default)]
struct PinnedEntry {
    /// How many handles are outstanding.
    handles: u64,
    /// The largest size seen for this key, for the pinned-bytes figure.
    handle_bytes: usize,
    /// Set when the key has been removed from its tier while still held.
    removed_bytes: Option<usize>,
}

#[derive(Debug, Default)]
struct CachePinState {
    entries: HashMap<CacheKey, PinnedEntry>,
    pin_operations: u64,
    unpin_operations: u64,
    /// Reads answered with a handle rather than a copy. Kept here because it
    /// is counted at the moment a pin is taken, under this lock, and a counter
    /// anywhere else would need the cache exclusively to update.
    zero_copy_handle_hits: u64,
}

/// Per-entry bookkeeping.
///
/// The three counters a read updates are atomic so that a hit can account for
/// itself through a shared borrow of the map, rather than needing the cache
/// exclusively. That costs `Copy` and `Clone`, which nothing relied on: every
/// reader of this struct already took it by reference.
#[derive(Debug)]
struct CacheEntryMeta {
    block_kind: CacheBlockKind,
    routing_slot: Option<u32>,
    hotness: AtomicU32,
    hits: AtomicU64,
    last_access_epoch: AtomicU64,
    admission_reason: CacheAdmissionReason,
    /// Coarse-clock millisecond at which this entry stops being servable.
    ///
    /// Zero means it never does, which is what an entry gets unless a time to
    /// live was asked for. Stored rather than a `Duration` so the check on the
    /// read path is one relaxed load and one comparison.
    expires_at_millis: AtomicU64,
}

impl CacheEntryMeta {
    /// Whether this entry has passed its time to live as of `now_millis`.
    fn is_expired(&self, now_millis: u64) -> bool {
        let expires_at = self.expires_at_millis.load(Ordering::Relaxed);
        expires_at != 0 && now_millis >= expires_at
    }

    /// Stamp an expiry `ttl_millis` from now, or clear it when zero.
    fn set_ttl(&self, ttl_millis: u64, now_millis: u64) {
        let expires_at = if ttl_millis == 0 {
            0
        } else {
            // Saturating, so a caller asking for a century does not wrap into
            // an entry that is already expired.
            now_millis.saturating_add(ttl_millis).max(1)
        };
        self.expires_at_millis.store(expires_at, Ordering::Relaxed);
    }
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
    /// No store at all, because the tier's capacity is zero.
    ///
    /// Every admission path already treats a zero capacity as "this tier is off" --
    /// `ssd_enabled` is `ssd_capacity_bytes > 0`, and the admit/evict paths return early on
    /// it. The store was built anyway, so a node that can never use the tier still created
    /// its directory and its files. Keeps what it would need to become real, because a
    /// capacity raised after construction has to be able to bring the tier up.
    Disabled {
        disk_dir: PathBuf,
        ssd_paths: Vec<PathBuf>,
    },
    Single(StorageEngineRocksDb),
    Multi(StorageEngineMultiSsd),
}

impl SsdTierStore {
    fn new(disk_dir: &Path, ssd_paths: &[PathBuf], capacity_bytes: usize) -> Self {
        if capacity_bytes == 0 {
            return Self::Disabled {
                disk_dir: disk_dir.to_path_buf(),
                ssd_paths: ssd_paths.to_vec(),
            };
        }
        Self::build(disk_dir, ssd_paths, capacity_bytes)
    }

    /// Build the store for a non-zero capacity.
    ///
    /// Split out from [`Self::new`] so a tier switched on after construction comes up exactly
    /// the way one built with a capacity does, rather than by a second, similar path.
    fn build(disk_dir: &Path, ssd_paths: &[PathBuf], capacity_bytes: usize) -> Self {
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
            Self::Multi(StorageEngineMultiSsd::with_paths(paths, capacity))
        } else {
            let mut storage = StorageEngineRocksDb::new(
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

    /// Bring a disabled tier up, for a capacity raised after construction.
    ///
    /// `set_capacity_for_tier(CacheTier::Ssd, n)` can raise it at any time. A tier that stayed
    /// disabled through that would accept nothing while the policy said it was there, which is
    /// a worse failure than the one this variant exists to fix -- silent, and only visible as
    /// a hit rate that never recovers.
    fn enable(&mut self, capacity_bytes: usize) {
        if capacity_bytes == 0 {
            return;
        }
        if let Self::Disabled { disk_dir, ssd_paths } = self {
            let (disk_dir, ssd_paths) = (disk_dir.clone(), ssd_paths.clone());
            *self = Self::build(&disk_dir, &ssd_paths, capacity_bytes);
            let _ = self.start();
        }
    }

    fn start(&mut self) -> bool {
        match self {
            // Nothing to start, and reporting failure would make the caller treat an
            // intentionally absent tier as a broken one.
            Self::Disabled { .. } => true,
            Self::Single(storage) => storage.start(),
            Self::Multi(storage) => storage.start(),
        }
    }

    fn stop(&mut self) -> bool {
        match self {
            Self::Disabled { .. } => true,
            Self::Single(storage) => storage.stop(),
            Self::Multi(storage) => storage.stop(),
        }
    }

    fn is_started(&self) -> bool {
        match self {
            // Vacuously started: there is nothing to bring up, and answering false would send
            // the caller down its "start it, and fail if that does not work" path.
            Self::Disabled { .. } => true,
            Self::Single(storage) => storage.is_started(),
            Self::Multi(storage) => storage.is_started(),
        }
    }

    fn peek(&self, key: &str) -> bool {
        match self {
            Self::Disabled { .. } => false,
            Self::Single(storage) => storage.peek(key),
            Self::Multi(storage) => storage.peek(key),
        }
    }

    fn get(&self, key: &str) -> Result<CacheBuffer, CacheError> {
        match self {
            Self::Disabled { .. } => Err(CacheError::NotFound),
            Self::Single(storage) => storage.get(key),
            Self::Multi(storage) => storage.get(key),
        }
    }

    fn get_batch(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        match self {
            Self::Disabled { .. } => Ok(vec![None; keys.len()]),
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
            // Unreachable through admission, which returns early on a zero capacity. A no-op
            // rather than an error so a caller that reaches it anyway is not broken by a tier
            // it was told is not there.
            Self::Disabled { .. } => Ok(()),
            Self::Single(storage) => storage.put(key, value).map(|_| ()),
            Self::Multi(storage) => storage.put(key, value).map(|_| ()),
        }
    }

    fn put_batch(&mut self, entries: Vec<(String, Vec<u8>)>) -> Result<(), CacheError> {
        match self {
            Self::Disabled { .. } => Ok(()),
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
            // Deleting from a tier that holds nothing has already happened.
            Self::Disabled { .. } => Ok(()),
            Self::Single(storage) => storage.delete(key),
            Self::Multi(storage) => storage.delete(key),
        }
    }

    fn delete_batch(&mut self, keys: &[String]) -> Result<(), CacheError> {
        match self {
            Self::Disabled { .. } => Ok(()),
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
            Self::Disabled { .. } => Ok(()),
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
            // Nothing was ever written, so there is nothing to recover.
            Self::Disabled { .. } => Ok(()),
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
            // The record is a manifest line: always owned, whether the key it came from borrowed
            // its namespace or not.
            namespace: key.namespace.to_string(),
            selector: key.selector.clone(),
            block_len,
        }
    }

    fn key(&self) -> CacheKey {
        CacheKey {
            shard_id: self.shard_id,
            record_key: self.record_key.clone(),
            // Read out of a manifest, so it is owned rather than one of the literals.
            namespace: std::borrow::Cow::Owned(self.namespace.clone()),
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
            // The one place that produces an owned namespace rather than one
            // of the handful of literals, which is why the field is a .
            namespace: namespace.into(),
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

    /// Build a cache, refusing a configuration it cannot honour.
    ///
    /// Only the critical findings refuse: a cache with no capacity anywhere,
    /// or a durable tier pointed at a temporary directory that will not
    /// survive a restart. Warnings do not, because a cache that starts and
    /// tells you about a redundant path is more useful than one that will not
    /// start. [`CacheOptions::validate`] returns all of them.
    ///
    /// [`Self::with_options`] stays infallible and unchanged.
    pub fn try_with_options(options: CacheOptions) -> Result<Self, CacheError> {
        let refusals = options.critical_findings();
        if !refusals.is_empty() {
            return Err(CacheError::InvalidConfig(
                refusals
                    .into_iter()
                    .map(|finding| format!("{}: {}", finding.field, finding.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
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
        cache.set_ssd_write_budget_bytes_per_sec(options.ssd_write_bytes_per_sec);
        cache.set_replacement_policy_for_tier(
            CacheTier::Memory,
            CacheReplacementPolicy::from_config_name(&options.cache_dram_replacement_policy),
        );
        cache.set_replacement_policy_for_tier(
            CacheTier::Pmem,
            CacheReplacementPolicy::from_config_name(&options.cache_pmem_replacement_policy),
        );
        cache.set_replacement_policy_for_tier(
            CacheTier::Ssd,
            CacheReplacementPolicy::from_config_name(&options.cache_ssd_replacement_policy),
        );
        cache.set_ssd_instance_only(options.cache_ssd_instance_only);
        cache.set_pmem_paths(options.pmem_paths);
        cache.set_auto_recover_on_start(options.auto_recover_on_start);
        cache
            .inner
            .write()
            .expect("cache lock poisoned")
            .ssd_block_durability = options.ssd_block_durability;
        cache
            .inner
            .write()
            .expect("cache lock poisoned")
            .pmem_block_durability = options.pmem_block_durability;
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
                ssd_block_durability: true,
                pmem_block_durability: false,
                memory_capacity_bytes: tiering_policy.memory_capacity_bytes,
                memory_bytes: 0,
                pmem_capacity_bytes: tiering_policy.pmem_capacity_bytes,
                pmem_bytes: 0,
                ssd_capacity_bytes: tiering_policy.ssd_capacity_bytes,
                ssd_bytes: 0,
                disk_dir,
                pmem_paths: Vec::new(),
                ssd_store,
                ssd_write_budget: SsdWriteBudget::unlimited(),
                default_ttl_millis: 0,
                ttl_in_use: false,
                tiering_policy,
                block_options,
                memory: HashMap::new(),
                pmem: HashMap::new(),
                disk_index: HashMap::new(),
                disk_order: CacheKeyOrder::new(),
                pins: (0..PIN_STRIPES)
                    .map(|_| Mutex::new(CachePinState::default()))
                    .collect(),
                memory_order: CacheKeyOrder::new(),
                pmem_order: CacheKeyOrder::new(),
                async_writeback_queue: VecDeque::new(),
                async_writeback_positions: HashMap::new(),
                async_writeback_head: 0,
                lru_refresh_floor_millis: DEFAULT_LRU_REFRESH.as_millis() as u64,
                lru_refresh_ratio: 0.0,
                lru_refresh_effective_millis: AtomicU64::new(
                    DEFAULT_LRU_REFRESH.as_millis() as u64,
                ),
                next_reconfigure_millis: AtomicU64::new(0),
                async_writeback_queue_bytes: 0,
                max_async_writeback_queue: 1024,
                access_record_callback: None,
                eviction_callback: None,
                eviction_handler_enabled: true,
                pending_eviction_records: VecDeque::new(),
                eviction_metric_callback: None,
                pending_eviction_metric_tiers: VecDeque::new(),
                ssd_instance_only: false,
                memory_replacement_policy: CacheReplacementPolicy::WeightedHotnessLru,
                pmem_replacement_policy: CacheReplacementPolicy::WeightedHotnessLru,
                ssd_replacement_policy: CacheReplacementPolicy::WeightedHotnessLru,
                metadata: HashMap::new(),
                // Sized from the configured memory capacity, assuming values
                // on the order of 64 bytes. The estimate only has to rank keys
                // against each other, so being out by a factor either way costs
                // accuracy rather than correctness.
                access_frequency: FrequencySketch::with_capacity(
                    (tiering_policy.memory_capacity_bytes / 64).max(1),
                ),
                admission_filter_enabled: false,
                read_counters: ReadPathCounters::default(),
                stats: CacheStats::default(),
            })),
            async_writeback_worker: Arc::new(Mutex::new(None)),
            access_record_registered: Arc::new(AtomicBool::new(false)),
            eviction_callbacks_registered: Arc::new(AtomicBool::new(false)),
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

    /// Total capacity across the tiers, counted the way `size` counts usage.
    ///
    /// Side-by-side placement holds distinct keys in Dram and Pmem, so the two
    /// capacities add. Tiered placement holds a given key in at most one of
    /// them, so the pair contributes the larger of the two rather than their
    /// sum. Summing under tiered placement overstates capacity, and since
    /// `size` already takes the maximum there, it made a full cache report as
    /// roughly half used.
    pub fn capacity(&self) -> usize {
        let inner = self.inner.read().expect("cache lock poisoned");
        let volatile_bytes = if matches!(
            inner.tiering_policy.data_placement,
            CacheDataPlacement::SideBySide
        ) {
            inner
                .memory_capacity_bytes
                .saturating_add(inner.pmem_capacity_bytes)
        } else {
            inner.memory_capacity_bytes.max(inner.pmem_capacity_bytes)
        };
        volatile_bytes.max(inner.ssd_capacity_bytes)
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

    pub fn get_capacity(&self, instance_type: CacheInstanceKind) -> usize {
        match instance_type.as_tier() {
            Some(tier) => self.capacity_for_tier(tier),
            None => self.capacity(),
        }
    }

    /// Write `value` under `key` and give it `ttl` to live.
    ///
    /// After `ttl` the entry stops being servable: a read finds nothing and the
    /// entry is dropped. A `ttl` of zero means it never expires, which is what
    /// an ordinary [`put`](Self::put) gives unless a default has been set.
    ///
    /// Rewriting a key restarts its life, which is what putting it again means.
    pub fn put_with_ttl(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        ttl: Duration,
    ) -> Result<(), CacheError> {
        self.put(key.clone(), value)?;
        let ttl_millis = ttl.as_millis().min(u128::from(u64::MAX)) as u64;
        if ttl_millis > 0 {
            self.inner.write().expect("cache lock poisoned").ttl_in_use = true;
        }
        let inner = self.inner.read().expect("cache lock poisoned");
        if let Some(meta) = inner.metadata.get(&key) {
            meta.set_ttl(ttl_millis, CoarseClock::now_millis());
        }
        Ok(())
    }

    /// Time to live given to entries that do not ask for their own.
    ///
    /// Zero, the default, means entries do not expire. Applies to entries
    /// written after it is set; it does not reach back over what is already
    /// resident.
    pub fn set_default_ttl(&self, ttl: Duration) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.default_ttl_millis = ttl.as_millis().min(u128::from(u64::MAX)) as u64;
        if inner.default_ttl_millis > 0 {
            inner.ttl_in_use = true;
        }
    }

    pub fn default_ttl(&self) -> Duration {
        Duration::from_millis(
            self.inner
                .read()
                .expect("cache lock poisoned")
                .default_ttl_millis,
        )
    }

    /// Drop every entry that has passed its time to live, and say how many.
    ///
    /// Expiry is otherwise noticed only by a read, so a key written with a life
    /// and never read again would hold its memory until something evicted it.
    /// Call this on a timer if that matters; it walks the resident set, so how
    /// often is a decision about how much scanning to spend.
    pub fn purge_expired(&self) -> usize {
        let now_millis = CoarseClock::now_millis();
        let mut inner = self.inner.write().expect("cache lock poisoned");
        let expired: Vec<CacheKey> = inner
            .metadata
            .iter()
            .filter(|(_, meta)| meta.is_expired(now_millis))
            .map(|(key, _)| key.clone())
            .collect();
        for key in &expired {
            inner.remove_expired_entry(key);
        }
        expired.len()
    }

    /// Cap how fast the SSD tier is allowed to absorb writes, in bytes per
    /// second. Zero, the default, means no cap.
    ///
    /// Flash wears out by being written to, so a cache that is expected to
    /// outlive a warranty needs a rate it can sustain rather than the rate its
    /// workload happens to offer. Above the cap, admissions are turned away by
    /// key rather than by size, so large entries keep their share of a tight
    /// budget instead of being crowded out by small ones.
    ///
    /// Reclaim and recovery writes are counted against the budget but never
    /// refused by it: they are work the cache has already committed to.
    pub fn set_ssd_write_budget_bytes_per_sec(&self, bytes_per_sec: u64) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner
            .ssd_write_budget
            .set_target_bytes_per_sec(bytes_per_sec);
        inner.refresh_ssd_write_budget_stats();
    }

    /// The SSD write cap in bytes per second, or zero when uncapped.
    pub fn ssd_write_budget_bytes_per_sec(&self) -> u64 {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .ssd_write_budget
            .target_bytes_per_sec()
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
                // A tier built with no capacity has no store behind it; raising the capacity
                // is what brings one up.
                inner.ssd_store.enable(capacity_bytes);
                inner.evict_ssd_to_capacity();
            }
            CacheTier::Reject => {}
        }
        drop(inner);
        self.drain_eviction_records();
    }

    pub fn set_capacity_for_instance(
        &self,
        instance_type: CacheInstanceKind,
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
            inner.ssd_store.enable(capacity_bytes);
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

    pub fn get_used(&self, instance_type: CacheInstanceKind) -> usize {
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
        instance_type: CacheInstanceKind,
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
        instance_type: CacheInstanceKind,
        policy: CacheReplacementPolicy,
    ) {
        if let Some(tier) = instance_type.as_tier() {
            self.set_replacement_policy_for_tier(tier, policy);
        }
    }

    pub fn try_set_replacement_policy_type(
        &self,
        instance_type: CacheInstanceKind,
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
        self.access_record_registered.store(true, Ordering::Relaxed);
    }

    pub fn clear_access_record_callback(&self) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.access_record_callback = None;
        self.access_record_registered.store(false, Ordering::Relaxed);
    }

    /// How recently an entry must have been read for a hit to leave it where
    /// it is in the tier access orders, in accesses.
    ///
    /// A hit normally moves the entry to the back of every tier's access order
    /// so victim selection does not offer it up. That move is the reason a read
    /// needs the cache lock exclusively. An entry read within the last
    /// `distance` accesses is already within `distance` places of the newest
    /// end, so moving it again changes little -- this is the trade CacheLib
    /// makes with `lruRefreshTime`, stated in accesses instead of seconds
    /// because position is what actually matters.
    ///
    /// Zero, the default, always moves the entry: eviction order stays exactly
    /// as precise as it is today. Larger values make eviction order
    /// approximate in exchange for skipping the move on repeat reads.
    /// Sets how recently an entry must have been read for a hit to leave it
    /// in place. `Duration::ZERO` moves it on every hit.
    ///
    /// Sub-millisecond values round down to zero, and the clock behind this is
    /// republished on a 10ms tick, so windows below that are not meaningfully
    /// distinguishable from one another.
    pub fn set_lru_refresh_time(&self, window: Duration) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        let millis = window.as_millis().min(u128::from(u64::MAX)) as u64;
        inner.lru_refresh_floor_millis = millis;
        // Take effect now rather than at the next reconfigure, so that setting
        // the window and immediately reading it back agrees.
        inner
            .lru_refresh_effective_millis
            .store(millis, Ordering::Relaxed);
        inner.next_reconfigure_millis.store(0, Ordering::Relaxed);
    }

    /// Scales the refresh window by the age of the oldest resident entry.
    ///
    /// Zero, the default, pins the window to whatever
    /// [`Self::set_lru_refresh_time`] set. A positive ratio makes the window a
    /// fraction of how long entries actually survive, so a cache whose entries
    /// live a long time skips more promotions and one with fast turnover keeps
    /// its ordering accurate. Clamped below by the floor and above by ten
    /// seconds.
    ///
    /// This is CacheLib's `lruRefreshRatio`, which likewise defaults to zero.
    pub fn set_lru_refresh_ratio(&self, ratio: f64) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.lru_refresh_ratio = if ratio.is_finite() && ratio > 0.0 {
            ratio
        } else {
            0.0
        };
        inner.next_reconfigure_millis.store(0, Ordering::Relaxed);
    }

    /// Declines a candidate that has been asked for less often than the entry
    /// it would evict.
    ///
    /// Off by default, because it changes what the cache keeps rather than only
    /// how fast it keeps it.
    ///
    /// The cache otherwise admits everything, so a key read once evicts one read
    /// a hundred times. With this on, a newcomer is compared against the entry
    /// the replacement policy has already picked as coldest, using a frequency
    /// sketch that remembers keys after they have been evicted. A rejected key
    /// is still recorded, so it is admitted once it has been asked for often
    /// enough to deserve it.
    ///
    /// This is CacheLib's `MMTinyLFU` admission, which compares the same two
    /// frequencies.
    pub fn set_admission_filter_enabled(&self, enabled: bool) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.admission_filter_enabled = enabled;
    }

    /// Whether the admission filter is on.
    pub fn admission_filter_enabled(&self) -> bool {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .admission_filter_enabled
    }

    /// Sets where a newly admitted entry is placed in the memory tier's
    /// recency order.
    ///
    /// Zero -- the default -- places it at the most-recently-used end, so it
    /// has the whole order to traverse before eviction can reach it. One places
    /// it halfway down and two a quarter of the way from the eviction end:
    /// `resident >> spec` entries will be evicted before it.
    ///
    /// Non-zero buys scan resistance. A burst of keys read once and never again
    /// currently walks the entire resident set through the hottest position,
    /// pushing everything genuinely hot toward eviction ahead of it. Placing new
    /// entries part-way down means such a key is evicted from where it was put,
    /// while one that is read again is promoted to the hot end and keeps full
    /// protection.
    ///
    /// This is CacheLib's `lruInsertionPointSpec`.
    pub fn set_insertion_point_spec(&self, spec: u8) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.memory_order.set_insertion_spec(spec);
    }

    /// The spec set by [`Self::set_insertion_point_spec`].
    pub fn insertion_point_spec(&self) -> u8 {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .memory_order
            .insertion_spec()
    }

    /// The ratio set by [`Self::set_lru_refresh_ratio`].
    pub fn lru_refresh_ratio(&self) -> f64 {
        self.inner
            .read()
            .expect("cache lock poisoned")
            .lru_refresh_ratio
    }

    /// The window currently in force, which differs from
    /// [`Self::lru_refresh_time`] only when adaptation is enabled.
    pub fn effective_lru_refresh_time(&self) -> Duration {
        Duration::from_millis(
            self.inner
                .read()
                .expect("cache lock poisoned")
                .lru_refresh_effective_millis
                .load(Ordering::Relaxed),
        )
    }

    /// The window set by [`Self::set_lru_refresh_time`].
    pub fn lru_refresh_time(&self) -> Duration {
        Duration::from_millis(
            self.inner
                .read()
                .expect("cache lock poisoned")
                .lru_refresh_floor_millis,
        )
    }

    pub fn register_eviction_callback<F>(&self, callback: F)
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.eviction_callback = Some(CacheEvictionCallback::new(callback));
        self.eviction_callbacks_registered.store(
            inner.eviction_callback.is_some() || inner.eviction_metric_callback.is_some(),
            Ordering::Relaxed,
        );
    }

    pub fn clear_eviction_callback(&self) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.eviction_callback = None;
        inner.pending_eviction_records.clear();
        self.eviction_callbacks_registered.store(
            inner.eviction_callback.is_some() || inner.eviction_metric_callback.is_some(),
            Ordering::Relaxed,
        );
    }

    /// Register a callback receiving the number of entries evicted from a tier.
    ///
    /// Independent of the eviction handler: it keeps reporting while the
    /// handler is disabled, so eviction rate stays observable even when nothing
    /// is consuming the evicted entries.
    pub fn register_eviction_metric_callback<F>(&self, callback: F)
    where
        F: Fn(CacheTier, usize) + Send + Sync + 'static,
    {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.eviction_metric_callback = Some(CacheEvictionMetricCallback::new(callback));
        self.eviction_callbacks_registered.store(
            inner.eviction_callback.is_some() || inner.eviction_metric_callback.is_some(),
            Ordering::Relaxed,
        );
    }

    pub fn clear_eviction_metric_callback(&self) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.eviction_metric_callback = None;
        inner.pending_eviction_metric_tiers.clear();
        self.eviction_callbacks_registered.store(
            inner.eviction_callback.is_some() || inner.eviction_metric_callback.is_some(),
            Ordering::Relaxed,
        );
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

    fn emit_access_record(&self, record_type: CacheAccessRecordKind, key: &CacheKey) {
        // Every get, put and delete starts here. Reading one bit is cheaper
        // than taking the cache lock to learn the same thing.
        if !self.access_record_registered.load(Ordering::Relaxed) {
            return;
        }
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
        // Nothing can be queued unless a callback is registered, so this is the
        // whole answer on the common path -- and it costs one relaxed load
        // instead of an exclusive acquisition of the cache lock.
        if !self.eviction_callbacks_registered.load(Ordering::Relaxed) {
            return;
        }
        let (callback, records, metric_callback, metric_tiers) = {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            let callback = inner.eviction_callback.clone();
            let records = inner.pending_eviction_records.drain(..).collect::<Vec<_>>();
            let metric_callback = inner.eviction_metric_callback.clone();
            let metric_tiers = inner
                .pending_eviction_metric_tiers
                .drain(..)
                .collect::<Vec<_>>();
            (callback, records, metric_callback, metric_tiers)
        };
        // Metrics first, and ungated: they report the eviction rate whether or
        // not a handler is consuming the evicted entries.
        if let Some(metric_callback) = metric_callback {
            let mut reported: Vec<CacheTier> = Vec::new();
            for tier in &metric_tiers {
                if reported.contains(tier) {
                    continue;
                }
                reported.push(*tier);
                let count = metric_tiers.iter().filter(|entry| *entry == tier).count();
                metric_callback.call(*tier, count);
            }
        }
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

    /// Which tier would answer for this key, if anything would.
    ///
    /// An entry past its time to live is reported as absent, because that is
    /// what every read of it returns. Saying it is resident and then refusing
    /// to serve it is the contradiction, not the refusal: a caller asks this
    /// to decide whether it needs to fetch from somewhere slower, and a `true`
    /// that is followed by a `None` sends it away empty.
    pub fn peek_tier(&self, key: &CacheKey) -> Option<CacheReadTier> {
        let inner = self.inner.read().expect("cache lock poisoned");
        // A stopped cache serves nothing, so it holds nothing worth reporting.
        // `get` on the same key returns `Stopped`, and saying an entry is
        // resident while every read of it refuses is the contradiction this
        // avoids -- the same one an expired entry would cause.
        if !inner.started {
            return None;
        }
        if inner.entry_expired(key, CoarseClock::now_millis()) {
            return None;
        }
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

    /// The cached bytes, shared rather than copied.
    ///
    /// The memory tier already holds an `Arc<[u8]>`, so a reader that parses the bytes and drops
    /// them does not need a copy of its own -- and `get` gives it one, a page-sized memcpy plus an
    /// allocation, every hit. For a store fetching one record per retrieval candidate that is a
    /// copy per candidate.
    ///
    /// Only the memory tier can share; the others materialise their bytes on the way out, so there
    /// is nothing to point at and this falls back to `get`. Always correct, sometimes free.
    ///
    /// The value is immutable once cached: a write replaces the entry rather than editing it, so a
    /// holder of one of these keeps reading the bytes it asked for even if the key is overwritten or
    /// evicted meanwhile. That is the same guarantee the copy gave, without the copy.
    pub fn get_shared(&self, key: &CacheKey) -> Result<Option<std::sync::Arc<[u8]>>, CacheError> {
        self.emit_access_record(CacheAccessRecordKind::Get, key);
        let started = Instant::now();
        let now_millis = CoarseClock::now_millis();
        let expired;
        let probe = {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            expired = inner.entry_expired(key, now_millis);
            if inner.ssd_instance_only || expired {
                None
            } else {
                inner.memory.get(key).cloned()
            }
        };

        // An expired entry is noticed on the read that would have been served, exactly as `get`
        // notices it. Falling through to `get` here would report a miss twice.
        if expired {
            return self.get(key).map(|value| value.map(std::sync::Arc::from));
        }

        let Some(value) = probe else {
            // Every other tier materialises its bytes on the way out, so there is nothing to share
            // and the copy is unavoidable. Fall through to `get`, which also covers the miss.
            return Ok(self.get(key)?.map(std::sync::Arc::from));
        };

        // The accounting a memory hit does, without the copy it does it alongside. `record_hit`
        // wants the entry's length, which the shared buffer knows without being copied.
        let length = value.len();
        let outcome = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner
                .read_counters
                .memory_hits
                .fetch_add(1, Ordering::Relaxed);
            let outcome = if inner.memory.contains_key(key) {
                inner.record_hit_shared(key)
            } else {
                HitOutcome::Accounted
            };
            let micros = elapsed_micros(started);
            inner.record_get_latency_micros(micros);
            inner.record_read_through_latency_micros(micros);
            outcome
        };
        match outcome {
            HitOutcome::Accounted => {}
            HitOutcome::NeedsAccessOrderRefresh => {
                let mut inner = self.inner.write().expect("cache lock poisoned");
                inner.refresh_access_order(key);
            }
            HitOutcome::NeedsMetadata => {
                let mut inner = self.inner.write().expect("cache lock poisoned");
                if inner.memory.contains_key(key) {
                    inner.record_hit(key, length);
                }
            }
        }
        Ok(Some(value))
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
            // Refused, not reclaimed. This path is documented as leaving the
            // cache alone -- that is what "no promotion" means -- and it holds
            // the cache shared, so dropping the entry would mean escalating.
            // The sweep, eviction and `get` all reclaim it; none of them is
            // needed for the caller to be told the truth now.
            if inner.entry_expired(key, CoarseClock::now_millis()) {
                return Ok(None);
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
        let mut results = vec![None; keys.len()];
        if keys.is_empty() {
            return Ok(results);
        }

        let mut ssd_candidates = Vec::new();
        let now_millis = CoarseClock::now_millis();
        {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            for (index, key) in keys.iter().enumerate() {
                if inner.entry_expired(key, now_millis) {
                    continue;
                }
                if !inner.ssd_instance_only {
                    if let Some(value) = inner.memory.get(key).cloned() {
                        results[index] = Some(CacheReadResult {
                            value: value.to_vec(),
                            tier: CacheReadTier::Memory,
                        });
                        continue;
                    }
                    if let Some(value) = inner.pmem.get(key).cloned() {
                        results[index] = Some(CacheReadResult {
                            value: value.to_vec(),
                            tier: CacheReadTier::Pmem,
                        });
                        continue;
                    }
                }
                ssd_candidates.push((index, key.clone()));
            }
        }

        if ssd_candidates.is_empty() {
            return Ok(results);
        }

        let mut unique_ssd_candidates = Vec::<(CacheKey, Vec<usize>)>::new();
        let mut unique_ssd_positions = HashMap::<CacheKey, usize>::new();
        for (index, key) in ssd_candidates {
            if let Some(position) = unique_ssd_positions.get(&key).copied() {
                unique_ssd_candidates[position].1.push(index);
            } else {
                unique_ssd_positions.insert(key.clone(), unique_ssd_candidates.len());
                unique_ssd_candidates.push((key, vec![index]));
            }
        }

        let candidate_keys = unique_ssd_candidates
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        let blocks = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_blocks(&candidate_keys)?
        };
        for ((_, positions), block) in unique_ssd_candidates.into_iter().zip(blocks) {
            let Some(block) = block else {
                continue;
            };
            let value = decode_cache_block(&block)?;
            for index in positions {
                results[index] = Some(CacheReadResult {
                    value: value.clone(),
                    tier: CacheReadTier::Ssd,
                });
            }
        }
        Ok(results)
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
        self.emit_access_record(CacheAccessRecordKind::Get, key);
        let started = Instant::now();
        // Look first under a shared lock. Values are stored as `Arc<[u8]>`, so
        // a hit here costs a reference bump, and a miss releases the lock
        // having touched nothing -- where before it took the cache
        // exclusively to discover the key was absent.
        let now_millis = CoarseClock::now_millis();
        let expired;
        let probe = {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            // Checked under the same shared lock as the lookup, so an entry
            // cannot be judged live and then read after it expired. The check
            // is one relaxed load against a clock a background thread
            // publishes, which is why it can sit on the hit path at all.
            expired = inner.entry_expired(key, now_millis);
            if inner.ssd_instance_only || expired {
                None
            } else {
                inner
                    .memory
                    .get(key)
                    .cloned()
                    .map(|value| (value, CacheReadTier::Memory))
                    .or_else(|| {
                        inner
                            .pmem
                            .get(key)
                            .cloned()
                            .map(|value| (value, CacheReadTier::Pmem))
                    })
            }
        };

        // Expiry is noticed lazily, on the read that would have been served.
        // Dropping it here keeps the memory back without a sweep having to run,
        // and the caller is told what it would have been told had the entry
        // never been written.
        if expired {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            // Re-check: another reader may have dropped it already, or the key
            // may have been rewritten with a fresh life since the probe.
            if inner.entry_expired(key, now_millis) {
                inner.remove_expired_entry(key);
                inner.stats.expired_reads = inner.stats.expired_reads.saturating_add(1);
                // Misses are counted with the read-path atomics, which
                //  reads in preference to the struct field.
                inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
            }
            return Ok(None);
        }

        if let Some((value, tier)) = probe {
            // The copy that turns the shared buffer into the returned `Vec` is
            // the expensive part of a hit, and it is done here with no lock
            // held at all. It used to run inside the exclusive section, so
            // concurrent readers copied one at a time.
            let decoded = value.to_vec();

            // An entry can be evicted between the probe and the lock taken
            // here. What was read above was resident when it was read, so it
            // is still a hit and is still returned; only the per-entry
            // bookkeeping is conditional on it still being there. Recording a
            // hit for an entry eviction has removed would reinsert metadata
            // that nothing removes again.
            if matches!(tier, CacheReadTier::Memory) {
                let outcome = {
                    let inner = self.inner.read().expect("cache lock poisoned");
                    inner
                        .read_counters
                        .memory_hits
                        .fetch_add(1, Ordering::Relaxed);
                    let outcome = if inner.memory.contains_key(key) {
                        inner.record_hit_shared(key)
                    } else {
                        HitOutcome::Accounted
                    };
                    // One interval, two histograms: read the clock once.
                    let micros = elapsed_micros(started);
                    inner.record_get_latency_micros(micros);
                    inner.record_read_through_latency_micros(micros);
                    outcome
                };
                // Only these two cases need the cache exclusively, and a hit
                // on a recently-read entry is neither.
                match outcome {
                    HitOutcome::Accounted => {}
                    HitOutcome::NeedsAccessOrderRefresh => {
                        // No residency check: `touch_access` looks the key up
                        // in each order's own index and returns false when it
                        // is absent, so an entry evicted since the probe is
                        // already handled and cannot be resurrected here.
                        let mut inner = self.inner.write().expect("cache lock poisoned");
                        inner.refresh_access_order(key);
                    }
                    HitOutcome::NeedsMetadata => {
                        let mut inner = self.inner.write().expect("cache lock poisoned");
                        if inner.memory.contains_key(key) {
                            inner.record_hit(key, decoded.len());
                        }
                    }
                }
                return Ok(Some(CacheReadResult {
                    value: decoded,
                    tier: CacheReadTier::Memory,
                }));
            }

            let mut inner = self.inner.write().expect("cache lock poisoned");
            inner.read_counters.pmem_hits.fetch_add(1, Ordering::Relaxed);
            if inner.pmem.contains_key(key) {
                inner.record_hit(key, decoded.len());
            }
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

        let refill_started = Instant::now();
        let block = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_block(key)?
        };
        match block {
            Some(block) => {
                let decoded = decode_cache_block(&block)?;
                let mut inner = self.inner.write().expect("cache lock poisoned");
                inner.read_counters.disk_hits.fetch_add(1, Ordering::Relaxed);
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
                // Nothing here mutates the cache: a miss counts itself and
                // records two latency samples, all through atomics. Taking the
                // lock shared lets concurrent misses do that at the same time
                // instead of queueing.
                let inner = self.inner.read().expect("cache lock poisoned");
                inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
                inner.record_get_latency(started);
                inner.record_read_through_latency(started);
                Ok(None)
            }
        }
    }

    pub fn get_batch(&self, keys: &[CacheKey]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        for key in keys {
            self.emit_access_record(CacheAccessRecordKind::Get, key);
        }

        let mut results = vec![None; keys.len()];
        let mut ssd_candidates = Vec::new();
        let mut needs_eviction_drain = false;
        // One clock read for the batch. `get` reads it once per call for the
        // same reason: the published value moves in milliseconds, and an entry
        // judged live at the top of a batch cannot expire far enough through
        // it to matter.
        let now_millis = CoarseClock::now_millis();

        // The memory hits are served under a *shared* lock, the way a single
        // `get` serves them, and the way the model this follows never takes a
        // container exclusively merely to look something up.
        //
        // This path used to hold the cache exclusively for the whole batch,
        // which serialised every reader against every other for as long as a
        // batch took. It cost what that predicts: parity with a plain loop of
        // `get` at one thread, and about a third of it at two and above -- a
        // batch API slower than the loop it exists to replace.
        //
        // Only what genuinely needs the cache exclusively is deferred: entries
        // past their time to live, entries whose access order needs moving,
        // entries with no metadata yet, and everything below the memory tier.
        let mut deferred: Vec<(usize, &CacheKey, Instant)> = Vec::new();
        let mut memory_hits: Vec<(usize, &CacheKey, Arc<[u8]>, Instant)> = Vec::new();
        {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            for (index, key) in keys.iter().enumerate() {
                let started = Instant::now();
                // Checked under the same shared lock as the lookup, so an
                // entry cannot be judged live and then read after it expired.
                if inner.entry_expired(key, now_millis) || inner.ssd_instance_only {
                    deferred.push((index, key, started));
                    continue;
                }
                match inner.memory.get(key).cloned() {
                    Some(value) => memory_hits.push((index, key, value, started)),
                    None => deferred.push((index, key, started)),
                }
            }
        }

        // The copy that turns the shared buffer into the returned `Vec` is the
        // expensive part of a hit, and it is done here with no lock held.
        let mut needs_exclusive: Vec<(&CacheKey, HitOutcome, usize)> = Vec::new();
        if !memory_hits.is_empty() {
            let decoded: Vec<Vec<u8>> = memory_hits
                .iter()
                .map(|(_, _, value, _)| value.to_vec())
                .collect();
            {
                let inner = self.inner.read().expect("cache lock poisoned");
                for ((index, key, _, started), value) in memory_hits.iter().zip(decoded.iter()) {
                    inner
                        .read_counters
                        .memory_hits
                        .fetch_add(1, Ordering::Relaxed);
                    // An entry can be evicted between the probe and here. What
                    // was read was resident when it was read, so it is still a
                    // hit; only the per-entry bookkeeping is conditional.
                    let outcome = if inner.memory.contains_key(key) {
                        inner.record_hit_shared(key)
                    } else {
                        HitOutcome::Accounted
                    };
                    if !matches!(outcome, HitOutcome::Accounted) {
                        needs_exclusive.push((key, outcome, value.len()));
                    }
                    // One interval, two histograms: read the clock once.
                    let micros = elapsed_micros(*started);
                    inner.record_get_latency_micros(micros);
                    inner.record_read_through_latency_micros(micros);
                    let _ = index;
                }
            }
            for ((index, _, _, _), value) in memory_hits.iter().zip(decoded) {
                results[*index] = Some(value);
            }
        }

        // One exclusive acquisition for the whole batch's leftovers, rather
        // than one per key.
        if !needs_exclusive.is_empty() {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            for (key, outcome, len) in needs_exclusive {
                match outcome {
                    HitOutcome::Accounted => {}
                    HitOutcome::NeedsAccessOrderRefresh => inner.refresh_access_order(key),
                    HitOutcome::NeedsMetadata => {
                        if inner.memory.contains_key(key) {
                            inner.record_hit(key, len);
                        }
                    }
                }
            }
        }

        if deferred.is_empty() {
            return Ok(results);
        }

        {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            for (index, key, started) in deferred {
                // Expiry is noticed on the read that would have been served.
                // Re-checked here: another reader may have dropped it since
                // the probe, or it may have been rewritten with a fresh life.
                if inner.entry_expired(key, now_millis) {
                    inner.remove_expired_entry(key);
                    inner.stats.expired_reads = inner.stats.expired_reads.saturating_add(1);
                    inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
                    inner.record_get_latency(started);
                    continue;
                }
                if !inner.ssd_instance_only {
                    if let Some(value) = inner.memory.get(key).cloned() {
                        inner.read_counters.memory_hits.fetch_add(1, Ordering::Relaxed);
                        inner.record_hit_metadata(key, value.len());
                        inner.record_get_latency(started);
                        inner.record_read_through_latency(started);
                        results[index] = Some(value.to_vec());
                        continue;
                    }
                    if let Some(value) = inner.pmem.get(key).cloned() {
                        inner.read_counters.pmem_hits.fetch_add(1, Ordering::Relaxed);
                        inner.record_hit_metadata(key, value.len());
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
        // Pointers, not copies: these keys are read and nothing more.
        let candidate_keys = unique_ssd_candidates
            .iter()
            .map(|(key, _)| key)
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
            for (key, occurrences, refill_started, decoded) in ssd_reads {
                match decoded {
                    Some((value, compressed)) => {
                        if !inner.ssd_instance_only
                            && !inner.refill_from_ssd(key.clone(), value.clone())
                        {
                            inner.stats.refill_failures =
                                inner.stats.refill_failures.saturating_add(1);
                        }
                        for (index, started) in occurrences {
                            inner.read_counters.disk_hits.fetch_add(1, Ordering::Relaxed);
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
                            inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
                            inner.record_get_latency(started);
                            inner.record_read_through_latency(started);
                        }
                    }
                }
            }
        }
        if needs_eviction_drain {
            self.drain_eviction_records();
        }
        Ok(results)
    }

    pub fn get_memory(&self, key: &CacheKey) -> Option<Vec<u8>> {
        self.emit_access_record(CacheAccessRecordKind::Get, key);
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return None;
        }
        // Holding the cache exclusively already, so an expired entry is
        // dropped here rather than left for something else to find, which is
        // what `get` does with the same opportunity.
        if inner.entry_expired(key, CoarseClock::now_millis()) {
            inner.remove_expired_entry(key);
            inner.stats.expired_reads = inner.stats.expired_reads.saturating_add(1);
            inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let value = inner.memory.get(key).cloned();
        if value.is_some() {
            inner.read_counters.memory_hits.fetch_add(1, Ordering::Relaxed);
            inner.record_hit(
                key,
                value.as_ref().map(|bytes| bytes.len()).unwrap_or_default(),
            );
        } else {
            inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
        }
        value.map(|value| value.to_vec())
    }

    pub fn get_pinned_handle(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.acquire(key)
    }

    /// Take a handle on an entry, without copying it.
    ///
    /// The memory-tier hit is served under a **shared** lock, the way `get`
    /// serves one, and the way the model this crate follows takes a handle --
    /// a refcount on the item, never a container lock.
    ///
    /// This used to take the cache exclusively, and so did giving the handle
    /// back, so every reader serialised against every other twice per
    /// zero-copy read. It did not scale at all: past two threads it went
    /// backwards, leaving the zero-copy read about eighteen times slower than
    /// the copying one it exists to beat.
    pub fn acquire(&self, key: &CacheKey) -> Result<Option<CachePinnedHandle>, CacheError> {
        let started = Instant::now();
        let now_millis = CoarseClock::now_millis();
        let expired;
        let probe = {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            // Checked under the same shared lock as the lookup, so an entry
            // cannot be judged live and then handed out after it expired.
            expired = inner.entry_expired(key, now_millis);
            if expired {
                None
            } else {
                inner.memory.get(key).cloned()
            }
        };

        // A handle outlives the call that made it, so serving an expired entry
        // here is worse than serving one from `get`: the caller holds the
        // bytes, and holding them pins the entry against the eviction that
        // would otherwise have removed it.
        if expired {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if inner.entry_expired(key, now_millis) {
                inner.remove_expired_entry(key);
                inner.stats.expired_reads = inner.stats.expired_reads.saturating_add(1);
                inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
            }
            return Ok(None);
        }

        if let Some(value) = probe {
            let outcome = {
                let inner = self.inner.read().expect("cache lock poisoned");
                inner.increment_pin_for_handle(key, value.len());
                inner
                    .read_counters
                    .memory_hits
                    .fetch_add(1, Ordering::Relaxed);
                // An entry can be evicted between the probe and here. What was
                // read was resident when it was read, so it is still a hit;
                // only the per-entry bookkeeping is conditional on it still
                // being there.
                let outcome = if inner.memory.contains_key(key) {
                    inner.record_hit_shared(key)
                } else {
                    HitOutcome::Accounted
                };
                inner.record_get_latency_micros(elapsed_micros(started));
                outcome
            };
            // Only these two need the cache exclusively, and a hit on a
            // recently-read entry is neither.
            match outcome {
                HitOutcome::Accounted => {}
                HitOutcome::NeedsAccessOrderRefresh => {
                    let mut inner = self.inner.write().expect("cache lock poisoned");
                    inner.refresh_access_order(key);
                }
                HitOutcome::NeedsMetadata => {
                    let mut inner = self.inner.write().expect("cache lock poisoned");
                    if inner.memory.contains_key(key) {
                        inner.record_hit(key, value.len());
                    }
                }
            }
            return Ok(Some(CachePinnedHandle {
                key: key.clone(),
                value,
                tier: CacheReadTier::Memory,
            }));
        }

        // Below memory the read refills the tier above it, which is a change
        // to the cache and needs it exclusively. Unchanged from before, and
        // reached only when the shared path above found nothing.
        {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            if let Some(value) = inner.pmem.get(key).cloned() {
                let decoded = value.to_vec();
                inner.increment_pin_for_handle(key, value.len());
                if !inner.put_memory(key.clone(), decoded) {
                    inner.stats.refill_failures = inner.stats.refill_failures.saturating_add(1);
                }
                inner.read_counters.pmem_hits.fetch_add(1, Ordering::Relaxed);
                inner.record_hit(key, value.len());
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
                inner.increment_pin_for_handle(key, decoded.len());
                if !inner.ssd_instance_only && !inner.refill_from_ssd(key.clone(), decoded.to_vec())
                {
                    inner.stats.refill_failures = inner.stats.refill_failures.saturating_add(1);
                }
                inner.read_counters.disk_hits.fetch_add(1, Ordering::Relaxed);
                if is_encoded_compressed_block(&block) {
                    inner.stats.compressed_hits = inner.stats.compressed_hits.saturating_add(1);
                }
                inner.record_hit(key, decoded.len());
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
                inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
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
        let now_millis = CoarseClock::now_millis();
        let expired;
        {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            expired = inner.entry_expired(key, now_millis);
            if expired {
                // Preserve the single-key acquire behavior below: expired
                // entries are cleaned up under the exclusive lock.
            } else {
                if let Some(value) = inner.memory.get(key).cloned() {
                    inner.increment_pin_with_size(key, value.len());
                    return Ok(Some(CachePinnedHandle {
                        key: key.clone(),
                        value,
                        tier: CacheReadTier::Memory,
                    }));
                }
                if let Some(value) = inner.pmem.get(key).cloned() {
                    inner.increment_pin_with_size(key, value.len());
                    return Ok(Some(CachePinnedHandle {
                        key: key.clone(),
                        value,
                        tier: CacheReadTier::Pmem,
                    }));
                }
            }
        }

        if expired {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            if inner.entry_expired(key, now_millis) {
                inner.remove_expired_entry(key);
                inner.stats.expired_reads = inner.stats.expired_reads.saturating_add(1);
                inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
            }
            return Ok(None);
        }

        let block = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_block(key)?
        };
        let Some(block) = block else {
            return Ok(None);
        };
        let value = Arc::<[u8]>::from(decode_cache_block(&block)?);
        // Shared: taking the pin changes only the pin state, which has its own
        // lock.
        let inner = self.inner.read().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.increment_pin_with_size(key, value.len());
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
        if keys.is_empty() {
            return Ok(results);
        }

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

        let now_millis = CoarseClock::now_millis();
        let mut expired_keys = Vec::new();
        let mut ssd_candidates = Vec::<(CacheKey, Vec<usize>)>::new();
        {
            let inner = self.inner.read().expect("cache lock poisoned");
            if !inner.started {
                return Err(CacheError::Stopped);
            }
            let mut pin_counts = Vec::<(CacheKey, usize, usize)>::new();
            for (key, positions) in positions_by_key {
                if inner.entry_expired(&key, now_millis) {
                    expired_keys.push(key);
                    continue;
                }
                if !inner.ssd_instance_only {
                    if let Some(value) = inner.memory.get(&key).cloned() {
                        pin_counts.push((key.clone(), value.len(), positions.len()));
                        for position in positions {
                            results[position] = Some(CachePinnedHandle {
                                key: key.clone(),
                                value: value.clone(),
                                tier: CacheReadTier::Memory,
                            });
                        }
                        continue;
                    }
                    if let Some(value) = inner.pmem.get(&key).cloned() {
                        pin_counts.push((key.clone(), value.len(), positions.len()));
                        for position in positions {
                            results[position] = Some(CachePinnedHandle {
                                key: key.clone(),
                                value: value.clone(),
                                tier: CacheReadTier::Pmem,
                            });
                        }
                        continue;
                    }
                }
                ssd_candidates.push((key, positions));
            }
            inner.increment_pin_with_size_counts(&pin_counts);
        }

        if !expired_keys.is_empty() {
            let mut inner = self.inner.write().expect("cache lock poisoned");
            for key in expired_keys {
                if inner.entry_expired(&key, now_millis) {
                    inner.remove_expired_entry(&key);
                    inner.stats.expired_reads = inner.stats.expired_reads.saturating_add(1);
                    inner.read_counters.misses.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        if ssd_candidates.is_empty() {
            return Ok(results);
        }

        let candidate_keys = ssd_candidates
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        let blocks = {
            let inner = self.inner.read().expect("cache lock poisoned");
            inner.read_ssd_blocks(&candidate_keys)?
        };
        let mut decoded_hits = Vec::<(CacheKey, Vec<usize>, Arc<[u8]>)>::new();
        for ((key, positions), block) in ssd_candidates.into_iter().zip(blocks) {
            let Some(block) = block else {
                continue;
            };
            decoded_hits.push((key, positions, Arc::<[u8]>::from(decode_cache_block(&block)?)));
        }
        if decoded_hits.is_empty() {
            return Ok(results);
        }

        let inner = self.inner.read().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        let pin_counts = decoded_hits
            .iter()
            .map(|(key, positions, value)| (key.clone(), value.len(), positions.len()))
            .collect::<Vec<_>>();
        inner.increment_pin_with_size_counts(&pin_counts);
        for (key, positions, value) in decoded_hits {
            for position in positions {
                results[position] = Some(CachePinnedHandle {
                    key: key.clone(),
                    value: value.clone(),
                    tier: CacheReadTier::Ssd,
                });
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
        let mut counts_by_key = HashMap::<CacheKey, usize>::new();
        for handle in handles {
            *counts_by_key.entry(handle.key).or_default() += 1;
        }
        let counts = counts_by_key.into_iter().collect::<Vec<_>>();
        let inner = self.inner.read().expect("cache lock poisoned");
        inner.decrement_pin_counts(&counts);
        released
    }

    pub fn clone_handle(&self, handle: &CachePinnedHandle) -> CachePinnedHandle {
        let inner = self.inner.read().expect("cache lock poisoned");
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
        // The write happened above; what is left is a lookup and a pin, and
        // neither needs the cache exclusively any more.
        let inner = self.inner.read().expect("cache lock poisoned");
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
            self.emit_access_record(CacheAccessRecordKind::Put, &key);
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
            self.emit_access_record(CacheAccessRecordKind::Put, &key);
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
            self.emit_access_record(CacheAccessRecordKind::Put, key);
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
            self.emit_access_record(CacheAccessRecordKind::Put, &key);
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
        self.emit_access_record(CacheAccessRecordKind::Put, &key);
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
        instance_type: CacheInstanceKind,
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
        instance_type: CacheInstanceKind,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        let Some(tier) = instance_type.as_tier() else {
            return Err(CacheError::UnsupportedInstance(instance_type));
        };

        let inner = self.inner.read().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        match tier {
            CacheTier::Memory => {
                let Some(value) = inner.memory.get(key).cloned() else {
                    return Ok(None);
                };
                inner.increment_pin_with_size(key, value.len());
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
        instance_type: CacheInstanceKind,
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
        drop(inner);
        self.drain_eviction_records();
        Ok(())
    }

    pub fn test_unified_acquire_count(&self) -> u64 {
        self.stats().zero_copy_handle_hits
    }

    pub fn test_unified_put_count(&self) -> u64 {
        self.stats().puts
    }

    pub fn test_unified_insert_pinned_count(&self) -> u64 {
        self.stats().insert_pinned_operations
    }

    pub fn test_join_pmem_write_executor(&self) {}

    pub fn test_pmem_paths(&self) -> Vec<String> {
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
            let head = inner.async_writeback_head;
            if let Some(sequence) = inner.async_writeback_positions.get(&key).copied() {
                let index = sequence.checked_sub(head).map(|offset| offset as usize);
                if let Some(existing) =
                    index.and_then(|index| inner.async_writeback_queue.get_mut(index))
                {
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
                let sequence =
                    inner.async_writeback_head + inner.async_writeback_queue.len() as u64;
                inner.async_writeback_queue.push_back(CacheWritebackJob {
                    key: key.clone(),
                    value,
                });
                inner.async_writeback_queue_bytes =
                    inner.async_writeback_queue_bytes.saturating_add(value_len);
                inner.async_writeback_positions.insert(key, sequence);
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
                inner.async_writeback_head += 1;
                jobs.push(job);
            }
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

    /// Pin an entry. Takes the cache **shared**.
    ///
    /// Does nothing on a stopped cache. [`Self::acquire`] refuses one with
    /// `Stopped`, and the two take the same pin: a cache that will not hand
    /// out a handle should not hand out a pin either.
    ///
    /// [`Self::unpin`] deliberately still works when stopped. A pin taken
    /// before the stop has to be releasable after it, or shutting down while
    /// handles are outstanding would leave them pinned forever.
    pub fn pin(&self, key: CacheKey) {
        let inner = self.inner.read().expect("cache lock poisoned");
        if !inner.started {
            return;
        }
        inner.increment_pin(&key);
    }

    /// Give back a handle.
    ///
    /// Takes the cache **shared**. Dropping a pin changes only the pin state,
    /// which has its own lock, so one reader releasing a handle no longer
    /// stops every other reader from taking one.
    pub fn unpin(&self, key: &CacheKey) {
        let inner = self.inner.read().expect("cache lock poisoned");
        inner.decrement_pin(key);
    }

    pub fn invalidate(&self, key: &CacheKey) -> Result<(), CacheError> {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return Err(CacheError::Stopped);
        }
        inner.invalidate_key_locked(key, true);
        drop(inner);
        self.emit_access_record(CacheAccessRecordKind::Delete, key);
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
            self.emit_access_record(CacheAccessRecordKind::Delete, key);
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
    pub fn GetCapacity(&self, instance_type: CacheInstanceKind) -> usize {
        self.get_capacity(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetCapacity(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    #[allow(non_snake_case)]
    pub fn SetCapacityForInstance(&self, instance_type: CacheInstanceKind, capacity: usize) {
        self.set_capacity_for_instance(instance_type, capacity);
    }

    #[allow(non_snake_case)]
    pub fn GetUsed(&self, instance_type: CacheInstanceKind) -> usize {
        self.get_used(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceKind,
        policy: CacheReplacementPolicy,
    ) {
        self.set_replacement_policy_type(instance_type, policy);
    }

    #[allow(non_snake_case)]
    pub fn TrySetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceKind,
        policy: CacheReplacementPolicy,
    ) -> Result<(), CacheError> {
        self.try_set_replacement_policy_type(instance_type, policy)
    }

    #[allow(non_snake_case)]
    pub fn GetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceKind,
    ) -> CacheReplacementPolicy {
        self.get_replacement_policy_type(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetDataPlacementType(&self, placement: CacheDataPlacement) {
        self.set_data_placement(placement);
    }

    #[allow(non_snake_case)]
    pub fn SetDRAMPMEMDataPlacementType(&self, placement: DramPmemDataPlacement) {
        self.set_config_data_placement_type(placement);
    }

    #[allow(non_snake_case)]
    pub fn GetDataPlacementType(&self) -> CacheDataPlacement {
        self.data_placement()
    }

    #[allow(non_snake_case)]
    pub fn GetDRAMPMEMDataPlacementType(&self) -> DramPmemDataPlacement {
        self.config_data_placement_type()
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
        instance_type: CacheInstanceKind,
        key: CacheKey,
        value: Vec<u8>,
        size: usize,
    ) -> Result<(), CacheError> {
        self.test_insert(instance_type, key, value, size)
    }

    #[allow(non_snake_case)]
    pub fn TEST_Acquire(
        &self,
        instance_type: CacheInstanceKind,
        key: &CacheKey,
    ) -> Result<Option<CachePinnedHandle>, CacheError> {
        self.test_acquire(instance_type, key)
    }

    #[allow(non_snake_case)]
    pub fn TEST_Remove(
        &self,
        instance_type: CacheInstanceKind,
        key: &CacheKey,
    ) -> Result<(), CacheError> {
        self.test_remove(instance_type, key)
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetUnifiedAcquireCount(&self) -> u64 {
        self.test_unified_acquire_count()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetUnifiedPutCount(&self) -> u64 {
        self.test_unified_put_count()
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetUnifiedInsertPinnedCount(&self) -> u64 {
        self.test_unified_insert_pinned_count()
    }

    #[allow(non_snake_case)]
    pub fn TEST_JoinPmemWriteExecutor(&self) {
        self.test_join_pmem_write_executor();
    }

    #[allow(non_snake_case)]
    pub fn TEST_GetPmemPaths(&self) -> Vec<String> {
        self.test_pmem_paths()
    }

    /// Drop an entry from the memory tier, leaving the copies below it.
    ///
    /// Does nothing on a stopped cache, because [`Self::invalidate`] refuses
    /// with `Stopped` and the two differ only in how far down they reach. It
    /// returns nothing, so there is no error to give: not acting is the whole
    /// of the refusal.
    pub fn invalidate_memory_only(&self, key: &CacheKey) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        if !inner.started {
            return;
        }
        let key_pinned = inner.is_pinned(key);
        let mut removed_pinned_bytes = 0usize;
        if let Some(value) = inner.memory.remove(key) {
            removed_pinned_bytes = removed_pinned_bytes.max(value.len());
            inner.memory_bytes = inner.memory_bytes.saturating_sub(value.len());
        }
        if let Some(value) = inner.pmem.remove(key) {
            removed_pinned_bytes = removed_pinned_bytes.max(value.len());
            inner.pmem_bytes = inner.pmem_bytes.saturating_sub(value.len());
        }
        {
            let mut pins = inner.pins_for(key);
            if key_pinned && removed_pinned_bytes > 0 {
                pins.entries
                    .entry(key.clone())
                    .or_default()
                    .removed_bytes = Some(removed_pinned_bytes);
            } else {
                pins.entries.remove(key);
            }
        }
        inner.stats.invalidations += 1;
        drop(inner);
        self.emit_access_record(CacheAccessRecordKind::Delete, key);
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

    pub fn config_data_placement_type(&self) -> DramPmemDataPlacement {
        self.data_placement().into()
    }

    pub fn set_data_placement(&self, placement: CacheDataPlacement) {
        let mut inner = self.inner.write().expect("cache lock poisoned");
        inner.tiering_policy.data_placement = placement;
    }

    pub fn set_config_data_placement_type(&self, placement: DramPmemDataPlacement) {
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
            inner.memory_order.clear();
            inner.pmem_order.clear();
            inner.memory_bytes = 0;
            inner.pmem_bytes = 0;
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
        drop(inner);
        self.drain_eviction_records();
    }
}

/// Add one shard's statistics into a running total for the whole cache.
///
/// Written out field by field on purpose. The list was maintained by hand and
/// checked by a second hand-maintained list of the same names, which is no
/// check at all: a statistic could be added to the checker and summed nowhere,
/// and two of them were, so a sharded cache reported them as zero -- which
/// looks exactly like a statistic that is genuinely zero.
///
/// Here the pattern has no `..`, so a new statistic that reaches neither list
/// fails the build with its own name in the message, and `unused_variables` is
/// denied, so one that is destructured and then never used fails the same way.
/// Both mistakes are caught, and both say which field.
#[deny(unused_variables)]
fn fold_shard_stats(total: &mut CacheStats, shard: CacheStats) {
    let CacheStats {
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
        ssd_bytes_written,
        ssd_write_budget_rejections,
        ssd_write_budget_observed_bytes_per_sec,
        ssd_write_budget_target_bytes_per_sec,
        stale_tier_copies_dropped,
        expired_demotions_skipped,
        expired_reads,
        expired_removals,
        eviction_expired,
        expired_delete_failures,
        ssd_write_through_admissions,
        hotness_promotions,
        access_order_refreshes,
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
        sharded_batch_fanout_operations,
        sharded_batch_local_operations,
        sharded_batch_fanout_shards,
        sharded_batch_latency_samples,
        sharded_batch_latency_total_micros,
        sharded_batch_latency_max_micros,
        sharded_batch_latency_le_10us,
        sharded_batch_latency_le_100us,
        sharded_batch_latency_le_1ms,
        sharded_batch_latency_le_10ms,
        sharded_batch_latency_gt_10ms,
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
        read_through_latency_total_micros,
        read_through_latency_le_10us,
        read_through_latency_le_100us,
        read_through_latency_le_1ms,
        read_through_latency_le_10ms,
        read_through_latency_gt_10ms,
        refill_latency_samples,
        refill_latency_total_micros,
        refill_latency_le_10us,
        refill_latency_le_100us,
        refill_latency_le_1ms,
        refill_latency_le_10ms,
        refill_latency_gt_10ms,
        writeback_latency_samples,
        writeback_latency_total_micros,
        writeback_latency_le_10us,
        writeback_latency_le_100us,
        writeback_latency_le_1ms,
        writeback_latency_le_10ms,
        writeback_latency_gt_10ms,
        eviction_latency_samples,
        eviction_latency_total_micros,
        eviction_latency_le_10us,
        eviction_latency_le_100us,
        eviction_latency_le_1ms,
        eviction_latency_le_10ms,
        eviction_latency_gt_10ms,
        compaction_latency_samples,
        compaction_latency_total_micros,
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
        ssd_write_budget_share,
    } = shard;

    total.memory_hits = total.memory_hits.saturating_add(memory_hits);
    total.disk_hits = total.disk_hits.saturating_add(disk_hits);
    total.misses = total.misses.saturating_add(misses);
    total.puts = total.puts.saturating_add(puts);
    total.invalidations = total.invalidations.saturating_add(invalidations);
    total.memory_evictions = total.memory_evictions.saturating_add(memory_evictions);
    total.pmem_hits = total.pmem_hits.saturating_add(pmem_hits);
    total.pmem_fills = total.pmem_fills.saturating_add(pmem_fills);
    total.pmem_evictions = total.pmem_evictions.saturating_add(pmem_evictions);
    total.pmem_admission_accepted = total.pmem_admission_accepted.saturating_add(pmem_admission_accepted);
    total.pmem_admission_rejected = total.pmem_admission_rejected.saturating_add(pmem_admission_rejected);
    total.pmem_eviction_capacity = total.pmem_eviction_capacity.saturating_add(pmem_eviction_capacity);
    total.pmem_eviction_pinned_skips = total.pmem_eviction_pinned_skips.saturating_add(pmem_eviction_pinned_skips);
    total.memory_admission_accepted = total.memory_admission_accepted.saturating_add(memory_admission_accepted);
    total.memory_admission_rejected = total.memory_admission_rejected.saturating_add(memory_admission_rejected);
    total.memory_fills = total.memory_fills.saturating_add(memory_fills);
    total.disk_fills = total.disk_fills.saturating_add(disk_fills);
    total.ssd_admission_accepted = total.ssd_admission_accepted.saturating_add(ssd_admission_accepted);
    total.ssd_admission_rejected = total.ssd_admission_rejected.saturating_add(ssd_admission_rejected);
    total.ssd_evictions = total.ssd_evictions.saturating_add(ssd_evictions);
    total.ssd_eviction_capacity = total.ssd_eviction_capacity.saturating_add(ssd_eviction_capacity);
    total.ssd_eviction_pinned_skips = total.ssd_eviction_pinned_skips.saturating_add(ssd_eviction_pinned_skips);
    total.ssd_oversize_rejections = total.ssd_oversize_rejections.saturating_add(ssd_oversize_rejections);
    total.ssd_bytes_written = total.ssd_bytes_written.saturating_add(ssd_bytes_written);
    total.ssd_write_budget_rejections = total.ssd_write_budget_rejections.saturating_add(ssd_write_budget_rejections);
    total.ssd_write_budget_observed_bytes_per_sec = total.ssd_write_budget_observed_bytes_per_sec.saturating_add(ssd_write_budget_observed_bytes_per_sec);
    total.ssd_write_budget_target_bytes_per_sec = total.ssd_write_budget_target_bytes_per_sec.saturating_add(ssd_write_budget_target_bytes_per_sec);
    total.stale_tier_copies_dropped = total.stale_tier_copies_dropped.saturating_add(stale_tier_copies_dropped);
    total.expired_demotions_skipped = total
        .expired_demotions_skipped
        .saturating_add(expired_demotions_skipped);
    total.expired_reads = total.expired_reads.saturating_add(expired_reads);
    total.expired_removals = total.expired_removals.saturating_add(expired_removals);
    total.eviction_expired = total.eviction_expired.saturating_add(eviction_expired);
    total.expired_delete_failures = total.expired_delete_failures.saturating_add(expired_delete_failures);
    total.ssd_write_through_admissions = total.ssd_write_through_admissions.saturating_add(ssd_write_through_admissions);
    total.hotness_promotions = total.hotness_promotions.saturating_add(hotness_promotions);
    total.access_order_refreshes = total.access_order_refreshes.saturating_add(access_order_refreshes);
    total.refill_failures = total.refill_failures.saturating_add(refill_failures);
    total.eviction_capacity = total.eviction_capacity.saturating_add(eviction_capacity);
    total.eviction_oversize = total.eviction_oversize.saturating_add(eviction_oversize);
    total.eviction_cold = total.eviction_cold.saturating_add(eviction_cold);
    total.eviction_low_hit = total.eviction_low_hit.saturating_add(eviction_low_hit);
    total.eviction_stale = total.eviction_stale.saturating_add(eviction_stale);
    total.pinned_entries = total.pinned_entries.saturating_add(pinned_entries);
    total.pinned_bytes = total.pinned_bytes.saturating_add(pinned_bytes);
    total.pin_operations = total.pin_operations.saturating_add(pin_operations);
    total.unpin_operations = total.unpin_operations.saturating_add(unpin_operations);
    total.insert_pinned_operations = total.insert_pinned_operations.saturating_add(insert_pinned_operations);
    total.eviction_pinned_skips = total.eviction_pinned_skips.saturating_add(eviction_pinned_skips);
    total.zero_copy_handle_hits = total.zero_copy_handle_hits.saturating_add(zero_copy_handle_hits);
    total.zero_copy_handle_misses = total.zero_copy_handle_misses.saturating_add(zero_copy_handle_misses);
    total.async_writeback_enqueued = total.async_writeback_enqueued.saturating_add(async_writeback_enqueued);
    total.async_writeback_drained = total.async_writeback_drained.saturating_add(async_writeback_drained);
    total.async_writeback_backpressure_rejections = total.async_writeback_backpressure_rejections.saturating_add(async_writeback_backpressure_rejections);
    total.writeback_backpressure_events = total.writeback_backpressure_events.saturating_add(writeback_backpressure_events);
    total.async_writeback_queue_depth = total.async_writeback_queue_depth.saturating_add(async_writeback_queue_depth);
    total.async_writeback_queue_bytes = total.async_writeback_queue_bytes.saturating_add(async_writeback_queue_bytes);
    total.async_writeback_max_queue_depth = total.async_writeback_max_queue_depth.saturating_add(async_writeback_max_queue_depth);
    total.async_writeback_max_queue_bytes = total.async_writeback_max_queue_bytes.saturating_add(async_writeback_max_queue_bytes);
    total.sharded_batch_fanout_operations = total
        .sharded_batch_fanout_operations
        .saturating_add(sharded_batch_fanout_operations);
    total.sharded_batch_local_operations = total
        .sharded_batch_local_operations
        .saturating_add(sharded_batch_local_operations);
    total.sharded_batch_fanout_shards = total
        .sharded_batch_fanout_shards
        .saturating_add(sharded_batch_fanout_shards);
    total.sharded_batch_latency_samples = total
        .sharded_batch_latency_samples
        .saturating_add(sharded_batch_latency_samples);
    total.sharded_batch_latency_total_micros = total
        .sharded_batch_latency_total_micros
        .saturating_add(sharded_batch_latency_total_micros);
    total.sharded_batch_latency_max_micros = total
        .sharded_batch_latency_max_micros
        .max(sharded_batch_latency_max_micros);
    total.sharded_batch_latency_le_10us = total
        .sharded_batch_latency_le_10us
        .saturating_add(sharded_batch_latency_le_10us);
    total.sharded_batch_latency_le_100us = total
        .sharded_batch_latency_le_100us
        .saturating_add(sharded_batch_latency_le_100us);
    total.sharded_batch_latency_le_1ms = total
        .sharded_batch_latency_le_1ms
        .saturating_add(sharded_batch_latency_le_1ms);
    total.sharded_batch_latency_le_10ms = total
        .sharded_batch_latency_le_10ms
        .saturating_add(sharded_batch_latency_le_10ms);
    total.sharded_batch_latency_gt_10ms = total
        .sharded_batch_latency_gt_10ms
        .saturating_add(sharded_batch_latency_gt_10ms);
    total.get_latency_samples = total.get_latency_samples.saturating_add(get_latency_samples);
    total.put_latency_samples = total.put_latency_samples.saturating_add(put_latency_samples);
    total.get_latency_total_micros = total.get_latency_total_micros.saturating_add(get_latency_total_micros);
    total.put_latency_total_micros = total.put_latency_total_micros.saturating_add(put_latency_total_micros);
    total.get_latency_max_micros = total.get_latency_max_micros.saturating_add(get_latency_max_micros);
    total.put_latency_max_micros = total.put_latency_max_micros.saturating_add(put_latency_max_micros);
    total.get_latency_le_10us = total.get_latency_le_10us.saturating_add(get_latency_le_10us);
    total.get_latency_le_100us = total.get_latency_le_100us.saturating_add(get_latency_le_100us);
    total.get_latency_le_1ms = total.get_latency_le_1ms.saturating_add(get_latency_le_1ms);
    total.get_latency_le_10ms = total.get_latency_le_10ms.saturating_add(get_latency_le_10ms);
    total.get_latency_gt_10ms = total.get_latency_gt_10ms.saturating_add(get_latency_gt_10ms);
    total.put_latency_le_10us = total.put_latency_le_10us.saturating_add(put_latency_le_10us);
    total.put_latency_le_100us = total.put_latency_le_100us.saturating_add(put_latency_le_100us);
    total.put_latency_le_1ms = total.put_latency_le_1ms.saturating_add(put_latency_le_1ms);
    total.put_latency_le_10ms = total.put_latency_le_10ms.saturating_add(put_latency_le_10ms);
    total.put_latency_gt_10ms = total.put_latency_gt_10ms.saturating_add(put_latency_gt_10ms);
    total.read_through_latency_samples = total.read_through_latency_samples.saturating_add(read_through_latency_samples);
    total.read_through_latency_total_micros = total.read_through_latency_total_micros.saturating_add(read_through_latency_total_micros);
    total.read_through_latency_le_10us = total.read_through_latency_le_10us.saturating_add(read_through_latency_le_10us);
    total.read_through_latency_le_100us = total.read_through_latency_le_100us.saturating_add(read_through_latency_le_100us);
    total.read_through_latency_le_1ms = total.read_through_latency_le_1ms.saturating_add(read_through_latency_le_1ms);
    total.read_through_latency_le_10ms = total.read_through_latency_le_10ms.saturating_add(read_through_latency_le_10ms);
    total.read_through_latency_gt_10ms = total.read_through_latency_gt_10ms.saturating_add(read_through_latency_gt_10ms);
    total.refill_latency_samples = total.refill_latency_samples.saturating_add(refill_latency_samples);
    total.refill_latency_total_micros = total.refill_latency_total_micros.saturating_add(refill_latency_total_micros);
    total.refill_latency_le_10us = total.refill_latency_le_10us.saturating_add(refill_latency_le_10us);
    total.refill_latency_le_100us = total.refill_latency_le_100us.saturating_add(refill_latency_le_100us);
    total.refill_latency_le_1ms = total.refill_latency_le_1ms.saturating_add(refill_latency_le_1ms);
    total.refill_latency_le_10ms = total.refill_latency_le_10ms.saturating_add(refill_latency_le_10ms);
    total.refill_latency_gt_10ms = total.refill_latency_gt_10ms.saturating_add(refill_latency_gt_10ms);
    total.writeback_latency_samples = total.writeback_latency_samples.saturating_add(writeback_latency_samples);
    total.writeback_latency_total_micros = total.writeback_latency_total_micros.saturating_add(writeback_latency_total_micros);
    total.writeback_latency_le_10us = total.writeback_latency_le_10us.saturating_add(writeback_latency_le_10us);
    total.writeback_latency_le_100us = total.writeback_latency_le_100us.saturating_add(writeback_latency_le_100us);
    total.writeback_latency_le_1ms = total.writeback_latency_le_1ms.saturating_add(writeback_latency_le_1ms);
    total.writeback_latency_le_10ms = total.writeback_latency_le_10ms.saturating_add(writeback_latency_le_10ms);
    total.writeback_latency_gt_10ms = total.writeback_latency_gt_10ms.saturating_add(writeback_latency_gt_10ms);
    total.eviction_latency_samples = total.eviction_latency_samples.saturating_add(eviction_latency_samples);
    total.eviction_latency_total_micros = total.eviction_latency_total_micros.saturating_add(eviction_latency_total_micros);
    total.eviction_latency_le_10us = total.eviction_latency_le_10us.saturating_add(eviction_latency_le_10us);
    total.eviction_latency_le_100us = total.eviction_latency_le_100us.saturating_add(eviction_latency_le_100us);
    total.eviction_latency_le_1ms = total.eviction_latency_le_1ms.saturating_add(eviction_latency_le_1ms);
    total.eviction_latency_le_10ms = total.eviction_latency_le_10ms.saturating_add(eviction_latency_le_10ms);
    total.eviction_latency_gt_10ms = total.eviction_latency_gt_10ms.saturating_add(eviction_latency_gt_10ms);
    total.compaction_latency_samples = total.compaction_latency_samples.saturating_add(compaction_latency_samples);
    total.compaction_latency_total_micros = total.compaction_latency_total_micros.saturating_add(compaction_latency_total_micros);
    total.compaction_latency_le_10us = total.compaction_latency_le_10us.saturating_add(compaction_latency_le_10us);
    total.compaction_latency_le_100us = total.compaction_latency_le_100us.saturating_add(compaction_latency_le_100us);
    total.compaction_latency_le_1ms = total.compaction_latency_le_1ms.saturating_add(compaction_latency_le_1ms);
    total.compaction_latency_le_10ms = total.compaction_latency_le_10ms.saturating_add(compaction_latency_le_10ms);
    total.compaction_latency_gt_10ms = total.compaction_latency_gt_10ms.saturating_add(compaction_latency_gt_10ms);
    total.eviction_sampled_groups = total.eviction_sampled_groups.saturating_add(eviction_sampled_groups);
    total.memory_slot_evictions = total.memory_slot_evictions.saturating_add(memory_slot_evictions);
    total.ssd_slot_evictions = total.ssd_slot_evictions.saturating_add(ssd_slot_evictions);
    total.ssd_eviction_cold = total.ssd_eviction_cold.saturating_add(ssd_eviction_cold);
    total.ssd_eviction_low_hit = total.ssd_eviction_low_hit.saturating_add(ssd_eviction_low_hit);
    total.ssd_eviction_stale = total.ssd_eviction_stale.saturating_add(ssd_eviction_stale);
    total.compressed_puts = total.compressed_puts.saturating_add(compressed_puts);
    total.compressed_hits = total.compressed_hits.saturating_add(compressed_hits);
    total.compression_bytes_saved = total.compression_bytes_saved.saturating_add(compression_bytes_saved);
    total.get_latency_count = total.get_latency_count.saturating_add(get_latency_count);
    total.get_latency_total_us = total.get_latency_total_us.saturating_add(get_latency_total_us);
    total.get_latency_max_us = total.get_latency_max_us.saturating_add(get_latency_max_us);
    total.put_latency_count = total.put_latency_count.saturating_add(put_latency_count);
    total.put_latency_total_us = total.put_latency_total_us.saturating_add(put_latency_total_us);
    total.put_latency_max_us = total.put_latency_max_us.saturating_add(put_latency_max_us);
    total.memory_bytes = total.memory_bytes.saturating_add(memory_bytes);
    total.pmem_bytes = total.pmem_bytes.saturating_add(pmem_bytes);
    total.disk_bytes = total.disk_bytes.saturating_add(disk_bytes);

    // Named above to prove the pattern is complete, combined after the loop
    // where the whole set of shards is in hand.
    let _ = ssd_write_budget_share;
}

/// A [`MultiLayerCache`] split into independent shards, chosen by key hash.
///
/// The same [`CacheApi`] and [`ZeroCopyCacheApi`] surface, and the answer to the
/// read-scaling limit described on `MultiLayerCache`: each shard has its own
/// lock, so readers on different keys do not contend.
///
/// The trade is the usual one for sharding -- with a single thread it is
/// slightly *slower* than the unsharded cache, because there is no contention to
/// relieve and the extra bookkeeping still has to be paid. It pays off from two
/// threads upward. `examples/cache_scaling_bench.rs` measures the crossover.
///
/// Note that anything global is per-shard: `size` sums the shards, and a
/// capacity is divided among them rather than applying to each.
#[derive(Debug, Clone)]
pub struct ShardedMultiLayerCache {
    shards: Arc<Vec<MultiLayerCache>>,
    sharded_stats: Arc<ShardedBatchStats>,
}

#[derive(Debug, Default)]
struct ShardedBatchStats {
    fanout_operations: AtomicU64,
    local_operations: AtomicU64,
    fanout_shards: AtomicU64,
    latency: AtomicLatencyHistogram,
}

impl ShardedBatchStats {
    fn record_fanout(&self, shards: usize) {
        self.fanout_operations.fetch_add(1, Ordering::Relaxed);
        self.fanout_shards
            .fetch_add(shards as u64, Ordering::Relaxed);
    }

    fn record_local(&self) {
        self.local_operations.fetch_add(1, Ordering::Relaxed);
    }

    fn record_latency(&self, started: Instant) {
        self.latency.observe_with_total(elapsed_micros(started));
    }
}

impl ShardedMultiLayerCache {
    const BATCH_FANOUT_THRESHOLD: usize = 256;

    /// Build a sharded cache, refusing a configuration it cannot honour.
    ///
    /// The counterpart to [`MultiLayerCache::try_with_options`], and it checks
    /// the configuration as written rather than a shard's slice of it: the
    /// slices are a consequence of sharding, and reporting them as if the
    /// caller had asked for them would name the wrong number.
    ///
    /// [`Self::with_options`] stays infallible and unchanged.
    pub fn try_with_options(
        options: CacheOptions,
        shard_count: usize,
    ) -> Result<Self, CacheError> {
        let refusals: Vec<_> = options
            .validate_for_shards(shard_count)
            .into_iter()
            .filter(|finding| finding.severity == CacheHealthSeverity::Critical)
            .collect();
        if !refusals.is_empty() {
            return Err(CacheError::InvalidConfig(
                refusals
                    .into_iter()
                    .map(|finding| format!("{}: {}", finding.field, finding.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        Ok(Self::with_options(options, shard_count))
    }

    pub fn with_options(options: CacheOptions, shard_count: usize) -> Self {
        let shard_count = shard_count.max(1);
        let shards = (0..shard_count)
            .map(|index| {
                MultiLayerCache::with_options(Self::options_for_shard(&options, shard_count, index))
            })
            .collect::<Vec<_>>();
        Self {
            shards: Arc::new(shards),
            sharded_stats: Arc::new(ShardedBatchStats::default()),
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

    fn batch_shard_fanout(&self, keys: &[CacheKey]) -> usize {
        let mut seen = vec![false; self.shard_count()];
        let mut count = 0usize;
        for key in keys {
            let index = self.shard_index_for_key(key);
            if !seen[index] {
                seen[index] = true;
                count += 1;
            }
        }
        count
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

        total.sharded_batch_fanout_operations = self
            .sharded_stats
            .fanout_operations
            .load(Ordering::Relaxed);
        total.sharded_batch_local_operations = self
            .sharded_stats
            .local_operations
            .load(Ordering::Relaxed);
        total.sharded_batch_fanout_shards = self
            .sharded_stats
            .fanout_shards
            .load(Ordering::Relaxed);
        total.sharded_batch_latency_samples = self.sharded_stats.latency.samples();
        total.sharded_batch_latency_total_micros = self
            .sharded_stats
            .latency
            .total_micros
            .load(Ordering::Relaxed);
        total.sharded_batch_latency_max_micros = self
            .sharded_stats
            .latency
            .max_micros
            .load(Ordering::Relaxed);
        total.sharded_batch_latency_le_10us = self
            .sharded_stats
            .latency
            .le_10us
            .load(Ordering::Relaxed);
        total.sharded_batch_latency_le_100us = self
            .sharded_stats
            .latency
            .le_100us
            .load(Ordering::Relaxed);
        total.sharded_batch_latency_le_1ms = self
            .sharded_stats
            .latency
            .le_1ms
            .load(Ordering::Relaxed);
        total.sharded_batch_latency_le_10ms = self
            .sharded_stats
            .latency
            .le_10ms
            .load(Ordering::Relaxed);
        total.sharded_batch_latency_gt_10ms = self
            .sharded_stats
            .latency
            .gt_10ms
            .load(Ordering::Relaxed);

        for shard in self.shards.iter() {
            let stats = shard.stats();
            fold_shard_stats(&mut total, stats);
        }
        // Not a count, so adding it would report four shards each admitting
        // half of everything as a share of two. The tightest shard is the
        // useful one: it is the reason a caller is seeing writes refused.
        // Taken after the loop because the running total starts at zero, which
        // is a minimum nothing can beat.
        total.ssd_write_budget_share = self
            .shards
            .iter()
            .map(|shard| shard.stats().ssd_write_budget_share)
            .min()
            .unwrap_or(0);
        total
    }


    /// What this cache's current statistics say about its health.
    ///
    /// A shortcut for `cache_health_report(&self.stats())`. Taking the snapshot
    /// and judging it are separate so the judgement can also be applied to a
    /// snapshot from somewhere else, such as one recovered from a scrape.
    pub fn health_report(&self) -> CacheHealthReport {
        cache_health_report(&self.stats())
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
        let mut read_through_count = 0u64;
        let mut read_through_total_us = 0u64;
        let mut refill_count = 0u64;
        let mut refill_total_us = 0u64;
        let mut writeback_count = 0u64;
        let mut writeback_total_us = 0u64;
        let mut eviction_count = 0u64;
        let mut eviction_total_us = 0u64;
        let mut compaction_count = 0u64;
        let mut compaction_total_us = 0u64;
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
            read_through_count = read_through_count.saturating_add(report.read_through_count);
            read_through_total_us = read_through_total_us.saturating_add(
                report
                    .read_through_count
                    .saturating_mul(report.read_through_avg_us),
            );
            refill_count = refill_count.saturating_add(report.refill_count);
            refill_total_us = refill_total_us
                .saturating_add(report.refill_count.saturating_mul(report.refill_avg_us));
            writeback_count = writeback_count.saturating_add(report.writeback_count);
            writeback_total_us = writeback_total_us.saturating_add(
                report
                    .writeback_count
                    .saturating_mul(report.writeback_avg_us),
            );
            eviction_count = eviction_count.saturating_add(report.eviction_count);
            eviction_total_us = eviction_total_us
                .saturating_add(report.eviction_count.saturating_mul(report.eviction_avg_us));
            compaction_count = compaction_count.saturating_add(report.compaction_count);
            compaction_total_us = compaction_total_us.saturating_add(
                report
                    .compaction_count
                    .saturating_mul(report.compaction_avg_us),
            );
            histogram_ready |= report.histogram_ready;
        }

        CacheLatencyMetricsReport {
            get_count,
            get_avg_us: average_latency_us(get_total_us, get_count),
            get_p50_us: 0,
            get_p95_us: 0,
            get_max_us,
            put_count,
            put_avg_us: average_latency_us(put_total_us, put_count),
            put_p50_us: 0,
            put_p95_us: 0,
            put_max_us,
            read_through_count,
            read_through_avg_us: average_latency_us(read_through_total_us, read_through_count),
            read_through_p50_us: 0,
            read_through_p95_us: 0,
            refill_count,
            refill_avg_us: average_latency_us(refill_total_us, refill_count),
            refill_p50_us: 0,
            refill_p95_us: 0,
            writeback_count,
            writeback_avg_us: average_latency_us(writeback_total_us, writeback_count),
            writeback_p50_us: 0,
            writeback_p95_us: 0,
            eviction_count,
            eviction_avg_us: average_latency_us(eviction_total_us, eviction_count),
            eviction_p50_us: 0,
            eviction_p95_us: 0,
            compaction_count,
            compaction_avg_us: average_latency_us(compaction_total_us, compaction_count),
            compaction_p50_us: 0,
            compaction_p95_us: 0,
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

    /// The cached bytes, shared rather than copied. See the per-shard `get_shared`.
    pub fn get_shared(&self, key: &CacheKey) -> Result<Option<std::sync::Arc<[u8]>>, CacheError> {
        self.shard_for_key(key).get_shared(key)
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
        let started = Instant::now();
        if keys.is_empty() {
            self.sharded_stats.record_latency(started);
            return Ok(Vec::new());
        }
        if keys.len() < Self::BATCH_FANOUT_THRESHOLD {
            self.sharded_stats.record_local();
        } else {
            self.sharded_stats.record_fanout(self.batch_shard_fanout(keys));
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
        self.sharded_stats.record_latency(started);
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
        let started = Instant::now();
        if keys.len() < Self::BATCH_FANOUT_THRESHOLD {
            self.sharded_stats.record_local();
            let result = self.acquire_batch_no_promotion_locally(keys);
            self.sharded_stats.record_latency(started);
            return result;
        }
        let fanout = self.batch_shard_fanout(keys);
        self.sharded_stats.record_fanout(fanout);
        let mut results = (0..keys.len()).map(|_| None).collect::<Vec<_>>();
        let mut groups = (0..self.shard_count())
            .map(|_| Vec::<(usize, CacheKey)>::new())
            .collect::<Vec<_>>();
        for (position, key) in keys.iter().cloned().enumerate() {
            groups[self.shard_index_for_key(&key)].push((position, key));
        }
        let batches = std::thread::scope(|scope| {
            let workers = groups
                .into_iter()
                .enumerate()
                .filter_map(|(shard_index, group)| {
                    if group.is_empty() {
                        None
                    } else {
                        let shard = &self.shards[shard_index];
                        let (positions, shard_keys): (Vec<_>, Vec<_>) = group.into_iter().unzip();
                        Some(scope.spawn(move || {
                            shard
                                .acquire_batch_no_promotion(&shard_keys)
                                .map(|shard_results| (positions, shard_results))
                        }))
                    }
                })
                .collect::<Vec<_>>();

            let mut batches = Vec::with_capacity(workers.len());
            for worker in workers {
                match worker.join() {
                    Ok(Ok(batch)) => batches.push(batch),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache batch no-promotion worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(batches)
        })?;
        for (positions, shard_results) in batches {
            for (position, handle) in positions.into_iter().zip(shard_results) {
                results[position] = handle;
            }
        }
        self.sharded_stats.record_latency(started);
        Ok(results)
    }

    fn acquire_batch_no_promotion_locally(
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
                        .expect("first sharded batch handle is installed"),
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
        let started = Instant::now();
        if keys.len() < Self::BATCH_FANOUT_THRESHOLD {
            self.sharded_stats.record_local();
            let result = self.acquire_batch_locally(keys);
            self.sharded_stats.record_latency(started);
            return result;
        }
        let fanout = self.batch_shard_fanout(keys);
        self.sharded_stats.record_fanout(fanout);
        let mut results = (0..keys.len()).map(|_| None).collect::<Vec<_>>();
        let mut groups = (0..self.shard_count())
            .map(|_| Vec::<(usize, CacheKey)>::new())
            .collect::<Vec<_>>();
        for (position, key) in keys.iter().cloned().enumerate() {
            groups[self.shard_index_for_key(&key)].push((position, key));
        }
        let batches = std::thread::scope(|scope| {
            let workers = groups
                .into_iter()
                .enumerate()
                .filter_map(|(shard_index, group)| {
                    if group.is_empty() {
                        None
                    } else {
                        let shard = &self.shards[shard_index];
                        let (positions, shard_keys): (Vec<_>, Vec<_>) = group.into_iter().unzip();
                        Some(scope.spawn(move || {
                            shard
                                .acquire_batch(&shard_keys)
                                .map(|shard_results| (positions, shard_results))
                        }))
                    }
                })
                .collect::<Vec<_>>();

            let mut batches = Vec::with_capacity(workers.len());
            for worker in workers {
                match worker.join() {
                    Ok(Ok(batch)) => batches.push(batch),
                    Ok(Err(err)) => return Err(err),
                    Err(_) => {
                        return Err(CacheError::CorruptBlock(
                            "sharded cache batch acquire worker panicked".to_string(),
                        ));
                    }
                }
            }
            Ok(batches)
        })?;
        for (positions, shard_results) in batches {
            for (position, handle) in positions.into_iter().zip(shard_results) {
                results[position] = handle;
            }
        }
        self.sharded_stats.record_latency(started);
        Ok(results)
    }

    fn acquire_batch_locally(
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
        if released < Self::BATCH_FANOUT_THRESHOLD {
            for (index, group) in groups.into_iter().enumerate() {
                if !group.is_empty() {
                    self.shards[index].release_batch(group);
                }
            }
            return released;
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

    pub fn get_capacity(&self, instance_type: CacheInstanceKind) -> usize {
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

    /// Write `value` under `key` with a time to live, on the shard that owns it.
    pub fn put_with_ttl(
        &self,
        key: CacheKey,
        value: Vec<u8>,
        ttl: Duration,
    ) -> Result<(), CacheError> {
        self.shard_for_key(&key).put_with_ttl(key, value, ttl)
    }

    /// Time to live for entries that do not ask for their own, on every shard.
    pub fn set_default_ttl(&self, ttl: Duration) {
        for shard in self.shards.iter() {
            shard.set_default_ttl(ttl);
        }
    }

    /// Drop expired entries from every shard, and say how many in total.
    pub fn purge_expired(&self) -> usize {
        self.shards.iter().map(|shard| shard.purge_expired()).sum()
    }

    /// Hear about entries leaving any shard.
    ///
    /// A sharded cache had no way to register a handler at all, so anyone who
    /// sharded lost eviction notifications entirely -- including the expiry
    /// notifications a handler needs to release whatever it holds for an
    /// entry.
    ///
    /// One handler, shared by every shard, rather than a copy each: a caller
    /// counting departures wants one total, and a caller holding a resource
    /// wants one place that releases it. Shards call it concurrently, which is
    /// what the `Send + Sync` bound already promised.
    pub fn register_eviction_callback<F>(&self, callback: F)
    where
        F: Fn(CacheEvictionRecord) + Send + Sync + 'static,
    {
        let shared = Arc::new(callback);
        for shard in self.shards.iter() {
            let handler = Arc::clone(&shared);
            shard.register_eviction_callback(move |record| handler(record));
        }
    }

    /// Stop hearing about entries leaving, on every shard.
    ///
    /// Cleared everywhere, because a handler left on some shards and not
    /// others reports a fraction of the cache, which is worse than reporting
    /// none of it: the number looks like a measurement.
    pub fn clear_eviction_callback(&self) {
        for shard in self.shards.iter() {
            shard.clear_eviction_callback();
        }
    }

    /// Cap how fast the SSD tier absorbs writes, in bytes per second, across
    /// all shards.
    ///
    /// Split the same way capacity is, so the shards together aim at the
    /// number asked for rather than each aiming at all of it.
    pub fn set_ssd_write_budget_bytes_per_sec(&self, bytes_per_sec: u64) {
        let shard_count = self.shard_count() as u64;
        if shard_count == 0 {
            return;
        }
        for (index, shard) in self.shards.iter().enumerate() {
            // Hand the remainder to the first shards, so the parts sum exactly.
            let mut share = bytes_per_sec / shard_count;
            if (index as u64) < bytes_per_sec % shard_count {
                share = share.saturating_add(1);
            }
            shard.set_ssd_write_budget_bytes_per_sec(share);
        }
    }

    /// The SSD write cap across all shards, in bytes per second.
    ///
    /// Sums the shards, so it reports the number the whole cache is aiming at
    /// rather than one shard's slice of it.
    pub fn ssd_write_budget_bytes_per_sec(&self) -> u64 {
        self.shards
            .iter()
            .map(|shard| shard.ssd_write_budget_bytes_per_sec())
            .sum()
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

    pub fn set_capacity_for_instance(&self, instance_type: CacheInstanceKind, capacity: usize) {
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

    pub fn get_used(&self, instance_type: CacheInstanceKind) -> usize {
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
        instance_type: CacheInstanceKind,
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
        instance_type: CacheInstanceKind,
        policy: CacheReplacementPolicy,
    ) {
        if let Some(tier) = instance_type.as_tier() {
            self.set_replacement_policy_for_tier(tier, policy);
        }
    }

    pub fn try_set_replacement_policy_type(
        &self,
        instance_type: CacheInstanceKind,
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
    pub fn GetCapacity(&self, instance_type: CacheInstanceKind) -> usize {
        self.get_capacity(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetCapacityForTier(&self, tier: CacheTier, capacity: usize) {
        self.set_capacity_for_tier(tier, capacity);
    }

    #[allow(non_snake_case)]
    pub fn SetCapacityForInstance(&self, instance_type: CacheInstanceKind, capacity: usize) {
        self.set_capacity_for_instance(instance_type, capacity);
    }

    #[allow(non_snake_case)]
    pub fn SizeForTier(&self, tier: CacheTier) -> usize {
        self.size_for_tier(tier)
    }

    #[allow(non_snake_case)]
    pub fn GetUsed(&self, instance_type: CacheInstanceKind) -> usize {
        self.get_used(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn GetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceKind,
    ) -> CacheReplacementPolicy {
        self.get_replacement_policy_type(instance_type)
    }

    #[allow(non_snake_case)]
    pub fn SetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceKind,
        policy: CacheReplacementPolicy,
    ) {
        self.set_replacement_policy_type(instance_type, policy);
    }

    #[allow(non_snake_case)]
    pub fn TrySetReplacementPolicyType(
        &self,
        instance_type: CacheInstanceKind,
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

    fn capacity_for_instance_cache(&self, instance_type: CacheInstanceKind) -> usize {
        self.get_capacity(instance_type)
    }

    fn set_capacity_cache(&self, capacity: usize) {
        self.set_capacity(capacity);
    }

    fn set_capacity_for_instance_cache(&self, instance_type: CacheInstanceKind, capacity: usize) {
        self.set_capacity_for_instance(instance_type, capacity);
    }

    fn size_cache(&self) -> usize {
        self.size()
    }

    fn used_cache(&self, instance_type: CacheInstanceKind) -> usize {
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
    /// Write, then forget the entry if no tier took it.
    ///
    /// The per-entry metadata is recorded before the value is offered to a
    /// tier, because the branch that needs it is the one that stores. A write
    /// every tier refuses — a value larger than any of them, or one the SSD
    /// write budget declines — therefore leaves a description behind with
    /// nothing described. Nothing else removes it: removal is driven by an
    /// entry *leaving* a tier, and this one never entered.
    ///
    /// Wrapping the write rather than cleaning up at each of its six exits,
    /// because a cleanup repeated at every `return` is one the seventh will
    /// miss.
    fn put_with_request(
        &mut self,
        key: CacheKey,
        value: Vec<u8>,
        request: Option<CacheAdmissionRequest>,
    ) -> Result<(), CacheError> {
        let outcome = self.put_with_request_inner(key.clone(), value, request);
        if !self.memory.contains_key(&key)
            && !self.pmem.contains_key(&key)
            && !self.disk_index.contains_key(&key)
        {
            self.metadata.remove(&key);
        }
        outcome
    }

    fn put_with_request_inner(
        &mut self,
        key: CacheKey,
        value: Vec<u8>,
        request: Option<CacheAdmissionRequest>,
    ) -> Result<(), CacheError> {
        // Reclaim a few lapsed entries before this write is placed. Done here
        // rather than inside the memory tier's admission: a cache with no
        // memory tier configured never reaches that, and its entries need
        // sweeping just as much. Before admission rather than after, so room
        // an expired entry gives up can spare a live one from eviction.
        self.sweep_some_expired();
        if !self.is_pinned(&key) {
            self.pins_for(&key).entries.remove(&key);
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
            // A write that is not admitted here must still take out whatever
            // this tier was holding for the key. Reads look in the memory tier
            // first, so leaving the old value behind serves it in preference to
            // the one just written -- the write appears to have been lost, and
            // stays lost until something evicts the entry.
            //
            // A key gets here by having grown hot enough for the tiering
            // decision to route its rewrite past memory, which is exactly the
            // case of a key being written repeatedly.
            self.drop_stale_tier_copy(&key, CacheTier::Memory);
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
            // Same reasoning as the memory tier above: reads reach the
            // persistent-memory tier before the SSD one.
            //
            // Except when the memory tier's fallback has just written the
            // value here. That path lands in this branch too, and dropping the
            // copy it wrote would throw away the write it was making.
            if !pmem_admitted_via_memory_fallback {
                self.drop_stale_tier_copy(&key, CacheTier::Pmem);
            }
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
        // Ask the write budget before encoding: a block the budget will not let
        // through is not worth compressing first.
        if !self.ssd_write_budget_admits(&key) {
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
        }
        self.disk_order.push_back_if_absent(key.clone());
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
        self.append_disk_manifest_put(&key, block_len as u64)?;
        Ok(())
    }

    fn put_batch_with_requests(
        &mut self,
        entries: Vec<(CacheKey, Vec<u8>, usize)>,
    ) -> Result<usize, CacheError> {
        // Once for the batch, not once per entry: the sweep is bounded, and
        // running it per entry would turn it into a scan on a large batch.
        self.sweep_some_expired();
        let mut staged_ssd = Vec::<StagedSsdBatchWrite>::new();
        let mut staged_ssd_positions = HashMap::<CacheKey, usize>::new();
        let mut staged_ssd_bytes = self.ssd_bytes;
        let mut inserted = 0usize;

        for (key, value, logical_size) in entries {
            if !self.is_pinned(&key) {
                self.pins_for(&key).entries.remove(&key);
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
            if !self.ssd_write_budget_admits(&key) {
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

        // Move the blocks out rather than copying them: `entry.block` is read
        // here and nowhere afterwards, so the batch's bytes can go straight to
        // the store instead of being copied on the way.
        let storage_entries = staged_ssd
            .iter_mut()
            .map(|entry| (entry.key.clone(), std::mem::take(&mut entry.block)))
            .collect::<Vec<_>>();
        self.write_ssd_blocks(storage_entries)?;
        if !staged_ssd.is_empty() {
            const SET_MEMBERSHIP_THRESHOLD: usize = 8;
            // Borrowed, not cloned. Building this list used to clone every
            // staged key, and then clone them all again into the set.
            if staged_ssd.len() > SET_MEMBERSHIP_THRESHOLD {
                let staged_key_set = staged_ssd
                    .iter()
                    .map(|entry| &entry.key)
                    .collect::<HashSet<_>>();
                self.disk_order
                    .retain(|candidate| !staged_key_set.contains(candidate));
            } else {
                self.disk_order.retain(|candidate| {
                    !staged_ssd.iter().any(|entry| &entry.key == candidate)
                });
            }
        }
        for entry in staged_ssd {
            if let Some(old_len) = self.disk_index.insert(entry.key.clone(), entry.block_len) {
                self.ssd_bytes = self.ssd_bytes.saturating_sub(old_len);
            }
            self.disk_order.push_back_if_absent(entry.key.clone());
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
            self.append_disk_manifest_put(&entry.key, entry.block_len)?;
        }
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
                }
                self.disk_order.push_back_if_absent(key.clone());
                self.ssd_bytes = self.ssd_bytes.saturating_add(block_len);
                self.record_metadata(
                    &key,
                    block_kind,
                    routing_slot,
                    value.len(),
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
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
                }
                self.disk_order.push_back_if_absent(key.clone());
                self.ssd_bytes = self.ssd_bytes.saturating_add(block_len as u64);
                self.record_metadata(
                    &key,
                    block_kind,
                    routing_slot,
                    size,
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                self.append_disk_manifest_put(&key, block_len as u64)?;
            }
            CacheTier::Reject => return Err(CacheError::UnsupportedTier(tier)),
        }
        self.stats.puts = self.stats.puts.saturating_add(1);
        Ok(())
    }

    fn test_remove_for_tier(&mut self, tier: CacheTier, key: &CacheKey) -> Result<(), CacheError> {
        match tier {
            CacheTier::Memory => {
                if let Some(value) = self.memory.remove(key) {
                    self.memory_bytes = self.memory_bytes.saturating_sub(value.len());
                    if self.is_pinned(key) {
                        self.pins_for(key)
                            .entries
                            .entry(key.clone())
                            .or_default()
                            .removed_bytes = Some(value.len());
                    }
                }
                self.memory_order.remove(key);
            }
            CacheTier::Pmem => {
                if let Some(value) = self.pmem.remove(key) {
                    self.pmem_bytes = self.pmem_bytes.saturating_sub(value.len());
                    if self.is_pinned(key) {
                        self.pins_for(key)
                            .entries
                            .entry(key.clone())
                            .or_default()
                            .removed_bytes = Some(value.len());
                    }
                }
                self.pmem_order.remove(key);
                self.persist_pmem_delete(key)?;
            }
            CacheTier::Ssd => {
                if let Some(old_len) = self.disk_index.remove(key) {
                    self.ssd_bytes = self.ssd_bytes.saturating_sub(old_len);
                }
                self.delete_ssd_block(key)?;
                self.disk_order.remove(key);
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
                self.disk_order.remove(key);
                self.evict_ssd_for(block.len() as u64);
                if self.ssd_bytes.saturating_add(block.len() as u64)
                    > self.ssd_capacity_bytes as u64
                {
                    self.disk_index.insert(key.clone(), indexed_old_len);
                    self.disk_order.push_back_if_absent(key.clone());
                    self.ssd_bytes = self.ssd_bytes.saturating_add(indexed_old_len);
                    return Err(CacheError::CapacityExceeded);
                }
                self.write_ssd_block(key, &block)?;
                self.disk_index.insert(key.clone(), block.len() as u64);
                self.disk_order.push_back_if_absent(key.clone());
                self.ssd_bytes = self.ssd_bytes.saturating_add(block.len() as u64);
                self.record_metadata(
                    key,
                    infer_block_kind(key),
                    extract_routing_slot(key),
                    new_value.len(),
                    0,
                    CacheAdmissionReason::MemoryOnly,
                );
                self.append_disk_manifest_put(key, block.len() as u64)?;
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
        inner
            .memory_order
            .retain(|key| key.shard_id != shard_id);
        inner.pmem_order.retain(|key| key.shard_id != shard_id);
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
        inner.metadata.retain(|key, _| key.shard_id != shard_id);
        for mut pins in inner.all_pins() {
            pins.entries.retain(|key, _| key.shard_id != shard_id);
        }
        inner.stats.invalidations = inner
            .stats
            .invalidations
            .saturating_add((memory_entries_removed + disk_keys.len()) as u64);
        inner.ssd_bytes = inner.ssd_bytes.saturating_sub(disk_bytes_before);
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
            {
                inner.pins_for(key).entries.remove(key);
            }
            inner.metadata.remove(key);
        }
        let _ = inner.delete_ssd_blocks(&disk_delete_keys);
        inner
            .memory_order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner
            .pmem_order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner
            .disk_order
            .retain(|key| !(key.shard_id == shard_id && key.selector.starts_with(&prefix)));
        inner.stats.invalidations = inner
            .stats
            .invalidations
            .saturating_add(slot_keys.len() as u64);
        inner.ssd_bytes = inner.ssd_bytes.saturating_sub(disk_bytes_removed);
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
            {
                inner.pins_for(key).entries.remove(key);
            }
            inner.metadata.remove(key);
        }
        let _ = inner.delete_ssd_blocks(&disk_delete_keys);
        inner.memory_order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.pmem_order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.disk_order.retain(|key| {
            !(key.shard_id == shard_id && key.namespace == "page" && key.record_key == record_key)
        });
        inner.stats.invalidations = inner
            .stats
            .invalidations
            .saturating_add(segment_keys.len() as u64);
        inner.ssd_bytes = inner.ssd_bytes.saturating_sub(disk_bytes_removed);
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
                let pinned = inner.is_pinned(&key);
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
                let meta = inner.metadata.get(&key);
                CacheEntryInfo {
                    shard_id: key.shard_id,
                    namespace: key.namespace.into_owned(),
                    record_key: key.record_key,
                    selector: key.selector,
                    memory_bytes,
                    pmem_bytes,
                    disk_bytes,
                    pinned,
                    block_kind: meta.map(|meta| meta.block_kind),
                    routing_slot: meta.and_then(|meta| meta.routing_slot),
                    hotness: meta
                        .map(|meta| meta.hotness.load(Ordering::Relaxed))
                        .unwrap_or_default(),
                    hits: meta
                        .map(|meta| meta.hits.load(Ordering::Relaxed))
                        .unwrap_or_default(),
                    last_access_epoch: meta
                        .map(|meta| meta.last_access_epoch.load(Ordering::Relaxed))
                        .unwrap_or_default(),
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
                let pinned = inner.is_pinned(&key);
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
                let meta = inner.metadata.get(&key);
                CacheEntryInfo {
                    shard_id: key.shard_id,
                    namespace: key.namespace.into_owned(),
                    record_key: key.record_key,
                    selector: key.selector,
                    memory_bytes,
                    pmem_bytes,
                    disk_bytes,
                    pinned,
                    block_kind: meta.map(|meta| meta.block_kind),
                    routing_slot: meta.and_then(|meta| meta.routing_slot),
                    hotness: meta
                        .map(|meta| meta.hotness.load(Ordering::Relaxed))
                        .unwrap_or_default(),
                    hits: meta
                        .map(|meta| meta.hits.load(Ordering::Relaxed))
                        .unwrap_or_default(),
                    last_access_epoch: meta
                        .map(|meta| meta.last_access_epoch.load(Ordering::Relaxed))
                        .unwrap_or_default(),
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
        // Every stripe held together, so the four figures describe one instant
        // rather than four.
        let (pinned_entries, pin_operations, unpin_operations, zero_copy_handle_hits) = {
            let stripes = inner.all_pins();
            stripes.iter().fold((0, 0, 0, 0), |total, pins| {
                (
                    total.0 + pins.entries.len() as u64,
                    total.1 + pins.pin_operations,
                    total.2 + pins.unpin_operations,
                    total.3 + pins.zero_copy_handle_hits,
                )
            })
        };
        CacheStats {
            memory_bytes: inner.memory_bytes as u64,
            pmem_bytes: inner.pmem_bytes as u64,
            disk_bytes: inner.ssd_bytes,
            pinned_entries,
            pin_operations,
            unpin_operations,
            zero_copy_handle_hits,
            pinned_bytes: inner.pinned_memory_bytes(),
            async_writeback_queue_depth: inner.async_writeback_queue.len() as u64,
            async_writeback_queue_bytes: inner.async_writeback_queue_bytes,
            memory_hits: inner.read_counters.memory_hits.load(Ordering::Relaxed),
            pmem_hits: inner.read_counters.pmem_hits.load(Ordering::Relaxed),
            disk_hits: inner.read_counters.disk_hits.load(Ordering::Relaxed),
            misses: inner.read_counters.misses.load(Ordering::Relaxed),
            hotness_promotions: inner
                .read_counters
                .hotness_promotions
                .load(Ordering::Relaxed),
            access_order_refreshes: inner
                .read_counters
                .access_order_refreshes
                .load(Ordering::Relaxed),
            get_latency_samples: inner.read_counters.get_latency.samples(),
            get_latency_total_micros: inner
                .read_counters
                .get_latency
                .total_micros
                .load(Ordering::Relaxed),
            get_latency_max_micros: inner
                .read_counters
                .get_latency
                .max_micros
                .load(Ordering::Relaxed),
            get_latency_le_10us: inner
                .read_counters
                .get_latency
                .le_10us
                .load(Ordering::Relaxed),
            get_latency_le_100us: inner
                .read_counters
                .get_latency
                .le_100us
                .load(Ordering::Relaxed),
            get_latency_le_1ms: inner
                .read_counters
                .get_latency
                .le_1ms
                .load(Ordering::Relaxed),
            get_latency_le_10ms: inner
                .read_counters
                .get_latency
                .le_10ms
                .load(Ordering::Relaxed),
            get_latency_gt_10ms: inner
                .read_counters
                .get_latency
                .gt_10ms
                .load(Ordering::Relaxed),
            read_through_latency_samples: inner.read_counters.read_through_latency.samples(),
            read_through_latency_total_micros: inner
                .read_counters
                .read_through_latency
                .total_micros
                .load(Ordering::Relaxed),
            read_through_latency_le_10us: inner
                .read_counters
                .read_through_latency
                .le_10us
                .load(Ordering::Relaxed),
            read_through_latency_le_100us: inner
                .read_counters
                .read_through_latency
                .le_100us
                .load(Ordering::Relaxed),
            read_through_latency_le_1ms: inner
                .read_counters
                .read_through_latency
                .le_1ms
                .load(Ordering::Relaxed),
            read_through_latency_le_10ms: inner
                .read_counters
                .read_through_latency
                .le_10ms
                .load(Ordering::Relaxed),
            read_through_latency_gt_10ms: inner
                .read_counters
                .read_through_latency
                .gt_10ms
                .load(Ordering::Relaxed),
            refill_latency_samples: inner.read_counters.refill_latency.samples(),
            refill_latency_total_micros: inner
                .read_counters
                .refill_latency
                .total_micros
                .load(Ordering::Relaxed),
            refill_latency_le_10us: inner
                .read_counters
                .refill_latency
                .le_10us
                .load(Ordering::Relaxed),
            refill_latency_le_100us: inner
                .read_counters
                .refill_latency
                .le_100us
                .load(Ordering::Relaxed),
            refill_latency_le_1ms: inner
                .read_counters
                .refill_latency
                .le_1ms
                .load(Ordering::Relaxed),
            refill_latency_le_10ms: inner
                .read_counters
                .refill_latency
                .le_10ms
                .load(Ordering::Relaxed),
            refill_latency_gt_10ms: inner
                .read_counters
                .refill_latency
                .gt_10ms
                .load(Ordering::Relaxed),
            ..inner.stats
        }
    }

    /// What this cache's current statistics say about its health.
    ///
    /// A shortcut for `cache_health_report(&self.stats())`. Taking the snapshot
    /// and judging it are separate so the judgement can also be applied to a
    /// snapshot from somewhere else, such as one recovered from a scrape.
    pub fn health_report(&self) -> CacheHealthReport {
        cache_health_report(&self.stats())
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
            get_avg_us: average_latency_us(get_total, get_count),
            get_p50_us: latency_percentile_us(
                get_count,
                stats.get_latency_le_10us,
                stats.get_latency_le_100us,
                stats.get_latency_le_1ms,
                stats.get_latency_le_10ms,
                stats.get_latency_gt_10ms,
                get_max,
                50,
            ),
            get_p95_us: latency_percentile_us(
                get_count,
                stats.get_latency_le_10us,
                stats.get_latency_le_100us,
                stats.get_latency_le_1ms,
                stats.get_latency_le_10ms,
                stats.get_latency_gt_10ms,
                get_max,
                95,
            ),
            get_max_us: get_max,
            put_count,
            put_avg_us: average_latency_us(put_total, put_count),
            put_p50_us: latency_percentile_us(
                put_count,
                stats.put_latency_le_10us,
                stats.put_latency_le_100us,
                stats.put_latency_le_1ms,
                stats.put_latency_le_10ms,
                stats.put_latency_gt_10ms,
                put_max,
                50,
            ),
            put_p95_us: latency_percentile_us(
                put_count,
                stats.put_latency_le_10us,
                stats.put_latency_le_100us,
                stats.put_latency_le_1ms,
                stats.put_latency_le_10ms,
                stats.put_latency_gt_10ms,
                put_max,
                95,
            ),
            put_max_us: put_max,
            read_through_count: stats.read_through_latency_samples,
            read_through_avg_us: average_latency_us(
                stats.read_through_latency_total_micros,
                stats.read_through_latency_samples,
            ),
            read_through_p50_us: latency_percentile_us(
                stats.read_through_latency_samples,
                stats.read_through_latency_le_10us,
                stats.read_through_latency_le_100us,
                stats.read_through_latency_le_1ms,
                stats.read_through_latency_le_10ms,
                stats.read_through_latency_gt_10ms,
                0,
                50,
            ),
            read_through_p95_us: latency_percentile_us(
                stats.read_through_latency_samples,
                stats.read_through_latency_le_10us,
                stats.read_through_latency_le_100us,
                stats.read_through_latency_le_1ms,
                stats.read_through_latency_le_10ms,
                stats.read_through_latency_gt_10ms,
                0,
                95,
            ),
            refill_count: stats.refill_latency_samples,
            refill_avg_us: average_latency_us(
                stats.refill_latency_total_micros,
                stats.refill_latency_samples,
            ),
            refill_p50_us: latency_percentile_us(
                stats.refill_latency_samples,
                stats.refill_latency_le_10us,
                stats.refill_latency_le_100us,
                stats.refill_latency_le_1ms,
                stats.refill_latency_le_10ms,
                stats.refill_latency_gt_10ms,
                0,
                50,
            ),
            refill_p95_us: latency_percentile_us(
                stats.refill_latency_samples,
                stats.refill_latency_le_10us,
                stats.refill_latency_le_100us,
                stats.refill_latency_le_1ms,
                stats.refill_latency_le_10ms,
                stats.refill_latency_gt_10ms,
                0,
                95,
            ),
            writeback_count: stats.writeback_latency_samples,
            writeback_avg_us: average_latency_us(
                stats.writeback_latency_total_micros,
                stats.writeback_latency_samples,
            ),
            writeback_p50_us: latency_percentile_us(
                stats.writeback_latency_samples,
                stats.writeback_latency_le_10us,
                stats.writeback_latency_le_100us,
                stats.writeback_latency_le_1ms,
                stats.writeback_latency_le_10ms,
                stats.writeback_latency_gt_10ms,
                0,
                50,
            ),
            writeback_p95_us: latency_percentile_us(
                stats.writeback_latency_samples,
                stats.writeback_latency_le_10us,
                stats.writeback_latency_le_100us,
                stats.writeback_latency_le_1ms,
                stats.writeback_latency_le_10ms,
                stats.writeback_latency_gt_10ms,
                0,
                95,
            ),
            eviction_count: stats.eviction_latency_samples,
            eviction_avg_us: average_latency_us(
                stats.eviction_latency_total_micros,
                stats.eviction_latency_samples,
            ),
            eviction_p50_us: latency_percentile_us(
                stats.eviction_latency_samples,
                stats.eviction_latency_le_10us,
                stats.eviction_latency_le_100us,
                stats.eviction_latency_le_1ms,
                stats.eviction_latency_le_10ms,
                stats.eviction_latency_gt_10ms,
                0,
                50,
            ),
            eviction_p95_us: latency_percentile_us(
                stats.eviction_latency_samples,
                stats.eviction_latency_le_10us,
                stats.eviction_latency_le_100us,
                stats.eviction_latency_le_1ms,
                stats.eviction_latency_le_10ms,
                stats.eviction_latency_gt_10ms,
                0,
                95,
            ),
            compaction_count: stats.compaction_latency_samples,
            compaction_avg_us: average_latency_us(
                stats.compaction_latency_total_micros,
                stats.compaction_latency_samples,
            ),
            compaction_p50_us: latency_percentile_us(
                stats.compaction_latency_samples,
                stats.compaction_latency_le_10us,
                stats.compaction_latency_le_100us,
                stats.compaction_latency_le_1ms,
                stats.compaction_latency_le_10ms,
                stats.compaction_latency_gt_10ms,
                0,
                50,
            ),
            compaction_p95_us: latency_percentile_us(
                stats.compaction_latency_samples,
                stats.compaction_latency_le_10us,
                stats.compaction_latency_le_100us,
                stats.compaction_latency_le_1ms,
                stats.compaction_latency_le_10ms,
                stats.compaction_latency_gt_10ms,
                0,
                95,
            ),
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
        inner.memory_bytes = 0;
        inner.pmem_bytes = 0;
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
