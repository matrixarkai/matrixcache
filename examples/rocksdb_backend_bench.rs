// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

use matrixcache::{CacheDataPlacement, CacheKey, CacheOptions, CacheReadTier, MultiLayerCache};
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

#[derive(Debug, Clone, Copy)]
struct BenchConfig {
    iterations: usize,
    value_bytes: usize,
    dram_capacity_bytes: usize,
    pmem_capacity_bytes: usize,
    ssd_capacity_bytes: usize,
    placement_threshold_bytes: usize,
    replacement_soak_iterations: usize,
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
            "--help" | "-h" => {
                println!(
                    "usage: rocksdb_backend_bench [iterations] [--iterations N] \
                     [--value-bytes N] [--dram-capacity-bytes N] \
                     [--pmem-capacity-bytes N] [--ssd-capacity-bytes N] \
                     [--placement-threshold-bytes N] \
                     [--replacement-soak-iterations N]"
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

    println!("{{");
    println!("  \"report_version\": \"matrixcache_rocksdb_backend_v1\",");
    println!(
        "  \"backend\": \"{}\",",
        if cfg!(feature = "rocksdb-ssd") {
            "rocksdb"
        } else {
            "file-compat"
        }
    );
    println!("  \"iterations\": {},", iterations);
    println!("  \"data_dir\": \"{}\",", dir.display());
    println!("  \"workload\": {{");
    println!("    \"value_bytes\": {},", config.value_bytes);
    println!(
        "    \"dram_capacity_bytes\": {},",
        config.dram_capacity_bytes
    );
    println!(
        "    \"pmem_capacity_bytes\": {},",
        config.pmem_capacity_bytes
    );
    println!("    \"ssd_capacity_bytes\": {},", config.ssd_capacity_bytes);
    println!(
        "    \"placement_threshold_bytes\": {},",
        config.placement_threshold_bytes
    );
    println!(
        "    \"replacement_soak_iterations\": {}",
        config.replacement_soak_iterations
    );
    println!("  }},");
    print_timing("put", put_timing, true);
    println!("  \"resident_hot_key_count\": {},", resident_hot_keys.len());
    print_timing("resident_hot_get", resident_hot_timing, true);
    print_timing("hot_get", hot_timing, true);
    print_timing("cold_ssd_refill_get", cold_timing, true);
    println!("  \"cold_ssd_refills\": {},", cold_ssd_refills);
    println!("  \"memory_hits\": {},", stats.memory_hits);
    println!("  \"pmem_hits\": {},", stats.pmem_hits);
    println!("  \"ssd_hits\": {},", stats.disk_hits);
    println!("  \"memory_evictions\": {},", stats.memory_evictions);
    println!("  \"pmem_evictions\": {},", stats.pmem_evictions);
    println!("  \"ssd_evictions\": {},", stats.ssd_evictions);
    println!("  \"refill_failures\": {},", stats.refill_failures);
    println!("  \"disk_fills\": {},", stats.disk_fills);
    println!("  \"pmem_fills\": {},", stats.pmem_fills);
    println!(
        "  \"main_pressure_passed\": {},",
        stats.memory_evictions > 0
            && stats.pmem_evictions > 0
            && cold_ssd_refills > 0
            && stats.refill_failures == 0
    );
    println!("  \"replacement_soak_iterations\": {},", soak_iterations);
    println!(
        "  \"replacement_soak_passed\": {},",
        if soak.passed { "true" } else { "false" }
    );
    println!("  \"replacement_soak_reasons\": {:?},", soak.reasons);
    println!(
        "  \"async_writeback_backpressure\": {},",
        soak.observed_async_writeback_backpressure
    );
    println!(
        "  \"restart_disk_refill_ready\": {},",
        soak.restart_disk_refill_ready
    );
    println!("  \"matrixcache_contract\": {{");
    println!("    \"dram_to_pmem_eviction\": {},", dram_to_pmem_eviction);
    println!("    \"pmem_to_ssd_eviction\": {},", pmem_to_ssd_eviction);
    println!(
        "    \"ssd_read_through_refill\": {},",
        ssd_read_through_refill
    );
    println!("    \"replacement_soak\": {},", replacement_soak_ready);
    println!(
        "    \"async_writeback_backpressure\": {},",
        async_writeback_backpressure_ready
    );
    println!(
        "    \"restart_disk_refill\": {},",
        restart_disk_refill_ready
    );
    println!("    \"passed\": {}", contract_passed);
    println!("  }},");
    println!("  \"matrixcache_contract_evidence\": {{");
    println!("    \"dram_to_pmem_eviction\": {{");
    println!("      \"observed\": {},", dram_to_pmem_eviction);
    println!("      \"source\": \"matrixcache_rocksdb_backend_bench\",");
    println!("      \"metric\": \"memory_evictions > 0 && pmem_fills > 0\",");
    println!("      \"memory_evictions\": {},", stats.memory_evictions);
    println!("      \"pmem_fills\": {}", stats.pmem_fills);
    println!("    }},");
    println!("    \"pmem_to_ssd_eviction\": {{");
    println!("      \"observed\": {},", pmem_to_ssd_eviction);
    println!("      \"source\": \"matrixcache_rocksdb_backend_bench\",");
    println!("      \"metric\": \"pmem_evictions > 0 && disk_fills > 0\",");
    println!("      \"pmem_evictions\": {},", stats.pmem_evictions);
    println!("      \"disk_fills\": {}", stats.disk_fills);
    println!("    }},");
    println!("    \"ssd_read_through_refill\": {{");
    println!("      \"observed\": {},", ssd_read_through_refill);
    println!("      \"source\": \"matrixcache_rocksdb_backend_bench\",");
    println!("      \"metric\": \"cold_ssd_refills > 0 && refill_failures == 0\",");
    println!("      \"cold_ssd_refills\": {},", cold_ssd_refills);
    println!("      \"refill_failures\": {}", stats.refill_failures);
    println!("    }},");
    println!("    \"replacement_soak\": {{");
    println!("      \"observed\": {},", replacement_soak_ready);
    println!("      \"source\": \"matrixcache_rocksdb_backend_bench\",");
    println!("      \"metric\": \"replacement_policy_soak.passed\",");
    println!("      \"iterations\": {},", soak_iterations);
    println!("      \"reasons\": {:?}", soak.reasons);
    println!("    }},");
    println!("    \"async_writeback_backpressure\": {{");
    println!(
        "      \"observed\": {},",
        async_writeback_backpressure_ready
    );
    println!("      \"source\": \"matrixcache_rocksdb_backend_bench\",");
    println!("      \"metric\": \"observed_async_writeback_backpressure > 0\",");
    println!(
        "      \"observed_async_writeback_backpressure\": {}",
        soak.observed_async_writeback_backpressure
    );
    println!("    }},");
    println!("    \"restart_disk_refill\": {{");
    println!("      \"observed\": {},", restart_disk_refill_ready);
    println!("      \"source\": \"matrixcache_rocksdb_backend_bench\",");
    println!("      \"metric\": \"replacement_policy_soak.restart_disk_refill_ready\",");
    println!(
        "      \"restart_disk_refill_ready\": {}",
        soak.restart_disk_refill_ready
    );
    println!("    }}");
    println!("  }}");
    println!("}}");
}

fn print_timing(name: &str, timing: Timing, trailing_comma: bool) {
    println!("  \"{name}\": {{");
    println!("    \"count\": {},", timing.count);
    println!("    \"total_ms\": {},", timing.total.as_millis());
    println!("    \"total_us\": {},", timing.total.as_micros());
    println!("    \"qps\": {:.2},", timing.qps);
    println!("    \"p50_us\": {},", timing.p50_us);
    println!("    \"p95_us\": {},", timing.p95_us);
    println!("    \"p99_us\": {},", timing.p99_us);
    println!("    \"p50_ns\": {},", timing.p50_ns);
    println!("    \"p95_ns\": {},", timing.p95_ns);
    println!("    \"p99_ns\": {}", timing.p99_ns);
    println!("  }}{}", if trailing_comma { "," } else { "" });
}
