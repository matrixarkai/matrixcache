// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust-side allocation cost of exact SSD existence probes.
//! Native storage allocations and physical device reads are not counted.

use matrixcache::{StorageEngineApi, StorageEngineMultiSsd, StorageEngineRocksDb};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::time::Instant;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn measure(name: &str, store: &impl StorageEngineApi) {
    assert!(store.peek("payload"));
    assert!(store.peek("empty"));
    assert!(!store.peek("missing"));
    let started = Instant::now();
    let region = Region::new(GLOBAL);
    for _ in 0..1000 {
        assert!(std::hint::black_box(store.peek("payload")));
        assert!(std::hint::black_box(store.peek("empty")));
        assert!(!std::hint::black_box(store.peek("missing")));
    }
    let stats = region.change();
    let elapsed = started.elapsed().as_micros();
    assert_eq!(
        stats.bytes_allocated, 0,
        "exact probes must not copy payloads"
    );
    println!(
        "{name},3000,{},{},{elapsed}",
        stats.allocations, stats.bytes_allocated
    );
}

fn main() {
    let directory = tempfile::tempdir().unwrap();
    println!("backend,probes,rust_allocations,rust_allocated_bytes,elapsed_us");
    let mut single = StorageEngineRocksDb::new(
        directory
            .path()
            .join("single")
            .to_string_lossy()
            .into_owned(),
    );
    assert!(single.start());
    single.put("payload", vec![7; 64 * 1024]).unwrap();
    single.put("empty", Vec::new()).unwrap();
    measure("single", &single);
    let mut multi = StorageEngineMultiSsd::with_paths(
        [directory.path().join("a"), directory.path().join("b")],
        1 << 20,
    );
    assert!(multi.start());
    multi.put("payload", vec![7; 64 * 1024]).unwrap();
    multi.put("empty", Vec::new()).unwrap();
    measure("multi", &multi);
}
