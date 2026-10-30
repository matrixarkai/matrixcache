// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust allocations during SSD eviction with value callbacks; excludes native allocations.

use matrixcache::{CacheKey, CacheOptions, CacheTier, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::{
    alloc::System,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn main() {
    for size in [1024, 65536] {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 0, 16 * 1024 * 1024).with_ssd_paths([dir.path().join("ssd")]),
        );
        cache.start().unwrap();
        for index in 0..128 {
            cache
                .put(CacheKey::string(0, &index.to_string()), vec![7; size])
                .unwrap();
        }
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&count);
        cache.register_eviction_callback(move |record| {
            assert_eq!(record.tier, CacheTier::Ssd);
            assert_eq!(record.value.len(), size);
            assert!(record.value.iter().all(|byte| *byte == 7));
            seen.fetch_add(1, Ordering::Relaxed);
        });
        let region = Region::new(GLOBAL);
        cache.set_capacity_for_tier(CacheTier::Ssd, 0);
        let stats = region.change();
        assert_eq!(count.load(Ordering::Relaxed), 128);
        println!(
            "bytes={size} evictions=128 rust_allocations={} rust_bytes={}",
            stats.allocations, stats.bytes_allocated
        );
        assert!(cache.stop());
    }
}
