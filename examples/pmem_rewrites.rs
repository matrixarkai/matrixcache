// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! File-backed PMEM rewrites. Linux write_bytes excludes device-level amplification.

use matrixcache::{CacheDataPlacement, CacheKey, CacheOptions, MultiLayerCache};
use std::{sync::Arc, time::Instant};

fn written_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/io")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("write_bytes: ")?.trim().parse().ok())
}

fn main() {
    for (size, count) in [(1024, 4096), (65536, 1024)] {
        for changed in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let options = || {
                CacheOptions::new(0, 1024 * 1024, 0)
                    .with_pmem_paths([dir.path().join("pmem")])
                    .with_auto_recover_on_start(true)
            };
            let cache = MultiLayerCache::with_options(options());
            cache.start().unwrap();
            let mut policy = cache.production_tiering_policy();
            policy.data_placement = CacheDataPlacement::SideBySide;
            policy.data_placement_threshold_bytes = 0;
            cache.update_production_tiering_policy(policy);
            let key = CacheKey::string(0, "pmem-rewrite");
            let mut value: Arc<[u8]> = vec![7; size].into();
            cache.put_shared(key.clone(), Arc::clone(&value)).unwrap();
            let before = written_bytes();
            let started = Instant::now();
            for i in 0..count {
                if changed {
                    Arc::make_mut(&mut value)[0] = i as u8;
                }
                cache.put_shared(key.clone(), Arc::clone(&value)).unwrap();
            }
            let elapsed = started.elapsed();
            let written = before
                .zip(written_bytes())
                .map(|(a, b)| b.saturating_sub(a));
            assert_eq!(cache.get(&key).unwrap().as_deref(), Some(value.as_ref()));
            assert!(cache.stop());
            drop(cache);
            let recovered = MultiLayerCache::with_options(options());
            recovered.start().unwrap();
            assert_eq!(
                recovered.get(&key).unwrap().as_deref(),
                Some(value.as_ref())
            );
            assert!(recovered.stop());
            println!("bytes={size} writes={count} changed={changed} elapsed_ms={} linux_write_bytes={written:?}", elapsed.as_millis());
        }
    }
}
