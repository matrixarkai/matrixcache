// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Do concurrent *batch* reads scale the way single reads do?
//!
//! Not the way single reads do. `get` serves a memory hit under a shared lock;
//! `get_batch` still takes the cache exclusively across its whole first loop,
//! so both columns below degrade with thread count and the refresh distance
//! barely moves them, because the exclusive acquisition happens either way.
//!
//! That is a real limit, but it is **not** the same as `get_batch` being worse
//! than the loop it replaces — it is not, see the note at the end — and the
//! obvious fix for it measures worse still.
//!
//! `hit_concurrency_bench` is the single-key equivalent and does scale, so the
//! two together show the gap rather than just asserting it.
//!
//! As in that bench, the refresh distance is the variable that would decide
//! whether the read path had anything left to do exclusively. It is compared
//! at zero, where every hit needs its entry moved, and at a distance well
//! above the reuse gap, where almost none do. A distance *below* the reuse gap
//! behaves exactly like zero — reads here cycle over `RESIDENT` keys, so an
//! entry is seen again about `RESIDENT` accesses later.
//!
//! **A recorded negative result, so it is not repeated blind.** Moving the
//! first loop to a shared lock and collecting the work that needs `&mut` --
//! pmem refills, access-order moves, metadata inserts -- into one exclusive
//! section afterwards *does* help where most hits do not need their entry
//! moved. Interleaved against `main`, six rounds, on an idle machine:
//!
//! ```text
//!  threads   default (distance 0)   above the reuse gap
//!        2          0.96x  (0/6)          3.38x  (6/6)
//!        4          0.89x  (0/6)          5.02x  (6/6)
//!        8          0.80x  (0/6)          1.40x  (6/6)
//! ```
//!
//! The bracket is rounds won out of six. At refresh distance zero every hit
//! needs its entry moved, so the shared phase cannot avoid the exclusive lock
//! and only adds a second acquisition — a consistent loss, in every round at
//! four and eight threads.
//!
//! **That refactor is abandoned, not deferred.** The note here used to say it
//! became worth redoing if `lru_refresh_distance` stopped defaulting to zero.
//! It did stop, and the prediction was wrong. Rebuilt on current `main` and
//! measured under a *skewed* workload at the new default of 512 — skewed
//! because round-robin has a reuse gap of the whole resident set, so 512
//! behaves exactly like zero there and the comparison could not have said
//! anything — it still loses: 0.70x / 0.81x / 0.80x / 0.70x at 1/2/4/8
//! threads, winning one round out of twenty-four.
//!
//! So the regression was never about the default. It is the two-pass
//! structure: a shared pass that collects indices and an exclusive pass that
//! revisits them costs more than doing the work inline while the key is still
//! in cache, whichever lock is held. `examples/batch_skew_bench.rs` is that
//! measurement.
//!
//! And the premise was shaky too. `get_batch` on `main` already beats a loop
//! of single `get` calls by 1.11x to 1.87x, rising with thread count. The
//! batch path was earning its keep the whole time.
//!
//! An earlier measurement of the same two binaries, taken while this box was
//! at load average 40, put the default-case loss at 0.86x / 0.74x / 0.68x and
//! showed no gain at all at eight threads (1.01x, losing half its rounds).
//! The direction was right and the decision would have been the same, but the
//! magnitudes were not, and the eight-thread gain was real. The numbers above
//! are the idle-machine ones.
//!
//! ```text
//! cargo run --release --no-default-features --example batch_concurrency_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 4096;
const BATCH: usize = 32;
const BATCHES_PER_THREAD: usize = 500;
const REPEATS: usize = 5;
/// Longer than any run here, so no hit ever finds its entry stale.
const NEVER_MOVES: Duration = Duration::from_secs(3_600);

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

fn build(refresh_window: Duration) -> Arc<MultiLayerCache> {
    let cache =
        MultiLayerCache::try_with_options(CacheOptions::new(RESIDENT * VALUE_BYTES * 8, 0, 0))
            .expect("cache");
    cache.set_lru_refresh_time(refresh_window);
    for index in 0..RESIDENT {
        cache
            .put(
                CacheKey::string(0, &format!("bat-{index:05}")),
                vec![b'v'; VALUE_BYTES],
            )
            .expect("put");
    }
    Arc::new(cache)
}

/// Aggregate keys read per second across `threads` batch readers.
fn throughput(cache: &Arc<MultiLayerCache>, threads: usize) -> f64 {
    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(cache);
            std::thread::spawn(move || {
                let keys_read = BATCHES_PER_THREAD * BATCH;
                let started = Instant::now();
                for round in 0..BATCHES_PER_THREAD {
                    let batch = (0..BATCH)
                        .map(|slot| {
                            let index = (worker * 31 + round * 7 + slot * 13) % RESIDENT;
                            CacheKey::string(0, &format!("bat-{index:05}"))
                        })
                        .collect::<Vec<_>>();
                    let values = cache.get_batch(&batch).expect("get_batch");
                    assert!(
                        values.iter().all(|value| value.is_some()),
                        "every key is resident"
                    );
                }
                keys_read as f64 / started.elapsed().as_secs_f64()
            })
        })
        .collect::<Vec<_>>();
    // Each worker times its own run, so a thread descheduled by other load
    // inflates its own elapsed time rather than everyone's.
    workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .sum()
}

fn main() {
    println!(
        "{RESIDENT} resident values of {VALUE_BYTES} bytes, batches of {BATCH}, \
         {BATCHES_PER_THREAD} batches/thread, median of {REPEATS}\n"
    );
    println!(
        "{:<10}{:>18}{:>18}",
        "threads", "distance 0 Mkeys/s", "no-move Mkeys/s"
    );
    let always_moves = build(Duration::ZERO);
    let mostly_still = build(NEVER_MOVES);
    for threads in [1_usize, 2, 4, 8] {
        let zero = median(
            (0..REPEATS)
                .map(|_| throughput(&always_moves, threads))
                .collect(),
        );
        let far = median(
            (0..REPEATS)
                .map(|_| throughput(&mostly_still, threads))
                .collect(),
        );
        println!("{threads:<10}{:>18.4}{:>18.4}", zero / 1e6, far / 1e6);
    }
}
