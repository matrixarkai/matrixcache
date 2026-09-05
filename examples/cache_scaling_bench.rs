// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Read-path cost and read concurrency for the multi-tier cache.
//!
//! Two things are worth watching on the read path, and they move
//! independently.
//!
//! The first is what a single memory-tier hit costs, and whether that cost
//! stays put as the resident set grows. A lookup that scans anything will show
//! up here as a number that climbs with the entry count.
//!
//! The second is what happens when several threads read at once. A cache that
//! serialises its readers gets *slower* per operation as threads are added
//! rather than faster in aggregate, and that shows up as throughput falling in
//! the scaling table below. Reads currently take the cache lock exclusively
//! because the hit updates statistics, hotness and latency histograms on the
//! way out, so this table is the measurement to beat when that changes.
//!
//! The third table puts `MultiLayerCache` next to `ShardedMultiLayerCache`
//! holding the same total capacity. The sharded cache spreads keys over
//! independent shards, so readers of different shards do not queue behind one
//! another. It is the supported answer to the contention in the second table,
//! and this comparison is what says whether it earns its extra bookkeeping.
//!
//! ```text
//! cargo run --release --example cache_scaling_bench
//! cargo run --release --example cache_scaling_bench -- 8192
//! cargo run --release --no-default-features --example cache_scaling_bench -- 1024 --json-output /tmp/matrixcache-read-scaling.json --require-passed
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache, ShardedMultiLayerCache};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const VALUE_BYTES: usize = 64;
const REPEATS: usize = 5;
const SHARDS: usize = 16;

fn bench_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("matrixcache-scaling-{name}"))
}

/// Keys plus a visit order that touches each one but not in insertion order.
fn workload(entries: usize) -> Vec<CacheKey> {
    (0..entries)
        .map(|index| CacheKey::string(0, &format!("scaling-key-{index:010}")))
        .collect()
}

fn scattered(index: usize, len: usize) -> usize {
    index.wrapping_mul(2_654_435_761) % len.max(1)
}

fn ns_per_op(elapsed: Duration, ops: usize) -> f64 {
    if ops == 0 {
        return 0.0;
    }
    elapsed.as_nanos() as f64 / ops as f64
}

