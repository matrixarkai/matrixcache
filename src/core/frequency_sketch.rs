// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI
//
// A count-min sketch of how often keys are accessed, including keys that are
// not resident.
//
// The cache already knows how often each *resident* entry has been read -- that
// is what `hits` and `hotness` are, and what victim selection scores. What it
// cannot know is how often a key it is about to admit has been read in the
// past, because nothing remembers a key once it has been evicted. So admission
// has no basis on which to decline: every miss is admitted, and a key read once
// evicts one that has been read a hundred times.
//
// This is the structure that answers it. It is the estimator underneath
// CacheLib's MMTinyLFU, whose admission compares the two:
//
//     bool admitToMain(const T& tinyNode, const T& mainNode) const noexcept {
//       auto tinyFreq = accessFreq_.getCount(hashNode(tinyNode));
//       auto mainFreq = accessFreq_.getCount(hashNode(mainNode));
//       ...
//       return tinyFreq > mainFreq;
//     }
//
// A count-min sketch never *under*-counts. Collisions can only inflate an
// estimate, so a key the sketch says is cold really is cold, while one it says
// is hot might be a collision with something hot. That asymmetry is the right
// way round for admission: it errs towards admitting, which is the current
// behaviour, rather than towards rejecting something valuable.

/// How many independent counters each key is hashed to.
///
/// Four is the usual choice for this structure: the estimate is the smallest of
/// the four, so the chance of an inflated answer is the chance of colliding in
/// every one of them at once.
const SKETCH_HASHES: usize = 4;

/// Counters per resident entry.
///
/// The sketch has to be sized for the *key population*, not the resident set --
/// its whole purpose is remembering keys that are no longer resident, so it is
/// oversubscribed by design and sizing it to capacity guarantees collisions.
/// CacheLib sizes from its window, `capacity * windowToCacheSizeRatio`, for the
/// same reason.
///
/// At eight per entry a cache of 1024 estimated only 85% of singleton keys
/// correctly across a key space twice its size, which is too blunt to decide
/// admission with. Thirty-two costs a byte per counter -- 128KiB for a cache
/// holding four thousand values, against the megabytes the values occupy.
const COUNTERS_PER_ENTRY: usize = 32;

/// How many increments before every counter is halved, as a multiple of
/// capacity.
///
/// This is what makes the estimate a *recent* frequency rather than a lifetime
/// one -- without it a key that was hot an hour ago outranks one that is hot
/// now, forever. CacheLib calls the same idea `windowToCacheSizeRatio` and also
/// defaults it to 32.
const WINDOW_TO_CAPACITY_RATIO: u64 = 32;

/// An approximate count of how often each key has been accessed recently.
///
/// Estimates are capped at 255: the question asked of this is only ever "which
/// of these two is hotter", and a key seen 255 times has answered it.
#[derive(Debug)]
pub struct FrequencySketch {
    /// Atomic because the read path records into this while holding the cache
    /// lock shared. Unlike a single global counter these are spread over
    /// `capacity * 32` cells, so two threads collide only on the same key or an
    /// unlucky hash.
    counters: Vec<AtomicU8>,
    /// `counters.len() - 1`, for turning a hash into an index. The length is a
    /// power of two so this is a mask rather than a modulo.
    mask: usize,
    /// Increments since the last halving. Contended, but only one add per
    /// recorded access against four counter updates, and sampling keeps
    /// recorded accesses well below total accesses.
    window: AtomicU64,
    /// Increments at which to halve.
    max_window: u64,
}

impl FrequencySketch {
    /// Sizes a sketch for a cache holding roughly `entries` values.
    ///
    /// A capacity of zero still yields a usable sketch -- one counter that
    /// saturates immediately -- rather than an empty one that would panic on
    /// the first index.
    pub fn with_capacity(entries: usize) -> Self {
        let width = (entries.max(1) * COUNTERS_PER_ENTRY).next_power_of_two();
        Self {
            counters: (0..width).map(|_| AtomicU8::new(0)).collect(),
            mask: width - 1,
            window: AtomicU64::new(0),
            max_window: (entries.max(1) as u64).saturating_mul(WINDOW_TO_CAPACITY_RATIO),
        }
    }

    /// The four positions this key hashes to.
    ///
    /// One pass over the key, not four. Hashing dominated the cost of recording
    /// -- 86ns per record against roughly 226ns for an entire cache read -- and
    /// four passes over the same bytes was nearly all of it.
    ///
    /// `h1 + i * h2` from a single pair of hashes behaves like `i` independent
    /// hash functions for this purpose (Kirsch and Mitzenmacher), which is how
    /// production Bloom filters and sketches are built.
    fn positions(&self, key: &CacheKey) -> [usize; SKETCH_HASHES] {
        let bytes = key.record_key.as_bytes();
        let first = xxh32_with_seed(bytes, 0) as usize;
        // Forced odd: with a power-of-two width an even step could be congruent
        // to zero, collapsing all four positions onto one counter and turning
        // the minimum of four into a minimum of one.
        let step = (xxh32_with_seed(bytes, 0x9E37_79B9) as usize) | 1;
        let mut out = [0_usize; SKETCH_HASHES];
        for (row, slot) in out.iter_mut().enumerate() {
            *slot = first.wrapping_add(row.wrapping_mul(step)) & self.mask;
        }
        out
    }

    /// Records one access. Safe while holding the cache lock shared.
    pub fn record(&self, key: &CacheKey) {
        for position in self.positions(key) {
            // A counter at its maximum is not written at all -- `fetch_update`
            // returns without a store when the closure declines. The keys that
            // would contend are the hot ones, and the hot ones are the ones
            // that saturate, so the contention removes itself.
            let _ = self.counters[position].fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |count| {
                    if count == u8::MAX {
                        None
                    } else {
                        Some(count + 1)
                    }
                },
            );
        }
        self.window.fetch_add(1, Ordering::Relaxed);
        if self.window.load(Ordering::Relaxed) >= self.max_window {
            self.decay();
        }
    }

    /// How often this key has been accessed recently, never underestimated.
    pub fn estimate(&self, key: &CacheKey) -> u8 {
        self.positions(key)
            .into_iter()
            .map(|position| self.counters[position].load(Ordering::Relaxed))
            .min()
            .unwrap_or(0)
    }

    /// Halves every counter, and the window with it.
    ///
    /// Halving rather than clearing keeps the ordering between keys: something
    /// seen a hundred times still outranks something seen twice afterwards,
    /// which a reset would throw away.
    fn decay(&self) {
        for counter in &self.counters {
            let current = counter.load(Ordering::Relaxed);
            if current != 0 {
                counter.store(current >> 1, Ordering::Relaxed);
            }
        }
        let window = self.window.load(Ordering::Relaxed);
        self.window.store(window >> 1, Ordering::Relaxed);
    }

    /// Counters held, for tests and for reporting memory use.
    pub fn width(&self) -> usize {
        self.counters.len()
    }

    /// Accesses recorded since the last halving.
    pub fn window(&self) -> u64 {
        self.window.load(Ordering::Relaxed)
    }
}
