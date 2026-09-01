// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What does recording an access into the frequency sketch cost?
//!
//! This is the measurement that decided where the sketch gets called from, so
//! it is worth keeping alongside the answer.
//!
//! The obvious design is TinyLFU's: record *every* access, so the sketch knows
//! how often every key is wanted whether or not it is resident. That means four
//! counter updates on every read. Against a cache read costing roughly 226ns,
//! recording measured 67ns at one thread and 230ns at eight — a tax of between
//! a third and the whole of a read, on the path this codebase has spent a dozen
//! changes making cheap.
//!
//! So the sketch is recorded on the **admission** path instead, which a miss
//! already pays for and which already holds the cache exclusively. That is a
//! deliberate divergence from CacheLib, and it is available to us because
//! unlike CacheLib we keep an exact hit count on every resident entry: the
//! sketch only has to answer for keys that are *not* resident, and the victim's
//! own counters answer for the other side of the comparison.
//!
//! Two access patterns:
//!
//! * **spread** — keys drawn from a space larger than the sketch, so counters
//!   are touched at random.
//! * **concentrated** — most records into a handful of keys, whose counters
//!   saturate. A counter at 255 is not written at all, so this should be the
//!   *cheaper* column, not the dearer one.
//!
//! It is: 0.85x of spread, across eight runs spanning 0.81 to 0.93. Two effects
//! compound — the saturated counters are not written, and sixteen keys' counters
//! sit in L1 where a random walk over 128KiB does not.
//!
//! An earlier version with atomic counters, built so the read path could record
//! under the shared lock, put that ratio at 0.39-0.51x at two to eight threads:
//! the counters every thread wanted were exactly the ones that had stopped being
//! written, so the contention removed itself. That version is not in the tree,
//! for the reason above, but the saturation behaviour is worth keeping for the
//! day something records concurrently.
//!
//! ```text
//! cargo run --release --no-default-features --example sketch_cost_bench
//! ```

use matrixcache::{CacheKey, FrequencySketch};
use std::time::Instant;

const CAPACITY: usize = 4_096;
const OPS: usize = 400_000;
const REPEATS: usize = 7;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

/// Nanoseconds per record.
fn bench(concentrate: bool) -> f64 {
    let sketch = FrequencySketch::with_capacity(CAPACITY);
    // Keys built first: formatting them is not what is being measured and
    // would swamp four counter updates several times over.
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let keys = (0..OPS)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let index = if concentrate {
                (state >> 33) as usize % 16
            } else {
                (state >> 33) as usize % (CAPACITY * 4)
            };
            CacheKey::string(0, &format!("k-{index:06}"))
        })
        .collect::<Vec<_>>();

    let started = Instant::now();
    for key in &keys {
        sketch.record(key);
    }
    let elapsed = started.elapsed().as_nanos() as f64 / OPS as f64;
    std::hint::black_box(sketch.estimate(&keys[0]));
    elapsed
}

fn main() {
    println!("{OPS} records into a sketch sized for {CAPACITY} entries, median of {REPEATS}\n");
    // Both patterns inside each repeat, alternating which goes first.
    // Taking `REPEATS` of one and then `REPEATS` of the other compares two
    // different stretches of wall clock, and on a machine whose load is moving
    // that difference lands in the ratio. Each call builds its own sketch, so
    // measuring them together costs nothing.
    let mut spread_samples = Vec::with_capacity(REPEATS);
    let mut concentrated_samples = Vec::with_capacity(REPEATS);
    let mut ratios = Vec::with_capacity(REPEATS);
    for repeat in 0..REPEATS {
        let (spread, concentrated) = if repeat % 2 == 0 {
            let spread = bench(false);
            (spread, bench(true))
        } else {
            let concentrated = bench(true);
            (bench(false), concentrated)
        };
        ratios.push(concentrated / spread.max(f64::MIN_POSITIVE));
        spread_samples.push(spread);
        concentrated_samples.push(concentrated);
    }
    let spread = median(spread_samples);
    let concentrated = median(concentrated_samples);
    ratios.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    println!("{:<20}{:>12}", "pattern", "ns/record");
    println!("{:<20}{spread:>12.1}", "spread");
    println!("{:<20}{concentrated:>12.1}", "concentrated");
    println!(
        "\nconcentrated / spread: {:.2}x  (per-repeat spread {:.2}x..{:.2}x)",
        ratios[ratios.len() / 2],
        ratios[0],
        ratios[ratios.len() - 1]
    );
    println!(
        "\nBelow 1.00 is the saturation shortcut working: a counter already at its\n\
         maximum is not written, and the keys that would be written most are the\n\
         ones that reach it. Against a cache read of roughly 226ns, this is why\n\
         the sketch is recorded on the admission path and not on every read."
    );
}
