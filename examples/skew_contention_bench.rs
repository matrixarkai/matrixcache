// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Does concentrating reads on a few keys cost more than spreading them?
//!
//! The read path serves a memory hit under a shared lock, but it still writes
//! per-entry state on the way through: the entry's hit count, its hotness, the
//! epoch it was last seen at, and a process-wide access epoch. Those are
//! atomics, so they do not need the exclusive lock — but an atomic
//! read-modify-write is not free when several cores aim at the same cache
//! line, and a *skewed* workload aims all of them at the same few entries by
//! definition.
//!
//! That predicts something specific and testable: scaling should be worse when
//! reads are concentrated than when they are spread, even though the work per
//! read is identical. If concentrated and spread scale alike, the per-entry
//! writes are not the limit and the guess is wrong.
//!
//! Both patterns read the same number of keys from the same cache with the
//! same value size, and every key is resident, so the only difference is
//! *which* entries the threads land on.
//!
//! The behaviour this is looking for: a `Get` that reads an entry's flag and
//! writes it only when the state actually changes — `Fetched` then `Active` —
//! so an entry already `Active` is read without being written at all. Hot
//! entries stop being written to, which is exactly the case a skewed workload
//! produces, and it is why the two patterns can differ at all.
//!
//! ```text
//! cargo run --release --no-default-features --example skew_contention_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::Arc;
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const KEY_SPACE: usize = 8192;
const READS_PER_THREAD: usize = 60_000;
const REPEATS: usize = 5;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

/// Spread: every key equally likely, so threads rarely collide on one entry.
fn spread(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    ((*state >> 33) as usize) % KEY_SPACE
}

/// Concentrated: cubed, so most reads land in the first few hundred keys and
/// every thread is writing the same handful of entries.
fn concentrated(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let unit = ((*state >> 11) as f64) / ((1u64 << 53) as f64);
    let cubed = unit * unit * unit;
    ((cubed * KEY_SPACE as f64) as usize).min(KEY_SPACE - 1)
}

fn throughput(cache: &Arc<MultiLayerCache>, threads: usize, concentrate: bool) -> f64 {
    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(cache);
            std::thread::spawn(move || {
                // Pre-build the keys so the timed section is the cache and not
                // string formatting, which would otherwise dominate and hide
                // the effect being looked for.
                let mut state = 0x2545_F491_4F6C_DD1D ^ ((worker as u64) << 32);
                let keys = (0..READS_PER_THREAD)
                    .map(|_| {
                        let index = if concentrate {
                            concentrated(&mut state)
                        } else {
                            spread(&mut state)
                        };
                        CacheKey::string(0, &format!("c-{index:05}"))
                    })
                    .collect::<Vec<_>>();
                // Residency is checked once, here, rather than inside the
                // timed loop where it would be a second read per key.
                assert!(
                    cache.get(&keys[0]).expect("get").is_some(),
                    "keys should all be resident"
                );
                let started = Instant::now();
                for key in &keys {
                    let _ = cache.get(key).expect("get");
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
                CacheKey::string(0, &format!("c-{index:05}")),
                vec![b'v'; VALUE_BYTES],
            )
            .expect("put");
    }

    println!(
        "{KEY_SPACE} resident values of {VALUE_BYTES} bytes, {READS_PER_THREAD} reads/thread, \
         median of {REPEATS}"
    );
    println!("refresh window: {:?}\n", cache.lru_refresh_time());
    println!(
        "{:<10}{:>16}{:>16}{:>14}",
        "threads", "spread Mops/s", "concentrated", "conc/spread"
    );

    let mut spread_base = 0.0_f64;
    let mut conc_base = 0.0_f64;
    for threads in [1_usize, 2, 4, 8] {
        let sp = median(
            (0..REPEATS)
                .map(|_| throughput(&cache, threads, false))
                .collect(),
        );
        let co = median(
            (0..REPEATS)
                .map(|_| throughput(&cache, threads, true))
                .collect(),
        );
        if threads == 1 {
            spread_base = sp;
            conc_base = co;
        }
        println!(
            "{threads:<10}{:>16.4}{:>16.4}{:>13.2}x",
            sp / 1e6,
            co / 1e6,
            co / sp.max(f64::MIN_POSITIVE)
        );
    }
    println!(
        "\nscaling against each pattern's own single-thread figure:\n  \
         spread {:.2}x, concentrated {:.2}x at 8 threads",
        median((0..REPEATS).map(|_| throughput(&cache, 8, false)).collect())
            / spread_base.max(f64::MIN_POSITIVE),
        median((0..REPEATS).map(|_| throughput(&cache, 8, true)).collect())
            / conc_base.max(f64::MIN_POSITIVE),
    );
    println!(
        "If concentrated scales worse than spread, the per-entry atomic writes\n\
         are a limit. If they scale alike, they are not and the guess is wrong."
    );
}
