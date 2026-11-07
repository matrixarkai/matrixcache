// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Measures large sharded write-back batches after the cache is warm.
//!
//! ```text
//! cargo run --release --no-default-features --example sharded_writeback_cost
//! ```

use matrixcache::{CacheKey, CacheOptions, ShardedMultiLayerCache};
use std::time::Instant;

const SHARDS: usize = 16;
const ENTRIES: usize = 512;
const VALUE_BYTES: usize = 64;
const PASSES: usize = 9;

fn entries(prefix: &str, pass: usize) -> Vec<(CacheKey, Vec<u8>)> {
    (0..ENTRIES)
        .map(|index| {
            (
                CacheKey::string(
                    (index % SHARDS) as u64,
                    &format!("{prefix}-{pass:02}-{index:04}"),
                ),
                vec![b'w' + (pass % 7) as u8; VALUE_BYTES],
            )
        })
        .collect()
}

fn median_ns_per_entry(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|left, right| left.partial_cmp(right).expect("finite sample"));
    samples[samples.len() / 2]
}

fn cache(queue_limit: usize) -> ShardedMultiLayerCache {
    let cache = ShardedMultiLayerCache::with_options(CacheOptions::new(64 << 20, 0, 0), SHARDS);
    cache.set_async_writeback_queue_limit_for_test(queue_limit);
    cache
}

fn enqueue_cost() -> f64 {
    let cache = cache(ENTRIES * 4);
    cache
        .enqueue_async_writeback_batch(entries("warm-enqueue", 0))
        .expect("warm enqueue");
    cache.flush_async_writeback().expect("warm flush");

    median_ns_per_entry(
        (0..PASSES)
            .map(|pass| {
                let started = Instant::now();
                cache
                    .enqueue_async_writeback_batch(entries("enqueue", pass))
                    .expect("enqueue");
                let elapsed = started.elapsed().as_nanos() as f64 / ENTRIES as f64;
                cache.flush_async_writeback().expect("flush");
                elapsed
            })
            .collect(),
    )
}

fn drain_cost() -> f64 {
    let cache = cache(ENTRIES * 4);
    cache
        .enqueue_async_writeback_batch(entries("warm-drain", 0))
        .expect("warm enqueue");
    cache.flush_async_writeback().expect("warm flush");

    median_ns_per_entry(
        (0..PASSES)
            .map(|pass| {
                cache
                    .enqueue_async_writeback_batch(entries("drain", pass))
                    .expect("enqueue");
                let started = Instant::now();
                cache.flush_async_writeback().expect("flush");
                started.elapsed().as_nanos() as f64 / ENTRIES as f64
            })
            .collect(),
    )
}

fn fallback_cost() -> f64 {
    let cache = cache(0);
    cache
        .submit_async_writeback_batch_or_write_through(entries("warm-fallback", 0))
        .expect("warm fallback");

    median_ns_per_entry(
        (0..PASSES)
            .map(|pass| {
                let started = Instant::now();
                let report = cache
                    .submit_async_writeback_batch_or_write_through(entries("fallback", pass))
                    .expect("fallback");
                assert_eq!(report.queued, 0);
                assert_eq!(report.write_through, ENTRIES);
                started.elapsed().as_nanos() as f64 / ENTRIES as f64
            })
            .collect(),
    )
}

fn main() {
    println!("operation,entries,shards,median_ns_per_entry");
    println!("enqueue,{ENTRIES},{SHARDS},{:.1}", enqueue_cost());
    println!("flush,{ENTRIES},{SHARDS},{:.1}", drain_cost());
    println!("write_through,{ENTRIES},{SHARDS},{:.1}", fallback_cost());
}
