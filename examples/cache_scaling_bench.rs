// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Read-path cost and read concurrency for the multi-tier cache.
//!
//! Two things are worth watching on the read path, and they move
//! independently.
//!
//! The first is what a single memory-tier hit costs, and whether that cost
//! stays put as the resident set grows. A lookup that scans anything will show
//! up here as a number that climbs with the entry count.
//!
//! The second is what happens when several threads read at once. A cache that
//! serialises its readers gets *slower* per operation as threads are added
//! rather than faster in aggregate, and that shows up as throughput falling in
//! the scaling table below. Reads currently take the cache lock exclusively
//! because the hit updates statistics, hotness and latency histograms on the
//! way out, so this table is the measurement to beat when that changes.
//!
//! The third table puts `MultiLayerCache` next to `ShardedMultiLayerCache`
//! holding the same total capacity. The sharded cache spreads keys over
//! independent shards, so readers of different shards do not queue behind one
//! another. It is the supported answer to the contention in the second table,
//! and this comparison is what says whether it earns its extra bookkeeping.
//!
//! ```text
//! cargo run --release --example cache_scaling_bench
//! cargo run --release --example cache_scaling_bench -- 8192
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache, ShardedMultiLayerCache};
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
const REPEATS: usize = 5;

fn bench_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("matrixcache-scaling-{name}"))
}

/// Keys plus a visit order that touches each one but not in insertion order.
fn workload(entries: usize) -> Vec<CacheKey> {
    (0..entries)
        .map(|index| CacheKey::string(0, &format!("scaling-key-{index:010}")))
        .collect()
}

fn scattered(index: usize, len: usize) -> usize {
    index.wrapping_mul(2_654_435_761) % len.max(1)
}

fn ns_per_op(elapsed: Duration, ops: usize) -> f64 {
    if ops == 0 {
        return 0.0;
    }
    elapsed.as_nanos() as f64 / ops as f64
}

