// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Logical write bytes during deletion near the fallback journal compaction threshold.
//! Run on Linux with --no-default-features. This does not measure device wear.

#[cfg(all(target_os = "linux", not(feature = "rocksdb-ssd")))]
fn main() {
    use matrixcache::{StorageEngineApi, StorageEngineRocksDb};

    fn written() -> (u64, u64) {
        let io = std::fs::read_to_string("/proc/self/io").unwrap();
        let counter = |name| {
            io.lines()
                .find_map(|line| line.strip_prefix(name))
                .unwrap()
                .trim()
                .parse::<u64>()
                .unwrap()
        };
        (counter("wchar:"), counter("syscw:"))
    }

    println!("keys,single_delete_write_bytes,batch_delete_write_bytes,single_write_calls,batch_write_calls");
    for count in [64, 256] {
        let mut costs = Vec::new();
        for batched in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("store").to_string_lossy().to_string();
            let mut store = StorageEngineRocksDb::new(path.clone());
            assert!(store.start());
            let keys: Vec<_> = (0..count)
                .map(|i| format!("{i:03}-{}", "k".repeat(512)))
                .collect();
            store
                .put_batch(keys.iter().map(|k| (k.clone(), vec![7; 4096])).collect())
                .unwrap();
            assert!(store.stop());
            assert!(store.start());
            store
                .put_batch(
                    keys[..count - 1]
                        .iter()
                        .map(|k| (k.clone(), vec![8; 4096]))
                        .collect(),
                )
                .unwrap();
            let before = written();
            if batched {
                assert_eq!(store.delete_batch(&keys).unwrap(), count);
            } else {
                for key in &keys {
                    assert_eq!(store.delete_batch(std::slice::from_ref(key)).unwrap(), 1);
                }
            }
            let after = written();
            costs.push((after.0 - before.0, after.1 - before.1));
            // Do not stop: recovery must use the snapshot and any remaining journal.
            drop(store);
            let mut recovered = StorageEngineRocksDb::new(path);
            assert!(recovered.start());
            assert_eq!(recovered.get_batch(&keys).unwrap(), vec![None; count]);
        }
        assert!(costs[1].0 < costs[0].0);
        println!(
            "{count},{},{},{},{}",
            costs[0].0, costs[1].0, costs[0].1, costs[1].1
        );
    }
}

#[cfg(not(all(target_os = "linux", not(feature = "rocksdb-ssd"))))]
fn main() {
    eprintln!("Run on Linux with --no-default-features for the fallback journal probe.");
    std::process::exit(1);
}
