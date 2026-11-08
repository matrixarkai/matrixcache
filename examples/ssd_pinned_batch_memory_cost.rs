// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Focused SSD-only pinned-batch latency with hot storage blocks.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 4_096;
const BATCH: usize = 64;
const WARMUPS: usize = 5;
const PASSES: usize = 31;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|left, right| left.partial_cmp(right).expect("no NaN"));
    samples[samples.len() / 2]
}

fn pinned_pass(cache: &MultiLayerCache, keys: &[CacheKey]) -> f64 {
    let started = Instant::now();
    let mut served = 0usize;
    for chunk in keys.chunks(BATCH) {
        let handles = cache.acquire_batch(chunk).expect("SSD pinned batch");
        served += handles.iter().filter(|handle| handle.is_some()).count();
        cache.release_batch_iter(handles.into_iter().flatten());
    }
    assert_eq!(served, keys.len());
    started.elapsed().as_nanos() as f64 / keys.len() as f64
}

fn main() {
    let mut json_output = None::<PathBuf>;
    let mut require_passed = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json-output" => {
                json_output = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("missing value for --json-output");
                    process::exit(2);
                })));
            }
            "--require-passed" => require_passed = true,
            "--help" | "-h" => {
                println!(
                    "usage: ssd_pinned_batch_memory_cost [--json-output PATH] [--require-passed]"
                );
                return;
            }
            _ => {
                eprintln!("unknown argument: {arg}");
                process::exit(2);
            }
        }
    }

    let directory = tempfile::tempdir().expect("SSD directory");
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(0, 0, 64 << 20)
            .with_ssd_paths([directory.path().join("ssd")])
            .with_ssd_instance_only(true),
    );
    let keys = (0..RESIDENT)
        .map(|index| CacheKey::string(0, &format!("ssd-pinned-batch-{index:05}")))
        .collect::<Vec<_>>();
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("SSD put");
    }

    for _ in 0..WARMUPS {
        let _ = pinned_pass(&cache, &keys);
    }
    let mut samples = Vec::with_capacity(PASSES);
    for _ in 0..PASSES {
        samples.push(pinned_pass(&cache, &keys));
    }
    let pinned_ns = median(samples);
    let region = Region::new(GLOBAL);
    let _ = pinned_pass(&cache, &keys);
    let change = region.change();
    let batches = (RESIDENT / BATCH) as f64;
    let allocations_per_batch = change.allocations as f64 / batches;
    let bytes_per_batch = change.bytes_allocated as f64 / batches;
    println!("SSD pinned lifecycle: {pinned_ns:.1} ns/item");
    println!(
        "SSD pinned lifecycle: {allocations_per_batch:.2} allocations and {bytes_per_batch:.1} bytes/batch"
    );

    #[cfg(feature = "rocksdb-ssd")]
    const MAX_ALLOCATIONS_PER_BATCH: f64 = 205.0;
    #[cfg(feature = "rocksdb-ssd")]
    const MAX_BYTES_PER_BATCH: f64 = 41_500.0;
    #[cfg(not(feature = "rocksdb-ssd"))]
    const MAX_ALLOCATIONS_PER_BATCH: f64 = 135.0;
    #[cfg(not(feature = "rocksdb-ssd"))]
    const MAX_BYTES_PER_BATCH: f64 = 32_500.0;
    let passed = allocations_per_batch <= MAX_ALLOCATIONS_PER_BATCH
        && bytes_per_batch <= MAX_BYTES_PER_BATCH;

    if let Some(path) = json_output {
        let report = format!(
            concat!(
                "{{\n",
                "  \"report_version\": \"matrixcache_ssd_pinned_batch_memory_cost_v1\",\n",
                "  \"resident_values\": {},\n",
                "  \"batch_size\": {},\n",
                "  \"value_bytes\": {},\n",
                "  \"passes\": {},\n",
                "  \"pinned_ns_per_item\": {:.1},\n",
                "  \"allocations_per_batch\": {:.2},\n",
                "  \"bytes_per_batch\": {:.1},\n",
                "  \"passed\": {}\n",
                "}}\n"
            ),
            RESIDENT,
            BATCH,
            VALUE_BYTES,
            PASSES,
            pinned_ns,
            allocations_per_batch,
            bytes_per_batch,
            passed,
        );
        std::fs::write(&path, report).expect("write report");
        println!("wrote {}", path.display());
    }
    if require_passed && !passed {
        eprintln!(
            "SSD pinned batches exceeded {MAX_ALLOCATIONS_PER_BATCH:.0} allocations or \
             {MAX_BYTES_PER_BATCH:.0} allocated bytes per batch"
        );
        process::exit(1);
    }
}
