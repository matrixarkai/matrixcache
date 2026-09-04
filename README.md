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
cargo run --release --example rocksdb_backend_bench -- 5000
```

The `rocksdb_backend_bench` example drives the multi-tier cache against the
RocksDB SSD backend and prints a JSON report (backend, tier evictions, resident
hot key count, cold SSD refills, pressure and replacement-soak status) that is
useful as local performance/behavior evidence.

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
- `scan_resistance_bench`, `admission_filter_bench` -- what the admission
  policy is worth against a scan
- `soak` -- long-running memory-pressure and latency stability. Add `--json`
  to append a machine-readable report for Grafana/comparison archives; add
  `--require-passed` when CI or a scale script should fail the process on a
  missed memory, hit-rate, or p99 latency gate. Use `--duration-seconds` and
  `--sample-seconds` for short validation runs. The JSON latency section
  includes average, p50, p95, p99, and max estimates from the same histogram
  buckets exported to Prometheus.
- `metrics_server` -- serves Prometheus text metrics for Grafana. Import
  [`docs/grafana/matrixcache-dashboard.json`](docs/grafana/matrixcache-dashboard.json)
  and see [`docs/grafana.md`](docs/grafana.md) for a local scrape setup. The
  exporter includes direct p50/p95/p99 latency gauges alongside Prometheus
  histograms.

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
