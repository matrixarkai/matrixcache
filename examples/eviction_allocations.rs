// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust allocation volume for bounded memory eviction, excluding setup.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn main() {
    for entries in [128, 1024, 8192] {
        let cache = MultiLayerCache::with_options(CacheOptions::new(entries * 64, 0, 0));
        let keys: Vec<_> = (0..entries + 512)
            .map(|i| CacheKey::string(0, &format!("entry-{i:08}")))
            .collect();
        for key in &keys[..entries] {
            cache.put(key.clone(), vec![1; 64]).unwrap();
        }
        // Warm allocation tables before measuring steady eviction.
        for key in &keys[entries..entries + 256] {
            cache.put(key.clone(), vec![1; 64]).unwrap();
        }
        let before = cache.stats();
        let region = Region::new(GLOBAL);
        for key in &keys[entries + 256..] {
            cache.put(key.clone(), vec![1; 64]).unwrap();
        }
        let measured = region.change();
        let after = cache.stats();
        assert_eq!(after.memory_evictions - before.memory_evictions, 256);
        assert!(after.memory_bytes <= (entries * 64) as u64);
        println!(
            "entries={entries} allocations_per_write={} allocated_bytes_per_write={}",
            measured.allocations as f64 / 256.0,
            measured.bytes_allocated as f64 / 256.0
        );
    }
}
