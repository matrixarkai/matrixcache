// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Focused cross-shard PMEM copied-batch cost and allocation reuse.

use matrixcache::{CacheKey, CacheOptions, ShardedMultiLayerCache};
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 4_096;
const SHARDS: usize = 16;
const WARMUPS: usize = 3;
const PASSES: usize = 21;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|left, right| left.partial_cmp(right).expect("no NaN"));
    samples[samples.len() / 2]
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
                println!("usage: sharded_pmem_batch_cost [--json-output PATH] [--require-passed]");
                return;
            }
            _ => {
                eprintln!("unknown argument: {arg}");
                process::exit(2);
            }
        }
    }

    let directory = tempfile::tempdir().expect("PMEM directory");
    let cache = ShardedMultiLayerCache::with_options(
        CacheOptions::new(0, 64 << 20, 0).with_pmem_paths([directory.path().join("pmem")]),
        SHARDS,
    );
    let keys = (0..RESIDENT)
        .map(|index| CacheKey::string(index as u64, &format!("sharded-pmem-{index:05}")))
        .collect::<Vec<_>>();
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("PMEM put");
    }

    let mut values = Vec::with_capacity(keys.len());
    for _ in 0..WARMUPS {
        cache
            .get_batch_into(&keys, &mut values)
            .expect("warm sharded PMEM batch");
    }
    let mut samples = Vec::with_capacity(PASSES);
    for _ in 0..PASSES {
        let started = Instant::now();
        cache
            .get_batch_into(&keys, &mut values)
            .expect("sharded PMEM batch");
        assert!(values.iter().all(Option::is_some));
        samples.push(started.elapsed().as_nanos() as f64 / keys.len() as f64);
    }
    let read_ns = median(samples);

    let region = Region::new(GLOBAL);
    cache
        .get_batch_into(&keys, &mut values)
        .expect("measured sharded PMEM batch");
    let change = region.change();
    let allocations_per_batch = change.allocations as f64;
    let bytes_per_batch = change.bytes_allocated as f64;

    println!("sharded PMEM batch: {read_ns:.1} ns/item");
    println!(
        "sharded PMEM batch: {allocations_per_batch:.0} allocations and {bytes_per_batch:.0} bytes/pass"
    );

    const MAX_ALLOCATIONS_PER_BATCH: f64 = 256.0;
    const MAX_BYTES_PER_BATCH: f64 = 1_000_000.0;
    let passed = allocations_per_batch <= MAX_ALLOCATIONS_PER_BATCH
        && bytes_per_batch <= MAX_BYTES_PER_BATCH;

    if let Some(path) = json_output {
        let report = format!(
            concat!(
                "{{\n",
                "  \"report_version\": \"matrixcache_sharded_pmem_batch_cost_v1\",\n",
                "  \"resident_values\": {},\n",
                "  \"shards\": {},\n",
                "  \"value_bytes\": {},\n",
                "  \"passes\": {},\n",
                "  \"read_ns_per_item\": {:.1},\n",
                "  \"allocations_per_batch\": {:.0},\n",
                "  \"bytes_per_batch\": {:.0},\n",
                "  \"passed\": {}\n",
                "}}\n"
            ),
            RESIDENT,
            SHARDS,
            VALUE_BYTES,
            PASSES,
            read_ns,
            allocations_per_batch,
            bytes_per_batch,
            passed,
        );
        std::fs::write(&path, report).expect("write report");
        println!("wrote {}", path.display());
    }
    if require_passed && !passed {
        eprintln!(
            "sharded PMEM batches exceeded {MAX_ALLOCATIONS_PER_BATCH:.0} allocations or \
             {MAX_BYTES_PER_BATCH:.0} allocated bytes per pass"
        );
        process::exit(1);
    }
}
