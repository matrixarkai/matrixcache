// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Compare hot-set retention while SSD scans compete for a small DRAM tier.

use matrixcache::{
    CacheCompression, CacheKey, CacheOptions, CacheReadTier, CacheTier, MultiLayerCache,
};
use std::hint::black_box;
use std::time::Instant;

const ENTRIES: usize = 2_048;
const HOT_KEYS: usize = 32;
const VALUE_BYTES: usize = 4_096;
const ROUNDS: usize = 2;

fn run(promote: bool, filter: bool) {
    let dir = tempfile::tempdir().unwrap();
    let mut options = CacheOptions::new(256 << 10, 0, 64 << 20)
        .with_ssd_paths([dir.path().join("ssd")])
        .with_read_promotion(true, promote)
        .with_admission_filters(filter, false);
    options.block_options.compression = CacheCompression::None;
    let cache = MultiLayerCache::with_options(options);
    cache.set_ssd_instance_only(true);
    let keys = (0..ENTRIES)
        .map(|index| CacheKey::string(4, &format!("scan-{index:05}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache
            .put(key.clone(), vec![(index % 251) as u8; VALUE_BYTES])
            .unwrap();
    }
    cache.set_ssd_instance_only(false);
    assert_eq!(cache.admission_filter_enabled(), filter);

    for _ in 0..80 {
        for key in &keys[..HOT_KEYS] {
            black_box(cache.get_shared(key).unwrap().unwrap());
        }
    }
    let before = cache.stats();
    let started = Instant::now();
    let mut reads = 0;
    for _ in 0..ROUNDS {
        for index in HOT_KEYS..ENTRIES {
            black_box(cache.get_shared(&keys[index]).unwrap().unwrap());
            reads += 1;
            if index % 8 == 0 {
                black_box(cache.get_shared(&keys[index % HOT_KEYS]).unwrap().unwrap());
                reads += 1;
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    let after = cache.stats();
    let hot_resident = keys[..HOT_KEYS]
        .iter()
        .filter(|key| cache.peek_tier(key) == Some(CacheReadTier::Memory))
        .count();
    assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), ENTRIES);
    println!(
        "{promote},{filter},{:.0},{},{},{},{}",
        reads as f64 / elapsed,
        after.disk_hits - before.disk_hits,
        after.memory_fills - before.memory_fills,
        after.memory_evictions - before.memory_evictions,
        hot_resident,
    );
}

fn main() {
    println!(
        "promote,admission_filter,read_qps,disk_hits,memory_fills,memory_evictions,hot_resident"
    );
    run(true, false);
    run(true, true);
    run(false, false);
    run(true, true);
    run(true, false);
}
