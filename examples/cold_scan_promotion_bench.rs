// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Read a cold SSD working set larger than DRAM with and without promotion.

use matrixcache::{CacheCompression, CacheKey, CacheOptions, CacheTier, MultiLayerCache};
use std::hint::black_box;
use std::time::Instant;

fn positive_env(name: &str, default: usize) -> usize {
    std::env::var(name).map_or(default, |value| {
        let parsed = value.parse::<usize>().expect("positive integer setting");
        assert!(parsed > 0, "{name} must be positive");
        parsed
    })
}

fn percentile(samples: &mut [u64], numerator: usize, denominator: usize) -> u64 {
    samples.sort_unstable();
    samples[(samples.len() - 1) * numerator / denominator]
}

fn run(promote: bool, entries: usize, value_bytes: usize, rounds: usize, memory_bytes: usize) {
    let dir = tempfile::tempdir().expect("temporary SSD directory");
    let ssd_capacity = entries
        .checked_mul(value_bytes)
        .and_then(|bytes| bytes.checked_mul(2))
        .expect("working set too large");
    let mut options = CacheOptions::new(memory_bytes, 0, ssd_capacity)
        .with_ssd_paths([dir.path().join("ssd")])
        .with_read_promotion(true, promote);
    options.block_options.compression = CacheCompression::None;
    let cache = MultiLayerCache::with_options(options);
    cache.set_ssd_instance_only(true);
    let keys = (0..entries)
        .map(|index| CacheKey::string(4, &format!("scan-{index:05}")))
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        cache
            .put(key.clone(), vec![(index % 251) as u8; value_bytes])
            .expect("SSD write");
    }
    assert_eq!(cache.item_count_for_tier(CacheTier::Ssd), entries);
    cache.set_ssd_instance_only(false);
    let before = cache.stats();
    let mut samples = Vec::with_capacity(entries * rounds);
    let started = Instant::now();
    for _ in 0..rounds {
        for (index, key) in keys.iter().enumerate() {
            let read_started = Instant::now();
            let value = cache
                .get_shared(key)
                .expect("SSD read")
                .expect("stored value");
            samples.push(u64::try_from(read_started.elapsed().as_nanos()).unwrap_or(u64::MAX));
            assert_eq!(value.len(), value_bytes);
            black_box(value[index % value_bytes]);
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    let after = cache.stats();
    assert_eq!(
        after.disk_hits - before.disk_hits,
        (entries * rounds) as u64
    );
    println!(
        "{promote},{:.0},{},{},{},{},{},{},{}",
        samples.len() as f64 / seconds,
        percentile(&mut samples.clone(), 50, 100),
        percentile(&mut samples.clone(), 95, 100),
        percentile(&mut samples, 99, 100),
        after.disk_hits - before.disk_hits,
        after.memory_fills - before.memory_fills,
        after.memory_evictions - before.memory_evictions,
        cache.footprint().payload_bytes,
    );
}

fn main() {
    let entries = positive_env("MATRIXCACHE_SCAN_ENTRIES", 2_048);
    let value_bytes = positive_env("MATRIXCACHE_SCAN_VALUE_BYTES", 4_096);
    let rounds = positive_env("MATRIXCACHE_SCAN_ROUNDS", 2);
    let memory_bytes = positive_env("MATRIXCACHE_SCAN_MEMORY_BYTES", 256 << 10);
    println!(
        "# entries={entries} value_bytes={value_bytes} rounds={rounds} memory_bytes={memory_bytes}"
    );
    println!("promote,read_qps,p50_ns,p95_ns,p99_ns,disk_hits,memory_fills,memory_evictions,resident_payload_bytes");
    for promote in [true, false, false, true] {
        run(promote, entries, value_bytes, rounds, memory_bytes);
    }
}
