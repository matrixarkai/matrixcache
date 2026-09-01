// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! A long-running mixed workload that reports whether the cache holds up.
//!
//! Benchmarks answer "how fast is this right now" in a few seconds. There is a
//! class of defect they cannot see at all, because it needs hours of real
//! traffic to become visible:
//!
//! * **Bookkeeping that outlives its entry.** The read path admits a hit under
//!   a shared lock and finishes its per-entry accounting under an exclusive
//!   one, so an entry can be evicted in between. Every such site is guarded,
//!   but a guard that is wrong leaks one metadata record per race — invisible
//!   in a unit test, unmistakable after six hours.
//! * **Throughput that decays.** A structure that degrades as it is churned —
//!   an access order that grows, a map that never rehashes down — reads as
//!   healthy in the first minute and not in the fifth hour.
//! * **Hit rate that drifts.** Eviction quality is a property of the steady
//!   state, and the steady state takes a while to reach.
//!
//! So this runs a fixed skewed workload across several threads and prints one
//! row per interval: throughput and hit rate **for that interval**, not
//! cumulative, because a cumulative average hides a decline. Alongside them go
//! the resident entry count and the byte total, which are what a leak moves.
//!
//! **Read the throughput ceiling, not the floor.** A slow interval means
//! either the cache is degrading or something else had the machine, and they
//! are indistinguishable in any single interval. The summary therefore reports
//! the best and worst rate in each third of the run: a falling ceiling is
//! decay, while a moving floor under a flat ceiling is contention. The first
//! eight-hour run here would have looked like a 4x collapse by its worst
//! interval and was in fact flat at 8.5 / 8.6 / 8.5 Kops/s at the ceiling.
//!
//! Everything is memory-only and bounded, so it does not compete for the disk
//! this machine shares.
//!
//! ```text
//! cargo run --release --no-default-features --example soak -- <minutes> <threads>
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 256;
const KEY_SPACE: usize = 32_768;
/// Room for a quarter of the key space, so eviction runs continuously.
const RESIDENT: usize = KEY_SPACE / 4;
const SAMPLE_SECONDS: u64 = 60;

