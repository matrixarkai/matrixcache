// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Does draining the write-back queue cost more when the queue is longer?
//!
//! It should not. Draining a batch handles the jobs in that batch; the ones
//! still queued behind them are not touched. If the per-job number below climbs
//! with the queue length, each drain is doing work proportional to what is
//! still waiting, and a deep queue drained in batches pays that repeatedly.
//!
//! The queue is drained in small batches on purpose, because that is the shape
//! a background worker produces: take a few, write them, come back. Draining
//! everything in one call would hide a per-drain cost behind a single pass.
//!
//! The result is the **shape of the column**, not the absolute nanoseconds; a
//! trend within one run survives a busy machine far better than a comparison
//! against a different run.
//!
//! ```text
//! cargo run --release --no-default-features --example writeback_drain_bench
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const BATCH: usize = 8;
const PASSES: usize = 3;

/// Nanoseconds per job to enqueue `queued` jobs and drain them `BATCH` at a time.
fn drain_cost_ns(queued: usize) -> f64 {
    let mut samples: Vec<f64> = (0..PASSES)
        .map(|_| {
            let cache =
                MultiLayerCache::try_with_options(CacheOptions::new(1 << 24, 0, 0)).expect("cache");
            cache.set_async_writeback_queue_limit_for_test(queued * 2 + 16);

            let entries: Vec<(CacheKey, Vec<u8>)> = (0..queued)
                .map(|i| {
                    (
                        CacheKey::string(0, &format!("job-{i:08}")),
                        vec![b'w'; VALUE_BYTES],
                    )
                })
                .collect();
            cache
                .enqueue_async_writeback_batch(entries)
                .expect("enqueue");

            // Time only the draining.
            let started = Instant::now();
            loop {
                let report = cache.drain_async_writeback(BATCH).expect("drain");
                if report.drained == 0 {
                    break;
                }
            }
            started.elapsed().as_nanos() as f64 / queued as f64
        })
        .collect();
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    samples[samples.len() / 2]
}

fn main() {
    println!(
        "write-back drain cost against queue depth, batches of {BATCH}, median of {PASSES} passes\n"
    );
    println!("{:>10}{:>14}{:>12}", "queued", "ns/job", "vs base");

    let mut baseline = 0.0;
    for (index, queued) in [256usize, 1024, 4096, 16384].into_iter().enumerate() {
        let ns = drain_cost_ns(queued);
        if index == 0 {
            baseline = ns;
        }
        println!(
            "{:>10}{:>14.1}{:>11.2}x",
            queued,
            ns,
            ns / baseline.max(f64::MIN_POSITIVE)
        );
    }

    println!("\nA drain handles its own batch, so this column should be flat.");
}
