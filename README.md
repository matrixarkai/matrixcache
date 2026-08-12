# MatrixCache

[![CI](https://github.com/bjmeetsfo/MatrixCache/actions/workflows/ci.yml/badge.svg)](https://github.com/bjmeetsfo/MatrixCache/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.74-blue.svg)](Cargo.toml)

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

The default feature set compiles RocksDB from source, which requires a C/C++
toolchain plus `clang`/`libclang` and `cmake`:

```bash
# Ubuntu / Debian
sudo apt-get install -y build-essential pkg-config libssl-dev clang libclang-dev cmake
```

## Usage

```bash
cargo build
cargo test
cargo test --no-default-features       # file-backed compatibility store
cargo run --release --example rocksdb_parity_bench -- 5000
```

The `rocksdb_parity_bench` example drives the multi-tier cache against the
RocksDB SSD backend and prints a JSON report (backend, tier evictions, resident
hot key count, cold SSD refills, pressure and replacement-soak status) that is
useful as local performance/behavior evidence.

## Minimum Supported Rust Version

MSRV is **1.74**.

## Contributing, security, and license

See [`CONTRIBUTING.md`](CONTRIBUTING.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md),
[`SECURITY.md`](SECURITY.md), and [`CHANGELOG.md`](CHANGELOG.md). Licensed under
the Apache License, Version 2.0 ([`LICENSE`](LICENSE)).

Product and crate names are trademarks; see [`TRADEMARKS.md`](TRADEMARKS.md).

Third-party dependency licenses and attributions are listed in
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
