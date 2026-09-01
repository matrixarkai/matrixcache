// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What does a put cost once the cache is full?
//!
//! A cache spends nearly all its life at capacity, so the interesting cost of a
//! put is the one that includes choosing and removing a victim — not the cost
//! of writing into a tier with room to spare.
//!
//! Victim selection weighs a window of the access order, grouping candidates and
//! scoring each group, rather than taking the least recently used entry
//! directly. CacheLib's LRU evicts from the tail in constant time; weighing a
//! window buys a better choice of victim and pays for it on every eviction, so
//! it is worth knowing what that costs.
//!
//! Two numbers, measured on the same cache in one process:
//!
//! * **with room** — puts into a tier that is not yet full, so no eviction runs.
//! * **at capacity** — puts into a full tier, so each one evicts.
//!
//! The gap is the price of an eviction. It should not grow with the size of the
//! cache, because the window is bounded; if it does, the bound is not holding.
//!
//! ```text
//! cargo run --release --no-default-features --example steady_state_put_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const PUTS: usize = 4000;
const PASSES: usize = 3;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

/// What one pass measured, both conditions inside the same time window.
struct PutCosts {
    with_room: f64,
    at_capacity: f64,
}

/// (ns/put with room, ns/put at capacity, eviction multiplier, its spread).
///
/// Both conditions are measured in every pass rather than one condition for
/// `PASSES` passes and then the other. Two blocks of passes are two stretches
/// of wall clock, and the multiplier -- which is the number this bench exists
/// to report -- would carry whatever the machine's load did between them.
///
/// They can share a pass because each builds its own cache, so neither leaves
/// the other anything. That is not true of every A/B: two conditions that are
/// states of the *same* cache cannot be interleaved at all, only repeated as a
/// pair.
fn put_costs_ns(entries: usize) -> (f64, f64, f64, f64, f64) {
    // Capacity for exactly `entries` values, so the tier fills and then stays
    // full for the rest of the run.
    let capacity = entries * VALUE_BYTES;

    let measure_with_room = || {
        let cache = MultiLayerCache::try_with_options(CacheOptions::new(capacity * 4, 0, 0))
            .expect("cache");
        let started = Instant::now();
        for i in 0..PUTS {
            cache
                .put(
                    CacheKey::string(0, &format!("fresh-{i:08}")),
                    vec![b'v'; VALUE_BYTES],
                )
                .expect("put");
        }
        started.elapsed().as_nanos() as f64 / PUTS as f64
    };

    let measure_at_capacity = || {
        {
            let cache = MultiLayerCache::try_with_options(CacheOptions::new(capacity, 0, 0))
                .expect("cache");
            // Fill it first; this part is not timed.
            for i in 0..entries {
                cache
                    .put(
                        CacheKey::string(0, &format!("resident-{i:08}")),
                        vec![b'v'; VALUE_BYTES],
                    )
                    .expect("put");
            }
            let started = Instant::now();
            for i in 0..PUTS {
                cache
                    .put(
                        CacheKey::string(1, &format!("churn-{i:08}")),
                        vec![b'v'; VALUE_BYTES],
                    )
                    .expect("put");
            }
            started.elapsed().as_nanos() as f64 / PUTS as f64
        }
    };

    let mut passes = Vec::with_capacity(PASSES);
    for pass in 0..PASSES {
        // Alternate which condition runs first: the first one pays for
        // whatever the pass before it left in the page cache.
        let costs = if pass % 2 == 0 {
            let with_room = measure_with_room();
            PutCosts {
                with_room,
                at_capacity: measure_at_capacity(),
            }
        } else {
            let at_capacity = measure_at_capacity();
            PutCosts {
                with_room: measure_with_room(),
                at_capacity,
            }
        };
        passes.push(costs);
    }

    let mut multipliers: Vec<f64> = passes
        .iter()
        .map(|pass| pass.at_capacity / pass.with_room.max(f64::MIN_POSITIVE))
        .collect();
    multipliers.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    (
        median(passes.iter().map(|pass| pass.with_room).collect()),
        median(passes.iter().map(|pass| pass.at_capacity).collect()),
        multipliers[multipliers.len() / 2],
        multipliers[0],
        multipliers[multipliers.len() - 1],
    )
}

fn main() {
    println!("put cost with room against at capacity, median of {PASSES} passes of {PUTS} puts\n");
    println!(
        "{:>10}{:>14}{:>14}{:>12}{:>18}",
        "entries", "with room", "at capacity", "eviction", "per-pass spread"
    );

    for entries in [1024usize, 4096, 16384] {
        let (with_room, at_capacity, multiplier, low, high) = put_costs_ns(entries);
        println!(
            "{:>10}{:>14.1}{:>14.1}{:>11.1}x{:>13.1}..{:.1}x",
            entries, with_room, at_capacity, multiplier, low, high
        );
    }

    println!("\nThe candidate window is bounded, so the eviction column should not grow.");
}
