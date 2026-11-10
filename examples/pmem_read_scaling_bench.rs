// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! PMEM-resident shared-read throughput when the memory tier cannot admit.
//!
//! Every read remains a PMEM hit and exercises hit accounting plus the refill
//! decision. Keys are built before timing so formatting does not hide cache
//! lookup and synchronization costs.

use matrixcache::{CacheDataPlacement, CacheKey, CacheOptions, CacheReadTier, MultiLayerCache};
use std::sync::{Arc, Barrier};
use std::time::Instant;

const ENTRIES: usize = 4_096;
const VALUE_BYTES: usize = 128;
const READS_PER_THREAD: usize = 50_000;
const REPEATS: usize = 5;
const BATCH: usize = 64;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|left, right| left.partial_cmp(right).expect("finite throughput"));
    samples[samples.len() / 2]
}

fn throughput(cache: &Arc<MultiLayerCache>, keys: &Arc<Vec<CacheKey>>, threads: usize) -> f64 {
    let start = Arc::new(Barrier::new(threads + 1));
    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(cache);
            let keys = Arc::clone(keys);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                let mut bytes = 0_usize;
                for read in 0..READS_PER_THREAD {
                    let slot = (read.wrapping_mul(7_919) + worker.wrapping_mul(1_009)) % keys.len();
                    bytes += cache
                        .get_shared(&keys[slot])
                        .expect("PMEM read")
                        .expect("resident value")
                        .len();
                }
                assert_eq!(bytes, READS_PER_THREAD * VALUE_BYTES);
            })
        })
        .collect::<Vec<_>>();
    start.wait();
    let started = Instant::now();
    for worker in workers {
        worker.join().expect("reader");
    }
    (threads * READS_PER_THREAD) as f64 / started.elapsed().as_secs_f64()
}

fn batch_throughput(
    cache: &Arc<MultiLayerCache>,
    keys: &Arc<Vec<CacheKey>>,
    threads: usize,
) -> f64 {
    let batches_per_thread = READS_PER_THREAD / BATCH;
    let start = Arc::new(Barrier::new(threads + 1));
    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(cache);
            let keys = Arc::clone(keys);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                let mut bytes = 0_usize;
                let mut batch = Vec::with_capacity(BATCH);
                for read in 0..batches_per_thread {
                    batch.clear();
                    for offset in 0..BATCH {
                        let slot = (read.wrapping_mul(BATCH * 7_919)
                            + offset.wrapping_mul(1_009)
                            + worker.wrapping_mul(101))
                            % keys.len();
                        batch.push(keys[slot].clone());
                    }
                    let values = cache.get_shared_batch(&batch).expect("PMEM batch read");
                    bytes += values
                        .iter()
                        .map(|value| value.as_ref().expect("resident value").len())
                        .sum::<usize>();
                }
                assert_eq!(bytes, batches_per_thread * BATCH * VALUE_BYTES);
            })
        })
        .collect::<Vec<_>>();
    start.wait();
    let started = Instant::now();
    for worker in workers {
        worker.join().expect("batch reader");
    }
    (threads * batches_per_thread * BATCH) as f64 / started.elapsed().as_secs_f64()
}

fn main() {
    let directory = tempfile::tempdir().expect("temporary PMEM directory");
    let cache = Arc::new(MultiLayerCache::with_options(
        CacheOptions::new(0, ENTRIES * VALUE_BYTES * 2, 0)
            .with_pmem_paths([directory.path().join("pmem")]),
    ));
    cache.start().expect("start cache");
    let mut policy = cache.production_tiering_policy();
    policy.data_placement = CacheDataPlacement::SideBySide;
    policy.data_placement_threshold_bytes = 0;
    cache.update_production_tiering_policy(policy);

    let keys = Arc::new(
        (0..ENTRIES)
            .map(|index| CacheKey::string(0, &format!("pmem-read-{index:05}")))
            .collect::<Vec<_>>(),
    );
    for key in keys.iter() {
        cache
            .put_shared(key.clone(), Arc::<[u8]>::from(vec![b'v'; VALUE_BYTES]))
            .expect("PMEM insert");
        assert_eq!(cache.peek_tier(key), Some(CacheReadTier::Pmem));
    }

    println!(
        "{ENTRIES} PMEM values of {VALUE_BYTES} bytes, {READS_PER_THREAD} reads/thread, median of {REPEATS}"
    );
    println!(
        "{:<10}{:>18}{:>18}",
        "threads", "single Mops/s", "batch Mops/s"
    );
    for threads in [1_usize, 2, 4, 8] {
        let reads = median(
            (0..REPEATS)
                .map(|_| throughput(&cache, &keys, threads))
                .collect(),
        );
        let batch_reads = median(
            (0..REPEATS)
                .map(|_| batch_throughput(&cache, &keys, threads))
                .collect(),
        );
        println!(
            "{threads:<10}{:>18.4}{:>18.4}",
            reads / 1e6,
            batch_reads / 1e6
        );
    }
}
