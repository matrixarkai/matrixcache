// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Full-capacity SSD batch overwrites, with value and capacity validation.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::Instant;

fn main() {
    for count in [1024, 4096] {
        let dir = tempfile::tempdir().unwrap();
        let cache = MultiLayerCache::with_options(
            CacheOptions::new(0, 0, count * 90).with_ssd_paths([dir.path().join("ssd")]),
        );
        cache.start().unwrap();
        let keys: Vec<_> = (0..count)
            .map(|i| CacheKey::string(0, &format!("overwrite-{i}")))
            .collect();
        for round in 0..4u8 {
            let entries = keys
                .iter()
                .map(|key| (key.clone(), vec![round; 64]))
                .collect();
            let started = Instant::now();
            cache.put_batch(entries).unwrap();
            let elapsed = started.elapsed();
            for value in cache.get_batch(&keys).unwrap() {
                assert_eq!(value, Some(vec![round; 64]));
            }
            let stats = cache.stats();
            assert_eq!(stats.disk_bytes, (count * 90) as u64);
            assert_eq!(stats.ssd_evictions, 0);
            if round > 0 {
                println!(
                    "entries={count} round={round} write_us={}",
                    elapsed.as_micros()
                );
            }
        }
        assert!(cache.stop());
    }
}
