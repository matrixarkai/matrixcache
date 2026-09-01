// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI
//
// What one line in the persistent-memory manifest costs.
//
// Every put and every delete on the persistent tier appends a line, and the
// expiry sweep appends one per reclaimed entry whether or not the entry was
// ever on that tier. Counting the syscalls per line is the point: run it under
// `strace -c -f` and read openat, close, fsync and the directory calls.
//
// **Pass `keep` as the fourth argument when profiling.** Otherwise the run ends
// by deleting everything it wrote, and those unlinks land in the profile: one
// per SSD block and one per persistent block, which is roughly two per put and
// is the benchmark tidying up rather than the cache doing anything. Traced at
// 200 puts, the teardown was 200 `.cache_block` and 136 `.bin` unlinks -- all
// of it cleanup, none of it the write path.
//
//   arguments: <entries> <ssd durability 0|1> <pmem durability 0|1> [keep]

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
    // Fourth argument: leave the directory behind, so the teardown's unlinks
    // stay out of a syscall profile of this run.
    let keep = std::env::args().nth(4).is_some_and(|arg| arg == "keep");
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
    // Left in place when asked, so a syscall profile measures the writes and
    // not the tidying up afterwards.
    if keep {
        println!("  left {} in place", root.display());
    } else {
        let _ = std::fs::remove_dir_all(&root);
    }
}
