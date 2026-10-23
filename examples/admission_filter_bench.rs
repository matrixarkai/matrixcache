// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Does declining a cold candidate keep more of what matters?
//!
//! The cache admits everything, so a key read once evicts one read a hundred
//! times. The admission filter compares a newcomer against the entry the
//! replacement policy has already picked as coldest, using a sketch that
//! remembers keys after eviction, and declines the newcomer if it has been
//! wanted less often.
//!
//! Two workloads, because they fail in different ways:
//!
//! * **scan** — a hot set read constantly, interrupted by bursts of keys read
//!   once and never again. Every one of those is currently admitted and each
//!   costs something worth keeping. The filter should decline nearly all of
//!   them: on a first sighting they lose to any entry that has been asked for
//!   before.
//! * **skewed** — no scan at all, just a heavily unequal popularity
//!   distribution over a key space several times capacity. This is the workload
//!   the structure was designed for, and also the one where a filter can do
//!   harm: reject too eagerly and the cache cannot follow a shifting working
//!   set.
//!
//! Reported against the insertion point as well, since both are aimed at the
//! same problem and the interesting question is whether the second adds anything
//! to the first.
//!
//! Deterministic sequences, one process, so the differences are exact.
//!
//! ```text
//! cargo run --release --no-default-features --example admission_filter_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 2_048;
const HOT_KEYS: usize = 1_536;
const SCAN_KEYS: usize = 20_000;
const ROUNDS: usize = 12;
const PER_ROUND: usize = 4_000;

fn build(filter: bool, spec: u8) -> MultiLayerCache {
    let cache = MultiLayerCache::try_with_options(CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0))
        .expect("cache");
    cache.set_admission_filter_enabled(filter);
    cache.set_insertion_point_spec(spec);
    cache
}

/// Hot-set hit rate under alternating hot traffic and scans.
fn scan_workload(filter: bool, spec: u8) -> (f64, u64) {
    let cache = build(filter, spec);
    for index in 0..HOT_KEYS {
        cache
            .put(
                CacheKey::string(0, &format!("hot-{index:05}")),
                vec![b'h'; VALUE_BYTES],
            )
            .expect("put");
    }

    let mut hits = 0_u64;
    let mut misses = 0_u64;
    let mut cursor = 0_usize;
    for round in 0..ROUNDS {
        for step in 0..PER_ROUND {
            let index = (round * 7 + step * 13) % HOT_KEYS;
            let key = CacheKey::string(0, &format!("hot-{index:05}"));
            if cache.get(&key).expect("get").is_some() {
                hits += 1;
            } else {
                misses += 1;
                cache.put(key, vec![b'h'; VALUE_BYTES]).expect("put");
            }
        }
        for _ in 0..PER_ROUND {
            let key = CacheKey::string(0, &format!("scan-{:06}", cursor % SCAN_KEYS));
            cursor += 1;
            if cache.get(&key).expect("get").is_none() {
                cache.put(key, vec![b's'; VALUE_BYTES]).expect("put");
            }
        }
    }
    let total = hits + misses;
    let rate = if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64 * 100.0
    };
    (rate, cache.stats().memory_evictions)
}

/// Overall hit rate on a skewed distribution with no scan.
fn skewed_workload(filter: bool, spec: u8) -> (f64, u64) {
    let cache = build(filter, spec);
    let key_space = RESIDENT * 6;
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut hits = 0_u64;
    let mut misses = 0_u64;

    for _ in 0..(ROUNDS * PER_ROUND * 2) {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let unit = ((state >> 11) as f64) / ((1u64 << 53) as f64);
        let index = ((unit * unit * unit * key_space as f64) as usize).min(key_space - 1);
        let key = CacheKey::string(0, &format!("z-{index:06}"));
        if cache.get(&key).expect("get").is_some() {
            hits += 1;
        } else {
            misses += 1;
            cache.put(key, vec![b'z'; VALUE_BYTES]).expect("put");
        }
    }
    let total = hits + misses;
    let rate = if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64 * 100.0
    };
    (rate, cache.stats().memory_evictions)
}

/// A value size the default assumption gets wrong.
///
/// The sketch is sized from the memory capacity divided by what an entry is
/// expected to cost, and that expectation defaults to sixty-four bytes. A
/// cache of kibibyte values is therefore taken to hold sixteen times the keys
/// it can hold, and the sketch is sized for all of them.
const MISSIZED_VALUE_BYTES: usize = 1024;

