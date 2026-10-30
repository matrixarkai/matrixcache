// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Isolated frequency accounting cost; not an end-to-end cache throughput result.
use matrixcache::{CacheKey, FrequencySketch};
use std::{hint::black_box, time::Instant};

fn main() {
    for key_bytes in [32, 4096] {
        let key = CacheKey::string(0, &"k".repeat(key_bytes));
        for count in [1, 16, 128] {
            let mut timings = [Vec::new(), Vec::new()];
            for trial in 0..7 {
                for offset in 0..2 {
                    let mode = (trial + offset) % 2;
                    let sketch = FrequencySketch::with_capacity(64);
                    let start = Instant::now();
                    for _ in 0..2000 {
                        if mode == 0 {
                            for _ in 0..count {
                                sketch.record(black_box(&key));
                            }
                        } else {
                            sketch.record_many(black_box(&key), black_box(count));
                        }
                    }
                    timings[mode].push(start.elapsed().as_nanos() / 2000);
                    assert!(sketch.estimate(&key) > 0);
                }
            }
            for times in &mut timings {
                times.sort_unstable();
            }
            println!("key_bytes={key_bytes} samples={count} individual_ns_per_batch={} reused_hash_ns_per_batch={}", timings[0][3], timings[1][3]);
        }
    }
}
