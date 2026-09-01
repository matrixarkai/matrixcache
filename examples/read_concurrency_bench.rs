// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What do concurrent readers cost each other, split by what they do?
//!
//! `cache_scaling_bench` reads 64-byte values that all hit. That is the case
//! least sensitive to how the read path takes the cache lock: the work inside
//! the exclusive section is a hash lookup and a handful of counter updates
//! either way, so the table barely moves however the locking is arranged.
//!
//! Two other cases are sensitive to it, and this measures those.
//!
//! * **Large hits.** The copy that turns the stored `Arc<[u8]>` into the
//!   returned `Vec` grows with the value. If it is made inside the exclusive
//!   section, readers copy one at a time; outside it, they copy in parallel.
//! * **Misses.** A read that finds nothing in memory has no bookkeeping to do
//!   at all, so any exclusivity it takes is pure serialisation.
//!
//! Absolute numbers on a loaded machine are not worth much. Run this against
//! two builds alternately and compare the pairs.
//!
//! ```text
//! cargo run --release --no-default-features --example read_concurrency_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::Arc;
use std::time::Instant;

const LARGE_VALUE_BYTES: usize = 64 * 1024;
const RESIDENT: usize = 512;
const READS_PER_THREAD: usize = 2_000;
const REPEATS: usize = 5;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

/// Aggregate reads per second across `threads` readers.
fn throughput(cache: &Arc<MultiLayerCache>, threads: usize, hit: bool) -> f64 {
    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(cache);
            std::thread::spawn(move || {
                let started = Instant::now();
                for round in 0..READS_PER_THREAD {
                    let index = (worker * 31 + round * 7) % RESIDENT;
                    let key = if hit {
                        CacheKey::string(0, &format!("resident-{index:05}"))
                    } else {
                        CacheKey::string(0, &format!("absent-{index:05}"))
                    };
                    let _ = cache.get(&key).expect("get");
                }
                started.elapsed().as_secs_f64()
            })
        })
        .collect::<Vec<_>>();
    // Each worker times only its own run, so a thread descheduled by other
    // load inflates its own elapsed time rather than everyone's. Summing the
    // per-thread rates is the aggregate the cache actually delivered.
    workers
        .into_iter()
        .map(|worker| READS_PER_THREAD as f64 / worker.join().expect("worker"))
        .sum()
}

fn main() {
    // Room for the whole resident set, so nothing here measures eviction.
    let capacity = RESIDENT * LARGE_VALUE_BYTES * 2;
    let cache = Arc::new(
        MultiLayerCache::try_with_options(CacheOptions::new(capacity, 0, 0)).expect("cache"),
    );
    for index in 0..RESIDENT {
        cache
            .put(
                CacheKey::string(0, &format!("resident-{index:05}")),
                vec![b'v'; LARGE_VALUE_BYTES],
            )
            .expect("put");
    }

    println!(
        "{RESIDENT} resident values of {} KiB, {READS_PER_THREAD} reads/thread, median of {REPEATS}\n",
        LARGE_VALUE_BYTES / 1024
    );
    println!("{:<10}{:>16}{:>16}", "threads", "hit Mops/s", "miss Mops/s");
    for threads in [1_usize, 2, 4, 8] {
        let hit = median(
            (0..REPEATS)
                .map(|_| throughput(&cache, threads, true))
                .collect(),
        );
        let miss = median(
            (0..REPEATS)
                .map(|_| throughput(&cache, threads, false))
                .collect(),
        );
        println!("{threads:<10}{:>16.4}{:>16.4}", hit / 1e6, miss / 1e6);
    }
}
