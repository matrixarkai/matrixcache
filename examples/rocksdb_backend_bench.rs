// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::{CacheDataPlacement, CacheKey, CacheOptions, CacheReadTier, MultiLayerCache};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
struct Timing {
    count: usize,
    total: Duration,
    p50_us: u128,
    p95_us: u128,
    p99_us: u128,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    qps: f64,
}

fn percentile(sorted_values: &[u128], pct: f64) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }
    let idx = ((sorted_values.len() as f64 - 1.0) * pct).round() as usize;
    sorted_values[idx.min(sorted_values.len() - 1)]
}

fn summarize(mut samples: Vec<Duration>, total: Duration) -> Timing {
    let count = samples.len();
    samples.sort();
    let micros = samples
        .iter()
        .map(|duration| duration.as_micros())
        .collect::<Vec<_>>();
    let nanos = samples
        .iter()
        .map(|duration| duration.as_nanos())
        .collect::<Vec<_>>();
    let seconds = total.as_secs_f64().max(0.000_001);
    Timing {
        count,
        total,
        p50_us: percentile(&micros, 0.50),
        p95_us: percentile(&micros, 0.95),
        p99_us: percentile(&micros, 0.99),
        p50_ns: percentile(&nanos, 0.50),
        p95_ns: percentile(&nanos, 0.95),
        p99_ns: percentile(&nanos, 0.99),
        qps: count as f64 / seconds,
    }
}

fn unique_bench_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("matrixcache-rocksdb-bench-{nanos}"))
}

#[derive(Debug, Clone)]
struct BenchConfig {
    iterations: usize,
    value_bytes: usize,
    dram_capacity_bytes: usize,
    pmem_capacity_bytes: usize,
    ssd_capacity_bytes: usize,
    placement_threshold_bytes: usize,
    replacement_soak_iterations: usize,
    json_output: Option<PathBuf>,
    require_passed: bool,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            iterations: 5_000,
            value_bytes: 256,
            dram_capacity_bytes: 16 * 1024,
            pmem_capacity_bytes: 32 * 1024,
            ssd_capacity_bytes: 64 * 1024 * 1024,
            placement_threshold_bytes: 1024 * 1024,
            replacement_soak_iterations: 0,
            json_output: None,
            require_passed: false,
        }
    }
}

fn parse_usize_flag(name: &str, value: Option<String>) -> usize {
    let raw = value.unwrap_or_else(|| {
        eprintln!("missing value for {name}");
        process::exit(2);
    });
    raw.parse::<usize>().unwrap_or_else(|error| {
        eprintln!("invalid value for {name}: {raw}: {error}");
        process::exit(2);
    })
}

fn parse_config() -> BenchConfig {
    let mut config = BenchConfig::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => config.iterations = parse_usize_flag("--iterations", args.next()),
            "--value-bytes" => config.value_bytes = parse_usize_flag("--value-bytes", args.next()),
            "--dram-capacity-bytes" => {
                config.dram_capacity_bytes = parse_usize_flag("--dram-capacity-bytes", args.next())
            }
            "--pmem-capacity-bytes" => {
                config.pmem_capacity_bytes = parse_usize_flag("--pmem-capacity-bytes", args.next())
            }
            "--ssd-capacity-bytes" => {
                config.ssd_capacity_bytes = parse_usize_flag("--ssd-capacity-bytes", args.next())
            }
            "--placement-threshold-bytes" => {
                config.placement_threshold_bytes =
                    parse_usize_flag("--placement-threshold-bytes", args.next())
            }
            "--replacement-soak-iterations" => {
                config.replacement_soak_iterations =
                    parse_usize_flag("--replacement-soak-iterations", args.next())
            }
            "--json-output" => {
                config.json_output = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("missing value for --json-output");
                    process::exit(2);
                })));
            }
            "--require-passed" => config.require_passed = true,
            "--help" | "-h" => {
                println!(
                    "usage: rocksdb_backend_bench [iterations] [--iterations N] \
                     [--value-bytes N] [--dram-capacity-bytes N] \
                     [--pmem-capacity-bytes N] [--ssd-capacity-bytes N] \
                     [--placement-threshold-bytes N] \
                     [--replacement-soak-iterations N] [--json-output PATH] \
                     [--require-passed]"
                );
                process::exit(0);
            }
            value if !value.starts_with('-') => {
                config.iterations = value.parse::<usize>().unwrap_or_else(|error| {
                    eprintln!("invalid iterations value {value}: {error}");
                    process::exit(2);
                });
            }
            _ => {
                eprintln!("unknown argument: {arg}");
                process::exit(2);
            }
        }
    }
    config.iterations = config.iterations.max(500);
    config.value_bytes = config.value_bytes.max(1);
    if config.replacement_soak_iterations == 0 {
        config.replacement_soak_iterations = config.iterations.max(128);
    } else {
        config.replacement_soak_iterations = config.replacement_soak_iterations.max(128);
    }
    config
}