/// The skewed workload again, at a value size the default gets wrong, with the
/// sketch's own size reported beside the hit rate.
///
/// `told` is what the cache is told an entry costs before the filter goes on.
/// `None` is the default -- the case a caller who never heard of the setting
/// gets.
fn skewed_workload_sized(
    filter: bool,
    spec: u8,
    value_bytes: usize,
    told: Option<usize>,
) -> (f64, u64, usize) {
    let cache = MultiLayerCache::try_with_options(CacheOptions::new(RESIDENT * value_bytes, 0, 0))
        .expect("cache");
    if let Some(bytes) = told {
        cache.set_expected_entry_bytes(bytes);
    }
    cache.set_admission_filter_enabled(filter);
    cache.set_insertion_point_spec(spec);

    let key_space = RESIDENT * 6;
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut hits = 0_u64;
    let mut misses = 0_u64;
    for _ in 0..(ROUNDS * PER_ROUND * 2) {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let unit = ((state >> 11) as f64) / ((1u64 << 53) as f64);
        let index = ((unit * unit * unit * key_space as f64) as usize).min(key_space - 1);
        let key = CacheKey::string(0, &format!("z-{index:06}"));
        if cache.get(&key).expect("get").is_some() {
            hits += 1;
        } else {
            misses += 1;
            cache.put(key, vec![b'z'; value_bytes]).expect("put");
        }
    }
    let total = hits + misses;
    let rate = if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64 * 100.0
    };
    (
        rate,
        cache.stats().memory_evictions,
        cache.footprint().sketch_bytes,
    )
}

fn main() {
    println!(
        "room for {RESIDENT}; scan: {HOT_KEYS} hot keys against {SCAN_KEYS} one-shot keys;          skewed key space {}
",
        RESIDENT * 6
    );
    println!(
        "{:<12}{:<9}{:>15}{:>17}{:>13}{:>13}",
        "insertion", "filter", "scan hit rate", "skewed hit rate", "scan evict", "skew evict"
    );

    for spec in [0_u8, 1] {
        for filter in [false, true] {
            let (scan, scan_evictions) = scan_workload(filter, spec);
            let (skew, skew_evictions) = skewed_workload(filter, spec);

            // The guard belongs on the baseline, not on the treatment.
            //
            // A workload that never evicted anything scores well for the wrong
            // reason, and that is worth checking -- but only with the filter
            // off. With it on, *fewer* evictions is the entire point, and
            // demanding churn from the arm designed to avoid it rejects exactly
            // the result being looked for. An earlier version of this assertion
            // did precisely that and aborted the run.
            if !filter {
                assert!(
                    scan_evictions > 10_000 && skew_evictions > 5_000,
                    "spec {spec} baseline: only {scan_evictions} and {skew_evictions}                      evictions -- nothing was under pressure, so these numbers mean                      nothing"
                );
            }

            println!(
                "{:<12}{:<9}{scan:>14.2}%{skew:>16.2}%{scan_evictions:>13}{skew_evictions:>13}",
                format!("spec {spec}"),
                if filter { "on" } else { "off" },
            );
        }
    }

    println!();
    println!(
        "the same skewed workload at {MISSIZED_VALUE_BYTES}-byte values, where the default \
         assumption"
    );
    println!(
        "about what an entry costs is sixteen times wrong, with the sketch's own size beside the"
    );
    println!("hit rate");
    println!();
    println!(
        "{:<28}{:>17}{:>13}{:>14}",
        "filter", "skewed hit rate", "skew evict", "sketch bytes"
    );
    for (label, filter, told) in [
        ("off", false, None),
        ("on, told nothing", true, None),
        ("on, told the entry size", true, Some(MISSIZED_VALUE_BYTES)),
    ] {
        let (rate, evictions, sketch) =
            skewed_workload_sized(filter, 1, MISSIZED_VALUE_BYTES, told);
        println!("{label:<28}{rate:>16.2}%{evictions:>13}{sketch:>14}");
    }

    println!(
        "
The eviction columns are as much the result as the hit rates: the filter
         works by declining admissions, so it should churn far less for the same or
         better hit rate. A skewed workload that nearly stops evicting is the
         failure to watch for -- that is a cache that can no longer follow a
         working set that moves, however good its hit rate looks today."
    );
}
