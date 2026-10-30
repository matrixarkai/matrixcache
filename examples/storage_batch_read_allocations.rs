// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Rust allocation bytes for storage batches containing one repeated key.
//! Native backend allocations are not included in this allocator's counters.

use matrixcache::{StorageEngineApi, StorageEngineMultiSsd, StorageEngineRocksDb};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn report(name: &str, count: usize, mut read: impl FnMut() -> Vec<Option<Vec<u8>>>) {
    drop(read());
    let region = Region::new(GLOBAL);
    for _ in 0..10 {
        let values = read();
        assert_eq!(values.len(), count + 1);
        for (position, value) in values.iter().enumerate() {
            let expected = if position == count { 0 } else { position as u8 };
            let bytes = value.as_ref().unwrap();
            assert_eq!(bytes.len(), 4096);
            assert!(bytes.iter().all(|byte| *byte == expected));
        }
        std::hint::black_box(values);
    }
    let stats = region.change();
    // Allow routing bookkeeping, but not another full payload copy per unique key.
    let bytes_per_batch = stats.bytes_allocated / 10;
    assert!(bytes_per_batch < (count + 1) * 4096 + count * 512 + 4096);
    println!(
        "{name},{count},{},{}",
        stats.allocations / 10,
        bytes_per_batch
    );
}

fn main() {
    println!("backend,unique_keys,rust_allocations_per_batch,rust_bytes_per_batch");
    for count in [8, 128] {
        let directory = tempfile::tempdir().unwrap();
        let keys: Vec<_> = (0..count).map(|i| format!("key-{i:03}")).collect();
        let entries = || {
            keys.iter()
                .enumerate()
                .map(|(i, k)| (k.clone(), vec![i as u8; 4096]))
                .collect()
        };
        let mut requested = keys.clone();
        requested.push(keys[0].clone());
        let mut single = StorageEngineRocksDb::new(
            directory
                .path()
                .join("single")
                .to_string_lossy()
                .to_string(),
        );
        assert!(single.start());
        single.put_batch(entries()).unwrap();
        report("single", count, || single.get_batch(&requested).unwrap());
        let mut multi = StorageEngineMultiSsd::with_paths(
            [directory.path().join("a"), directory.path().join("b")],
            8 * 1024 * 1024,
        );
        assert!(multi.start());
        multi.put_batch(entries()).unwrap();
        report("multi", count, || multi.get_batch(&requested).unwrap());
    }
}
