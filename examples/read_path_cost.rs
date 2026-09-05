// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Where the time goes on a memory-tier hit.
//!
//! `cache_scaling_bench` shows that a hit costs upwards of a microsecond and
//! that read throughput *falls* as threads are added. This narrows that down by
//! putting three paths that answer the same question side by side:
//!
//! * `get_no_promotion` — takes the cache lock **shared** and does no
//!   bookkeeping. This is the floor: a hash lookup, an `Arc` clone and a copy
//!   of the value.
//! * `peek_tier` — shared lock, no value copy at all.
//! * `get` — takes the lock **exclusively**, updates hit counters, hotness and
//!   two latency histograms, and separately takes a *second*, shared
//!   acquisition first to see whether an access-record callback is registered.
//!
//! The gap between the first and the last is what the bookkeeping costs, and it
//! is paid on every read whether or not anything consumes it.
//!
//! Timing on a shared machine is noisy, so every pass measures all three paths
//! back to back and the reported overhead is the median of the **per-pass**
//! figures. Measuring each path in its own block of passes and then dividing
//! the medians compares three different time windows, and on a machine whose
//! load is moving that inverts: the same binary reported the bookkeeping as
//! -5% of a read at load 12.7 and 65-88% at load 4, and only the second is
//! true. The order of the three rotates between passes so none of them is
//! always the one that warms the caches.
//!
//! The spread across passes is printed alongside the median. A run whose
//! spread is wide, or whose sign changes, measured the machine rather than the
//! cache, and the number should be thrown away rather than quoted.
//!
//! ```text
//! cargo run --release --no-default-features --example read_path_cost
//! cargo run --release --no-default-features --example read_path_cost -- 16384
//! cargo run --release --no-default-features --example read_path_cost -- 4096 --json-output /tmp/matrixcache-read-path.json --require-passed
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

const VALUE_BYTES: usize = 64;
const PASSES: usize = 7;

fn bench_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("matrixcache-readpath-{name}"))
}

fn build(entries: usize, dir: &std::path::Path) -> (MultiLayerCache, Vec<CacheKey>) {
    let _ = std::fs::remove_dir_all(dir);
    // Memory capacity generous enough that nothing is evicted: this measures
    // the hit path, not the eviction path.
    let options = CacheOptions::new(entries * VALUE_BYTES * 4, 0, 1 << 20)
        .with_ssd_paths([dir.to_path_buf()]);
    let cache = MultiLayerCache::try_with_options(options).expect("cache");
    let keys: Vec<CacheKey> = (0..entries)
        .map(|i| CacheKey::string(0, &format!("key-{i:08}")))
        .collect();
    for key in &keys {
        cache
            .put(key.clone(), vec![b'v'; VALUE_BYTES])
            .expect("put");
    }
    (cache, keys)
}

/// Nanoseconds per operation for one sweep of the keys.
fn time_ns(mut run: impl FnMut(&[CacheKey]) -> u64, keys: &[CacheKey]) -> f64 {
    let started = Instant::now();
    let observed = run(keys);
    std::hint::black_box(observed);
    started.elapsed().as_nanos() as f64 / keys.len() as f64
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    sorted[sorted.len() / 2]
}

/// What one pass measured, with all three paths inside the same time window.
struct PassTimings {
    peek: f64,
    no_promotion: f64,
    full: f64,
}

fn option_f64_json(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.4}"))
        .unwrap_or_else(|| "null".to_string())
}

