// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Do concurrent memory hits scale, and what does the refresh distance cost?
//!
//! A hit has two parts. Accounting for it — counters, hotness, the access
//! epoch — is per-entry work that concurrent readers could do at the same
//! time. Moving the entry in the tier access orders is surgery on a shared
//! structure and cannot be.
//!
//! `lru_refresh_distance` decides how often the second happens: an entry read
//! again within that many accesses keeps its place and is not moved. **Zero
//! means every hit moves it**, which is the default.
//!
//! The setting is compared against the gap since the entry was last read, and
//! that gap is about `RESIDENT` here because reads cycle. A distance below the
//! resident count is therefore indistinguishable from zero -- every hit
//! exceeds it. `NEVER_MOVES` is chosen well above it.
//!
//! So this measures the same workload at two settings. If the read path only
//! takes the cache exclusively when it actually has to move something, the
//! large-distance column should scale with threads and the distance-0 column
//! should not. If it always takes it exclusively, the two columns are the same
//! and neither scales.
//!
//! Values are small on purpose: a large value makes the copy dominate and
//! hides the locking, which is the thing under test here.
//!
//! ```text
//! cargo run --release --no-default-features --example hit_concurrency_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 4096;
const READS_PER_THREAD: usize = 20_000;
const REPEATS: usize = 5;
/// Longer than any run here, so no hit ever finds its entry stale.
const NEVER_MOVES: Duration = Duration::from_secs(3_600);

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

fn build(refresh_window: Duration) -> Arc<MultiLayerCache> {
    let capacity = RESIDENT * VALUE_BYTES * 8;
    let cache =
        MultiLayerCache::try_with_options(CacheOptions::new(capacity, 0, 0)).expect("cache");
    cache.set_lru_refresh_time(refresh_window);
    for index in 0..RESIDENT {
        cache
            .put(
                CacheKey::string(0, &format!("hit-{index:05}")),
                vec![b'v'; VALUE_BYTES],
            )
            .expect("put");
    }
    Arc::new(cache)
}

/// Which read each worker performs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadShape {
    /// Copies the value out.
    Get,
    /// Takes a pinned handle and gives it back, which is the zero-copy read a
    /// caller makes when it wants the bytes without the copy.
    AcquireRelease,
}

/// Aggregate hits per second across `threads` readers.
fn throughput(cache: &Arc<MultiLayerCache>, threads: usize, shape: ReadShape) -> f64 {
    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(cache);
            std::thread::spawn(move || {
                let started = Instant::now();
                for round in 0..READS_PER_THREAD {
                    let index = (worker * 31 + round * 7) % RESIDENT;
                    let key = CacheKey::string(0, &format!("hit-{index:05}"));
                    match shape {
                        ReadShape::Get => {
                            let found = cache.get(&key).expect("get");
                            assert!(found.is_some(), "resident key missed");
                        }
                        ReadShape::AcquireRelease => {
                            let handle = cache.acquire(&key).expect("acquire");
                            let handle = handle.expect("resident key missed");
                            cache.release(handle);
                        }
                    }
                }
                started.elapsed().as_secs_f64()
            })
        })
        .collect::<Vec<_>>();
    // Each worker times its own run, so a thread descheduled by other load
    // inflates its own elapsed time rather than everyone's.
    workers
        .into_iter()
        .map(|worker| READS_PER_THREAD as f64 / worker.join().expect("worker"))
        .sum()
}

fn main() {
    println!(
        "{RESIDENT} resident values of {VALUE_BYTES} bytes, \
         {READS_PER_THREAD} hits/thread, median of {REPEATS}\n"
    );
    println!(
        "{:<10}{:>18}{:>18}{:>18}",
        "threads", "distance 0 Mops/s", "no-move Mops/s", "acquire Mops/s"
    );
    let always_moves = build(Duration::ZERO);
    let mostly_still = build(NEVER_MOVES);
    for threads in [1_usize, 2, 4, 8] {
        // All three arms inside each repeat, rotating which goes first.
        // Measuring one arm for every repeat and then the next compares
        // separate stretches of wall clock, and a threaded read benchmark is
        // the most sensitive thing here to what else the machine is doing.
        let mut zeros = Vec::with_capacity(REPEATS);
        let mut stills = Vec::with_capacity(REPEATS);
        let mut acquires = Vec::with_capacity(REPEATS);
        for repeat in 0..REPEATS {
            let (zero, still, acquire) = match repeat % 3 {
                0 => {
                    let zero = throughput(&always_moves, threads, ReadShape::Get);
                    let still = throughput(&mostly_still, threads, ReadShape::Get);
                    (
                        zero,
                        still,
                        throughput(&mostly_still, threads, ReadShape::AcquireRelease),
                    )
                }
                1 => {
                    let still = throughput(&mostly_still, threads, ReadShape::Get);
                    let acquire = throughput(&mostly_still, threads, ReadShape::AcquireRelease);
                    (
                        throughput(&always_moves, threads, ReadShape::Get),
                        still,
                        acquire,
                    )
                }
                _ => {
                    let acquire = throughput(&mostly_still, threads, ReadShape::AcquireRelease);
                    let zero = throughput(&always_moves, threads, ReadShape::Get);
                    (
                        zero,
                        throughput(&mostly_still, threads, ReadShape::Get),
                        acquire,
                    )
                }
            };
            zeros.push(zero);
            stills.push(still);
            acquires.push(acquire);
        }
        println!(
            "{threads:<10}{:>18.4}{:>18.4}{:>18.4}",
            median(zeros) / 1e6,
            median(stills) / 1e6,
            median(acquires) / 1e6
        );
    }

    println!(
        "\nThe acquire column is measured against the same cache as `no-move`, so\n\
         the two are comparable directly: it is the same read, taking a handle\n\
         instead of a copy. A zero-copy read that scales worse than the copying\n\
         one is not doing what it is for."
    );
}
