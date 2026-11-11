// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn disabled_ssd_missing_peeks_do_not_allocate_storage_key_buffers() {
    let directory = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(CacheOptions::new(4096, 0, 0)
        .with_ssd_paths([directory.path().join("ssd")]));
    cache.start().unwrap();
    let keys = [CacheKey::hash(1, "short-entity", "missing"),
        CacheKey::hash(1, &"long-entity\t%/".repeat(1024), "missing")];
    for key in &keys {
        assert!(!cache.peek(key));
    }
    let region = Region::new(GLOBAL);
    for _ in 0..128 {
        for key in &keys {
            assert!(!std::hint::black_box(cache.peek(key)));
            assert!(std::hint::black_box(cache.peek_tier(key)).is_none());
        }
    }
    let stats = region.change();
    eprintln!("DISABLED_SSD_PEEK probes=512 allocations={} bytes={}", stats.allocations, stats.bytes_allocated);
    // The process-wide clock ticker may finish its one-time startup inside this global region.
    assert!(stats.allocations <= 1, "disabled backends must not render storage keys: {stats:?}");
    assert!(stats.bytes_allocated <= 64, "storage-key buffers must not be allocated: {stats:?}");
    assert!(cache.stop());
}