fn write_json_report(
    path: Option<&PathBuf>,
    entries: usize,
    peek_ns: f64,
    no_promotion_ns: f64,
    full_ns: f64,
    overhead_ns: f64,
    overhead_median_percent: f64,
    overhead_low_percent: f64,
    overhead_high_percent: f64,
    max_full_ns: Option<f64>,
    max_overhead_percent: Option<f64>,
    max_spread_percent: Option<f64>,
    passed: bool,
) -> String {
    let spread_percent = overhead_high_percent - overhead_low_percent;
    let mut report = String::new();
    writeln!(&mut report, "{{").expect("format report");
    writeln!(
        &mut report,
        "  \"report_version\": \"matrixcache_read_path_v1\","
    )
    .expect("format report");
    writeln!(&mut report, "  \"entries\": {entries},").expect("format report");
    writeln!(&mut report, "  \"value_bytes\": {VALUE_BYTES},").expect("format report");
    writeln!(&mut report, "  \"passes\": {PASSES},").expect("format report");
    writeln!(&mut report, "  \"peek_ns_per_op\": {peek_ns:.4},").expect("format report");
    writeln!(
        &mut report,
        "  \"no_promotion_ns_per_op\": {no_promotion_ns:.4},"
    )
    .expect("format report");
    writeln!(&mut report, "  \"full_ns_per_op\": {full_ns:.4},").expect("format report");
    writeln!(&mut report, "  \"overhead_ns_per_op\": {overhead_ns:.4},").expect("format report");
    writeln!(
        &mut report,
        "  \"overhead_median_percent\": {overhead_median_percent:.4},"
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"overhead_low_percent\": {overhead_low_percent:.4},"
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"overhead_high_percent\": {overhead_high_percent:.4},"
    )
    .expect("format report");
    writeln!(&mut report, "  \"spread_percent\": {spread_percent:.4},").expect("format report");
    writeln!(
        &mut report,
        "  \"max_full_ns\": {},",
        option_f64_json(max_full_ns)
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"max_overhead_percent\": {},",
        option_f64_json(max_overhead_percent)
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"max_spread_percent\": {},",
        option_f64_json(max_spread_percent)
    )
    .expect("format report");
    writeln!(&mut report, "  \"checks\": {{").expect("format report");
    writeln!(
        &mut report,
        "    \"positive_timings\": {},",
        peek_ns > 0.0 && no_promotion_ns > 0.0 && full_ns > 0.0
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"full_hit_within_budget\": {},",
        max_full_ns.map(|limit| full_ns <= limit).unwrap_or(true)
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"overhead_within_budget\": {},",
        max_overhead_percent
            .map(|limit| overhead_median_percent <= limit)
            .unwrap_or(true)
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"spread_within_budget\": {}",
        max_spread_percent
            .map(|limit| spread_percent <= limit)
            .unwrap_or(true)
    )
    .expect("format report");
    writeln!(&mut report, "  }},").expect("format report");
    writeln!(&mut report, "  \"passed\": {passed}").expect("format report");
    writeln!(&mut report, "}}").expect("format report");
    if let Some(path) = path {
        std::fs::write(path, &report).expect("write json report");
    }
    report
}

