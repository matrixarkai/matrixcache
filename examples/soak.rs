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
//! cargo run --release --no-default-features --example soak -- <minutes> <threads> [--json] [--json-output PATH] [--require-passed] [--sample-seconds N] [--duration-seconds N] [--max-get-p99-us N] [--max-put-p99-us N] [--min-hit-rate-percent N]
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 256;
const KEY_SPACE: usize = 32_768;
/// Room for a quarter of the key space, so eviction runs continuously.
const RESIDENT: usize = KEY_SPACE / 4;
const DEFAULT_SAMPLE_SECONDS: u64 = 60;

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
    let mut positional = Vec::new();
    let mut emit_json = false;
    let mut json_output: Option<PathBuf> = None;
    let mut require_passed = false;
    let mut sample_seconds = DEFAULT_SAMPLE_SECONDS;
    let mut duration_seconds = None;
    let mut max_get_p99_us = None;
    let mut max_put_p99_us = None;
    let mut min_hit_rate_percent = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => emit_json = true,
            "--json-output" => {
                json_output = args.next().map(PathBuf::from);
            }
            "--require-passed" => require_passed = true,
            "--sample-seconds" => {
                sample_seconds = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(DEFAULT_SAMPLE_SECONDS);
            }
            "--duration-seconds" => {
                duration_seconds = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0);
            }
            "--max-get-p99-us" => {
                max_get_p99_us = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0);
            }
            "--max-put-p99-us" => {
                max_put_p99_us = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0);
            }
            "--min-hit-rate-percent" => {
                min_hit_rate_percent = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| (0.0..=100.0).contains(value));
            }
            _ => positional.push(arg),
        }
    }
    let minutes: u64 = positional
        .first()
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(480);
    let threads: usize = positional
        .get(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(4);
    let total_duration = Duration::from_secs(duration_seconds.unwrap_or(minutes * 60));

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
    let mut hit_rates: Vec<f64> = Vec::new();
    let mut peak_entries = 0_usize;
    let mut peak_memory_bytes = 0_u64;

    while started.elapsed() < total_duration {
        let remaining = total_duration.saturating_sub(started.elapsed());
        std::thread::sleep(Duration::from_secs(sample_seconds).min(remaining));

        let now_reads = reads.load(Ordering::Relaxed);
        let stats = cache.stats();
        let elapsed = last_at.elapsed().as_secs_f64();
        last_at = Instant::now();

        let interval_reads = now_reads - last_reads;
        let interval_hits = stats.memory_hits - last_hits;
        let interval_misses = stats.misses - last_misses;
        if interval_reads == 0 && started.elapsed() >= total_duration {
            break;
        }
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
        hit_rates.push(hit_rate);
        peak_entries = peak_entries.max(entries);
        peak_memory_bytes = peak_memory_bytes.max(stats.memory_bytes);

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
    let mut thirds = Vec::new();
    for (index, chunk) in rates.chunks(window).take(3).enumerate() {
        let best = chunk.iter().copied().fold(f64::MIN, f64::max);
        let worst = chunk.iter().copied().fold(f64::MAX, f64::min);
        thirds.push((best, worst));
        println!(
            "  window {}: ceiling {best:6.1}   floor {worst:6.1}",
            index + 1
        );
    }
    let latency = cache.latency_metrics_report();
    println!(
        "get latency p99 {}us max {}us over {} samples",
        latency.get_p99_us, stats.get_latency_max_micros, stats.get_latency_samples
    );
    println!(
        "put latency p99 {}us max {}us over {} samples",
        latency.put_p99_us, stats.put_latency_max_micros, stats.put_latency_samples
    );

    if emit_json || json_output.is_some() || require_passed {
        let final_entries = cache.all_entries().len();
        let final_reads = reads.load(Ordering::Relaxed);
        let final_writes = writes.load(Ordering::Relaxed);
        let observed_hit_rate = if stats.memory_hits + stats.misses == 0 {
            0.0
        } else {
            stats.memory_hits as f64 / (stats.memory_hits + stats.misses) as f64 * 100.0
        };
        let best_rate = rates.iter().copied().fold(0.0_f64, f64::max);
        let worst_rate = rates.iter().copied().fold(f64::MAX, f64::min);
        let worst_rate = if rates.is_empty() { 0.0 } else { worst_rate };
        let min_hit_rate = hit_rates.iter().copied().fold(f64::MAX, f64::min);
        let min_hit_rate = if hit_rates.is_empty() {
            0.0
        } else {
            min_hit_rate
        };
        let max_hit_rate = hit_rates.iter().copied().fold(0.0_f64, f64::max);
        let bounded_entries = peak_entries <= RESIDENT + 64;
        let bounded_memory =
            peak_memory_bytes as usize <= RESIDENT * VALUE_BYTES + VALUE_BYTES * 64;
        let get_p99_within_budget = max_get_p99_us
            .map(|budget| latency.get_p99_us <= budget)
            .unwrap_or(true);
        let put_p99_within_budget = max_put_p99_us
            .map(|budget| latency.put_p99_us <= budget)
            .unwrap_or(true);
        let hit_rate_within_budget = min_hit_rate_percent
            .map(|budget| observed_hit_rate >= budget)
            .unwrap_or(true);
        let steady_throughput = match (thirds.first(), thirds.last()) {
            _ if thirds.len() < 3 => true,
            (Some((first_best, _)), Some((last_best, _))) if *first_best > 0.0 => {
                *last_best >= *first_best * 0.80
            }
            _ => true,
        };
        let passed = bounded_entries
            && bounded_memory
            && steady_throughput
            && get_p99_within_budget
            && put_p99_within_budget
            && hit_rate_within_budget;

        let mut report = String::new();
        writeln!(&mut report, "{{").expect("format report");
        writeln!(
            &mut report,
            "  \"report_version\": \"matrixcache_soak_v1\","
        )
        .expect("format report");
        writeln!(&mut report, "  \"minutes\": {minutes},").expect("format report");
        writeln!(&mut report, "  \"threads\": {threads},").expect("format report");
        writeln!(&mut report, "  \"key_space\": {KEY_SPACE},").expect("format report");
        writeln!(&mut report, "  \"resident_capacity_entries\": {RESIDENT},")
            .expect("format report");
        writeln!(&mut report, "  \"value_bytes\": {VALUE_BYTES},").expect("format report");
        writeln!(&mut report, "  \"sample_seconds\": {sample_seconds},").expect("format report");
        writeln!(
            &mut report,
            "  \"duration_seconds\": {},",
            total_duration.as_secs()
        )
        .expect("format report");
        writeln!(
            &mut report,
            "  \"max_get_p99_us\": {},",
            option_u64_json(max_get_p99_us)
        )
        .expect("format report");
        writeln!(
            &mut report,
            "  \"max_put_p99_us\": {},",
            option_u64_json(max_put_p99_us)
        )
        .expect("format report");
        writeln!(
            &mut report,
            "  \"min_hit_rate_percent\": {},",
            option_f64_json(min_hit_rate_percent)
        )
        .expect("format report");
        writeln!(&mut report, "  \"reads\": {final_reads},").expect("format report");
        writeln!(&mut report, "  \"writes\": {final_writes},").expect("format report");
        writeln!(&mut report, "  \"final_entries\": {final_entries},").expect("format report");
        writeln!(&mut report, "  \"peak_entries\": {peak_entries},").expect("format report");
        writeln!(
            &mut report,
            "  \"final_memory_bytes\": {},",
            stats.memory_bytes
        )
        .expect("format report");
        writeln!(&mut report, "  \"peak_memory_bytes\": {peak_memory_bytes},")
            .expect("format report");
        writeln!(&mut report, "  \"memory_hits\": {},", stats.memory_hits).expect("format report");
        writeln!(&mut report, "  \"misses\": {},", stats.misses).expect("format report");
        writeln!(
            &mut report,
            "  \"memory_evictions\": {},",
            stats.memory_evictions
        )
        .expect("format report");
        writeln!(
            &mut report,
            "  \"observed_hit_rate_percent\": {observed_hit_rate:.4},"
        )
        .expect("format report");
        writeln!(&mut report, "  \"interval_best_kops\": {best_rate:.4},").expect("format report");
        writeln!(&mut report, "  \"interval_worst_kops\": {worst_rate:.4},")
            .expect("format report");
        writeln!(
            &mut report,
            "  \"interval_min_hit_rate_percent\": {min_hit_rate:.4},"
        )
        .expect("format report");
        writeln!(
            &mut report,
            "  \"interval_max_hit_rate_percent\": {max_hit_rate:.4},"
        )
        .expect("format report");
        writeln!(&mut report, "  \"throughput_thirds\": [").expect("format report");
        for (index, (best, worst)) in thirds.iter().enumerate() {
            let comma = if index + 1 == thirds.len() { "" } else { "," };
            writeln!(
                &mut report,
                "    {{\"window\": {}, \"ceiling_kops\": {:.4}, \"floor_kops\": {:.4}}}{comma}",
                index + 1,
                best,
                worst
            )
            .expect("format report");
        }
        writeln!(&mut report, "  ],").expect("format report");
        writeln!(&mut report, "  \"latency\": {{").expect("format report");
        writeln!(&mut report, "    \"get_count\": {},", latency.get_count).expect("format report");
        writeln!(&mut report, "    \"get_avg_us\": {},", latency.get_avg_us)
            .expect("format report");
        writeln!(&mut report, "    \"get_p50_us\": {},", latency.get_p50_us)
            .expect("format report");
        writeln!(&mut report, "    \"get_p95_us\": {},", latency.get_p95_us)
            .expect("format report");
        writeln!(&mut report, "    \"get_p99_us\": {},", latency.get_p99_us)
            .expect("format report");
        writeln!(&mut report, "    \"get_max_us\": {},", latency.get_max_us)
            .expect("format report");
        writeln!(&mut report, "    \"put_count\": {},", latency.put_count).expect("format report");
        writeln!(&mut report, "    \"put_avg_us\": {},", latency.put_avg_us)
            .expect("format report");
        writeln!(&mut report, "    \"put_p50_us\": {},", latency.put_p50_us)
            .expect("format report");
        writeln!(&mut report, "    \"put_p95_us\": {},", latency.put_p95_us)
            .expect("format report");
        writeln!(&mut report, "    \"put_p99_us\": {},", latency.put_p99_us)
            .expect("format report");
        writeln!(&mut report, "    \"put_max_us\": {},", latency.put_max_us)
            .expect("format report");
        writeln!(
            &mut report,
            "    \"read_through_count\": {},",
            latency.read_through_count
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"read_through_avg_us\": {},",
            latency.read_through_avg_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"read_through_p50_us\": {},",
            latency.read_through_p50_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"read_through_p95_us\": {},",
            latency.read_through_p95_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"read_through_p99_us\": {},",
            latency.read_through_p99_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"read_through_max_us\": {},",
            latency.read_through_max_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"refill_count\": {},",
            latency.refill_count
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"refill_avg_us\": {},",
            latency.refill_avg_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"refill_p50_us\": {},",
            latency.refill_p50_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"refill_p95_us\": {},",
            latency.refill_p95_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"refill_p99_us\": {},",
            latency.refill_p99_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"refill_max_us\": {},",
            latency.refill_max_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"writeback_count\": {},",
            latency.writeback_count
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"writeback_avg_us\": {},",
            latency.writeback_avg_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"writeback_p50_us\": {},",
            latency.writeback_p50_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"writeback_p95_us\": {},",
            latency.writeback_p95_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"writeback_p99_us\": {},",
            latency.writeback_p99_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"writeback_max_us\": {},",
            latency.writeback_max_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"eviction_count\": {},",
            latency.eviction_count
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"eviction_avg_us\": {},",
            latency.eviction_avg_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"eviction_p50_us\": {},",
            latency.eviction_p50_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"eviction_p95_us\": {},",
            latency.eviction_p95_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"eviction_p99_us\": {},",
            latency.eviction_p99_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"eviction_max_us\": {},",
            latency.eviction_max_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"compaction_count\": {},",
            latency.compaction_count
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"compaction_avg_us\": {},",
            latency.compaction_avg_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"compaction_p50_us\": {},",
            latency.compaction_p50_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"compaction_p95_us\": {},",
            latency.compaction_p95_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"compaction_p99_us\": {},",
            latency.compaction_p99_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"compaction_max_us\": {},",
            latency.compaction_max_us
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"histogram_ready\": {}",
            latency.histogram_ready
        )
        .expect("format report");
        writeln!(&mut report, "  }},").expect("format report");
        writeln!(&mut report, "  \"checks\": {{").expect("format report");
        writeln!(&mut report, "    \"bounded_entries\": {bounded_entries},")
            .expect("format report");
        writeln!(&mut report, "    \"bounded_memory\": {bounded_memory},").expect("format report");
        writeln!(
            &mut report,
            "    \"steady_throughput_ceiling\": {steady_throughput},"
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"get_p99_within_budget\": {get_p99_within_budget},"
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"put_p99_within_budget\": {put_p99_within_budget},"
        )
        .expect("format report");
        writeln!(
            &mut report,
            "    \"hit_rate_within_budget\": {hit_rate_within_budget}"
        )
        .expect("format report");
        writeln!(&mut report, "  }},").expect("format report");
        writeln!(&mut report, "  \"passed\": {passed}").expect("format report");
        writeln!(&mut report, "}}").expect("format report");

        if emit_json {
            print!("\n{report}");
        }
        if let Some(path) = json_output {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent).expect("create JSON output directory");
            }
            std::fs::write(&path, &report).expect("write JSON soak report");
            eprintln!("matrixcache soak report written to {}", path.display());
        }

        if require_passed && !passed {
            eprintln!("matrixcache soak gate failed; see JSON checks for the failing condition");
            std::process::exit(1);
        }
    }
}

fn option_u64_json(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn option_f64_json(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.4}"))
        .unwrap_or_else(|| "null".to_string())
}
