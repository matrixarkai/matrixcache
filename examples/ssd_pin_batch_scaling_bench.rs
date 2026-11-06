// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! SSD-only pinned-batch scaling with hot storage blocks.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::hint::black_box;
use std::sync::{Arc, Barrier};
use std::time::Instant;

const KEYS: usize = 256;
const VALUE_BYTES: usize = 256;
const BATCH_SIZE: usize = 16;
const BATCHES_PER_THREAD: usize = 1_024;

fn pin_batch_qps(readers: usize) -> f64 {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 0, 64 << 20)
            .with_ssd_paths([directory.path().join("ssd")])
            .with_ssd_instance_only(true),
    );
    let keys = (0..KEYS)
        .map(|index| CacheKey::string(12, &format!("ssd-pin-batch-{index:04}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache
            .put(key.clone(), vec![(index % 251) as u8; VALUE_BYTES])
            .unwrap();
    }

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
                        let handles = cache.acquire_batch(&batch).unwrap();
                        black_box(&handles);
                        cache.release_batch(handles.into_iter().flatten().collect());
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
    println!("readers,pinned_values_per_second");
    for readers in [1, 2, 4, 8] {
        println!("{readers},{:.0}", pin_batch_qps(readers));
    }
}
