# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Severed the C++ parity coupling.** Removed the internal C++ parity tooling
  under `tools/` and every reference to the retired internal cache-engine name from
  the source, the example, the README, and the crate metadata. The crate is now a
  standalone Rust multi-tier cache library with no references to internal systems.
- Enforce `unsafe_code = "forbid"` via the `[lints]` table — the library is
  entirely unsafe-free.

### Added

- Open-source project files: `LICENSE` (Apache-2.0), a rewritten `README.md`,
  `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, this changelog, issue and
  pull-request templates, `CODEOWNERS`, and Dependabot configuration.
- Continuous integration: formatting, warnings-as-errors build, tests for both the
  default (RocksDB) and `--no-default-features` backends, docs, an MSRV (Rust 1.74)
  job, a blocking Clippy gate, and a `cargo-deny` license/advisory gate. Plus a
  tag-triggered crates.io release workflow.
- Published-crate metadata: declared MSRV, documentation/homepage links, keywords,
  categories, packaging `exclude`, and docs.rs configuration.
- Crate-level API documentation with a runnable usage example, and an informational
  coverage job (`cargo-llvm-cov`).
- Additional unit tests for the storage-config fixed-encoding helpers, storage
  engine-type conversions, and the SSD index / write buffer — raising that module's
  function coverage from ~44% to ~62% and overall coverage to ~82%.

## [0.1.0]

- Initial standalone Rust multi-tier cache library: a hot DRAM tier, a
  persistent-memory-like resident tier, and a RocksDB-backed SSD tier, with
  admission policy, cross-tier eviction, pinned (no-promotion) handles,
  read-through refill, invalidation, asynchronous writeback with backpressure
  accounting, persistent-tier auto-recovery, and tier/latency metrics.
