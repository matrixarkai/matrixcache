// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Measure hot-read latency while the background worker fills the SSD tier.
//!
//! Run both backend configurations on an otherwise idle host:
//! `cargo run --release --example cold_tier_read_latency_bench`
//! `cargo run --release --no-default-features --example cold_tier_read_latency_bench`

use matrixcache::{CacheBlockOptions, CacheKey, CacheTier, CacheTieringPolicy, MultiLayerCache};
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

const ENTRY_COUNT: usize = 4096;
const VALUE_BYTES: usize = 16 * 1024;
const HOT_COUNT: usize = 64;
const READERS: usize = 4;

struct ReadSample {
    operations: usize,
    elapsed: Duration,
    latency_ns: Vec<u64>,
}

fn run_reads(
    cache: &Arc<MultiLayerCache>,
    keys: &Arc<Vec<CacheKey>>,
    duration: Duration,
) -> ReadSample {
    let started = Instant::now();
    let workers = (0..READERS)
        .map(|worker_id| {
            let cache = Arc::clone(cache);
            let keys = Arc::clone(keys);
            std::thread::spawn(move || {
                let mut samples = Vec::new();
                let mut index = worker_id;
                let mut operations = 0;
                while started.elapsed() < duration {
                    let key = &keys[index % keys.len()];
                    if index % 64 == 0 {
                        let read_started = Instant::now();
                        let value = cache.get_shared(key).expect("read").expect("hot value");
                        black_box(value.len());
                        samples.push(read_started.elapsed().as_nanos() as u64);
                    } else {
                        let value = cache.get_shared(key).expect("read").expect("hot value");
                        black_box(value.len());
                    }
                    operations += 1;
                    index += 1;
                }
                (operations, samples)
            })
        })
        .collect::<Vec<_>>();
    let mut latency_ns = Vec::new();
    let mut operations = 0;
    for worker in workers {
        let (count, samples) = worker.join().expect("reader");
        operations += count;
        latency_ns.extend(samples);
    }
    let elapsed = started.elapsed();
    ReadSample {
        operations,
        elapsed,
        latency_ns,
    }
}

fn percentile(sorted: &[u64], numerator: usize, denominator: usize) -> u64 {
    sorted[(sorted.len() - 1) * numerator / denominator]
}

fn report(name: &str, mut sample: ReadSample) {
    sample.latency_ns.sort_unstable();
    println!(
        "{name}: reads={} sampled={} qps={:.0} p50_us={:.1} p99_us={:.1} p999_us={:.1} p9999_us={:.1} max_us={:.1}",
        sample.operations,
        sample.latency_ns.len(),
        sample.operations as f64 / sample.elapsed.as_secs_f64(),
        percentile(&sample.latency_ns, 50, 100) as f64 / 1000.0,
        percentile(&sample.latency_ns, 99, 100) as f64 / 1000.0,
        percentile(&sample.latency_ns, 999, 1000) as f64 / 1000.0,
        percentile(&sample.latency_ns, 9999, 10000) as f64 / 1000.0,
        sample.latency_ns.last().copied().unwrap_or(0) as f64 / 1000.0,
    );
}

fn main() {
    let dir = tempfile::tempdir().expect("cache directory");
    let policy = CacheTieringPolicy {
        memory_capacity_bytes: ENTRY_COUNT * VALUE_BYTES * 2,
        pmem_capacity_bytes: 0,
        ssd_capacity_bytes: ENTRY_COUNT * VALUE_BYTES * 2,
        ssd_write_through: false,
        ..CacheTieringPolicy::default()
    };
    let cache = Arc::new(MultiLayerCache::with_tiering_policy(
        dir.path(),
        policy,
        CacheBlockOptions::default(),
    ));
    let value = vec![b'v'; VALUE_BYTES];
    let mut hot_keys = Vec::with_capacity(HOT_COUNT);
    for index in 0..ENTRY_COUNT {
        let key = CacheKey::string(0, &format!("entry-{index:05}"));
        cache
            .put_bypass_storage_for_tier(CacheTier::Memory, key.clone(), value.clone())
            .expect("fill memory");
        if index >= ENTRY_COUNT - HOT_COUNT {
            hot_keys.push(key);
        }
    }
    let hot_keys = Arc::new(hot_keys);
    println!(
        "backend={} entries={ENTRY_COUNT} value_bytes={VALUE_BYTES} readers={READERS}",
        if cfg!(feature = "rocksdb-ssd") {
            "rocksdb"
        } else {
            "file"
        }
    );
    report(
        "baseline",
        run_reads(&cache, &hot_keys, Duration::from_secs(2)),
    );
    assert!(cache.start_cold_tier_migration_worker(4, Duration::from_millis(1)));
    report(
        "migrating",
        run_reads(&cache, &hot_keys, Duration::from_secs(2)),
    );
    assert!(cache.stop_cold_tier_migration_worker());
    let ssd_entries = cache.item_count_for_tier(CacheTier::Ssd);
    assert!(ssd_entries > 0, "the migration workload did no SSD writes");
    println!(
        "ssd_entries={ssd_entries} cache_footprint_bytes={} memory_payload_bytes={}",
        cache.footprint().total_bytes,
        cache.stats().memory_bytes,
    );
}
