// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What do sharded batch writes and pinned control batches cost?
//!
//! `batch_read_cost` keeps an eye on the resident read path. This companion
//! benchmark measures the write/control side that TemporalStore uses when it
//! warms, pins, releases, and rewrites groups of block entries. It prints
//! colocated batches next to fanout batches so a regression in the small/local
//! fast path is visible without a large soak run.
//!
//! ```text
//! cargo run --release --no-default-features --example batch_write_cost
//! ```

use matrixcache::{CacheKey, CacheOptions, ShardedMultiLayerCache};
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
const SHARDS: usize = 16;
const SMALL_BATCH: usize = 16;
const LARGE_BATCH: usize = 512;
const PASSES: usize = 9;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

fn ns_per_entry(elapsed: Duration, entries: usize) -> f64 {
    elapsed.as_nanos() as f64 / entries.max(1) as f64
}

fn bench_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("matrixcache-batch-write-cost")
}

fn fanout_keys(pass: usize, count: usize) -> Vec<CacheKey> {
    (0..count)
        .map(|index| CacheKey::string(index as u64, &format!("write-{pass:02}-{index:05}")))
        .collect()
}

fn colocated_page_keys(cache: &ShardedMultiLayerCache, pass: usize, count: usize) -> Vec<CacheKey> {
    let target = cache.shard_index_for_key(&CacheKey::page_with_slot(
        7,
        900_000 + pass as u64,
        0,
        VALUE_BYTES as u64,
        Some(31),
    ));
    (0..)
        .map(|index| {
            CacheKey::page_with_slot(
                7,
                900_000 + (pass * 10_000 + index) as u64,
                0,
                VALUE_BYTES as u64,
                Some(31),
            )
        })
        .filter(|key| cache.shard_index_for_key(key) == target)
        .take(count)
        .collect()
}

fn entries(keys: &[CacheKey], byte: u8) -> Vec<(CacheKey, Vec<u8>)> {
    keys.iter()
        .cloned()
        .map(|key| (key, vec![byte; VALUE_BYTES]))
        .collect()
}

fn sized_entries(keys: &[CacheKey], byte: u8) -> Vec<(CacheKey, Vec<u8>, usize)> {
    keys.iter()
        .cloned()
        .map(|key| (key, vec![byte; VALUE_BYTES], VALUE_BYTES))
        .collect()
}

fn bench_put(cache: &ShardedMultiLayerCache, local: bool, batch: usize) -> f64 {
    median(
        (0..PASSES)
            .map(|pass| {
                let keys = if local {
                    colocated_page_keys(cache, pass, batch)
                } else {
                    fanout_keys(pass, batch)
                };
                let started = Instant::now();
                let inserted = cache
                    .put_batch(entries(&keys, b'w' + (pass % 7) as u8))
                    .expect("put_batch");
                assert_eq!(inserted, keys.len());
                ns_per_entry(started.elapsed(), keys.len())
            })
            .collect(),
    )
}

fn bench_insert_pinned_release(cache: &ShardedMultiLayerCache, local: bool, batch: usize) -> f64 {
    median(
        (0..PASSES)
            .map(|pass| {
                let keys = if local {
                    colocated_page_keys(cache, 100 + pass, batch)
                } else {
                    fanout_keys(100 + pass, batch)
                };
                let started = Instant::now();
                let handles = cache
                    .insert_pinned_batch_sized(sized_entries(&keys, b'p' + (pass % 7) as u8))
                    .expect("insert_pinned_batch_sized");
                assert!(handles.iter().all(Option::is_some));
                let released = cache.release_batch(handles.into_iter().flatten().collect());
                assert_eq!(released, keys.len());
                ns_per_entry(started.elapsed(), keys.len())
            })
            .collect(),
    )
}

fn bench_acquire_release(cache: &ShardedMultiLayerCache, local: bool, batch: usize) -> f64 {
    let keys = if local {
        colocated_page_keys(cache, 200, batch)
    } else {
        fanout_keys(200, batch)
    };
    assert_eq!(
        cache.put_batch(entries(&keys, b'a')).expect("seed keys"),
        keys.len()
    );
    median(
        (0..PASSES)
            .map(|_| {
                let started = Instant::now();
                let handles = cache.acquire_batch(&keys).expect("acquire_batch");
                assert!(handles.iter().all(Option::is_some));
                let released = cache.release_batch(handles.into_iter().flatten().collect());
                assert_eq!(released, keys.len());
                ns_per_entry(started.elapsed(), keys.len())
            })
            .collect(),
    )
}

fn main() {
    let dir = bench_dir();
    let _ = std::fs::remove_dir_all(&dir);
    let cache = ShardedMultiLayerCache::try_with_options(
        CacheOptions::new(
            LARGE_BATCH * VALUE_BYTES * PASSES * 32,
            0,
            LARGE_BATCH * VALUE_BYTES * 64,
        )
        .with_ssd_paths(vec![dir.clone()]),
        SHARDS,
    )
    .expect("sharded cache");

    println!("{SHARDS} shards, value={VALUE_BYTES} bytes, median of {PASSES} passes, ns/entry\n");
    println!(
        "{:<26}{:>12}{:>14}{:>14}",
        "operation", "batch", "colocated", "fanout"
    );
    for batch in [SMALL_BATCH, LARGE_BATCH] {
        println!(
            "{:<26}{batch:>12}{:>14.1}{:>14.1}",
            "put_batch",
            bench_put(&cache, true, batch),
            bench_put(&cache, false, batch)
        );
        println!(
            "{:<26}{batch:>12}{:>14.1}{:>14.1}",
            "insert_pinned+release",
            bench_insert_pinned_release(&cache, true, batch),
            bench_insert_pinned_release(&cache, false, batch)
        );
        println!(
            "{:<26}{batch:>12}{:>14.1}{:>14.1}",
            "acquire+release",
            bench_acquire_release(&cache, true, batch),
            bench_acquire_release(&cache, false, batch)
        );
    }

    let stats = cache.stats();
    println!();
    println!(
        "sharded batch stats: local={} fanout={} fanout_shards={} latency_samples={} latency_max_us={}",
        stats.sharded_batch_local_operations,
        stats.sharded_batch_fanout_operations,
        stats.sharded_batch_fanout_shards,
        stats.sharded_batch_latency_samples,
        stats.sharded_batch_latency_max_micros
    );

    let _ = std::fs::remove_dir_all(&dir);
}
