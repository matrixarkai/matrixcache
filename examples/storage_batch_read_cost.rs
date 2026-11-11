// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Warm reopened SSD batch reads, including missing keys.
//! Timings include owned result allocation; native allocator bytes are not measured.

use matrixcache::{StorageEngineApi, StorageEngineRocksDb};
use std::{hint::black_box, time::Instant};

fn main() {
    println!("payload_bytes,batch_size,median_ns_per_requested_key");
    for bytes in [256, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ssd").to_string_lossy().into_owned();
        let mut storage = StorageEngineRocksDb::new(path.clone());
        assert!(storage.start());
        storage
            .put_batch(
                (0..2048)
                    .map(|i| (format!("key-{i:04}"), vec![(i % 251) as u8; bytes]))
                    .collect(),
            )
            .unwrap();
        assert!(storage.stop());
        drop(storage);
        let mut storage = StorageEngineRocksDb::new(path);
        assert!(storage.start());
        for batch in [1, 16, 128] {
            let requests: Vec<Vec<String>> = (0..32)
                .map(|offset| {
                    (0..batch)
                        .map(|i| format!("key-{:04}", (i * 17 + offset * 29) % 2064))
                        .collect()
                })
                .collect();
            for keys in &requests {
                let values = storage.get_batch(keys).unwrap();
                for (key, value) in keys.iter().zip(values) {
                    let index: usize = key[4..].parse().unwrap();
                    if index < 2048 {
                        assert_eq!(value.unwrap(), vec![(index % 251) as u8; bytes]);
                    } else {
                        assert!(value.is_none());
                    }
                }
            }
            let iterations = 32768 / batch;
            let mut samples = Vec::new();
            for _ in 0..5 {
                let start = Instant::now();
                for i in 0..iterations {
                    black_box(storage.get_batch(&requests[i % requests.len()]).unwrap());
                }
                samples.push(start.elapsed().as_nanos() / (iterations * batch) as u128);
            }
            samples.sort_unstable();
            println!("{bytes},{batch},{}", samples[2]);
        }
        assert!(storage.stop());
    }
}
