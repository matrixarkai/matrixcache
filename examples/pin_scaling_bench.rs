// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Does the cost of a put depend on how many entries are pinned?
//!
//! It should not. Pinning marks entries that eviction must skip; writing an
//! unrelated key has nothing to do with how many of those there are. If the
//! number below climbs with the pinned count, a put is doing work proportional
//! to the pinned set, and a workload that pins a lot pays for it on every
//! write.
//!
//! The interesting output is the **shape of the column**, not the absolute
//! nanoseconds. A trend measured within one run survives a noisy machine far
//! better than a number compared against another run, so this deliberately
//! reports the ratio against the unpinned case.
//!
//! ```text
//! cargo run --release --no-default-features --example pin_scaling_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const PUTS: usize = 2000;
const PASSES: usize = 5;

/// Nanoseconds per put, with `pinned` entries pinned in the cache beforehand.
/// Cost of a put with `pinned` other entries resident, pinned or not.
///
/// The `pin_them` control is what separates the two explanations for a row
/// that is not flat. Holding entries pinned and holding them merely resident
/// both make the memory tier bigger, and a bigger tier is slower to write into
/// whether or not anything is pinned. Without the control, residency is
/// charged to pinning.
fn put_cost_ns(pinned: usize, pin_them: bool) -> f64 {
    // Memory only: no SSD capacity and no SSD path, so a put stays on the CPU
    // path. With write-through enabled each put costs milliseconds of IO, and
    // any per-put CPU cost disappears underneath it.
    let capacity = (pinned + PUTS + 16) * VALUE_BYTES * 4;
    let options = CacheOptions::new(capacity, 0, 0);
    let cache = MultiLayerCache::try_with_options(options).expect("cache");

    for i in 0..pinned {
        let key = CacheKey::string(0, &format!("pinned-{i:08}"));
        cache
            .put(key.clone(), vec![b'p'; VALUE_BYTES])
            .expect("put");
        if pin_them {
            cache.pin(key);
        }
    }

    let keys: Vec<CacheKey> = (0..PUTS)
        .map(|i| CacheKey::string(1, &format!("write-{i:08}")))
        .collect();

    let mut samples: Vec<f64> = (0..PASSES)
        .map(|_| {
            let started = Instant::now();
            for key in &keys {
                cache
                    .put(key.clone(), vec![b'w'; VALUE_BYTES])
                    .expect("put");
            }
            started.elapsed().as_nanos() as f64 / keys.len() as f64
        })
        .collect();
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));

    drop(cache);
    samples[samples.len() / 2]
}

/// How many times each row measures its own baseline beside itself.
const PAIRS: usize = 3;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

fn main() {
    println!("put cost against pinned-set size, median of {PASSES} passes of {PUTS} puts");
    println!("each row measures its own unpinned baseline, {PAIRS} pairs\n");
    println!(
        "{:>10}{:>14}{:>14}{:>14}{:>18}",
        "pinned", "ns/put", "resident", "pinned", "per-pair spread"
    );

    for pinned in [0usize, 256, 1024, 4096] {
        // The baseline used to be measured once, at the top, and every later
        // row divided by it. Four measurements spread across a run compared to
        // the oldest of them is the shape most exposed to a machine whose load
        // is moving: the drift is not shared, it accumulates against a fixed
        // point. Each row now measures its own, next to itself.
        let mut pinned_samples = Vec::with_capacity(PAIRS);
        let mut ratios = Vec::with_capacity(PAIRS);
        let mut resident_ratios = Vec::with_capacity(PAIRS);
        for pair in 0..PAIRS {
            // Three measurements in one window: the empty baseline, the same
            // entries resident, and the same entries pinned. Rotating which
            // goes first so none of them is always the one that warms.
            let (baseline, resident, with_pins) = match pair % 3 {
                0 => {
                    let baseline = put_cost_ns(0, false);
                    let resident = put_cost_ns(pinned, false);
                    (baseline, resident, put_cost_ns(pinned, true))
                }
                1 => {
                    let resident = put_cost_ns(pinned, false);
                    let with_pins = put_cost_ns(pinned, true);
                    (put_cost_ns(0, false), resident, with_pins)
                }
                _ => {
                    let with_pins = put_cost_ns(pinned, true);
                    let baseline = put_cost_ns(0, false);
                    (baseline, put_cost_ns(pinned, false), with_pins)
                }
            };
            let baseline = baseline.max(f64::MIN_POSITIVE);
            ratios.push(with_pins / baseline);
            resident_ratios.push(resident / baseline);
            pinned_samples.push(with_pins);
        }
        ratios.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        resident_ratios.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        println!(
            "{:>10}{:>14.1}{:>13.2}x{:>13.2}x{:>13.2}..{:.2}x",
            pinned,
            median(pinned_samples),
            resident_ratios[resident_ratios.len() / 2],
            ratios[ratios.len() / 2],
            ratios[0],
            ratios[ratios.len() - 1]
        );
    }

    println!(
        "\nThe tier is sized so nothing is evicted, so a put does not touch the\n\
         pinned set and the last two columns should both be flat. Where they are\n\
         not, the `resident` column is the control: it holds the same entries\n\
         without pinning any of them, so whatever it shows is the cost of a\n\
         bigger tier and only the difference between the two columns is the cost\n\
         of pinning. The spread says whether that difference means anything."
    );
}