fn main() {
    let mut positional = Vec::new();
    let mut emit_json = false;
    let mut json_output: Option<PathBuf> = None;
    let mut require_passed = false;
    let mut max_full_ns: Option<f64> = None;
    let mut max_overhead_percent: Option<f64> = None;
    let mut max_spread_percent: Option<f64> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => emit_json = true,
            "--json-output" => json_output = args.next().map(PathBuf::from),
            "--require-passed" => require_passed = true,
            "--max-full-ns" => {
                max_full_ns = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0.0);
            }
            "--max-overhead-percent" => {
                max_overhead_percent = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value >= 0.0);
            }
            "--max-spread-percent" => {
                max_spread_percent = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value >= 0.0);
            }
            _ => positional.push(arg),
        }
    }
    let entries: usize = positional
        .first()
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(4096);

    println!("memory-tier hit, {entries} resident entries, median of {PASSES} passes\n");
    let dir = bench_dir("paths");
    let (cache, keys) = build(entries, &dir);

    // Warm the allocator and the page tables so the first path measured does
    // not pay for everything that follows.
    for key in &keys {
        std::hint::black_box(cache.get(key).expect("get"));
    }

    let peek_pass = |keys: &[CacheKey]| {
        let mut seen = 0u64;
        for key in keys {
            if cache.peek_tier(key).is_some() {
                seen += 1;
            }
        }
        seen
    };
    let no_promotion_pass = |keys: &[CacheKey]| {
        let mut seen = 0u64;
        for key in keys {
            if cache.get_no_promotion(key).expect("get").is_some() {
                seen += 1;
            }
        }
        seen
    };
    let full_pass = |keys: &[CacheKey]| {
        let mut seen = 0u64;
        for key in keys {
            if cache.get(key).expect("get").is_some() {
                seen += 1;
            }
        }
        seen
    };

    let mut passes = Vec::with_capacity(PASSES);
    for index in 0..PASSES {
        // Rotate which path goes first. Whichever runs first pays for warming
        // whatever the pass before it evicted, and a fixed order hands that
        // cost to the same path every time.
        let (peek, no_promotion, full) = match index % 3 {
            0 => {
                let peek = time_ns(peek_pass, &keys);
                let no_promotion = time_ns(no_promotion_pass, &keys);
                let full = time_ns(full_pass, &keys);
                (peek, no_promotion, full)
            }
            1 => {
                let no_promotion = time_ns(no_promotion_pass, &keys);
                let full = time_ns(full_pass, &keys);
                let peek = time_ns(peek_pass, &keys);
                (peek, no_promotion, full)
            }
            _ => {
                let full = time_ns(full_pass, &keys);
                let peek = time_ns(peek_pass, &keys);
                let no_promotion = time_ns(no_promotion_pass, &keys);
                (peek, no_promotion, full)
            }
        };
        passes.push(PassTimings {
            peek,
            no_promotion,
            full,
        });
    }

    let peek = median(&passes.iter().map(|p| p.peek).collect::<Vec<_>>());
    let no_promotion = median(&passes.iter().map(|p| p.no_promotion).collect::<Vec<_>>());
    let full = median(&passes.iter().map(|p| p.full).collect::<Vec<_>>());

    // The overhead is computed inside each pass and then summarised, so it
    // never compares two different time windows.
    let mut overheads: Vec<f64> = passes
        .iter()
        .map(|p| 100.0 * (p.full - p.no_promotion) / p.full.max(f64::MIN_POSITIVE))
        .collect();
    overheads.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    let overhead_median = overheads[overheads.len() / 2];
    let (overhead_low, overhead_high) = (overheads[0], overheads[overheads.len() - 1]);
    let overhead_ns = median(
        &passes
            .iter()
            .map(|p| p.full - p.no_promotion)
            .collect::<Vec<_>>(),
    );

    println!("{:<34}{:>12}{:>12}", "path", "ns/op", "vs floor");
    println!(
        "{:<34}{:>12.1}{:>12}",
        "peek_tier (shared, no copy)", peek, "-"
    );
    println!(
        "{:<34}{:>12.1}{:>11.2}x",
        "get_no_promotion (shared)",
        no_promotion,
        no_promotion / peek.max(f64::MIN_POSITIVE)
    );
    println!(
        "{:<34}{:>12.1}{:>11.2}x",
        "get (exclusive + bookkeeping)",
        full,
        full / peek.max(f64::MIN_POSITIVE)
    );
    println!(
        "\nbookkeeping and the exclusive lock cost {overhead_ns:.1} ns/op \
         ({overhead_median:.0}% of a read)"
    );
    println!(
        "per-pass spread across {PASSES} passes: {overhead_low:.0}%..{overhead_high:.0}%{}",
        if overhead_low <= 0.0 || overhead_high - overhead_low > 40.0 {
            "  <- too wide to quote; the machine was busy"
        } else {
            ""
        }
    );

    let spread_percent = overhead_high - overhead_low;
    let passed = peek > 0.0
        && no_promotion > 0.0
        && full > 0.0
        && max_full_ns.map(|limit| full <= limit).unwrap_or(true)
        && max_overhead_percent
            .map(|limit| overhead_median <= limit)
            .unwrap_or(true)
        && max_spread_percent
            .map(|limit| spread_percent <= limit)
            .unwrap_or(true);
    if emit_json || json_output.is_some() || require_passed {
        let report = write_json_report(
            json_output.as_ref(),
            entries,
            peek,
            no_promotion,
            full,
            overhead_ns,
            overhead_median,
            overhead_low,
            overhead_high,
            max_full_ns,
            max_overhead_percent,
            max_spread_percent,
            passed,
        );
        if emit_json {
            print!("{report}");
        }
        if let Some(path) = &json_output {
            println!("wrote {}", path.display());
        }
        println!(
            "read path gate: {}",
            if passed { "passed" } else { "failed" }
        );
        if require_passed && !passed {
            std::process::exit(1);
        }
    }

    drop(cache);
    let _ = std::fs::remove_dir_all(&dir);
}
