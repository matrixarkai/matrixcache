// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Is the read path's bookkeeping what stops it scaling?
//!
//! Both `get` and `get_no_promotion` serve a memory hit under the shared lock
//! and copy the value out. The difference is everything `get` does afterwards:
//!
//! * `access_epoch.fetch_add` — **one process-wide counter, bumped by every
//!   hit on every thread**;
//! * the entry's `hits`, `hotness` and `last_access_epoch`;
//! * `memory_hits`, and a bucket in each of two latency histograms — also
//!   process-wide.
//!
//! Those are atomics rather than locks, so they do not serialise. But an
//! atomic read-modify-write on a line several cores are writing costs far more
//! than one nobody else wants, and four of these are on lines *every* thread
//! writes on *every* read. That is a different bottleneck from the lock, and
//! removing the lock is what exposed it.
//!
//! `get_no_promotion` is the same read without any of that, and it is public,
//! so the comparison needs no instrumentation: same cache, same keys, same
//! process, one difference.
//!
//! But a hit does not always stay on the shared path. When it has to move its
//! entry in the access orders it escalates to the exclusive lock, and whether
//! it does is decided by `lru_refresh_distance` against the gap since the
//! entry was last read. Below the reuse gap, every hit escalates; above it,
//! almost none do. **Those are two different bottlenecks and a benchmark that
//! fixes the distance measures whichever one it happened to select.**
//!
//! That is not hypothetical — the first version of this bench ran only at the
//! default of 512, over a key space whose reuse gap is 8192, so every hit
//! escalated. It reported `get` at 0.74x against `get_no_promotion` at 6.49x
//! and looked like proof that the atomics were the wall. It was measuring the
//! lock.
//!
//! So the distance is swept, and the two paths are reported separately with
//! `get_no_promotion` as the ceiling for both:
//!
//! * **below the reuse gap** — every hit escalates; this is the exclusive path
//! * **above it** — hits stay shared; this is the atomics alone
//!
//! A gap between `get` and `get_no_promotion` that persists *above* the reuse
//! gap is the bookkeeping, and sharding those counters would be the fix. A gap
//! that closes above it means the atomics are not the limit and only
//! escalation is.
//!
//! ```text
//! cargo run --release --no-default-features --example bookkeeping_scaling_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
const KEY_SPACE: usize = 8192;
const READS_PER_THREAD: usize = 50_000;
const REPEATS: usize = 7;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

/// Aggregate reads per second, with bookkeeping (`get`) or without
/// (`get_no_promotion`).
fn throughput(cache: &Arc<MultiLayerCache>, threads: usize, bookkeeping: bool) -> f64 {
    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(cache);
            std::thread::spawn(move || {
                // Keys are built before timing: formatting them is
                // embarrassingly parallel and would dilute exactly the
                // serialisation this is looking for.
                let mut state = 0x2545_F491_4F6C_DD1D ^ ((worker as u64) << 32);
                let keys = (0..READS_PER_THREAD)
                    .map(|_| {
                        state = state
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1);
                        let index = ((state >> 33) as usize) % KEY_SPACE;
                        CacheKey::string(0, &format!("bk-{index:05}"))
                    })
                    .collect::<Vec<_>>();
                let started = Instant::now();
                if bookkeeping {
                    for key in &keys {
                        let _ = cache.get(key).expect("get");
                    }
                } else {
                    for key in &keys {
                        let _ = cache.get_no_promotion(key).expect("get");
                    }
                }
                READS_PER_THREAD as f64 / started.elapsed().as_secs_f64()
            })
        })
        .collect::<Vec<_>>();
    workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .sum()
}

fn main() {
    let cache = Arc::new(
        MultiLayerCache::try_with_options(CacheOptions::new(KEY_SPACE * VALUE_BYTES * 4, 0, 0))
            .expect("cache"),
    );
    for index in 0..KEY_SPACE {
        cache
            .put(
                CacheKey::string(0, &format!("bk-{index:05}")),
                vec![b'v'; VALUE_BYTES],
            )
            .expect("put");
    }

    println!(
        "{KEY_SPACE} resident values of {VALUE_BYTES} bytes, {READS_PER_THREAD} reads/thread, \
         median of {REPEATS}"
    );
    // Reads are uniform over KEY_SPACE, so an entry comes back around after
    // roughly KEY_SPACE accesses. A distance below that escalates every hit;
    // one comfortably above it escalates almost none.
    println!("reuse gap is about {KEY_SPACE} accesses\n");

    let plain: Vec<f64> = [1_usize, 2, 4, 8]
        .iter()
        .map(|threads| {
            median(
                (0..REPEATS)
                    .map(|_| throughput(&cache, *threads, false))
                    .collect(),
            )
        })
        .collect();

    for distance in [Duration::from_micros(1), Duration::from_secs(3_600)] {
        let label = if distance < Duration::from_millis(1) {
            "below the reuse time: every hit escalates"
        } else {
            "above the reuse time: hits stay on the shared path"
        };
        cache.set_lru_refresh_time(distance);
        println!("window {distance:?} -- {label}");
        println!(
            "{:<10}{:>14}{:>10}{:>16}{:>10}",
            "threads", "get Mops/s", "scaling", "no-promo Mops/s", "scaling"
        );
        let mut base = 0.0_f64;
        for (slot, threads) in [1_usize, 2, 4, 8].iter().enumerate() {
            let with = median(
                (0..REPEATS)
                    .map(|_| throughput(&cache, *threads, true))
                    .collect(),
            );
            if *threads == 1 {
                base = with;
            }
            println!(
                "{threads:<10}{:>14.4}{:>9.2}x{:>16.4}{:>9.2}x",
                with / 1e6,
                with / base.max(f64::MIN_POSITIVE),
                plain[slot] / 1e6,
                plain[slot] / plain[0].max(f64::MIN_POSITIVE),
            );
        }
        println!();
    }
    println!(
        "A gap that persists in the second table is the bookkeeping, and\n\
         sharding those counters is the fix. A gap only in the first table\n\
         means escalation is the limit and the atomics are not."
    );
}
