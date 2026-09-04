// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What does a batch read cost per key when every key hits memory?
//!
//! `get_batch` serves from the memory tier when it can, so a batch of resident
//! keys should cost about what the same number of single gets costs and no
//! more. `get_batch_no_promotion` is the scan path: it answers without moving
//! entries between tiers or updating replacement metadata. The pinned
//! no-promotion column measures the same scan shape when the caller wants
//! zero-copy handles and releases them in one batch. Anything the regular path
//! does *besides* answering -- bookkeeping whose result is not returned -- shows
//! up here as a per-key cost that has nothing to do with the value.
//!
//! The cache is given an SSD path so that entries are present in the disk
//! index as well as in memory. That combination is the one worth measuring:
//! the read is still served from memory, so the SSD is not in the timing, but
//! any per-key work conditioned on disk-index membership is.
//!
//! Two batch sizes, because a per-key cost scales with the batch and a
//! fixed one does not.
//!
//! **What this cannot do:** resolve small differences between two builds. The
//! baseline here is microseconds per key and this machine's load makes a
//! cross-process comparison swing by more than a factor of three run to run.
//! Comparing two binaries with it produced a confident-looking 2.12x for a
//! change whose components measure ~400ns. Use it for the shape of the cost
//! and for the ratio between the two batch sizes, which are measured in one
//! process; measure a specific removal by measuring that removal.
//!
//! ```text
//! cargo run --release --no-default-features --example batch_read_cost
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache, ShardedMultiLayerCache};
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 4096;
const PASSES: usize = 9;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

fn bench_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("matrixcache-batch-read-cost")
}

fn main() {
    let dir = bench_dir();
    let _ = std::fs::remove_dir_all(&dir);

    // Room for everything in memory, plus an SSD tier so the disk index is
    // populated for the same keys.
    let cache = MultiLayerCache::try_with_options(
        CacheOptions::new(RESIDENT * VALUE_BYTES * 8, 0, RESIDENT * VALUE_BYTES * 8)
            .with_ssd_paths(vec![dir.clone()]),
    )
    .expect("cache");
    let sharded_dir = dir.join("sharded");
    let sharded = ShardedMultiLayerCache::try_with_options(
        CacheOptions::new(RESIDENT * VALUE_BYTES * 8, 0, RESIDENT * VALUE_BYTES * 8)
            .with_ssd_paths(vec![sharded_dir]),
        16,
    )
    .expect("sharded cache");

    let keys: Vec<CacheKey> = (0..RESIDENT)
        .map(|i| CacheKey::string(0, &format!("batch-{i:05}")))
        .collect();
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("put");
        sharded
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("sharded put");
    }

    println!(
        "{RESIDENT} resident values of {VALUE_BYTES} bytes, all hitting memory, \
         median of {PASSES} passes\n"
    );
    println!(
        "{:<16}{:>14}{:>18}{:>22}{:>22}{:>28}",
        "batch size",
        "get_batch",
        "sharded_get",
        "get_batch_no_prom",
        "acquire_no_prom",
        "sharded_acquire_no_prom"
    );

    for batch in [64_usize, 1024] {
        let regular_ns = median(
            (0..PASSES)
                .map(|_| {
                    let started = Instant::now();
                    let mut served = 0_usize;
                    for chunk in keys.chunks(batch) {
                        let values = cache.get_batch(chunk).expect("get_batch");
                        served += values.iter().filter(|value| value.is_some()).count();
                    }
                    assert_eq!(served, RESIDENT, "every key should have hit memory");
                    started.elapsed().as_nanos() as f64 / RESIDENT as f64
                })
                .collect(),
        );
        let no_promotion_ns = median(
            (0..PASSES)
                .map(|_| {
                    let started = Instant::now();
                    let mut served = 0_usize;
                    for chunk in keys.chunks(batch) {
                        let values = cache
                            .get_batch_no_promotion(chunk)
                            .expect("get_batch_no_promotion");
                        served += values.iter().filter(|value| value.is_some()).count();
                    }
                    assert_eq!(served, RESIDENT, "every key should have hit memory");
                    started.elapsed().as_nanos() as f64 / RESIDENT as f64
                })
                .collect(),
        );
        let acquire_no_promotion_ns = median(
            (0..PASSES)
                .map(|_| {
                    let started = Instant::now();
                    let mut served = 0_usize;
                    for chunk in keys.chunks(batch) {
                        let handles = cache
                            .acquire_batch_no_promotion(chunk)
                            .expect("acquire_batch_no_promotion");
                        served += handles.iter().filter(|handle| handle.is_some()).count();
                        cache.release_batch(handles.into_iter().flatten().collect());
                    }
                    assert_eq!(served, RESIDENT, "every key should have hit memory");
                    started.elapsed().as_nanos() as f64 / RESIDENT as f64
                })
                .collect(),
        );
        let sharded_regular_ns = median(
            (0..PASSES)
                .map(|_| {
                    let started = Instant::now();
                    let mut served = 0_usize;
                    for chunk in keys.chunks(batch) {
                        let values = sharded.get_batch(chunk).expect("sharded_get_batch");
                        served += values.iter().filter(|value| value.is_some()).count();
                    }
                    assert_eq!(served, RESIDENT, "every key should have hit sharded memory");
                    started.elapsed().as_nanos() as f64 / RESIDENT as f64
                })
                .collect(),
        );
        let sharded_acquire_no_promotion_ns = median(
            (0..PASSES)
                .map(|_| {
                    let started = Instant::now();
                    let mut served = 0_usize;
                    for chunk in keys.chunks(batch) {
                        let handles = sharded
                            .acquire_batch_no_promotion(chunk)
                            .expect("sharded acquire_batch_no_promotion");
                        served += handles.iter().filter(|handle| handle.is_some()).count();
                        sharded.release_batch(handles.into_iter().flatten().collect());
                    }
                    assert_eq!(served, RESIDENT, "every key should have hit sharded memory");
                    started.elapsed().as_nanos() as f64 / RESIDENT as f64
                })
                .collect(),
        );
        println!(
            "{batch:<16}{regular_ns:>14.1}{sharded_regular_ns:>18.1}{no_promotion_ns:>22.1}{acquire_no_promotion_ns:>22.1}{sharded_acquire_no_promotion_ns:>28.1}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}