/// Cost of a single-threaded memory-tier hit at a given resident entry count.
fn hit_cost(entries: usize) -> f64 {
    let dir = bench_dir(&format!("hit-{entries}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::new(entries * 256, &dir);
    cache.start().expect("start cache");
    let keys = workload(entries);
    let value = vec![b'v'; VALUE_BYTES];
    for key in &keys {
        cache.put(key.clone(), value.clone()).expect("put");
    }

    let ops = entries * 8;
    let mut best = f64::MAX;
    for _ in 0..REPEATS {
        let started = Instant::now();
        for index in 0..ops {
            let _ = cache.get(&keys[scattered(index, entries)]).expect("get");
        }
        best = best.min(ns_per_op(started.elapsed(), ops));
    }
    let _ = std::fs::remove_dir_all(&dir);
    best
}

/// Aggregate read throughput with `threads` readers sharing one cache.
fn read_throughput(entries: usize, threads: usize) -> f64 {
    let dir = bench_dir(&format!("conc-{threads}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::new(entries * 256, &dir);
    cache.start().expect("start cache");
    let keys = workload(entries);
    let value = vec![b'v'; VALUE_BYTES];
    for key in &keys {
        cache.put(key.clone(), value.clone()).expect("put");
    }

    let per_thread = 40_000usize;
    let mut best = f64::MAX;
    for _ in 0..3 {
        let started = Instant::now();
        std::thread::scope(|scope| {
            for thread in 0..threads {
                let cache = &cache;
                let keys = &keys;
                scope.spawn(move || {
                    for index in 0..per_thread {
                        // Offset per thread so readers are not in lockstep.
                        let slot = scattered(index + thread * 7919, keys.len());
                        let _ = cache.get(&keys[slot]).expect("get");
                    }
                });
            }
        });
        best = best.min(ns_per_op(started.elapsed(), per_thread * threads));
    }
    let _ = std::fs::remove_dir_all(&dir);
    best
}

/// Options shared by both cache shapes, so the comparison is like for like.
fn scaling_options(dir: &std::path::Path, entries: usize) -> CacheOptions {
    CacheOptions::new(entries * 256, 0, 0).with_ssd_paths(vec![dir.to_path_buf()])
}

/// Read throughput for the single-lock cache, built from shared options.
fn single_lock_throughput(entries: usize, threads: usize) -> f64 {
    let dir = bench_dir(&format!("single-{threads}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::with_options(scaling_options(&dir, entries));
    cache.start().expect("start cache");
    let keys = workload(entries);
    let value = vec![b'v'; VALUE_BYTES];
    for key in &keys {
        cache.put(key.clone(), value.clone()).expect("put");
    }
    let best = drive_readers(threads, &keys, |key| {
        let _ = cache.get(key).expect("get");
    });
    let _ = std::fs::remove_dir_all(&dir);
    best
}

/// Read throughput for the sharded cache holding the same total capacity.
fn sharded_throughput(entries: usize, threads: usize, shards: usize) -> f64 {
    let dir = bench_dir(&format!("sharded-{shards}-{threads}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = ShardedMultiLayerCache::with_options(scaling_options(&dir, entries), shards);
    cache.start().expect("start cache");
    let keys = workload(entries);
    let value = vec![b'v'; VALUE_BYTES];
    for key in &keys {
        cache.put(key.clone(), value.clone()).expect("put");
    }
    let best = drive_readers(threads, &keys, |key| {
        let _ = cache.get(key).expect("get");
    });
    let _ = std::fs::remove_dir_all(&dir);
    best
}

/// Run `threads` readers over `keys` and return the best ns/op of three runs.
fn drive_readers<F>(threads: usize, keys: &[CacheKey], read: F) -> f64
where
    F: Fn(&CacheKey) + Sync,
{
    let per_thread = 40_000usize;
    let mut best = f64::MAX;
    for _ in 0..3 {
        let started = Instant::now();
        std::thread::scope(|scope| {
            for thread in 0..threads {
                let read = &read;
                scope.spawn(move || {
                    for index in 0..per_thread {
                        // Offset per thread so readers are not in lockstep.
                        let slot = scattered(index + thread * 7919, keys.len());
                        read(&keys[slot]);
                    }
                });
            }
        });
        best = best.min(ns_per_op(started.elapsed(), per_thread * threads));
    }
    best
}

#[derive(Clone, Copy)]
struct ThreadScalingRow {
    threads: usize,
    single_lock_ns_per_op: f64,
    sharded_ns_per_op: f64,
    single_lock_mops: f64,
    sharded_mops: f64,
    speedup: f64,
}

fn mops_from_ns(ns: f64) -> f64 {
    if ns > 0.0 {
        1_000.0 / ns
    } else {
        0.0
    }
}

fn option_f64_json(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.4}"))
        .unwrap_or_else(|| "null".to_string())
}

fn option_path_json(path: Option<&PathBuf>) -> String {
    path.map(|path| format!("\"{}\"", path.display()))
        .unwrap_or_else(|| "null".to_string())
}

fn write_json_report(
    path: Option<&PathBuf>,
    max_entries: usize,
    hit_costs: &[(usize, f64)],
    rows: &[ThreadScalingRow],
    min_sharded_speedup: Option<f64>,
    max_single_thread_ns: Option<f64>,
    passed: bool,
) -> String {
    let mut report = String::new();
    let best_sharded_mops = rows
        .iter()
        .map(|row| row.sharded_mops)
        .fold(0.0_f64, f64::max);
    let worst_speedup = rows.iter().map(|row| row.speedup).fold(f64::MAX, f64::min);
    let worst_speedup = if rows.is_empty() { 0.0 } else { worst_speedup };
    let first_hit_ns = hit_costs.first().map(|(_, ns)| *ns).unwrap_or(0.0);
    let last_hit_ns = hit_costs.last().map(|(_, ns)| *ns).unwrap_or(0.0);
    writeln!(&mut report, "{{").expect("format report");
    writeln!(
        &mut report,
        "  \"report_version\": \"matrixcache_read_scaling_v1\","
    )
    .expect("format report");
    writeln!(&mut report, "  \"max_entries\": {max_entries},").expect("format report");
    writeln!(&mut report, "  \"value_bytes\": {VALUE_BYTES},").expect("format report");
    writeln!(&mut report, "  \"repeats\": {REPEATS},").expect("format report");
    writeln!(&mut report, "  \"shards\": {SHARDS},").expect("format report");
    writeln!(
        &mut report,
        "  \"min_sharded_speedup\": {},",
        option_f64_json(min_sharded_speedup)
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"max_single_thread_ns\": {},",
        option_f64_json(max_single_thread_ns)
    )
    .expect("format report");
    writeln!(&mut report, "  \"hit_costs\": [").expect("format report");
    for (index, (entries, ns)) in hit_costs.iter().enumerate() {
        let comma = if index + 1 == hit_costs.len() {
            ""
        } else {
            ","
        };
        writeln!(
            &mut report,
            "    {{\"entries\": {entries}, \"ns_per_op\": {ns:.4}}}{comma}"
        )
        .expect("format report");
    }
    writeln!(&mut report, "  ],").expect("format report");
    writeln!(&mut report, "  \"thread_scaling\": [").expect("format report");
    for (index, row) in rows.iter().enumerate() {
        let comma = if index + 1 == rows.len() { "" } else { "," };
        writeln!(
            &mut report,
            "    {{\"threads\": {}, \"single_lock_ns_per_op\": {:.4}, \"sharded_ns_per_op\": {:.4}, \"single_lock_mops\": {:.4}, \"sharded_mops\": {:.4}, \"speedup\": {:.4}}}{comma}",
            row.threads,
            row.single_lock_ns_per_op,
            row.sharded_ns_per_op,
            row.single_lock_mops,
            row.sharded_mops,
            row.speedup
        )
        .expect("format report");
    }
    writeln!(&mut report, "  ],").expect("format report");
    writeln!(&mut report, "  \"summary\": {{").expect("format report");
    writeln!(
        &mut report,
        "    \"first_hit_ns_per_op\": {first_hit_ns:.4},"
    )
    .expect("format report");
    writeln!(&mut report, "    \"last_hit_ns_per_op\": {last_hit_ns:.4},").expect("format report");
    writeln!(
        &mut report,
        "    \"best_sharded_mops\": {best_sharded_mops:.4},"
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"worst_sharded_speedup\": {worst_speedup:.4},"
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"output_path\": {}",
        option_path_json(path)
    )
    .expect("format report");
    writeln!(&mut report, "  }},").expect("format report");
    writeln!(&mut report, "  \"checks\": {{").expect("format report");
    writeln!(
        &mut report,
        "    \"has_hit_costs\": {},",
        !hit_costs.is_empty()
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"has_thread_scaling\": {},",
        !rows.is_empty()
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"sharded_speedup_within_budget\": {},",
        min_sharded_speedup
            .map(|limit| worst_speedup >= limit)
            .unwrap_or(true)
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"single_thread_hit_within_budget\": {}",
        max_single_thread_ns
            .map(|limit| first_hit_ns <= limit)
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
    let mut min_sharded_speedup: Option<f64> = None;
    let mut max_single_thread_ns: Option<f64> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => emit_json = true,
            "--json-output" => {
                json_output = args.next().map(PathBuf::from);
            }
            "--require-passed" => require_passed = true,
            "--min-sharded-speedup" => {
                min_sharded_speedup = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0.0);
            }
            "--max-single-thread-ns" => {
                max_single_thread_ns = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0.0);
            }
            _ => positional.push(arg),
        }
    }
    let max_entries: usize = positional
        .first()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4_096);

    // Warm the allocator before the first measured case.
    let _ = hit_cost(256);

    println!("memory-tier hit, single thread");
    println!("{:>10} {:>14}", "entries", "ns/op");
    let mut hit_costs = Vec::new();
    let mut size = 1_024usize;
    while size <= max_entries {
        let ns = hit_cost(size);
        hit_costs.push((size, ns));
        println!("{size:>10} {ns:>14.1}");
        size *= 4;
    }

    println!();
    println!("read throughput, {max_entries} resident entries");
    println!("{:>10} {:>14} {:>14}", "threads", "ns/op", "Mops/s");
    for &threads in &[1usize, 2, 4, 8] {
        let ns = read_throughput(max_entries, threads);
        let mops = if ns > 0.0 { 1_000.0 / ns } else { 0.0 };
        println!("{threads:>10} {ns:>14.1} {mops:>14.2}");
    }

    println!();
    println!("single lock vs sharded, same total capacity, Mops/s");
    println!(
        "{:>10} {:>14} {:>14} {:>10}",
        "threads", "single", "sharded", "speedup"
    );
    let mut rows = Vec::new();
    for &threads in &[1usize, 2, 4, 8] {
        let single_ns = single_lock_throughput(max_entries, threads);
        let sharded_ns = sharded_throughput(max_entries, threads, SHARDS);
        let single = mops_from_ns(single_ns);
        let sharded = mops_from_ns(sharded_ns);
        let speedup = if single > 0.0 { sharded / single } else { 0.0 };
        rows.push(ThreadScalingRow {
            threads,
            single_lock_ns_per_op: single_ns,
            sharded_ns_per_op: sharded_ns,
            single_lock_mops: single,
            sharded_mops: sharded,
            speedup,
        });
        println!("{threads:>10} {single:>14.2} {sharded:>14.2} {speedup:>9.2}x");
    }

    let first_hit_ns = hit_costs.first().map(|(_, ns)| *ns).unwrap_or(0.0);
    let worst_speedup = rows.iter().map(|row| row.speedup).fold(f64::MAX, f64::min);
    let worst_speedup = if rows.is_empty() { 0.0 } else { worst_speedup };
    let passed = !hit_costs.is_empty()
        && !rows.is_empty()
        && min_sharded_speedup
            .map(|limit| worst_speedup >= limit)
            .unwrap_or(true)
        && max_single_thread_ns
            .map(|limit| first_hit_ns <= limit)
            .unwrap_or(true);
    if emit_json || json_output.is_some() || require_passed {
        let report = write_json_report(
            json_output.as_ref(),
            max_entries,
            &hit_costs,
            &rows,
            min_sharded_speedup,
            max_single_thread_ns,
            passed,
        );
        if emit_json {
            print!("{report}");
        }
        if let Some(path) = &json_output {
            println!("wrote {}", path.display());
        }
        println!(
            "read scaling gate: {}",
            if passed { "passed" } else { "failed" }
        );
        if require_passed && !passed {
            std::process::exit(1);
        }
    }
}