/// Deterministic skew: most draws land in the low keys.
fn skewed_index(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let unit = ((*state >> 11) as f64) / ((1u64 << 53) as f64);
    let skewed = unit * unit * unit;
    ((skewed * KEY_SPACE as f64) as usize).min(KEY_SPACE - 1)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let minutes: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(480);
    let threads: usize = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(4);

    let cache = Arc::new(
        MultiLayerCache::try_with_options(CacheOptions::new(RESIDENT * VALUE_BYTES, 0, 0))
            .expect("cache"),
    );
    // A refresh distance the workload can actually benefit from; zero would
    // send every hit through the exclusive path and measure only that.
    cache.set_lru_refresh_time(Duration::from_millis(500));

    let stop = Arc::new(AtomicBool::new(false));
    let reads = Arc::new(AtomicU64::new(0));
    let writes = Arc::new(AtomicU64::new(0));

    let workers = (0..threads)
        .map(|worker| {
            let cache = Arc::clone(&cache);
            let stop = Arc::clone(&stop);
            let reads = Arc::clone(&reads);
            let writes = Arc::clone(&writes);
            std::thread::spawn(move || {
                let mut state = 0x2545_F491_4F6C_DD1D ^ ((worker as u64) << 32);
                let mut local_reads = 0_u64;
                let mut local_writes = 0_u64;
                while !stop.load(Ordering::Relaxed) {
                    for _ in 0..1_000 {
                        let index = skewed_index(&mut state);
                        let key = CacheKey::string(0, &format!("soak-{index:06}"));
                        match cache.get(&key) {
                            Ok(Some(value)) => {
                                assert_eq!(
                                    value.len(),
                                    VALUE_BYTES,
                                    "key {index} came back the wrong size"
                                );
                                local_reads += 1;
                            }
                            Ok(None) => {
                                cache.put(key, vec![b'v'; VALUE_BYTES]).expect("put");
                                local_reads += 1;
                                local_writes += 1;
                            }
                            Err(err) => panic!("read failed: {err:?}"),
                        }
                    }
                    reads.fetch_add(local_reads, Ordering::Relaxed);
                    writes.fetch_add(local_writes, Ordering::Relaxed);
                    local_reads = 0;
                    local_writes = 0;
                }
            })
        })
        .collect::<Vec<_>>();

    println!(
        "soak: {minutes} minutes, {threads} threads, {KEY_SPACE} keys, room for {RESIDENT}, \
         {VALUE_BYTES}-byte values"
    );
    println!(
        "{:>6}{:>12}{:>11}{:>12}{:>12}{:>12}",
        "min", "Kops/s", "hit rate", "entries", "MiB", "writes"
    );

    let started = Instant::now();
    let mut last_reads = 0_u64;
    let mut last_hits = 0_u64;
    let mut last_misses = 0_u64;
    let mut last_at = Instant::now();
    // Every interval's rate, so the summary can look at the shape rather than
    // at one number. See the note where they are reported.
    let mut rates: Vec<f64> = Vec::new();

    while started.elapsed() < Duration::from_secs(minutes * 60) {
        std::thread::sleep(Duration::from_secs(SAMPLE_SECONDS));

        let now_reads = reads.load(Ordering::Relaxed);
        let stats = cache.stats();
        let elapsed = last_at.elapsed().as_secs_f64();
        last_at = Instant::now();

        let interval_reads = now_reads - last_reads;
        let interval_hits = stats.memory_hits - last_hits;
        let interval_misses = stats.misses - last_misses;
        last_reads = now_reads;
        last_hits = stats.memory_hits;
        last_misses = stats.misses;

        let rate = interval_reads as f64 / elapsed / 1000.0;
        let looked_up = interval_hits + interval_misses;
        let hit_rate = if looked_up == 0 {
            0.0
        } else {
            interval_hits as f64 / looked_up as f64 * 100.0
        };
        let entries = cache.all_entries().len();

        rates.push(rate);

        println!(
            "{:>6}{:>12.1}{:>10.2}%{:>12}{:>12.1}{:>12}",
            started.elapsed().as_secs() / 60,
            rate,
            hit_rate,
            entries,
            stats.memory_bytes as f64 / (1024.0 * 1024.0),
            writes.load(Ordering::Relaxed),
        );

        // The invariants a soak exists to check. Entries are bounded by
        // capacity; a metadata record that outlives its entry would push this
        // past it and keep going.
        assert!(
            entries <= RESIDENT + 64,
            "resident entries {entries} exceeded capacity {RESIDENT} -- \
             bookkeeping is outliving its entries"
        );
        assert!(
            stats.memory_bytes as usize <= RESIDENT * VALUE_BYTES + VALUE_BYTES * 64,
            "memory bytes {} exceeded capacity",
            stats.memory_bytes
        );
    }

    stop.store(true, Ordering::Relaxed);
    for worker in workers {
        worker.join().expect("worker");
    }

    let stats = cache.stats();
    println!(
        "\ncompleted {} minutes: {} reads, {} writes, {} entries resident",
        started.elapsed().as_secs() / 60,
        reads.load(Ordering::Relaxed),
        writes.load(Ordering::Relaxed),
        cache.all_entries().len()
    );
    // Report the ceiling over successive windows, not the floor.
    //
    // A slow interval means either the cache is degrading or something else
    // had the machine, and the worst interval cannot tell those apart -- the
    // first eight-hour run here ended at 0.23x its first interval purely
    // because benchmarks were sharing the cores. The best interval in a window
    // is the one least disturbed by other load, so a falling ceiling is decay
    // and a moving floor under a flat ceiling is contention.
    let window = (rates.len() / 3).max(1);
    println!("\nthroughput by third, Kops/s (ceiling is the decay signal):");
    for (index, chunk) in rates.chunks(window).take(3).enumerate() {
        let best = chunk.iter().copied().fold(f64::MIN, f64::max);
        let worst = chunk.iter().copied().fold(f64::MAX, f64::min);
        println!(
            "  window {}: ceiling {best:6.1}   floor {worst:6.1}",
            index + 1
        );
    }
    println!(
        "get latency max {}us over {} samples",
        stats.get_latency_max_micros, stats.get_latency_samples
    );
}
