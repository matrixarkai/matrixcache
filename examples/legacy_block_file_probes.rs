// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust heap allocation of a miss and of an invalidation, with and without raw
//! block files in the cache directory.
//!
//! A build with the embedded store writes no block files, so when the directory
//! holds none the cache skips building a path for one. The same operations are
//! run over a clean directory and over one holding a `shard-*` entry, and the
//! clean one must allocate less by at least two allocations an operation.
//! Native storage allocations are not counted.

use matrixcache::{CacheKey, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const OPERATIONS: u64 = 2000;

struct Counts {
    miss_allocations: u64,
    invalidate_allocations: u64,
}

fn measure(with_shard_entry: bool) -> Counts {
    let directory = tempfile::tempdir().unwrap();
    if with_shard_entry {
        std::fs::create_dir_all(directory.path().join("shard-4242")).unwrap();
    }
    let cache = MultiLayerCache::new(1 << 20, directory.path());
    let keys: Vec<_> = (0..OPERATIONS)
        .map(|index| CacheKey::string(7, &format!("absent-{index:08}")))
        .collect();
    // Warm every lazily built structure with one of each.
    assert!(cache.get(&keys[0]).unwrap().is_none());
    cache.invalidate(&keys[0]).unwrap();

    let region = Region::new(GLOBAL);
    for key in &keys {
        assert!(std::hint::black_box(cache.get(key).unwrap()).is_none());
    }
    let miss_allocations = region.change().allocations as u64;

    let region = Region::new(GLOBAL);
    for key in &keys {
        cache.invalidate(key).unwrap();
    }
    let invalidate_allocations = region.change().allocations as u64;
    Counts {
        miss_allocations,
        invalidate_allocations,
    }
}

fn main() {
    let clean = measure(false);
    let marked = measure(true);
    println!("directory,operations,miss_allocations,invalidate_allocations");
    println!(
        "clean,{OPERATIONS},{},{}",
        clean.miss_allocations, clean.invalidate_allocations
    );
    println!(
        "shard_entry,{OPERATIONS},{},{}",
        marked.miss_allocations, marked.invalidate_allocations
    );
    if cfg!(feature = "rocksdb-ssd") {
        let floor = 2 * OPERATIONS;
        assert!(
            marked.miss_allocations >= clean.miss_allocations + floor,
            "a miss over a clean directory must allocate at least two fewer times"
        );
        assert!(
            marked.invalidate_allocations >= clean.invalidate_allocations + floor,
            "an invalidation over a clean directory must allocate at least two fewer times"
        );
    } else {
        // Without the embedded store block files are written, so both runs probe them.
        assert!(marked.miss_allocations.abs_diff(clean.miss_allocations) < OPERATIONS);
    }
}
