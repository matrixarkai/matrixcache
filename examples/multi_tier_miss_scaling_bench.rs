// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Multi-tier negative batch lookup scaling for shared and pinned reads.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::hint::black_box;
use std::sync::{Arc, Barrier};
use std::time::Instant;

const KEYS: usize = 256;
const BATCH_SIZE: usize = 16;
const BATCHES_PER_THREAD: usize = 1_024;

fn cache() -> (tempfile::TempDir, MultiLayerCache, Vec<CacheKey>) {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(64 << 20, 0, 64 << 20).with_ssd_paths([directory.path().join("ssd")]),
    );
    let keys = (0..KEYS)
        .map(|index| CacheKey::string(17, &format!("multi-tier-miss-{index:04}")))
        .collect::<Vec<_>>();
    (directory, cache, keys)
}

fn batch_qps(readers: usize, pinned: bool) -> f64 {
    let (_directory, cache, keys) = cache();
    let barrier = Arc::new(Barrier::new(readers + 1));
    let started = std::thread::scope(|scope| {
        let workers = (0..readers)
            .map(|reader| {
                let barrier = Arc::clone(&barrier);
                let cache = &cache;
                let keys = &keys;
                scope.spawn(move || {
                    let mut batch = Vec::with_capacity(BATCH_SIZE);
                    barrier.wait();
                    for batch_index in 0..BATCHES_PER_THREAD {
                        batch.clear();
                        let first = (batch_index.wrapping_mul(17) + reader) % keys.len();
                        batch.extend(
                            (0..BATCH_SIZE)
                                .map(|offset| keys[(first + offset) % keys.len()].clone()),
                        );
                        if pinned {
                            black_box(cache.acquire_batch(&batch).unwrap());
                        } else {
                            black_box(cache.get_shared_batch(&batch).unwrap());
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let started = Instant::now();
        for worker in workers {
            worker.join().unwrap();
        }
        started
    });
    (readers * BATCHES_PER_THREAD * BATCH_SIZE) as f64 / started.elapsed().as_secs_f64()
}

fn main() {
    println!("readers,shared_batch_values_per_second,pinned_batch_values_per_second");
    for readers in [1, 2, 4, 8] {
        println!(
            "{readers},{:.0},{:.0}",
            batch_qps(readers, false),
            batch_qps(readers, true)
        );
    }
}
