// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Cold SSD batch promotion while unrelated memory writes are active.
//! The resident tier fits every value, so second-pass disk hits measure missed
//! promotion rather than capacity eviction. Timings include result validation.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

const KEYS: usize = 2048;
const VALUE_BYTES: usize = 4096;
const BATCH: usize = 32;

fn run(readers: usize, writer: bool) {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(32 << 20, 0, 64 << 20)
            .with_ssd_paths([directory.path().join("ssd")])
            .with_ssd_instance_only(true),
    );
    let keys: Vec<_> = (0..KEYS)
        .map(|i| CacheKey::string(7, &format!("cold-{i:05}")))
        .collect();
    for (index, key) in keys.iter().enumerate() {
        cache
            .put(key.clone(), vec![(index % 251) as u8; VALUE_BYTES])
            .unwrap();
    }
    cache.set_ssd_instance_only(false);
    let before = cache.stats();
    let barrier = Arc::new(Barrier::new(readers + usize::from(writer) + 1));
    let done = AtomicBool::new(false);
    let (writes, elapsed) = std::thread::scope(|scope| {
        let writer_thread = writer.then(|| {
            scope.spawn(|| {
                let key = CacheKey::string(7, "unrelated-writer");
                let mut writes = 0u64;
                barrier.wait();
                while !done.load(Ordering::Acquire) {
                    cache.put_memory_only(key.clone(), vec![42; 64]);
                    writes += 1;
                    std::thread::sleep(Duration::from_micros(50));
                }
                writes
            })
        });
        let threads: Vec<_> = (0..readers)
            .map(|reader| {
                let cache = &cache;
                let keys = &keys;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    for batch_index in (reader..KEYS / BATCH).step_by(readers) {
                        let offset = batch_index * BATCH;
                        let values = cache
                            .get_shared_batch(&keys[offset..offset + BATCH])
                            .unwrap();
                        for (position, value) in values.iter().enumerate() {
                            let value = value.as_ref().unwrap();
                            assert_eq!(value.len(), VALUE_BYTES);
                            assert!(value
                                .iter()
                                .all(|byte| *byte == ((offset + position) % 251) as u8));
                        }
                    }
                })
            })
            .collect();
        let started = Instant::now();
        barrier.wait();
        for thread in threads {
            thread.join().unwrap();
        }
        let elapsed = started.elapsed();
        done.store(true, Ordering::Release);
        let writes = writer_thread
            .map(|thread| thread.join().unwrap())
            .unwrap_or(0);
        (writes, elapsed)
    });
    let after = cache.stats();
    assert_eq!(after.refill_failures - before.refill_failures, 0);
    assert_eq!(after.memory_evictions - before.memory_evictions, 0);
    for batch in keys.chunks(BATCH) {
        assert!(cache
            .get_shared_batch(batch)
            .unwrap()
            .iter()
            .all(Option::is_some));
    }
    let second = cache.stats();
    if !writer {
        assert_eq!(after.memory_fills - before.memory_fills, KEYS as u64);
        assert_eq!(second.disk_hits - after.disk_hits, 0);
    }
    println!(
        "{readers},{writer},{writes},{},{},{:.0}",
        after.memory_fills - before.memory_fills - writes,
        second.disk_hits - after.disk_hits,
        KEYS as f64 / elapsed.as_secs_f64()
    );
}

fn main() {
    println!("readers,writer,writes,first_pass_refills,second_pass_disk_hits,read_qps");
    for _ in 0..3 {
        for readers in [1, 4] {
            for writer in [false, true] {
                run(readers, writer);
            }
        }
    }
}
