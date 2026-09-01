// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Cost of choosing an eviction victim, as the resident set grows.
//!
//! A cache at capacity evicts on almost every write, so whatever victim
//! selection costs is paid per write for the life of the cache. The thing to
//! watch is whether that cost stays put as the cache fills: a selector that
//! inspects every resident entry shows up here as a per-write cost that climbs
//! with the entry count, while one that inspects a bounded number of
//! candidates shows up as a flat line.
//!
//! Two numbers are reported per size. The first is wall time per write, which
//! is what a caller feels. The second is the number of candidate groups the
//! selector formed per evicted entry, which the cache already counts; it is
//! immune to load on the machine and is the number that says whether the
//! algorithm changed or only the weather did.
//!
//!
//! The hit-rate table reports promotions beside the hit rate, because the hit
//! rate follows them: a promotion is what keeps the access order carrying
//! recency, and the refresh window is what decides how often one happens. If
//! the hit rate here moves and the promotion count moved with it, the cause is
//! the window rather than anything about eviction.
//!
//! ```text
//! cargo run --release --no-default-features --example eviction_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::Instant;

const VALUE_BYTES: usize = 64;
/// Room for the value plus its per-entry overhead, so `entries` really fit.
const SLOT_BYTES: usize = VALUE_BYTES;

fn bench_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("matrixcache-eviction-{name}"))
}

fn key(index: usize) -> CacheKey {
    CacheKey::string(0, &format!("eviction-key-{index:010}"))
}

/// Spread successive steps over `len` slots so reads are not in insertion
/// order, which would flatter any policy that evicts from one end.
fn scattered(index: usize, len: usize) -> usize {
    index.wrapping_mul(2_654_435_761) % len.max(1)
}

/// Fill to capacity, then keep writing so every write evicts.
fn steady_state(entries: usize) -> (f64, f64) {
    let dir = bench_dir(&format!("steady-{entries}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(entries * SLOT_BYTES, 0, 0).with_ssd_paths(vec![dir.clone()]),
    );
    cache.start().expect("start cache");

    let value = vec![b'v'; VALUE_BYTES];
    // Fill to capacity. Keys are built up front so the timed region below
    // measures the cache rather than key formatting.
    for index in 0..entries {
        cache.put(key(index), value.clone()).expect("put");
    }

    let writes = 2_000usize;
    let fresh: Vec<CacheKey> = (entries..entries + writes).map(key).collect();

    let before = cache.stats();
    let started = Instant::now();
    for k in &fresh {
        cache.put(k.clone(), value.clone()).expect("put");
    }
    let elapsed = started.elapsed();
    let after = cache.stats();

    let evictions = after
        .memory_evictions
        .saturating_sub(before.memory_evictions);
    let groups = after
        .eviction_sampled_groups
        .saturating_sub(before.eviction_sampled_groups);

    let ns_per_write = elapsed.as_nanos() as f64 / writes as f64;
    let groups_per_eviction = if evictions == 0 {
        0.0
    } else {
        groups as f64 / evictions as f64
    };

    cache.stop();
    let _ = std::fs::remove_dir_all(&dir);
    (ns_per_write, groups_per_eviction)
}

/// Hit rate under a skewed read-through workload.
///
/// Bounding the candidate search only pays off if it still throws out the
/// right entries. This drives a working set several times larger than the
/// cache, with most reads landing on a small hot subset, and reports the share
/// of reads the cache served. A selector that evicts hot entries shows up here
/// as a hit rate below what the hot subset alone would guarantee.
fn hit_rate(entries: usize) -> (f64, u64) {
    let dir = bench_dir(&format!("hitrate-{entries}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(entries * SLOT_BYTES, 0, 0).with_ssd_paths(vec![dir.clone()]),
    );
    cache.start().expect("start cache");

    let value = vec![b'v'; VALUE_BYTES];
    // Four times as many keys as fit, with a hot subset that is half the
    // cache, so a selector that protects hot entries can hold all of them.
    let universe = entries * 4;
    let hot = entries / 2;
    let keys: Vec<CacheKey> = (0..universe).map(key).collect();

    let reads = 400_000usize;
    let mut hits = 0usize;
    for step in 0..reads {
        // Four reads in five land in the hot subset; the rest sweep the
        // universe and are the pressure that forces eviction.
        let slot = if step % 5 < 4 {
            scattered(step, hot)
        } else {
            scattered(step, universe)
        };
        let k = &keys[slot];
        if cache.get(k).expect("get").is_some() {
            hits += 1;
        } else {
            cache.put(k.clone(), value.clone()).expect("put");
        }
    }

    let refreshes = cache.stats().access_order_refreshes;
    cache.stop();
    let _ = std::fs::remove_dir_all(&dir);
    (hits as f64 * 100.0 / reads as f64, refreshes)
}

fn main() {
    println!("steady-state write cost with the cache at capacity");
    println!(
        "{:>10}  {:>14}  {:>22}",
        "entries", "ns/write", "groups/eviction"
    );
    for entries in [1_024usize, 2_048, 4_096, 8_192, 16_384, 32_768] {
        let (ns, groups) = steady_state(entries);
        println!("{entries:>10}  {ns:>14.0}  {groups:>22.1}");
    }

    println!();
    println!("hit rate, working set 4x the cache, 80% of reads on a hot half-cache");
    println!(
        "{:>10}  {:>14}  {:>14}",
        "entries", "hit rate %", "promotions"
    );
    for entries in [1_024usize, 4_096, 16_384] {
        let (rate, refreshes) = hit_rate(entries);
        println!("{entries:>10}  {rate:>14.2}  {refreshes:>14}");
    }
}
