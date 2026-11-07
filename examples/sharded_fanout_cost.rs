// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Measures repeated large resident reads that span cache shards.
//!
//! Setup and warmup are outside the samples. The benchmark isolates worker
//! dispatch and merge overhead from SSD initialization and value insertion.
//!
//! ```text
//! cargo run --release --no-default-features --example sharded_fanout_cost
//! ```

use matrixcache::{CacheKey, CacheOptions, ShardedMultiLayerCache};
use std::time::Instant;

const KEY_COUNT: usize = 4_096;
const VALUE_BYTES: usize = 64;
const PASSES: usize = 31;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|left, right| left.partial_cmp(right).expect("finite samples"));
    samples[samples.len() / 2]
}

fn main() {
    let cache = ShardedMultiLayerCache::try_with_options(
        CacheOptions::new(KEY_COUNT * VALUE_BYTES * 4, 0, 0),
        16,
    )
    .expect("sharded cache");
    let keys = (0..KEY_COUNT)
        .map(|index| CacheKey::string(index as u64, &format!("fanout-{index:05}")))
        .collect::<Vec<_>>();
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("resident insert");
    }

    for batch_size in [2_048usize, KEY_COUNT] {
        let batch = &keys[..batch_size];
        for _ in 0..3 {
            let values = cache.get_batch(batch).expect("warmup read");
            assert_eq!(
                values.iter().filter(|value| value.is_some()).count(),
                batch_size
            );
        }

        let ns_per_key = median(
            (0..PASSES)
                .map(|_| {
                    let started = Instant::now();
                    let values = cache.get_batch(batch).expect("measured read");
                    assert_eq!(
                        values.iter().filter(|value| value.is_some()).count(),
                        batch_size
                    );
                    started.elapsed().as_nanos() as f64 / batch_size as f64
                })
                .collect(),
        );
        let qps = 1_000_000_000.0 / ns_per_key;
        let mut acquire_samples = Vec::with_capacity(PASSES);
        for _ in 0..PASSES {
            let started = Instant::now();
            let handles = cache
                .acquire_batch_no_promotion(batch)
                .expect("measured pinned read");
            let elapsed = started.elapsed().as_nanos() as f64 / batch_size as f64;
            assert_eq!(
                handles.iter().filter(|value| value.is_some()).count(),
                batch_size
            );
            cache.release_batch_iter(handles.into_iter().flatten());
            acquire_samples.push(elapsed);
        }
        let acquire_ns_per_key = median(acquire_samples);
        let acquire_qps = 1_000_000_000.0 / acquire_ns_per_key;
        println!(
            "batch={batch_size} copied_ns_per_key={ns_per_key:.1} copied_qps={qps:.0} \
             pinned_ns_per_key={acquire_ns_per_key:.1} pinned_qps={acquire_qps:.0}"
        );
    }
}
