// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Focused PMEM-to-memory owned-batch cost.

use matrixcache::{CacheDataPlacement, CacheKey, CacheOptions, MultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 4_096;
const BATCH: usize = 64;
const WARMUPS: usize = 3;
const PASSES: usize = 21;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|left, right| left.partial_cmp(right).expect("no NaN"));
    samples[samples.len() / 2]
}

fn build_cache(keys: &[CacheKey]) -> (tempfile::TempDir, MultiLayerCache) {
    let directory = tempfile::tempdir().expect("PMEM directory");
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(8 << 20, 64 << 20, 0)
            .with_pmem_paths([directory.path().join("pmem")])
            .with_dram_pmem_data_placement(CacheDataPlacement::SideBySide, 0),
    );
    cache.start().expect("start cache");
    for key in keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("PMEM put");
    }
    cache.set_data_placement_threshold_bytes(usize::MAX);
    (directory, cache)
}

fn refill_pass(cache: &MultiLayerCache, keys: &[CacheKey]) -> (f64, Vec<Option<Vec<u8>>>) {
    let mut values = Vec::with_capacity(BATCH);
    let started = Instant::now();
    let mut served = 0usize;
    for chunk in keys.chunks(BATCH) {
        cache
            .get_batch_into(chunk, &mut values)
            .expect("PMEM owned refill batch");
        served += values.iter().filter(|value| value.is_some()).count();
    }
    assert_eq!(served, keys.len());
    (
        started.elapsed().as_nanos() as f64 / keys.len() as f64,
        values,
    )
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
                    "usage: pmem_owned_refill_batch_memory_cost [--json-output PATH] [--require-passed]"
                );
                return;
            }
            _ => {
                eprintln!("unknown argument: {arg}");
                process::exit(2);
            }
        }
    }

    let keys = (0..RESIDENT)
        .map(|index| CacheKey::string(0, &format!("pmem-owned-refill-{index:05}")))
        .collect::<Vec<_>>();

    for _ in 0..WARMUPS {
        let (_directory, cache) = build_cache(&keys);
        let _ = refill_pass(&cache, &keys);
    }
    let mut samples = Vec::with_capacity(PASSES);
    for _ in 0..PASSES {
        let (_directory, cache) = build_cache(&keys);
        samples.push(refill_pass(&cache, &keys).0);
    }
    let refill_ns = median(samples);

    let (_directory, cache) = build_cache(&keys);
    let region = Region::new(GLOBAL);
    let _ = refill_pass(&cache, &keys);
    let change = region.change();
    let batches = (RESIDENT / BATCH) as f64;
    let allocations_per_batch = change.allocations as f64 / batches;
    let bytes_per_batch = change.bytes_allocated as f64 / batches;

    println!("PMEM owned refill lifecycle: {refill_ns:.1} ns/item");
    println!(
        "PMEM owned refill lifecycle: {allocations_per_batch:.2} allocations and {bytes_per_batch:.1} bytes/batch"
    );

    const MAX_ALLOCATIONS_PER_BATCH: f64 = 4.0;
    const MAX_BYTES_PER_BATCH: f64 = 22_000.0;
    let passed = allocations_per_batch <= MAX_ALLOCATIONS_PER_BATCH
        && bytes_per_batch <= MAX_BYTES_PER_BATCH;

    if let Some(path) = json_output {
        let report = format!(
            concat!(
                "{{\n",
                "  \"report_version\": \"matrixcache_pmem_owned_refill_batch_memory_cost_v1\",\n",
                "  \"resident_values\": {},\n",
                "  \"batch_size\": {},\n",
                "  \"value_bytes\": {},\n",
                "  \"passes\": {},\n",
                "  \"refill_ns_per_item\": {:.1},\n",
                "  \"allocations_per_batch\": {:.2},\n",
                "  \"bytes_per_batch\": {:.1},\n",
                "  \"passed\": {}\n",
                "}}\n"
            ),
            RESIDENT,
            BATCH,
            VALUE_BYTES,
            PASSES,
            refill_ns,
            allocations_per_batch,
            bytes_per_batch,
            passed,
        );
        std::fs::write(&path, report).expect("write report");
        println!("wrote {}", path.display());
    }
    if require_passed && !passed {
        eprintln!(
            "PMEM owned refill batches exceeded {MAX_ALLOCATIONS_PER_BATCH:.0} allocations or \
             {MAX_BYTES_PER_BATCH:.0} allocated bytes per batch"
        );
        process::exit(1);
    }
}
