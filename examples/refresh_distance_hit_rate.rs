// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What does `lru_refresh_distance` cost in hit rate?
//!
//! The setting decides how stale the tier access orders are allowed to get: an
//! entry read again within that many accesses keeps its place rather than
//! being moved to the back. Zero — the default — moves it on every hit, which
//! keeps the order exact and makes every read take the cache exclusively.
//!
//! Raising it is a throughput win. The question this answers is what it costs
//! in cache quality, because a staler order means victim selection can evict
//! something it would otherwise have kept.
//!
//! Throughput benchmarks cannot answer that. This one holds the workload fixed
//! and reports the **hit rate** at each setting, which is the number that
//! decides whether a non-zero default is free or paid for.
//!
//! The access pattern is skewed rather than uniform, because a uniform one has
//! no ordering for the policy to get wrong: every key is equally valuable and
//! any victim is as good as any other. The skew is deterministic, so runs are
//! comparable.
//!
//! ```text
//! cargo run --release --no-default-features --example refresh_distance_hit_rate
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
/// The key space. The cache holds a quarter of it, so eviction decides the
/// hit rate rather than capacity alone.
const KEY_SPACE: usize = 16_384;
const RESIDENT: usize = KEY_SPACE / 4;
const ACCESSES: usize = 400_000;
/// Threads for the read-only phase that follows the warm-up.
const READ_THREADS: usize = 8;
const READS_PER_THREAD: usize = 40_000;

/// A deterministic skewed index: most draws land in the low keys.
///
/// `state` is advanced by a plain LCG, and the result is cubed as a fraction
/// so that the distribution is heavily weighted to the front of the key space.
/// The crate has no RNG dependency and this needs to be reproducible anyway.
fn skewed_index(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let unit = ((*state >> 11) as f64) / ((1u64 << 53) as f64);
    let skewed = unit * unit * unit;
    ((skewed * KEY_SPACE as f64) as usize).min(KEY_SPACE - 1)
}

fn main() {
    println!(
        "{KEY_SPACE} keys, room for {RESIDENT}, {ACCESSES} skewed accesses, \
         {VALUE_BYTES}-byte values\n"
    );
    println!(
        "{:<20}{:>12}{:>12}{:>12}{:>16}",
        "refresh distance", "hits", "misses", "hit rate", "read Mops/s"
    );

    for distance in [
        Duration::ZERO,
        Duration::from_millis(5),
        Duration::from_millis(50),
        Duration::from_millis(500),
        Duration::from_secs(60),
    ] {
        let cache =
            MultiLayerCache::try_with_options(CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0))
                .expect("cache");
        cache.set_lru_refresh_time(distance);

        // Same sequence at every setting.
        let mut state = 0x2545_F491_4F6C_DD1D;
        for _ in 0..ACCESSES {
            let index = skewed_index(&mut state);
            let key = CacheKey::string(0, &format!("zipf-{index:06}"));
            if cache.get(&key).expect("get").is_none() {
                cache.put(key, vec![b'v'; VALUE_BYTES]).expect("put");
            }
        }

        let stats = cache.stats();
        let total = stats.memory_hits + stats.misses;
        let rate = if total == 0 {
            0.0
        } else {
            stats.memory_hits as f64 / total as f64
        };
        // Now read the warmed cache concurrently, same skew, no writes, so
        // this measures the read path rather than admission.
        let cache = Arc::new(cache);
        let workers = (0..READ_THREADS)
            .map(|worker| {
                let cache = Arc::clone(&cache);
                std::thread::spawn(move || {
                    let mut state = 0x9E37_79B9_7F4A_7C15 ^ (worker as u64);
                    let started = Instant::now();
                    for _ in 0..READS_PER_THREAD {
                        let index = skewed_index(&mut state);
                        let _ = cache
                            .get(&CacheKey::string(0, &format!("zipf-{index:06}")))
                            .expect("get");
                    }
                    READS_PER_THREAD as f64 / started.elapsed().as_secs_f64()
                })
            })
            .collect::<Vec<_>>();
        let throughput = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .sum::<f64>();

        let label = if distance.is_zero() {
            "0 (always moves)".to_string()
        } else {
            format!("{distance:?}")
        };
        println!(
            "{label:<20}{:>12}{:>12}{:>11.2}%{:>16.4}",
            stats.memory_hits,
            stats.misses,
            rate * 100.0,
            throughput / 1e6
        );
    }

    println!(
        "\nThe setting worth having is the one where read Mops/s has risen and\n\
         hit rate has not yet fallen. Hit rate is exact; throughput is one run\n\
         on a shared machine, so read it for shape rather than precision."
    );
}
