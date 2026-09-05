// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

//! Cost of choosing an eviction victim, as the resident set grows.
//!
//! A cache at capacity evicts on almost every write, so whatever victim
//! selection costs is paid per write for the life of the cache. The thing to
//! watch is whether that cost stays put as the cache fills: a selector that
//! inspects every resident entry shows up here as a per-write cost that climbs
//! with the entry count, while one that inspects a bounded number of
//! candidates shows up as a flat line.
//!
//! Two numbers are reported per size. The first is wall time per write, which
//! is what a caller feels. The second is the number of candidate groups the
//! selector formed per evicted entry, which the cache already counts; it is
//! immune to load on the machine and is the number that says whether the
//! algorithm changed or only the weather did.
//!
//!
//! The hit-rate table reports promotions beside the hit rate, because the hit
//! rate follows them: a promotion is what keeps the access order carrying
//! recency, and the refresh window is what decides how often one happens. If
//! the hit rate here moves and the promotion count moved with it, the cause is
//! the window rather than anything about eviction.
//!
//! ```text
//! cargo run --release --no-default-features --example eviction_bench
//! cargo run --release --no-default-features --example eviction_bench -- --json-output /tmp/matrixcache-eviction.json --require-passed
//! cargo run --locked --no-default-features --example eviction_bench -- --smoke --json-output /tmp/matrixcache-eviction.json --require-passed
//! ```

use matrixcache::{CacheKey, CacheOptions, MultiLayerCache};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

const VALUE_BYTES: usize = 64;
/// Room for the value plus its per-entry overhead, so `entries` really fit.
const SLOT_BYTES: usize = VALUE_BYTES;

fn bench_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("matrixcache-eviction-{name}"))
}

fn key(index: usize) -> CacheKey {
    CacheKey::string(0, &format!("eviction-key-{index:010}"))
}

/// Spread successive steps over `len` slots so reads are not in insertion
/// order, which would flatter any policy that evicts from one end.
fn scattered(index: usize, len: usize) -> usize {
    index.wrapping_mul(2_654_435_761) % len.max(1)
}

/// Fill to capacity, then keep writing so every write evicts.
#[derive(Debug, Clone)]
struct SteadyRow {
    entries: usize,
    ns_per_write: f64,
    groups_per_eviction: f64,
}

#[derive(Debug, Clone)]
struct HitRateRow {
    entries: usize,
    hit_rate_percent: f64,
    promotions: u64,
}

#[derive(Debug, Clone, Default)]
struct Args {
    json_output: Option<PathBuf>,
    require_passed: bool,
    max_ns_per_write: Option<f64>,
    max_groups_per_eviction: Option<f64>,
    min_hit_rate_percent: Option<f64>,
    smoke: bool,
}

#[derive(Debug, Clone)]
struct Report {
    steady_rows: Vec<SteadyRow>,
    hit_rate_rows: Vec<HitRateRow>,
    max_ns_per_write: f64,
    max_groups_per_eviction: f64,
    min_hit_rate_percent: f64,
    total_promotions: u64,
    write_pressure_writes: usize,
    read_pressure_reads: usize,
    positive_timings: bool,
    candidate_groups_within_budget: bool,
    hit_rate_within_budget: bool,
    passed: bool,
}

