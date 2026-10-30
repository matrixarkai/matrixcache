// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust allocations for conditional updates, excluding setup and native allocations.
use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn main() {
    for size in [1024, 65536] {
        for pmem in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let capacity = size * 256;
            let cache = MultiLayerCache::with_options(
                CacheOptions::new(
                    if pmem { 0 } else { capacity },
                    if pmem { capacity } else { 0 },
                    0,
                )
                .with_pmem_paths([dir.path().join("pmem")]),
            );
            cache.start().unwrap();
            let mut updates = Vec::new();
            for i in 0..128 {
                let key = CacheKey::string(0, &format!("update-{i}"));
                cache.put(key.clone(), vec![7; size]).unwrap();
                let handle = cache.acquire(&key).unwrap().unwrap();
                updates.push((key, handle, vec![9; size]));
            }
            let region = Region::new(GLOBAL);
            for (key, handle, value) in &mut updates {
                cache
                    .update_cached_value_if_current(key, handle, std::mem::take(value))
                    .unwrap();
            }
            let stats = region.change();
            for (key, handle, _) in updates {
                assert!(handle.as_slice().iter().all(|v| *v == 7));
                assert_eq!(cache.get(&key).unwrap().unwrap(), vec![9; size]);
                cache.release(handle);
            }
            println!(
                "pmem={pmem} bytes={size} updates=128 rust_allocations={} rust_bytes={}",
                stats.allocations, stats.bytes_allocated
            );
            assert!(cache.stop());
        }
    }
}
