// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Concurrent batch reads on a **default-configured** cache under a **skewed**
//! workload.
//!
//! `batch_concurrency_bench` reads round-robin, which is the worst case for
//! this question and was chosen deliberately: every key is re-read exactly
//! `RESIDENT` accesses after its last sighting, so the reuse gap is always the
//! whole resident set. `lru_refresh_distance` is compared against that gap, so
//! anything below `RESIDENT` behaves identically to zero and every hit needs
//! its entry moved.
//!
//! Real cache traffic is not round-robin. It is skewed — that is the reason a
//! cache works at all — and under skew a hot key is re-read long before 512
//! other accesses have gone by. So the default of 512 that round-robin cannot
//! benefit from is exactly the case skew does.
//!
//! This bench therefore changes two things at once relative to that one, on
//! purpose: a skewed access pattern, and **no call to
//! `set_lru_refresh_distance` at all**. What it measures is what somebody gets
//! who constructs a cache and reads from it.
//!
//! Three read shapes are reported side by side: copying `get_batch`, shared
//! `get_shared_batch`, and a loop of single `get` calls. Retrieval callers that
//! parse bytes and drop them should prefer the shared path; it avoids the
//! caller-side allocation while still accounting the hit.
//!
//! ```text
//! cargo run --release --no-default-features --example batch_skew_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::Arc;
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const KEY_SPACE: usize = 8192;
const BATCH: usize = 32;
const BATCHES_PER_THREAD: usize = 400;
const REPEATS: usize = 5;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

/// Deterministic skew: cubing the unit draw puts most reads in the low keys,
/// so a hot key comes back around well inside the default refresh distance.
fn skewed(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let unit = ((*state >> 11) as f64) / ((1u64 << 53) as f64);
    let skewed = unit * unit * unit;
    ((skewed * KEY_SPACE as f64) as usize).min(KEY_SPACE - 1)
}

/// Everything resident, so this measures the read path and not eviction.
/// Deliberately left at whatever `lru_refresh_distance` defaults to.
fn build() -> Arc<MultiLayerCache> {
    let cache =
        MultiLayerCache::try_with_options(CacheOptions::new(KEY_SPACE * VALUE_BYTES * 4, 0, 0))
            .expect("cache");
    for index in 0..KEY_SPACE {
        cache
            .put(
                CacheKey::string(0, &format!("sk-{index:05}")),
                vec![b'v'; VALUE_BYTES],
            )
            .expect("put");
    }
    Arc::new(cache)
}

#[derive(Clone, Copy)]
enum ReadShape {
    CopyingBatch,
    SharedBatch,
    GetLoop,
}

/// Aggregate keys read per second for one read shape.
fn throughput(cache: &Arc<MultiLayerCache>, threads: usize, shape: ReadShape) -> f64 {
    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(cache);
            std::thread::spawn(move || {
                let mut state = 0x2545_F491_4F6C_DD1D ^ ((worker as u64) << 32);
                let keys_read = BATCHES_PER_THREAD * BATCH;
                let started = Instant::now();
                for _ in 0..BATCHES_PER_THREAD {
                    let batch = (0..BATCH)
                        .map(|_| CacheKey::string(0, &format!("sk-{:05}", skewed(&mut state))))
                        .collect::<Vec<_>>();
                    match shape {
                        ReadShape::CopyingBatch => {
                            let values = cache.get_batch(&batch).expect("get_batch");
                            assert!(values.iter().all(|value| value.is_some()), "all resident");
                        }
                        ReadShape::SharedBatch => {
                            let values = cache.get_shared_batch(&batch).expect("get_shared_batch");
                            assert!(values.iter().all(|value| value.is_some()), "all resident");
                        }
                        ReadShape::GetLoop => {
                            for key in &batch {
                                assert!(cache.get(key).expect("get").is_some(), "all resident");
                            }
                        }
                    }
                }
                keys_read as f64 / started.elapsed().as_secs_f64()
            })
        })
        .collect::<Vec<_>>();
    // Each worker times its own run, so a descheduled thread inflates its own
    // elapsed time rather than everyone's.
    workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .sum()
}

fn main() {
    let cache = build();
    println!(
        "{KEY_SPACE} resident values of {VALUE_BYTES} bytes, skewed reads, \
         batches of {BATCH}, median of {REPEATS}"
    );
    println!(
        "refresh window left at its default: {:?}\n",
        cache.lru_refresh_time()
    );
    println!(
        "{:<10}{:>18}{:>18}{:>18}{:>12}{:>12}",
        "threads",
        "get_batch Mkeys/s",
        "shared Mkeys/s",
        "get loop Mkeys/s",
        "batch/loop",
        "shared/loop"
    );
    for threads in [1_usize, 2, 4, 8] {
        // Both shapes inside each repeat, alternating which goes first.
        // Measuring all the repeats of one and then all of the other compares
        // two stretches of wall clock, and a threaded read benchmark is the
        // most sensitive thing here to what else the machine is doing. Both
        // are reads, so neither leaves the other anything.
        let mut batched_samples = Vec::with_capacity(REPEATS);
        let mut shared_samples = Vec::with_capacity(REPEATS);
        let mut looped_samples = Vec::with_capacity(REPEATS);
        let mut batch_ratios = Vec::with_capacity(REPEATS);
        let mut shared_ratios = Vec::with_capacity(REPEATS);
        for repeat in 0..REPEATS {
            let (batched, shared, looped) = match repeat % 3 {
                0 => {
                    let batched = throughput(&cache, threads, ReadShape::CopyingBatch);
                    let shared = throughput(&cache, threads, ReadShape::SharedBatch);
                    let looped = throughput(&cache, threads, ReadShape::GetLoop);
                    (batched, shared, looped)
                }
                1 => {
                    let shared = throughput(&cache, threads, ReadShape::SharedBatch);
                    let looped = throughput(&cache, threads, ReadShape::GetLoop);
                    let batched = throughput(&cache, threads, ReadShape::CopyingBatch);
                    (batched, shared, looped)
                }
                _ => {
                    let looped = throughput(&cache, threads, ReadShape::GetLoop);
                    let batched = throughput(&cache, threads, ReadShape::CopyingBatch);
                    let shared = throughput(&cache, threads, ReadShape::SharedBatch);
                    (batched, shared, looped)
                }
            };
            batch_ratios.push(batched / looped.max(f64::MIN_POSITIVE));
            shared_ratios.push(shared / looped.max(f64::MIN_POSITIVE));
            batched_samples.push(batched);
            shared_samples.push(shared);
            looped_samples.push(looped);
        }
        batch_ratios.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        shared_ratios.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        let batched = median(batched_samples);
        let shared = median(shared_samples);
        let looped = median(looped_samples);
        println!(
            "{threads:<10}{:>18.4}{:>18.4}{:>18.4}{:>11.2}x{:>11.2}x",
            batched / 1e6,
            shared / 1e6,
            looped / 1e6,
            batch_ratios[batch_ratios.len() / 2],
            shared_ratios[shared_ratios.len() / 2]
        );
    }
    println!(
        "\nThe shared column is the retrieval-shaped path: immutable cached bytes\n\
         are returned by shared buffer instead of copied into a new Vec."
    );
}
