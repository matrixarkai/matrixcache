// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! SSD working-set turnover with default and recent-frequency policies.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};

fn run(enabled: bool) -> usize {
    let dir = tempfile::tempdir().unwrap();
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 0, 64 * 90).with_ssd_paths([dir.path().join("ssd")]),
    );
    cache.start().unwrap();
    for i in 0..64 {
        let key = CacheKey::string(0, &format!("old-{i}"));
        cache.put(key.clone(), vec![1; 64]).unwrap();
        for _ in 0..128 {
            assert_eq!(cache.get(&key).unwrap(), Some(vec![1; 64]));
        }
    }
    cache.set_ssd_admission_filter_enabled(enabled);
    let mut hits = 0;
    for _ in 0..32 {
        for i in 0..64 {
            let key = CacheKey::string(0, &format!("new-{i}"));
            if let Some(value) = cache.get(&key).unwrap() {
                assert_eq!(value, vec![2; 64]);
                hits += 1;
            } else {
                cache.put(key, vec![2; 64]).unwrap();
            }
        }
        assert!(cache.stats().disk_bytes <= 64 * 90);
    }
    println!(
        "recent_frequency={enabled} shifted_reads=2048 hits={hits} misses={} ssd_evictions={}",
        2048 - hits,
        cache.stats().ssd_evictions
    );
    assert!(cache.stop());
    hits
}

fn main() {
    let default_hits = run(false);
    let frequency_hits = run(true);
    assert!(frequency_hits > default_hits);
    assert!(
        frequency_hits >= 1024,
        "new working set did not become useful"
    );
}