/// Cost of a single-threaded memory-tier hit at a given resident entry count.
fn hit_cost(entries: usize) -> f64 {
    let dir = bench_dir(&format!("hit-{entries}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::new(entries * 256, &dir);
    cache.start().expect("start cache");
    let keys = workload(entries);
    let value = vec![b'v'; VALUE_BYTES];
    for key in &keys {
        cache.put(key.clone(), value.clone()).expect("put");
    }

    let ops = entries * 8;
    let mut best = f64::MAX;
    for _ in 0..REPEATS {
        let started = Instant::now();
        for index in 0..ops {
            let _ = cache.get(&keys[scattered(index, entries)]).expect("get");
        }
        best = best.min(ns_per_op(started.elapsed(), ops));
    }
    let _ = std::fs::remove_dir_all(&dir);
    best
}

/// Aggregate read throughput with `threads` readers sharing one cache.
fn read_throughput(entries: usize, threads: usize) -> f64 {
    let dir = bench_dir(&format!("conc-{threads}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::new(entries * 256, &dir);
    cache.start().expect("start cache");
    let keys = workload(entries);
    let value = vec![b'v'; VALUE_BYTES];
    for key in &keys {
        cache.put(key.clone(), value.clone()).expect("put");
    }

    let per_thread = 40_000usize;
    let mut best = f64::MAX;
    for _ in 0..3 {
        let started = Instant::now();
        std::thread::scope(|scope| {
            for thread in 0..threads {
                let cache = &cache;
                let keys = &keys;
                scope.spawn(move || {
                    for index in 0..per_thread {
                        // Offset per thread so readers are not in lockstep.
                        let slot = scattered(index + thread * 7919, keys.len());
                        let _ = cache.get(&keys[slot]).expect("get");
                    }
                });
            }
        });
        best = best.min(ns_per_op(started.elapsed(), per_thread * threads));
    }
    let _ = std::fs::remove_dir_all(&dir);
    best
}

/// Options shared by both cache shapes, so the comparison is like for like.
fn scaling_options(dir: &std::path::Path, entries: usize) -> CacheOptions {
    CacheOptions::new(entries * 256, 0, 0).with_ssd_paths(vec![dir.to_path_buf()])
}

/// Read throughput for the single-lock cache, built from shared options.
fn single_lock_throughput(entries: usize, threads: usize) -> f64 {
    let dir = bench_dir(&format!("single-{threads}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::with_options(scaling_options(&dir, entries));
    cache.start().expect("start cache");
    let keys = workload(entries);
    let value = vec![b'v'; VALUE_BYTES];
    for key in &keys {
        cache.put(key.clone(), value.clone()).expect("put");
    }
    let best = drive_readers(threads, &keys, |key| {
        let _ = cache.get(key).expect("get");
    });
    let _ = std::fs::remove_dir_all(&dir);
    best
}

/// Read throughput for the sharded cache holding the same total capacity.
fn sharded_throughput(entries: usize, threads: usize, shards: usize) -> f64 {
    let dir = bench_dir(&format!("sharded-{shards}-{threads}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = ShardedMultiLayerCache::with_options(scaling_options(&dir, entries), shards);
    cache.start().expect("start cache");
    let keys = workload(entries);
    let value = vec![b'v'; VALUE_BYTES];
    for key in &keys {
        cache.put(key.clone(), value.clone()).expect("put");
    }
    let best = drive_readers(threads, &keys, |key| {
        let _ = cache.get(key).expect("get");
    });
    let _ = std::fs::remove_dir_all(&dir);
    best
}

/// Run `threads` readers over `keys` and return the best ns/op of three runs.
fn drive_readers<F>(threads: usize, keys: &[CacheKey], read: F) -> f64
where
    F: Fn(&CacheKey) + Sync,
{
    let per_thread = 40_000usize;
    let mut best = f64::MAX;
    for _ in 0..3 {
        let started = Instant::now();
        std::thread::scope(|scope| {
            for thread in 0..threads {
                let read = &read;
                scope.spawn(move || {
                    for index in 0..per_thread {
                        // Offset per thread so readers are not in lockstep.
                        let slot = scattered(index + thread * 7919, keys.len());
                        read(&keys[slot]);
                    }
                });
            }
        });
        best = best.min(ns_per_op(started.elapsed(), per_thread * threads));
    }
    best
}

fn main() {
    let max_entries: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(4_096);

    // Warm the allocator before the first measured case.
    let _ = hit_cost(256);

    println!("memory-tier hit, single thread");
    println!("{:>10} {:>14}", "entries", "ns/op");
    let mut size = 1_024usize;
    while size <= max_entries {
        println!("{size:>10} {:>14.1}", hit_cost(size));
        size *= 4;
    }

    println!();
    println!("read throughput, {max_entries} resident entries");
    println!("{:>10} {:>14} {:>14}", "threads", "ns/op", "Mops/s");
    for &threads in &[1usize, 2, 4, 8] {
        let ns = read_throughput(max_entries, threads);
        let mops = if ns > 0.0 { 1_000.0 / ns } else { 0.0 };
        println!("{threads:>10} {ns:>14.1} {mops:>14.2}");
    }

    println!();
    println!("single lock vs sharded, same total capacity, Mops/s");
    println!(
        "{:>10} {:>14} {:>14} {:>10}",
        "threads", "single", "sharded", "speedup"
    );
    for &threads in &[1usize, 2, 4, 8] {
        let single_ns = single_lock_throughput(max_entries, threads);
        let sharded_ns = sharded_throughput(max_entries, threads, 16);
        let single = if single_ns > 0.0 {
            1_000.0 / single_ns
        } else {
            0.0
        };
        let sharded = if sharded_ns > 0.0 {
            1_000.0 / sharded_ns
        } else {
            0.0
        };
        let speedup = if single > 0.0 { sharded / single } else { 0.0 };
        println!("{threads:>10} {single:>14.2} {sharded:>14.2} {speedup:>9.2}x");
    }
}
