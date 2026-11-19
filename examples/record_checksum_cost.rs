// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Record checksum CPU cost, excluding storage and allocation.

use matrixcache::crc32c_with_seed;
use std::{hint::black_box, time::Instant};

fn main() {
    println!("bytes,median_ns_per_checksum");
    for length in [0, 16, 128, 4096, 65536] {
        let payload: Vec<u8> = (0..length).map(|i| (i % 251) as u8).collect();
        let iterations = (8 * 1024 * 1024 / length.max(1)).clamp(256, 100000);
        let mut samples = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            for i in 0..iterations {
                black_box(crc32c_with_seed(black_box(&payload), black_box(i as u32)));
            }
            samples.push(start.elapsed().as_nanos() / iterations as u128);
        }
        samples.sort_unstable();
        println!("{length},{}", samples[2]);
    }
}
