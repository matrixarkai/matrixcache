// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Cost of the key index behind the cache recency order.
//!
//! ```text
//! cargo run --release --no-default-features --example order_index_bench
//! ```

use matrixcache::{CacheKey, CacheKeyOrder};
use std::time::Instant;

const KEYS: usize = 4_096;
const OPERATIONS: usize = 2_000_000;
const ROUNDS: usize = 9;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|left, right| left.total_cmp(right));
    samples[samples.len() / 2]
}

fn measure(mut operation: impl FnMut()) -> f64 {
    let mut samples = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let started = Instant::now();
        for _ in 0..OPERATIONS {
            operation();
        }
        samples.push(started.elapsed().as_nanos() as f64 / OPERATIONS as f64);
    }
    median(samples)
}

fn main() {
    let keys = (0..KEYS)
        .map(|index| CacheKey::string(0, &format!("order-{index:05}")))
        .collect::<Vec<_>>();
    let mut order = keys.iter().cloned().collect::<CacheKeyOrder>();

    let mut moving = 0usize;
    let move_hit = measure(|| {
        moving = (moving + 7) % keys.len();
        std::hint::black_box(order.touch_access(&keys[moving]));
    });

    let tail = keys.last().expect("non-empty keys");
    let tail_hit = measure(|| {
        std::hint::black_box(order.touch_access(tail));
    });

    let missing = CacheKey::string(0, "not-present");
    let miss = measure(|| {
        std::hint::black_box(order.touch_access(&missing));
    });

    println!("{KEYS} ordered keys, {OPERATIONS} operations/pass, median of {ROUNDS}");
    println!("move hit: {move_hit:.1} ns/op");
    println!("tail hit: {tail_hit:.1} ns/op");
    println!("miss:     {miss:.1} ns/op");
}
