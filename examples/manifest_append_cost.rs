// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI
//
// What one line in the persistent-memory manifest costs.
//
// Every put and every delete on the persistent tier appends a line, and the
// expiry sweep appends one per reclaimed entry whether or not the entry was
// ever on that tier. Counting the syscalls per line is the point: run this
// under  and read openat, close and the directory calls.

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};

fn main() {
    let root =
        std::env::temp_dir().join(format!("matrixcache-manifest-cost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // `durable` turns the two `fsync` calls per block write on and off, so a
    // run reports what crash durability costs on this path rather than leaving
    // it to be guessed.
    let durable = std::env::args()
        .nth(2)
        .map(|arg| arg != "0")
        .unwrap_or(true);
    // Third argument: whether the persistent tier flushes too. It does not by
    // default, so this is what turning that on would cost.
    let pmem_durable = std::env::args()
        .nth(3)
        .map(|arg| arg != "0")
        .unwrap_or(false);
    let cache = MultiLayerCache::with_options(CacheOptions {
        dram_capacity: 1 << 14,
        pmem_capacity: 1 << 22,
        pmem_paths: vec![root.join("pmem")],
        ssd_capacity: 1 << 22,
        ssd_paths: vec![root.join("ssd")],
        ssd_block_durability: durable,
        pmem_block_durability: pmem_durable,
        ..CacheOptions::default()
    });
    cache.start().unwrap();

    let entries: usize = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(2_000);

    // Memory is small, so these are demoted to the persistent tier as they go:
    // one manifest line each, and more as the tier turns over.
    let started = std::time::Instant::now();
    for index in 0..entries {
        cache
            .put(CacheKey::string(0, &format!("k{index:06}")), vec![7u8; 256])
            .unwrap();
    }
    let elapsed = started.elapsed();
    println!(
        "{entries} puts in {:?} ({:.1} us/put), pmem fills {}, block durability {}",
        elapsed,
        elapsed.as_secs_f64() * 1e6 / entries as f64,
        cache.stats().pmem_fills,
        if durable { "on" } else { "off" }
    );
    println!(
        "  persistent-tier durability {}",
        if pmem_durable { "on" } else { "off" }
    );
    let _ = std::fs::remove_dir_all(&root);
}
