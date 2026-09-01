// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What does skipping the LRU promotion on a repeat read save?
//!
//! A hit moves the entry to the back of **every** tier's access order, so
//! victim selection does not offer it up. Each move is a hash lookup and some
//! list surgery, and the move is the reason a read needs the cache lock
//! exclusively rather than shared.
//!
//! `set_lru_refresh_distance` skips the move for an entry read within the last
//! N accesses, which is already within N places of the newest end. CacheLib
//! makes the same trade with `lruRefreshTime`, stated in seconds; accesses are
//! the more direct statement, since what matters is where the entry sits in the
//! order rather than how long ago it was read.
//!
//! Two configurations, because the saving depends on how many orders actually
//! hold the entry:
//!
//! * **memory only** — the other two orders are empty and already skipped
//!   without any help, so this is close to the floor.
//! * **memory + ssd** — write-through leaves each entry in the disk order too,
//!   so a hit promotes it twice and there is twice as much to skip.
//!
//! The working set is deliberately smaller than the refresh distance, so every
//! read is a repeat inside the window — a hot working set, which is the case
//! the trade is aimed at. A working set larger than the distance promotes on
//! every read and saves nothing, which is the point: the knob only skips work
//! when the reads really are repeats.
//!
//! Both settings are measured **alternately in one process**, so drift on a
//! busy machine moves both and the ratio survives it.
//!
//! ```text
//! cargo run --release --no-default-features --example lru_refresh_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
const WORKING_SET: usize = 512;
const READS: usize = 20_000;
const ROUNDS: usize = 5;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

fn hot_keys() -> Vec<CacheKey> {
    (0..WORKING_SET)
        .map(|i| CacheKey::string(0, &format!("hot-{i:06}")))
        .collect()
}

fn fill(cache: &MultiLayerCache, keys: &[CacheKey]) {
    for key in keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("put");
    }
}

fn measure(label: &str, cache: &MultiLayerCache, keys: &[CacheKey]) {
    let read_pass = || {
        let started = Instant::now();
        for i in 0..READS {
            std::hint::black_box(cache.get(&keys[i % keys.len()]).expect("get"));
        }
        started.elapsed().as_nanos() as f64 / READS as f64
    };

    // Warm, so the first setting measured does not pay for everything after it.
    read_pass();

    let mut always = Vec::new();
    let mut throttled = Vec::new();
    for _ in 0..ROUNDS {
        cache.set_lru_refresh_time(Duration::ZERO);
        always.push(read_pass());
        cache.set_lru_refresh_time(Duration::from_secs(3_600));
        throttled.push(read_pass());
    }
    cache.set_lru_refresh_time(Duration::ZERO);

    let always = median(always);
    let throttled = median(throttled);
    println!(
        "{:<20}{:>12.1}{:>12.1}{:>10.2}x{:>9.0}%",
        label,
        always,
        throttled,
        always / throttled.max(f64::MIN_POSITIVE),
        100.0 * (always - throttled) / always.max(f64::MIN_POSITIVE)
    );
}

fn main() {
    println!("memory-tier hit, {WORKING_SET} hot keys, {READS} reads per pass, median of {ROUNDS} rounds\n");
    println!(
        "{:<20}{:>12}{:>12}{:>11}{:>10}",
        "populated orders", "always", "throttled", "speedup", "saved"
    );

    {
        let cache =
            MultiLayerCache::try_with_options(CacheOptions::new(1 << 24, 0, 0)).expect("cache");
        let keys = hot_keys();
        fill(&cache, &keys);
        measure("memory only", &cache, &keys);
    }

    {
        let dir = std::env::temp_dir().join("matrixcache-lru-refresh");
        let _ = std::fs::remove_dir_all(&dir);
        let options = CacheOptions::new(1 << 24, 0, 1 << 24).with_ssd_paths([dir.clone()]);
        let cache = MultiLayerCache::try_with_options(options).expect("cache");
        let keys = hot_keys();
        fill(&cache, &keys);
        measure("memory + ssd", &cache, &keys);
        drop(cache);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
