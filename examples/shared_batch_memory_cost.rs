// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Focused resident-batch latency for the shared, allocation-free read path.
//!
//! The broad batch benchmark exercises every owned, pinned, sharded and
//! no-promotion path. This one deliberately measures only the medium batch
//! where duplicate preflight and resident-hit staging should stay inline.
//!
//! ```text
//! cargo run --release --no-default-features --example shared_batch_memory_cost
//! ```

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

fn shared_pass(cache: &MultiLayerCache, keys: &[CacheKey]) -> f64 {
    let started = Instant::now();
    let mut results = Vec::with_capacity(BATCH);
    let mut served = 0usize;
    for chunk in keys.chunks(BATCH) {
        cache
            .get_shared_batch_into(chunk, &mut results)
            .expect("shared resident batch");
        served += results.iter().filter(|value| value.is_some()).count();
    }
    assert_eq!(served, keys.len());
    started.elapsed().as_nanos() as f64 / keys.len() as f64
}

fn copied_pass(cache: &MultiLayerCache, keys: &[CacheKey]) -> f64 {
    let started = Instant::now();
    let mut results = Vec::with_capacity(BATCH);
    let mut served = 0usize;
    for chunk in keys.chunks(BATCH) {
        cache
            .get_batch_into(chunk, &mut results)
            .expect("copied resident batch");
        served += results.iter().filter(|value| value.is_some()).count();
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
                println!("usage: shared_batch_memory_cost [--json-output PATH]");
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
        .map(|index| CacheKey::string(0, &format!("shared-batch-{index:05}")))
        .collect::<Vec<_>>();
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("resident put");
    }

    for _ in 0..WARMUPS {
        let _ = shared_pass(&cache, &keys);
        let _ = copied_pass(&cache, &keys);
    }

    let mut shared = Vec::with_capacity(PASSES);
    let mut copied = Vec::with_capacity(PASSES);
    for pass in 0..PASSES {
        if pass % 2 == 0 {
            shared.push(shared_pass(&cache, &keys));
            copied.push(copied_pass(&cache, &keys));
        } else {
            copied.push(copied_pass(&cache, &keys));
            shared.push(shared_pass(&cache, &keys));
        }
    }

    let shared_ns = median(shared);
    let copied_ns = median(copied);
    println!("shared resident batch: {shared_ns:.1} ns/item");
    println!("copied resident batch: {copied_ns:.1} ns/item");

    if let Some(path) = json_output {
        let report = format!(
            concat!(
                "{{\n",
                "  \"report_version\": \"matrixcache_shared_batch_memory_cost_v1\",\n",
                "  \"resident_values\": {},\n",
                "  \"batch_size\": {},\n",
                "  \"value_bytes\": {},\n",
                "  \"passes\": {},\n",
                "  \"shared_ns_per_item\": {:.1},\n",
                "  \"copied_ns_per_item\": {:.1}\n",
                "}}\n"
            ),
            RESIDENT, BATCH, VALUE_BYTES, PASSES, shared_ns, copied_ns,
        );
        std::fs::write(&path, report).expect("write report");
        println!("wrote {}", path.display());
    }
}
