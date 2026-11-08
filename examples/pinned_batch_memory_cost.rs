// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Focused resident-batch latency for the pinned read lifecycle.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::path::PathBuf;
use std::process;
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const RESIDENT: usize = 4_096;
const BATCH: usize = 64;
const WARMUPS: usize = 5;
const PASSES: usize = 31;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|left, right| left.partial_cmp(right).expect("no NaN"));
    samples[samples.len() / 2]
}

fn pinned_pass(cache: &MultiLayerCache, keys: &[CacheKey]) -> f64 {
    let started = Instant::now();
    let mut served = 0usize;
    for chunk in keys.chunks(BATCH) {
        let handles = cache
            .acquire_batch_no_promotion(chunk)
            .expect("resident pinned batch");
        served += handles.iter().filter(|handle| handle.is_some()).count();
        cache.release_batch_iter(handles.into_iter().flatten());
    }
    assert_eq!(served, keys.len());
    started.elapsed().as_nanos() as f64 / keys.len() as f64
}

fn main() {
    let mut json_output = None::<PathBuf>;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json-output" => {
                json_output = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("missing value for --json-output");
                    process::exit(2);
                })));
            }
            "--help" | "-h" => {
                println!("usage: pinned_batch_memory_cost [--json-output PATH]");
                return;
            }
            _ => {
                eprintln!("unknown argument: {arg}");
                process::exit(2);
            }
        }
    }

    let cache = MultiLayerCache::with_options(CacheOptions::new(RESIDENT * VALUE_BYTES * 4, 0, 0));
    let keys = (0..RESIDENT)
        .map(|index| CacheKey::string(0, &format!("pinned-batch-{index:05}")))
        .collect::<Vec<_>>();
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("resident put");
    }

    for _ in 0..WARMUPS {
        let _ = pinned_pass(&cache, &keys);
    }
    let mut samples = Vec::with_capacity(PASSES);
    for _ in 0..PASSES {
        samples.push(pinned_pass(&cache, &keys));
    }
    let pinned_ns = median(samples);
    println!("pinned resident lifecycle: {pinned_ns:.1} ns/item");

    if let Some(path) = json_output {
        let report = format!(
            concat!(
                "{{\n",
                "  \"report_version\": \"matrixcache_pinned_batch_memory_cost_v1\",\n",
                "  \"resident_values\": {},\n",
                "  \"batch_size\": {},\n",
                "  \"value_bytes\": {},\n",
                "  \"passes\": {},\n",
                "  \"pinned_ns_per_item\": {:.1}\n",
                "}}\n"
            ),
            RESIDENT, BATCH, VALUE_BYTES, PASSES, pinned_ns,
        );
        std::fs::write(&path, report).expect("write report");
        println!("wrote {}", path.display());
    }
}
