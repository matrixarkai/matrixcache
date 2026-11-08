// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Focused SSD-only shared-batch latency with hot storage blocks.

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

fn shared_pass(
    cache: &MultiLayerCache,
    keys: &[CacheKey],
    values: &mut Vec<Option<std::sync::Arc<[u8]>>>,
) -> f64 {
    let started = Instant::now();
    let mut served = 0usize;
    for chunk in keys.chunks(BATCH) {
        cache
            .get_shared_batch_into(chunk, values)
            .expect("SSD shared batch");
        served += values.iter().filter(|value| value.is_some()).count();
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
                    "usage: ssd_shared_batch_memory_cost [--json-output PATH] [--require-passed]"
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
        .map(|index| CacheKey::string(0, &format!("ssd-shared-batch-{index:05}")))
        .collect::<Vec<_>>();
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("SSD put");
    }

    let mut values = Vec::with_capacity(BATCH);
    for _ in 0..WARMUPS {
        let _ = shared_pass(&cache, &keys, &mut values);
    }
    let mut samples = Vec::with_capacity(PASSES);
    for _ in 0..PASSES {
        samples.push(shared_pass(&cache, &keys, &mut values));
    }
    let shared_ns = median(samples);
    let region = Region::new(GLOBAL);
    let _ = shared_pass(&cache, &keys, &mut values);
    let change = region.change();
    let batches = (RESIDENT / BATCH) as f64;
    let allocations_per_batch = change.allocations as f64 / batches;
    let bytes_per_batch = change.bytes_allocated as f64 / batches;
    println!("SSD shared lifecycle: {shared_ns:.1} ns/item");
    println!(
        "SSD shared lifecycle: {allocations_per_batch:.2} allocations and {bytes_per_batch:.1} bytes/batch"
    );

    #[cfg(feature = "rocksdb-ssd")]
    const MAX_ALLOCATIONS_PER_BATCH: f64 = 204.0;
    #[cfg(feature = "rocksdb-ssd")]
    const MAX_BYTES_PER_BATCH: f64 = 30_000.0;
    #[cfg(not(feature = "rocksdb-ssd"))]
    const MAX_ALLOCATIONS_PER_BATCH: f64 = 134.0;
    #[cfg(not(feature = "rocksdb-ssd"))]
    const MAX_BYTES_PER_BATCH: f64 = 21_000.0;
    let passed = allocations_per_batch <= MAX_ALLOCATIONS_PER_BATCH
        && bytes_per_batch <= MAX_BYTES_PER_BATCH;

    if let Some(path) = json_output {
        let report = format!(
            concat!(
                "{{\n",
                "  \"report_version\": \"matrixcache_ssd_shared_batch_memory_cost_v1\",\n",
                "  \"resident_values\": {},\n",
                "  \"batch_size\": {},\n",
                "  \"value_bytes\": {},\n",
                "  \"passes\": {},\n",
                "  \"shared_ns_per_item\": {:.1},\n",
                "  \"allocations_per_batch\": {:.2},\n",
                "  \"bytes_per_batch\": {:.1},\n",
                "  \"passed\": {}\n",
                "}}\n"
            ),
            RESIDENT,
            BATCH,
            VALUE_BYTES,
            PASSES,
            shared_ns,
            allocations_per_batch,
            bytes_per_batch,
            passed,
        );
        std::fs::write(&path, report).expect("write report");
        println!("wrote {}", path.display());
    }
    if require_passed && !passed {
        eprintln!(
            "SSD shared batches exceeded {MAX_ALLOCATIONS_PER_BATCH:.0} allocations or \
             {MAX_BYTES_PER_BATCH:.0} allocated bytes per batch"
        );
        process::exit(1);
    }
}
