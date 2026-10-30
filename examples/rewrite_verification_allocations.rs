// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust allocation during rejected identical rewrites; native storage is excluded.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::sync::Arc;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn main() {
    println!("value_bytes,operations,rust_allocations,rust_bytes");
    for size in [64, 65536] {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 0, 1024 * 1024).with_ssd_paths([dir.path().join("ssd")]),
        );
        cache.start().unwrap();
        let key = CacheKey::string(0, "rewrite");
        let mut state = 17u64;
        let value: Arc<[u8]> = (0..size)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect::<Vec<_>>()
            .into();
        cache.put_shared(key.clone(), Arc::clone(&value)).unwrap();
        let mut policy = cache.production_tiering_policy();
        policy.max_ssd_block_bytes = 1;
        cache.update_production_tiering_policy(policy);
        cache.put_shared(key.clone(), Arc::clone(&value)).unwrap();
        let region = Region::new(GLOBAL);
        for _ in 0..100 {
            cache.put_shared(key.clone(), Arc::clone(&value)).unwrap();
        }
        let stats = region.change();
        println!("{size},100,{},{}", stats.allocations, stats.bytes_allocated);
        assert_eq!(cache.get(&key).unwrap().as_deref(), Some(value.as_ref()));
    }
}