fn payload(i: usize, value_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(value_bytes);
    bytes.extend_from_slice(format!("value-{i:08}-").as_bytes());
    while bytes.len() < value_bytes {
        bytes.push(b'a' + (i % 26) as u8);
    }
    bytes.truncate(value_bytes);
    bytes
}

fn main() {
    let config = parse_config();
    let iterations = config.iterations;
    let dir = unique_bench_dir();
    let options = CacheOptions::new(
        config.dram_capacity_bytes,
        config.pmem_capacity_bytes,
        config.ssd_capacity_bytes,
    )
    .with_ssd_paths([dir.join("ssd")])
    .with_pmem_paths([dir.join("pmem")])
    .with_dram_pmem_data_placement(CacheDataPlacement::Tiered, config.placement_threshold_bytes);
    let cache = MultiLayerCache::with_options(options);

    let keys = (0..iterations)
        .map(|i| {
            CacheKey::page_with_slot(
                1,
                i as u64,
                0,
                config.value_bytes as u64,
                Some((i % 128) as u32),
            )
        })
        .collect::<Vec<_>>();

    let mut put_samples = Vec::with_capacity(iterations);
    let put_total_started = Instant::now();
    for (i, key) in keys.iter().enumerate() {
        let started = Instant::now();
        cache
            .put(key.clone(), payload(i, config.value_bytes))
            .expect("benchmark put should succeed");
        put_samples.push(started.elapsed());
    }
    let put_timing = summarize(put_samples, put_total_started.elapsed());

    let resident_hot_keys = keys
        .iter()
        .filter(|key| {
            matches!(
                cache.peek_tier(key),
                Some(CacheReadTier::Memory | CacheReadTier::Pmem)
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut resident_hot_get_samples = Vec::with_capacity(resident_hot_keys.len());
    let resident_hot_total_started = Instant::now();
    for key in &resident_hot_keys {
        let started = Instant::now();
        let read = cache
            .get_bypass_replacement_policy(key)
            .expect("resident hot get should succeed")
            .expect("resident hot key should exist");
        assert!(matches!(
            read.tier,
            CacheReadTier::Memory | CacheReadTier::Pmem
        ));
        resident_hot_get_samples.push(started.elapsed());
    }
    let resident_hot_timing = summarize(
        resident_hot_get_samples,
        resident_hot_total_started.elapsed(),
    );

    let mut hot_get_samples = Vec::with_capacity(iterations);
    let hot_total_started = Instant::now();
    for key in &keys {
        let started = Instant::now();
        let read = cache
            .get_with_tier(key)
            .expect("hot get should succeed")
            .expect("hot key should exist");
        assert!(matches!(
            read.tier,
            CacheReadTier::Memory | CacheReadTier::Pmem | CacheReadTier::Ssd
        ));
        hot_get_samples.push(started.elapsed());
    }
    let hot_timing = summarize(hot_get_samples, hot_total_started.elapsed());

    cache.clear_memory_for_test();
    let mut cold_ssd_refills = 0usize;
    let mut cold_get_samples = Vec::with_capacity(iterations);
    let cold_total_started = Instant::now();
    for key in &keys {
        let started = Instant::now();
        let read = cache
            .get_with_tier(key)
            .expect("cold get should succeed")
            .expect("cold key should exist");
        if matches!(read.tier, CacheReadTier::Ssd) {
            cold_ssd_refills += 1;
        }
        cold_get_samples.push(started.elapsed());
    }
    let cold_timing = summarize(cold_get_samples, cold_total_started.elapsed());
    let soak_dir = dir.join("replacement-soak");
    let soak_cache = MultiLayerCache::new(48, &soak_dir);
    let soak_iterations = config.replacement_soak_iterations;
    let soak = soak_cache.replacement_policy_soak(soak_iterations);
    let stats = cache.stats();
    let dram_to_pmem_eviction = stats.memory_evictions > 0 && stats.pmem_fills > 0;
    let pmem_to_ssd_eviction = stats.pmem_evictions > 0 && stats.disk_fills > 0;
    let ssd_read_through_refill = cold_ssd_refills > 0 && stats.refill_failures == 0;
    let replacement_soak_ready = soak.passed;
    let async_writeback_backpressure_ready = soak.observed_async_writeback_backpressure > 0;
    let restart_disk_refill_ready = soak.restart_disk_refill_ready;
    let contract_passed = dram_to_pmem_eviction
        && pmem_to_ssd_eviction
        && ssd_read_through_refill
        && replacement_soak_ready
        && async_writeback_backpressure_ready
        && restart_disk_refill_ready;

    let mut report = String::new();
    writeln!(&mut report, "{{").expect("format report");
    writeln!(
        &mut report,
        "  \"report_version\": \"matrixcache_rocksdb_backend_v1\","
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"backend\": \"{}\",",
        if cfg!(feature = "rocksdb-ssd") {
            "rocksdb"
        } else {
            "file-compat"
        }
    )
    .expect("format report");
    writeln!(&mut report, "  \"iterations\": {},", iterations).expect("format report");
    writeln!(&mut report, "  \"data_dir\": \"{}\",", dir.display()).expect("format report");
    writeln!(&mut report, "  \"workload\": {{").expect("format report");
    writeln!(&mut report, "    \"value_bytes\": {},", config.value_bytes).expect("format report");
    writeln!(
        &mut report,
        "    \"dram_capacity_bytes\": {},",
        config.dram_capacity_bytes
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"pmem_capacity_bytes\": {},",
        config.pmem_capacity_bytes
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"ssd_capacity_bytes\": {},",
        config.ssd_capacity_bytes
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"placement_threshold_bytes\": {},",
        config.placement_threshold_bytes
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"replacement_soak_iterations\": {}",
        config.replacement_soak_iterations
    )
    .expect("format report");
    writeln!(&mut report, "  }},").expect("format report");
    append_timing(&mut report, "put", put_timing, true);
    writeln!(
        &mut report,
        "  \"resident_hot_key_count\": {},",
        resident_hot_keys.len()
    )
    .expect("format report");
    append_timing(&mut report, "resident_hot_get", resident_hot_timing, true);
    append_timing(&mut report, "hot_get", hot_timing, true);
    append_timing(&mut report, "cold_ssd_refill_get", cold_timing, true);
    writeln!(&mut report, "  \"cold_ssd_refills\": {},", cold_ssd_refills).expect("format report");
    writeln!(&mut report, "  \"memory_hits\": {},", stats.memory_hits).expect("format report");
    writeln!(&mut report, "  \"pmem_hits\": {},", stats.pmem_hits).expect("format report");
    writeln!(&mut report, "  \"ssd_hits\": {},", stats.disk_hits).expect("format report");
    writeln!(
        &mut report,
        "  \"memory_evictions\": {},",
        stats.memory_evictions
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"pmem_evictions\": {},",
        stats.pmem_evictions
    )
    .expect("format report");
    writeln!(&mut report, "  \"ssd_evictions\": {},", stats.ssd_evictions).expect("format report");
    writeln!(
        &mut report,
        "  \"refill_failures\": {},",
        stats.refill_failures
    )
    .expect("format report");
    writeln!(&mut report, "  \"disk_fills\": {},", stats.disk_fills).expect("format report");
    writeln!(&mut report, "  \"pmem_fills\": {},", stats.pmem_fills).expect("format report");
    writeln!(
        &mut report,
        "  \"main_pressure_passed\": {},",
        stats.memory_evictions > 0
            && stats.pmem_evictions > 0
            && cold_ssd_refills > 0
            && stats.refill_failures == 0
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"replacement_soak_iterations\": {},",
        soak_iterations
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"replacement_soak_passed\": {},",
        if soak.passed { "true" } else { "false" }
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"replacement_soak_reasons\": {},",
        json_string_array(&soak.reasons)
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"async_writeback_backpressure\": {},",
        soak.observed_async_writeback_backpressure
    )
    .expect("format report");
    writeln!(
        &mut report,
        "  \"restart_disk_refill_ready\": {},",
        soak.restart_disk_refill_ready
    )
    .expect("format report");
    writeln!(&mut report, "  \"matrixcache_contract\": {{").expect("format report");
    writeln!(
        &mut report,
        "    \"dram_to_pmem_eviction\": {},",
        dram_to_pmem_eviction
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"pmem_to_ssd_eviction\": {},",
        pmem_to_ssd_eviction
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"ssd_read_through_refill\": {},",
        ssd_read_through_refill
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"replacement_soak\": {},",
        replacement_soak_ready
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"async_writeback_backpressure\": {},",
        async_writeback_backpressure_ready
    )
    .expect("format report");
    writeln!(
        &mut report,
        "    \"restart_disk_refill\": {},",
        restart_disk_refill_ready
    )
    .expect("format report");
    writeln!(&mut report, "    \"passed\": {}", contract_passed).expect("format report");
    writeln!(&mut report, "  }},").expect("format report");
    writeln!(&mut report, "  \"matrixcache_contract_evidence\": {{").expect("format report");
    append_evidence(
        &mut report,
        "dram_to_pmem_eviction",
        dram_to_pmem_eviction,
        "memory_evictions > 0 && pmem_fills > 0",
        &[
            ("memory_evictions", stats.memory_evictions),
            ("pmem_fills", stats.pmem_fills),
        ],
        true,
    );
    append_evidence(
        &mut report,
        "pmem_to_ssd_eviction",
        pmem_to_ssd_eviction,
        "pmem_evictions > 0 && disk_fills > 0",
        &[
            ("pmem_evictions", stats.pmem_evictions),
            ("disk_fills", stats.disk_fills),
        ],
        true,
    );
    append_evidence(
        &mut report,
        "ssd_read_through_refill",
        ssd_read_through_refill,
        "cold_ssd_refills > 0 && refill_failures == 0",
        &[
            ("cold_ssd_refills", cold_ssd_refills as u64),
            ("refill_failures", stats.refill_failures),
        ],
        true,
    );
    writeln!(&mut report, "    \"replacement_soak\": {{").expect("format report");
    writeln!(
        &mut report,
        "      \"observed\": {},",
        replacement_soak_ready
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"source\": \"matrixcache_rocksdb_backend_bench\","
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"metric\": \"replacement_policy_soak.passed\","
    )
    .expect("format report");
    writeln!(&mut report, "      \"iterations\": {},", soak_iterations).expect("format report");
    writeln!(
        &mut report,
        "      \"read_through_latency_max_micros\": {},",
        soak.read_through_latency_max_micros
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"refill_latency_max_micros\": {},",
        soak.refill_latency_max_micros
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"writeback_latency_max_micros\": {},",
        soak.writeback_latency_max_micros
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"eviction_latency_max_micros\": {},",
        soak.eviction_latency_max_micros
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"compaction_latency_max_micros\": {},",
        soak.compaction_latency_max_micros
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"reasons\": {}",
        json_string_array(&soak.reasons)
    )
    .expect("format report");
    writeln!(&mut report, "    }},").expect("format report");
    writeln!(&mut report, "    \"async_writeback_backpressure\": {{").expect("format report");
    writeln!(
        &mut report,
        "      \"observed\": {},",
        async_writeback_backpressure_ready
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"source\": \"matrixcache_rocksdb_backend_bench\","
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"metric\": \"observed_async_writeback_backpressure > 0\","
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"observed_async_writeback_backpressure\": {}",
        soak.observed_async_writeback_backpressure
    )
    .expect("format report");
    writeln!(&mut report, "    }},").expect("format report");
    writeln!(&mut report, "    \"restart_disk_refill\": {{").expect("format report");
    writeln!(
        &mut report,
        "      \"observed\": {},",
        restart_disk_refill_ready
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"source\": \"matrixcache_rocksdb_backend_bench\","
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"metric\": \"replacement_policy_soak.restart_disk_refill_ready\","
    )
    .expect("format report");
    writeln!(
        &mut report,
        "      \"restart_disk_refill_ready\": {}",
        soak.restart_disk_refill_ready
    )
    .expect("format report");
    writeln!(&mut report, "    }}").expect("format report");
    writeln!(&mut report, "  }}").expect("format report");
    writeln!(&mut report, "}}").expect("format report");

    print!("{report}");
    if let Some(path) = &config.json_output {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).expect("create JSON output directory");
        }
        std::fs::write(path, &report).expect("write JSON backend report");
        eprintln!(
            "matrixcache RocksDB backend report written to {}",
            path.display()
        );
    }
    if config.require_passed && !contract_passed {
        eprintln!("matrixcache RocksDB backend contract failed; see JSON report");
        process::exit(1);
    }
}

fn append_timing(report: &mut String, name: &str, timing: Timing, trailing_comma: bool) {
    writeln!(report, "  \"{name}\": {{").expect("format report");
    writeln!(report, "    \"count\": {},", timing.count).expect("format report");
    writeln!(report, "    \"total_ms\": {},", timing.total.as_millis()).expect("format report");
    writeln!(report, "    \"total_us\": {},", timing.total.as_micros()).expect("format report");
    writeln!(report, "    \"qps\": {:.2},", timing.qps).expect("format report");
    writeln!(report, "    \"p50_us\": {},", timing.p50_us).expect("format report");
    writeln!(report, "    \"p95_us\": {},", timing.p95_us).expect("format report");
    writeln!(report, "    \"p99_us\": {},", timing.p99_us).expect("format report");
    writeln!(report, "    \"p50_ns\": {},", timing.p50_ns).expect("format report");
    writeln!(report, "    \"p95_ns\": {},", timing.p95_ns).expect("format report");
    writeln!(report, "    \"p99_ns\": {}", timing.p99_ns).expect("format report");
    writeln!(report, "  }}{}", if trailing_comma { "," } else { "" }).expect("format report");
}

fn append_evidence(
    report: &mut String,
    name: &str,
    observed: bool,
    metric: &str,
    fields: &[(&str, u64)],
    trailing_comma: bool,
) {
    writeln!(report, "    \"{name}\": {{").expect("format report");
    writeln!(report, "      \"observed\": {observed},").expect("format report");
    writeln!(
        report,
        "      \"source\": \"matrixcache_rocksdb_backend_bench\","
    )
    .expect("format report");
    writeln!(report, "      \"metric\": \"{}\",", json_escape(metric)).expect("format report");
    for (index, (field, value)) in fields.iter().enumerate() {
        let comma = if index + 1 == fields.len() { "" } else { "," };
        writeln!(report, "      \"{field}\": {value}{comma}").expect("format report");
    }
    writeln!(report, "    }}{}", if trailing_comma { "," } else { "" }).expect("format report");
}

fn json_string_array(values: &[String]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(&json_escape(value));
        out.push('"');
    }
    out.push(']');
    out
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
