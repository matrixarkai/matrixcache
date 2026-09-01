// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Does placing new entries part-way down the order survive a scan?
//!
//! A scan is the workload caches are worst at: a burst of keys read once and
//! never again. Every one of them is admitted, and with new entries landing at
//! the most-recently-used end each one pushes the genuinely hot set a place
//! closer to eviction. A long enough scan evicts everything worth keeping and
//! the cache comes out of it cold.
//!
//! `lru_insertion_point_spec` is CacheLib's answer: place a new entry
//! `resident >> spec` from the eviction end rather than at the hot end. A
//! one-hit-wonder is then evicted from where it was put without displacing
//! anything, while an entry that is read a second time is promoted to the hot
//! end and keeps full protection.
//!
//! The workload alternates: a burst of reads over a small hot set that fits
//! comfortably, then a scan over a large cold key space that does not. What
//! matters is the hot set's hit rate — whether the scan cost it its residency.
//!
//! The comparison is within one process, one setting after another over an
//! identical deterministic sequence, so the differences are exact rather than
//! sampled and a busy machine cannot move them.
//!
//! ```text
//! cargo run --release --no-default-features --example scan_resistance_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, CacheReplacementPolicy, CacheTier, MultiLayerCache};

const VALUE_BYTES: usize = 64;
/// Three quarters of capacity: it fits, but only just, so a scan has to
/// fight it for room. At a fraction of capacity nothing is under pressure and
/// every setting looks identical -- which is what the first version of this
/// bench measured.
const HOT_KEYS: usize = 1_536;
/// Does not fit, and is never read twice.
const SCAN_KEYS: usize = 20_000;
const RESIDENT: usize = 2_048;
const ROUNDS: usize = 12;
const HOT_READS_PER_ROUND: usize = 4_000;
const SCAN_READS_PER_ROUND: usize = 4_000;

fn hot_key(index: usize) -> CacheKey {
    CacheKey::string(0, &format!("hot-{index:05}"))
}

fn scan_key(index: usize) -> CacheKey {
    CacheKey::string(0, &format!("scan-{index:06}"))
}

/// Hot-set hit rate under alternating hot traffic and scans.
/// Returns the hot-set hit rate, how many entries were evicted, and how many
/// scan keys were admitted. The last two exist so a result cannot be a
/// workload that never put the cache under pressure.
fn measure(spec: u8, policy: CacheReplacementPolicy) -> (f64, u64, u64) {
    let cache = MultiLayerCache::try_with_options(CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0))
        .expect("cache");
    cache.set_replacement_policy_for_tier(CacheTier::Memory, policy);
    cache.set_insertion_point_spec(spec);

    // Warm the hot set so it starts resident in every configuration.
    for index in 0..HOT_KEYS {
        cache
            .put(hot_key(index), vec![b'h'; VALUE_BYTES])
            .expect("put");
    }

    let mut hot_hits = 0_u64;
    let mut hot_misses = 0_u64;
    let mut scan_cursor = 0_usize;

    for round in 0..ROUNDS {
        // Traffic over the hot set. A miss is refilled, as a real caller would.
        for step in 0..HOT_READS_PER_ROUND {
            let index = (round * 7 + step * 13) % HOT_KEYS;
            let key = hot_key(index);
            if cache.get(&key).expect("get").is_some() {
                hot_hits += 1;
            } else {
                hot_misses += 1;
                cache.put(key, vec![b'h'; VALUE_BYTES]).expect("put");
            }
        }

        // A scan: fresh keys, each read once, each admitted on the miss.
        for _ in 0..SCAN_READS_PER_ROUND {
            let key = scan_key(scan_cursor % SCAN_KEYS);
            scan_cursor += 1;
            if cache.get(&key).expect("get").is_none() {
                cache.put(key, vec![b's'; VALUE_BYTES]).expect("put");
            }
        }
    }

    let total = hot_hits + hot_misses;
    let rate = if total == 0 {
        0.0
    } else {
        hot_hits as f64 / total as f64 * 100.0
    };
    let stats = cache.stats();
    (rate, stats.memory_evictions, stats.memory_fills)
}

fn main() {
    println!(
        "hot set of {HOT_KEYS} keys against a {SCAN_KEYS}-key scan, room for {RESIDENT},          {ROUNDS} rounds"
    );
    println!(
        "hot-set hit rate, and the change against that policy's own spec 0
"
    );
    println!(
        "{:<22}{:>10}{:>10}{:>10}{:>10}",
        "policy", "spec 0", "spec 1", "spec 2", "spec 3"
    );

    for (name, policy) in [
        (
            "WeightedHotnessLru",
            CacheReplacementPolicy::WeightedHotnessLru,
        ),
        ("Slru", CacheReplacementPolicy::Slru),
        ("Fifo", CacheReplacementPolicy::Fifo),
    ] {
        let mut baseline = 0.0_f64;
        let mut cells = String::new();
        let mut evictions = 0_u64;
        let mut fills = 0_u64;
        for spec in [0_u8, 1, 2, 3] {
            let (rate, evicted, filled) = measure(spec, policy);
            if spec == 0 {
                baseline = rate;
                evictions = evicted;
                fills = filled;
                cells.push_str(&format!("{rate:>9.2}%"));
            } else {
                cells.push_str(&format!("{:>+10.2}", rate - baseline));
            }
        }
        // A workload that never evicted anything would score 100 everywhere
        // and mean nothing. This is the guard the first version of this bench
        // lacked, which is why it reported a 0.02-point difference and read as
        // if the feature did nothing.
        assert!(
            evictions > 10_000,
            "{name}: only {evictions} evictions -- the cache was never under              pressure and these numbers are meaningless"
        );
        assert!(fills > 10_000, "{name}: only {fills} fills");
        println!("{name:<22}{cells}   ({evictions} evictions)");
    }

    println!(
        "
spec 0 is the absolute hit rate; the rest are points against it. The hot
         set is read constantly and only ever loses hits to the scan evicting it, so
         a positive number is the scan doing less damage.
         
         Fifo is unaffected by construction, not because it resists scans: it
         evicts in insertion order and never consults the access order, so an
         access-order insertion point cannot reach it. The policies that select
         victims from the access order are the ones this changes."
    );
}
