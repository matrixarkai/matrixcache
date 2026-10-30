// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust heap allocation during bulk invalidation, excluding setup and native storage.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn main() {
    println!("entries,key_bytes,rust_allocations,rust_bytes");
    for key_bytes in [32, 4096] {
        let cache = MultiLayerCache::with_options(CacheOptions::new(1 << 20, 0, 0));
        cache.start().unwrap();
        let keys: Vec<_> = (0..1024)
            .map(|index| CacheKey::string(0, &format!("{index:08}{}", "k".repeat(key_bytes - 8))))
            .collect();
        for key in &keys {
            cache.put(key.clone(), vec![1; 64]).unwrap();
        }
        let region = Region::new(GLOBAL);
        assert_eq!(cache.invalidate_batch(&keys).unwrap(), keys.len());
        let stats = region.change();
        println!(
            "1024,{key_bytes},{},{}",
            stats.allocations, stats.bytes_allocated
        );
        for key in &keys {
            assert!(cache.get(key).unwrap().is_none());
        }
    }
}