fn steady_state(entries: usize, writes: usize) -> SteadyRow {
    let dir = bench_dir(&format!("steady-{entries}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(entries * SLOT_BYTES, 0, 0).with_ssd_paths(vec![dir.clone()]),
    );
    cache.start().expect("start cache");

    let value = vec![b'v'; VALUE_BYTES];
    // Fill to capacity. Keys are built up front so the timed region below
    // measures the cache rather than key formatting.
    for index in 0..entries {
        cache.put(key(index), value.clone()).expect("put");
    }

    let fresh: Vec<CacheKey> = (entries..entries + writes).map(key).collect();

    let before = cache.stats();
    let started = Instant::now();
    for k in &fresh {
        cache.put(k.clone(), value.clone()).expect("put");
    }
    let elapsed = started.elapsed();
    let after = cache.stats();

    let evictions = after
        .memory_evictions
        .saturating_sub(before.memory_evictions);
    let groups = after
        .eviction_sampled_groups
        .saturating_sub(before.eviction_sampled_groups);

    let ns_per_write = elapsed.as_nanos() as f64 / writes as f64;
    let groups_per_eviction = if evictions == 0 {
        0.0
    } else {
        groups as f64 / evictions as f64
    };

    cache.stop();
    let _ = std::fs::remove_dir_all(&dir);
    SteadyRow {
        entries,
        ns_per_write,
        groups_per_eviction,
    }
}

/// Hit rate under a skewed read-through workload.
///
/// Bounding the candidate search only pays off if it still throws out the
/// right entries. This drives a working set several times larger than the
/// cache, with most reads landing on a small hot subset, and reports the share
/// of reads the cache served. A selector that evicts hot entries shows up here
/// as a hit rate below what the hot subset alone would guarantee.
fn hit_rate(entries: usize, reads: usize) -> HitRateRow {
    let dir = bench_dir(&format!("hitrate-{entries}"));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = MultiLayerCache::with_options(
        CacheOptions::new(entries * SLOT_BYTES, 0, 0).with_ssd_paths(vec![dir.clone()]),
    );
    cache.start().expect("start cache");

    let value = vec![b'v'; VALUE_BYTES];
    // Four times as many keys as fit, with a hot subset that is half the
    // cache, so a selector that protects hot entries can hold all of them.
    let universe = entries * 4;
    let hot = entries / 2;
    let keys: Vec<CacheKey> = (0..universe).map(key).collect();

    let mut hits = 0usize;
    for step in 0..reads {
        // Four reads in five land in the hot subset; the rest sweep the
        // universe and are the pressure that forces eviction.
        let slot = if step % 5 < 4 {
            scattered(step, hot)
        } else {
            scattered(step, universe)
        };
        let k = &keys[slot];
        if cache.get(k).expect("get").is_some() {
            hits += 1;
        } else {
            cache.put(k.clone(), value.clone()).expect("put");
        }
    }

    let refreshes = cache.stats().access_order_refreshes;
    cache.stop();
    let _ = std::fs::remove_dir_all(&dir);
    HitRateRow {
        entries,
        hit_rate_percent: hits as f64 * 100.0 / reads as f64,
        promotions: refreshes,
    }
}

fn parse_args() -> Args {
    let mut parsed = Args::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json-output" => {
                parsed.json_output = args.next().map(PathBuf::from);
                if parsed.json_output.is_none() {
                    eprintln!("missing value for --json-output");
                    std::process::exit(2);
                }
            }
            "--require-passed" => parsed.require_passed = true,
            "--max-ns-per-write" => {
                parsed.max_ns_per_write = Some(parse_f64(&arg, args.next()));
            }
            "--max-groups-per-eviction" => {
                parsed.max_groups_per_eviction = Some(parse_f64(&arg, args.next()));
            }
            "--min-hit-rate-percent" => {
                parsed.min_hit_rate_percent = Some(parse_f64(&arg, args.next()));
            }
            "--smoke" => parsed.smoke = true,
            "-h" | "--help" => {
                println!(
                    "usage: eviction_bench [--json-output PATH] [--require-passed] [--smoke] \
                     [--max-ns-per-write N] [--max-groups-per-eviction N] \
                     [--min-hit-rate-percent N]"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    parsed
}

fn parse_f64(flag: &str, value: Option<String>) -> f64 {
    let Some(value) = value else {
        eprintln!("missing value for {flag}");
        std::process::exit(2);
    };
    value.parse::<f64>().unwrap_or_else(|err| {
        eprintln!("invalid value for {flag}: {err}");
        std::process::exit(2);
    })
}

fn build_report(args: &Args) -> Report {
    let (steady_entries, hit_entries, write_pressure_writes, read_pressure_reads): (
        &[usize],
        &[usize],
        usize,
        usize,
    ) = if args.smoke {
        (&[256, 512, 1_024], &[256, 1_024], 256, 20_000)
    } else {
        (
            &[1_024, 2_048, 4_096, 8_192, 16_384, 32_768],
            &[1_024, 4_096, 16_384],
            2_000,
            400_000,
        )
    };
    let steady_rows: Vec<SteadyRow> = steady_entries
        .iter()
        .copied()
        .map(|entries| steady_state(entries, write_pressure_writes))
        .collect();
    let hit_rate_rows: Vec<HitRateRow> = hit_entries
        .iter()
        .copied()
        .map(|entries| hit_rate(entries, read_pressure_reads))
        .collect();

    let max_ns_per_write = steady_rows
        .iter()
        .map(|row| row.ns_per_write)
        .fold(0.0, f64::max);
    let max_groups_per_eviction = steady_rows
        .iter()
        .map(|row| row.groups_per_eviction)
        .fold(0.0, f64::max);
    let min_hit_rate_percent = hit_rate_rows
        .iter()
        .map(|row| row.hit_rate_percent)
        .fold(100.0, f64::min);
    let total_promotions = hit_rate_rows.iter().map(|row| row.promotions).sum();

    let positive_timings = steady_rows
        .iter()
        .all(|row| row.ns_per_write > 0.0 && row.groups_per_eviction > 0.0);
    let candidate_groups_within_budget = args
        .max_groups_per_eviction
        .is_none_or(|limit| max_groups_per_eviction <= limit);
    let hit_rate_within_budget = args
        .min_hit_rate_percent
        .is_none_or(|limit| min_hit_rate_percent >= limit);
    let latency_within_budget = args
        .max_ns_per_write
        .is_none_or(|limit| max_ns_per_write <= limit);
    let passed = positive_timings
        && candidate_groups_within_budget
        && hit_rate_within_budget
        && latency_within_budget;

    Report {
        steady_rows,
        hit_rate_rows,
        max_ns_per_write,
        max_groups_per_eviction,
        min_hit_rate_percent,
        total_promotions,
        write_pressure_writes,
        read_pressure_reads,
        positive_timings,
        candidate_groups_within_budget,
        hit_rate_within_budget,
        passed,
    }
}

fn render_json(report: &Report) -> String {
    let mut out = String::new();
    writeln!(&mut out, "{{").expect("format report");
    writeln!(
        &mut out,
        "  \"report_version\": \"matrixcache_eviction_v1\","
    )
    .expect("format report");
    writeln!(&mut out, "  \"value_bytes\": {VALUE_BYTES},").expect("format report");
    writeln!(
        &mut out,
        "  \"write_pressure_writes\": {},",
        report.write_pressure_writes
    )
    .expect("format report");
    writeln!(
        &mut out,
        "  \"read_pressure_reads\": {},",
        report.read_pressure_reads
    )
    .expect("format report");
    writeln!(&mut out, "  \"summary\": {{").expect("format report");
    writeln!(
        &mut out,
        "    \"max_ns_per_write\": {:.4},",
        report.max_ns_per_write
    )
    .expect("format report");
    writeln!(
        &mut out,
        "    \"max_groups_per_eviction\": {:.4},",
        report.max_groups_per_eviction
    )
    .expect("format report");
    writeln!(
        &mut out,
        "    \"min_hit_rate_percent\": {:.4},",
        report.min_hit_rate_percent
    )
    .expect("format report");
    writeln!(
        &mut out,
        "    \"total_promotions\": {}",
        report.total_promotions
    )
    .expect("format report");
    writeln!(&mut out, "  }},").expect("format report");
    writeln!(&mut out, "  \"steady_state\": [").expect("format report");
    for (index, row) in report.steady_rows.iter().enumerate() {
        let suffix = if index + 1 == report.steady_rows.len() {
            ""
        } else {
            ","
        };
        writeln!(
            &mut out,
            "    {{ \"entries\": {}, \"ns_per_write\": {:.4}, \"groups_per_eviction\": {:.4} }}{}",
            row.entries, row.ns_per_write, row.groups_per_eviction, suffix
        )
        .expect("format report");
    }
    writeln!(&mut out, "  ],").expect("format report");
    writeln!(&mut out, "  \"hit_rates\": [").expect("format report");
    for (index, row) in report.hit_rate_rows.iter().enumerate() {
        let suffix = if index + 1 == report.hit_rate_rows.len() {
            ""
        } else {
            ","
        };
        writeln!(
            &mut out,
            "    {{ \"entries\": {}, \"hit_rate_percent\": {:.4}, \"promotions\": {} }}{}",
            row.entries, row.hit_rate_percent, row.promotions, suffix
        )
        .expect("format report");
    }
    writeln!(&mut out, "  ],").expect("format report");
    writeln!(&mut out, "  \"checks\": {{").expect("format report");
    writeln!(
        &mut out,
        "    \"positive_timings\": {},",
        report.positive_timings
    )
    .expect("format report");
    writeln!(
        &mut out,
        "    \"candidate_groups_within_budget\": {},",
        report.candidate_groups_within_budget
    )
    .expect("format report");
    writeln!(
        &mut out,
        "    \"hit_rate_within_budget\": {}",
        report.hit_rate_within_budget
    )
    .expect("format report");
    writeln!(&mut out, "  }},").expect("format report");
    writeln!(&mut out, "  \"passed\": {}", report.passed).expect("format report");
    writeln!(&mut out, "}}").expect("format report");
    out
}

fn print_report(report: &Report) {
    println!("steady-state write cost with the cache at capacity");
    println!(
        "{:>10}  {:>14}  {:>22}",
        "entries", "ns/write", "groups/eviction"
    );
    for row in &report.steady_rows {
        println!(
            "{:>10}  {:>14.0}  {:>22.1}",
            row.entries, row.ns_per_write, row.groups_per_eviction
        );
    }

    println!();
    println!("hit rate, working set 4x the cache, 80% of reads on a hot half-cache");
    println!(
        "{:>10}  {:>14}  {:>14}",
        "entries", "hit rate %", "promotions"
    );
    for row in &report.hit_rate_rows {
        println!(
            "{:>10}  {:>14.2}  {:>14}",
            row.entries, row.hit_rate_percent, row.promotions
        );
    }
    println!(
        "eviction gate: {} (max write {:.0} ns, max groups {:.1}, min hit {:.2}%)",
        if report.passed { "passed" } else { "failed" },
        report.max_ns_per_write,
        report.max_groups_per_eviction,
        report.min_hit_rate_percent
    );
}

fn main() {
    let args = parse_args();
    let report = build_report(&args);
    print_report(&report);

    if let Some(path) = &args.json_output {
        std::fs::write(path, render_json(&report)).expect("write eviction report");
        println!("wrote {}", path.display());
    }
    if args.require_passed && !report.passed {
        std::process::exit(1);
    }
}
