# MatrixCache

[![CI](https://github.com/matrixarkai/MatrixCache/actions/workflows/ci.yml/badge.svg)](https://github.com/matrixarkai/MatrixCache/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](Cargo.toml)

`matrixcache` is a standalone, Rust-native multi-tier cache library extracted
from TemporalStore and reusable by any Rust service. It manages a hot in-memory
(DRAM) tier, a persistent-memory-like resident tier, and an SSD tier, with
admission control, cross-tier eviction, read-through refill, pinned handles,
invalidation, and asynchronous writeback with backpressure accounting.

## Features

- Multi-tier placement across DRAM, a PMEM-like resident tier, and an SSD tier.
- Admission policy, cross-tier eviction, and read-through refill.
- Pinned (no-promotion) handles and pinned-acquire APIs.
- Shared-buffer reads for retrieval and scan paths that can consume cached
  bytes without copying them.
- Asynchronous writeback with backpressure counters and cache-pressure helpers.
- Persistent-tier auto-recovery and restart refill.
- Latency and tier metrics for observability.

## SSD backend

RocksDB is the default SSD key-value backend, enabled by the `rocksdb-ssd`
feature (on by default), so `cargo build` and `cargo test` exercise the RocksDB
path. SSD writes use RocksDB as the block authority and do not double-write raw
block shadow files. A small file-backed compatibility store is available with
`--no-default-features` for lightweight local diagnostics; it is not intended as
a production backend.

The default feature set compiles RocksDB from source, which requires a native build toolchain plus `clang`/`libclang` and `cmake`:

```bash
# Ubuntu / Debian
sudo apt-get install -y build-essential pkg-config libssl-dev clang libclang-dev cmake
```

## Usage

```bash
cargo build
cargo test
cargo test --no-default-features       # file-backed compatibility store
cargo run --release --example rocksdb_backend_bench -- --iterations 5000 --json-output /tmp/matrixcache-rocksdb-backend.json --emit-operator-log --require-passed
cargo run --release --no-default-features --example metrics_server
```

The `rocksdb_backend_bench` example drives the multi-tier cache against the
RocksDB SSD backend and prints a JSON report (backend, tier evictions, resident
hot key count, cold SSD refills, PMEM soak activity, pressure and
replacement-soak status, p99 latency, average latency, and QPS) that is useful
as local performance/behavior evidence.
Each archive also includes an `operator_log` object with the compact fields most
useful in build logs and scale dashboards: pass/fail, QPS, average, p95 and p99
latency, tier evictions, refills, and write-back backpressure. Validate
archived reports before publishing or comparing them. The validator checks
report shape, cache evidence counters, QPS math, average latency math, and that
`operator_log` agrees with the detailed report:

```bash
tools/validate_backend_report.py /tmp/matrixcache-rocksdb-backend.json --expect-backend rocksdb --min-iterations 5000 --min-replacement-soak-iterations 5000 --min-cold-ssd-refills 1 --min-memory-evictions 1 --min-pmem-evictions 1 --min-disk-fills 1 --min-async-writeback-backpressure 1 --max-refill-failures 0
tools/emit_backend_operator_log.py /tmp/matrixcache-rocksdb-backend.json
```

Compare a new backend archive with a known-good run before accepting a cache
optimization as a scale improvement. The comparator checks p95/p99 latency,
average latency, QPS, replacement-loop max latency, and required cache evidence
counters:

```bash
tools/compare_backend_reports.py /tmp/matrixcache-rocksdb-backend-baseline.json /tmp/matrixcache-rocksdb-backend.json --max-hot-get-p95-regression 1.30 --max-cold-refill-p95-regression 1.45 --max-hot-get-p99-regression 1.35 --max-cold-refill-p99-regression 1.50 --max-hot-get-avg-regression 1.35 --max-cold-refill-avg-regression 1.50 --min-hot-get-qps-ratio 0.80 --min-cold-refill-qps-ratio 0.75 --min-counter-ratio 0.90
```

## Durability

Each durable tier decides separately whether a write also survives the machine
losing power, and the two default differently. Both are set on `CacheOptions`.

| option | default | what the default means |
|---|---|---|
| `ssd_block_durability` | `true` | every SSD block write is flushed, and so is the directory entry after the rename |
| `pmem_block_durability` | `false` | the persistent tier writes and renames, and flushes neither |

A block is always **whole** either way, because it arrives by rename -- no
reader ever sees a torn block. What the setting decides is whether a block the
cache believed it had written is still there after the machine stops abruptly.

The persistent tier's name invites the opposite assumption, so it is worth
stating plainly: real persistent memory is durable without being flushed, and
this tier is files standing in for it. Files are not.

Flushing is most of what a block write costs. Measured on one machine, 500 puts
of 64-byte values:

| | us/put | fsync calls |
|---|---|---|
| SSD tier flushed | ~7,600 | 1,000 |
| SSD tier not flushed | ~420 | 0 |
| persistent tier flushed | ~5,500 | 872, for 436 blocks |
| persistent tier not flushed | ~330 | 0 |

Those ratios are one machine's virtual disk and will be smaller on real flash;
the call counts will not be. `examples/manifest_append_cost.rs` takes both
settings as arguments, so the numbers can be re-measured rather than believed.

**Choosing.** On a tier that is purely a cache, a block lost to a crash is a
miss, and not flushing is the cheaper trade. It stops being a miss if something
recovers the tier and expects it to be complete: with `auto_recover_on_start`
set, recovery restores a tier with holes in it and no way to know which entries
used to be there. `CacheOptions::validate()` reports that combination.

`auto_recover_on_start` is `false` by default, so a default cache does not read
its SSD tier back at all -- and is paying for durability it never uses.

## Checking a configuration

`CacheOptions::validate()` reads a configuration back and says what the cache
will actually do where that differs from what was asked for: a replacement
policy name nobody offers, a tier given a size and no path, recovery from a
tier that was never flushed, or a shard count that makes a tier refuse values
it has room for. `MultiLayerCache::try_with_options` refuses the findings that
mean the cache cannot do its job and starts anyway on the rest.

## Measuring

The examples are measurements rather than demonstrations. Each prints a table
and says what the number means:

- `read_path_cost` -- where the time goes on a memory-tier hit, and what the
  promotion bookkeeping costs
- `hit_concurrency_bench` -- how reads and zero-copy handles scale with threads
- `eviction_bench`, `steady_state_put_bench` -- eviction cost at capacity, and
  whether it grows with the cache
- `manifest_append_cost` -- the write path's syscalls, with the durability
  settings above
- `rocksdb_backend_bench` -- multi-tier DRAM/PMEM/RocksDB-SSD pressure, cold
  read refill, restart refill, replacement soak, and writeback backpressure.
  Add `--json-output <path>` to archive the report and `--require-passed` to
  fail the process when the cache contract is not satisfied. Validate archived
  reports with `tools/validate_backend_report.py`; CI uses the same validator
  against the lightweight `--no-default-features` compatibility backend, while
  production evidence should use the default RocksDB backend. Use
  `tools/emit_backend_operator_log.py <report.json>` to print the compact
  log-friendly summary.
- `scan_resistance_bench`, `admission_filter_bench` -- what the admission
  policy is worth against a scan
- `soak` -- long-running memory-pressure and latency stability. Add `--json`
  to append a machine-readable report to stdout, or `--json-output <path>` to
  archive the same report as a standalone file for Grafana/comparison scripts.
  Add `--require-passed` when CI or a scale script should fail the process on a
  missed memory, hit-rate, or p99 latency gate. Use `--duration-seconds` and
  `--sample-seconds` for short validation runs. The JSON latency section
  includes average, p50, p95, p99, and max estimates from the same histogram
  buckets exported to Prometheus. Validate an archived report with
  `tools/validate_soak_report.py`; compare it to a baseline with
  `tools/compare_soak_reports.py`.
- `metrics_server` -- serves Prometheus text metrics for Grafana. Import
  [`docs/grafana/matrixcache-dashboard.json`](docs/grafana/matrixcache-dashboard.json)
  and see [`docs/grafana.md`](docs/grafana.md) for a local scrape setup. The
  exporter includes direct p50/p95/p99 latency gauges alongside Prometheus
  histograms.
- `batch_write_cost` -- bounded sharded write/control-path benchmark for
  `put_batch`, pinned insert/release, and acquire/release. It compares
  colocated batches against fanout batches and prints the sharded batch
  counters used by the Grafana/Prometheus path.
- `batch_read_cost` -- bounded read/retrieval benchmark for copied reads,
  shared-buffer reads, no-promotion scans, pinned acquire reads, and sharded
  colocated reads. Add `--json-output <path>` to archive a machine-readable
  report, then validate it with `tools/validate_batch_read_report.py`.
- `batch_concurrency_bench` -- concurrent batch-read throughput benchmark.
  Add `--json-output <path>` to archive scale evidence and validate it with
  `tools/validate_batch_concurrency_report.py`.
- `hit_concurrency_bench` -- concurrent memory-hit throughput benchmark. Add
  `--json-output <path>` to archive scale evidence and validate it with
  `tools/validate_hit_concurrency_report.py`.
- `tools/run_scale_reports.py` -- runs the read/retrieval scale benchmarks
  above, validates each JSON report, and writes a manifest that points to the
  archived reports. Add `--markdown-output <path>` when the same run should
  also leave an operator/Grafana-friendly summary.
- `tools/compare_scale_reports.py` -- compares two scale report manifests and
  fails on configured latency or throughput regressions.

Benchmarks that report a ratio measure both sides inside one pass and print the
spread across passes. A run whose spread is wide measured the machine rather
than the cache, and the number should be discarded rather than quoted.

## Minimum Supported Rust Version

MSRV is **1.88**, set by the `rocksdb` dependency behind the default
`rocksdb-ssd` feature.

## Contributing, security, and license

See [`CONTRIBUTING.md`](CONTRIBUTING.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md),
[`SECURITY.md`](SECURITY.md), and [`CHANGELOG.md`](CHANGELOG.md). Licensed under
the Apache License, Version 2.0 ([`LICENSE`](LICENSE)).

Product and crate names are trademarks; see [`TRADEMARKS.md`](TRADEMARKS.md).

Third-party dependency licenses and attributions are listed in
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
