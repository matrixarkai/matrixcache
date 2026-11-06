// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! SSD-only owned and shared read scaling with hot storage blocks.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::hint::black_box;
use std::sync::{Arc, Barrier};
use std::time::Instant;

const KEYS: usize = 1_024;
const VALUE_BYTES: usize = 256;
const READS_PER_THREAD: usize = 32_768;
const BATCH: usize = 16;

fn cache() -> (tempfile::TempDir, MultiLayerCache, Vec<CacheKey>) {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 0, 64 << 20)
            .with_ssd_paths([directory.path().join("ssd")])
            .with_ssd_instance_only(true),
    );
    let keys = (0..KEYS)
        .map(|index| CacheKey::string(7, &format!("ssd-read-{index:04}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache
            .put(key.clone(), vec![(index % 251) as u8; VALUE_BYTES])
            .unwrap();
    }
    (directory, cache, keys)
}

fn owned_read_qps(readers: usize) -> f64 {
    let (_directory, cache, keys) = cache();
    let barrier = Arc::new(Barrier::new(readers + 1));
    let started = std::thread::scope(|scope| {
        let workers = (0..readers)
            .map(|reader| {
                let barrier = Arc::clone(&barrier);
                let cache = &cache;
                let keys = &keys;
                scope.spawn(move || {
                    barrier.wait();
                    for read in 0..READS_PER_THREAD {
                        let index = (read.wrapping_mul(17) + reader) % keys.len();
                        black_box(cache.get(&keys[index]).unwrap().unwrap());
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
    (readers * READS_PER_THREAD) as f64 / started.elapsed().as_secs_f64()
}

fn shared_read_qps(readers: usize) -> f64 {
    let (_directory, cache, keys) = cache();
    let barrier = Arc::new(Barrier::new(readers + 1));
    let started = std::thread::scope(|scope| {
        let workers = (0..readers)
            .map(|reader| {
                let barrier = Arc::clone(&barrier);
                let cache = &cache;
                let keys = &keys;
                scope.spawn(move || {
                    barrier.wait();
                    for read in 0..READS_PER_THREAD {
                        let index = (read.wrapping_mul(17) + reader) % keys.len();
                        black_box(cache.get_shared(&keys[index]).unwrap().unwrap());
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
    (readers * READS_PER_THREAD) as f64 / started.elapsed().as_secs_f64()
}

fn batch_read_qps(readers: usize) -> f64 {
    let (_directory, cache, keys) = cache();
    let barrier = Arc::new(Barrier::new(readers + 1));
    let started = std::thread::scope(|scope| {
        let workers = (0..readers)
            .map(|reader| {
                let barrier = Arc::clone(&barrier);
                let cache = &cache;
                let keys = &keys;
                scope.spawn(move || {
                    let mut batch = Vec::with_capacity(BATCH);
                    barrier.wait();
                    for read in 0..(READS_PER_THREAD / BATCH) {
                        batch.clear();
                        let offset = (read * BATCH + reader) % keys.len();
                        for position in 0..BATCH {
                            batch.push(keys[(offset + position) % keys.len()].clone());
                        }
                        let values = cache.get_shared_batch(&batch).unwrap();
                        black_box(values);
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
    (readers * READS_PER_THREAD) as f64 / started.elapsed().as_secs_f64()
}

fn owned_batch_read_qps(readers: usize) -> f64 {
    let (_directory, cache, keys) = cache();
    let barrier = Arc::new(Barrier::new(readers + 1));
    let started = std::thread::scope(|scope| {
        let workers = (0..readers)
            .map(|reader| {
                let barrier = Arc::clone(&barrier);
                let cache = &cache;
                let keys = &keys;
                scope.spawn(move || {
                    let mut batch = Vec::with_capacity(BATCH);
                    barrier.wait();
                    for read in 0..(READS_PER_THREAD / BATCH) {
                        batch.clear();
                        let offset = (read * BATCH + reader) % keys.len();
                        for position in 0..BATCH {
                            batch.push(keys[(offset + position) % keys.len()].clone());
                        }
                        let values = cache.get_batch(&batch).unwrap();
                        black_box(values);
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
    (readers * READS_PER_THREAD) as f64 / started.elapsed().as_secs_f64()
}

fn main() {
    println!("readers,owned_read_qps,shared_read_qps,owned_batch_item_qps,shared_batch_item_qps");
    for readers in [1, 2, 4, 8] {
        println!(
            "{readers},{:.0},{:.0},{:.0},{:.0}",
            owned_read_qps(readers),
            shared_read_qps(readers),
            owned_batch_read_qps(readers),
            batch_read_qps(readers)
        );
    }
}
