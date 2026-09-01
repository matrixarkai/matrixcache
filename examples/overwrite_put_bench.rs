// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! What does re-writing a key the cache already holds cost?
//!
//! Overwriting is not a corner case: a write-through, a refill from a lower
//! tier and an ordinary update all land on the same path, and a cache serving
//! changing values spends most of its writes there.
//!
//! The interesting comparison is against writing a key the cache has never
//! seen. An overwrite has strictly less to do — the entry exists, its metadata
//! is already there, its place in the access order is already taken — so it
//! should not cost more than a first write.
//!
//! Memory only, so the numbers are CPU rather than IO. Both columns are
//! measured in every pass, alternating which goes first: the same cache in the
//! same process is not enough on its own, because taking all the passes of one
//! column and then all the passes of the other compares two stretches of wall
//! clock, and on a busy machine the ratio carries the difference. The cache has
//! eight times the room it needs, so nothing is evicted and neither column
//! leaves the other anything.
//!
//! ```text
//! cargo run --release --no-default-features --example overwrite_put_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const KEYS: usize = 4000;
const PASSES: usize = 5;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

fn main() {
    // Room for everything, so this measures the write path and not eviction.
    let capacity = KEYS * VALUE_BYTES * 8;
    let cache =
        MultiLayerCache::try_with_options(CacheOptions::new(capacity, 0, 0)).expect("cache");

    let keys: Vec<CacheKey> = (0..KEYS)
        .map(|i| CacheKey::string(0, &format!("key-{i:08}")))
        .collect();

    // Seed the keys that will be overwritten, before anything is timed.
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("put");
    }

    let first_write = |pass: usize| {
        let fresh: Vec<CacheKey> = (0..KEYS)
            .map(|i| CacheKey::string(1, &format!("fresh-{pass}-{i:08}")))
            .collect();
        let started = Instant::now();
        for key in &fresh {
            cache
                .put(key.clone(), vec![b'v'; VALUE_BYTES])
                .expect("put");
        }
        started.elapsed().as_nanos() as f64 / fresh.len() as f64
    };
    let overwrite_pass = || {
        let started = Instant::now();
        for key in &keys {
            cache
                .put(key.clone(), vec![b'w'; VALUE_BYTES])
                .expect("put");
        }
        started.elapsed().as_nanos() as f64 / keys.len() as f64
    };

    let mut first_samples = Vec::with_capacity(PASSES);
    let mut overwrite_samples = Vec::with_capacity(PASSES);
    let mut ratios = Vec::with_capacity(PASSES);
    for pass in 0..PASSES {
        // Alternate which runs first, so neither always pays for warming what
        // the previous pass disturbed.
        let (first, overwrite) = if pass % 2 == 0 {
            let first = first_write(pass);
            (first, overwrite_pass())
        } else {
            let overwrite = overwrite_pass();
            (first_write(pass), overwrite)
        };
        ratios.push(overwrite / first.max(f64::MIN_POSITIVE));
        first_samples.push(first);
        overwrite_samples.push(overwrite);
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    let first = median(first_samples);
    let overwrite = median(overwrite_samples);

    println!("put cost, {KEYS} keys of {VALUE_BYTES} bytes, median of {PASSES} passes\n");
    println!("{:<28}{:>12}", "path", "ns/put");
    println!("{:<28}{:>12.1}", "first write", first);
    println!("{:<28}{:>12.1}", "overwrite", overwrite);
    println!(
        "\noverwrite is {:.2}x the cost of a first write  \
         (per-pass spread {:.2}x..{:.2}x)",
        ratios[ratios.len() / 2],
        ratios[0],
        ratios[ratios.len() - 1]
    );
}
