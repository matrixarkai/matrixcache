// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Compare caller-path and background refill after SSD reads.
//! Set `MATRIXCACHE_PROMOTION_READERS` to compare contention at 1-64 readers.

use matrixcache::{CacheKey, CacheOptions, CacheTier, MultiLayerCache};
use std::hint::black_box;
use std::sync::{Arc, Barrier};
use std::time::Instant;

const KEYS: usize = 256;
const VALUE_BYTES: usize = 4096;
const DEFAULT_READERS: usize = 4;
const READS_PER_READER: usize = 512;

fn run(async_promotion: bool) {
    let readers = std::env::var("MATRIXCACHE_PROMOTION_READERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=64).contains(value))
        .unwrap_or(DEFAULT_READERS);
    let directory = tempfile::tempdir().expect("cache directory");
    let mut options = CacheOptions::new(64 * 1024, 0, 8 * 1024 * 1024)
        .with_ssd_paths([directory.path().join("ssd")]);
    options.ssd_block_durability = false;
    if async_promotion {
        options = options.with_async_promotion(256, 2 * 1024 * 1024, 32, 1);
    }
    let cache = MultiLayerCache::with_options(options);
    let keys = (0..KEYS)
        .map(|index| CacheKey::string(1, &format!("cold-{index:04}")))
        .collect::<Vec<_>>();
    for key in &keys {
        cache
            .put_bypass_storage_for_tier(CacheTier::Ssd, key.clone(), vec![b'x'; VALUE_BYTES])
            .expect("populate SSD");
    }
    let cache = Arc::new(cache);
    let keys = Arc::new(keys);
    let barrier = Arc::new(Barrier::new(readers + 1));
    let mut latencies = Vec::with_capacity(readers * READS_PER_READER);
    let started = std::thread::scope(|scope| {
        let workers = (0..readers)
            .map(|reader| {
                let cache = Arc::clone(&cache);
                let keys = Arc::clone(&keys);
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let mut samples = Vec::with_capacity(READS_PER_READER);
                    barrier.wait();
                    for read in 0..READS_PER_READER {
                        let key = &keys[(read * 17 + reader * 31) % KEYS];
                        let start = Instant::now();
                        let value = cache.get_shared(key).expect("read").expect("value");
                        black_box(value.len());
                        samples.push(start.elapsed().as_nanos() as u64);
                    }
                    samples
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let started = Instant::now();
        for worker in workers {
            latencies.extend(worker.join().expect("reader"));
        }
        started
    });
    latencies.sort_unstable();
    let stats = cache.stats();
    println!(
        "async={} backend={} readers={} qps={:.0} p50_us={:.1} p95_us={:.1} p99_us={:.1} memory_fills={} memory_evictions={} disk_hits={}",
        async_promotion,
        if cfg!(feature = "rocksdb-ssd") { "rocksdb" } else { "file" },
        readers,
        latencies.len() as f64 / started.elapsed().as_secs_f64(),
        latencies[latencies.len() / 2] as f64 / 1000.0,
        latencies[latencies.len() * 95 / 100] as f64 / 1000.0,
        latencies[latencies.len() * 99 / 100] as f64 / 1000.0,
        stats.memory_fills,
        stats.memory_evictions,
        stats.disk_hits,
    );
    assert!(cache.stop());
}

fn main() {
    run(false);
    run(true);
    run(true);
    run(false);
}
