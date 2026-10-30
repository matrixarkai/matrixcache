// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust allocations for single and batched cold reads, excluding native allocations.

use matrixcache::{CacheCompression, CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn main() {
    for size in [1024, 65536] {
        let dir = tempfile::tempdir().unwrap();
        let mut options =
            CacheOptions::new(0, 0, 4 * 1024 * 1024).with_ssd_paths([dir.path().join("ssd")]);
        options.block_options.compression = CacheCompression::None;
        let cache = MultiLayerCache::with_options(options);
        cache.start().unwrap();
        let mut keys: Vec<_> = (0..16)
            .map(|i| CacheKey::string(0, &i.to_string()))
            .collect();
        for (i, key) in keys.iter().enumerate() {
            cache.put(key.clone(), vec![i as u8; size]).unwrap();
        }
        keys.push(keys[0].clone());
        for batch in [false, true] {
            let region = Region::new(GLOBAL);
            for _ in 0..8 {
                let values = if batch {
                    cache.get_batch_no_promotion(&keys).unwrap()
                } else {
                    keys.iter()
                        .map(|key| cache.get_no_promotion(key).unwrap())
                        .collect()
                };
                for (i, value) in values.iter().enumerate() {
                    let value = &value.as_ref().unwrap().value;
                    assert_eq!(value.len(), size);
                    assert!(value.iter().all(|byte| *byte == (i % 16) as u8));
                }
            }
            let stats = region.change();
            println!(
                "bytes={size} batch={batch} reads=136 rust_allocations={} rust_bytes={}",
                stats.allocations, stats.bytes_allocated
            );
        }
        assert!(cache.stop());
    }
}
