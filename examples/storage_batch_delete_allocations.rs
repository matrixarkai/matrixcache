// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI
//! Rust allocation volume for deleting storage batches, excluding setup.
//! Native backend allocations and disk reads are not counted.
use matrixcache::{StorageEngineApi, StorageEngineMultiSsd, StorageEngineRocksDb};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn measure(name: &str, count: usize, width: usize, delete: impl FnOnce() -> usize) {
    let region = Region::new(GLOBAL);
    let deleted = delete();
    let stats = region.change();
    assert_eq!(deleted, count);
    // Native existence checks should scale with key bookkeeping, not values.
    #[cfg(feature = "rocksdb-ssd")]
    assert!(
        stats.bytes_allocated < count * 512 + 4096,
        "{name}: {count} keys of {width} bytes allocated {} bytes",
        stats.bytes_allocated
    );
    println!(
        "{name},{count},{width},{},{}",
        stats.allocations, stats.bytes_allocated
    );
}

fn main() {
    println!("backend,unique_keys,value_bytes,rust_allocations,rust_bytes");
    for count in [8, 128] {
        for width in [4096, 65536] {
            let directory = tempfile::tempdir().unwrap();
            let keys: Vec<_> = (0..count).map(|i| format!("key-{i:03}")).collect();
            let entries = || {
                keys.iter()
                    .enumerate()
                    .map(|(i, key)| (key.clone(), vec![i as u8; width]))
                    .collect()
            };
            let mut requested = keys.clone();
            requested.reverse();
            requested.push(keys[0].clone());
            requested.push("missing".into());

            let path = directory
                .path()
                .join("single")
                .to_string_lossy()
                .to_string();
            let mut single = StorageEngineRocksDb::new(path.clone());
            assert!(single.start());
            single.put_batch(entries()).unwrap();
            single.put_view("survivor", vec![23; width]).unwrap();
            measure("single", count, width, || {
                single.delete_batch(&requested).unwrap()
            });
            assert_eq!(single.delete_batch(&requested).unwrap(), 0);
            drop(single);
            let mut single = StorageEngineRocksDb::new(path);
            assert!(single.start());
            assert!(single.get_batch(&keys).unwrap().iter().all(Option::is_none));
            assert_eq!(single.get("survivor").unwrap().to_vec(), vec![23; width]);

            let paths = [directory.path().join("a"), directory.path().join("b")];
            let mut multi = StorageEngineMultiSsd::with_paths(paths.clone(), 64 * 1024 * 1024);
            assert!(multi.start());
            multi.put_batch(entries()).unwrap();
            multi.put_view("survivor", vec![23; width]).unwrap();
            measure("multi", count, width, || {
                multi.delete_batch(&requested).unwrap()
            });
            assert_eq!(multi.delete_batch(&requested).unwrap(), 0);
            drop(multi);
            let mut multi = StorageEngineMultiSsd::with_paths(paths, 64 * 1024 * 1024);
            assert!(multi.start());
            assert!(multi.get_batch(&keys).unwrap().iter().all(Option::is_none));
            assert_eq!(multi.get("survivor").unwrap().to_vec(), vec![23; width]);
        }
    }
}
