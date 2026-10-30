// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Resident reads with an SSD copy: median wall time, excluding setup and key construction.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::{
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

fn main() {
    for key_bytes in [32, 4096] {
        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(MultiLayerCache::with_options(
            CacheOptions::new(1024 * 1024, 0, 1024 * 1024).with_ssd_paths([dir.path().join("ssd")]),
        ));
        cache.start().unwrap();
        let mut policy = cache.production_tiering_policy();
        policy.ssd_write_through = true;
        cache.update_production_tiering_policy(policy);
        cache.set_lru_refresh_time(Duration::from_secs(3600));
        let keys: Arc<Vec<_>> = Arc::new(
            (0..64)
                .map(|i| CacheKey::string(0, &format!("{i:08}{}", "k".repeat(key_bytes - 8))))
                .collect(),
        );
        for key in keys.iter() {
            cache.put(key.clone(), vec![7; 64]).unwrap();
            assert!(cache.get(key).unwrap().is_some());
        }
        assert!(cache.stats().disk_bytes > 0);
        assert_eq!(cache.stats().memory_bytes, 64 * 64);
        for enabled in [false, true] {
            cache.set_ssd_admission_filter_enabled(enabled);
            for threads in [1, 4] {
                let mut times = Vec::new();
                for _ in 0..5 {
                    let barrier = Arc::new(Barrier::new(threads));
                    let workers: Vec<_> = (0..threads)
                        .map(|worker| {
                            let cache = Arc::clone(&cache);
                            let keys = Arc::clone(&keys);
                            let barrier = Arc::clone(&barrier);
                            std::thread::spawn(move || {
                                barrier.wait();
                                let start = Instant::now();
                                for i in 0..20000 {
                                    let value = cache
                                        .get(&keys[(i + worker * 17) % keys.len()])
                                        .unwrap()
                                        .unwrap();
                                    assert_eq!(value.len(), 64);
                                    assert_eq!(value[0], 7);
                                }
                                start.elapsed().as_nanos()
                            })
                        })
                        .collect();
                    let wall = workers
                        .into_iter()
                        .map(|w| w.join().unwrap())
                        .max()
                        .unwrap();
                    times.push(wall / (20000 * threads) as u128);
                }
                times.sort_unstable();
                println!("key_bytes={key_bytes} frequency={enabled} readers={threads} median_wall_ns_per_read={}", times[2]);
            }
        }
        assert!(cache.stop());
    }
}
