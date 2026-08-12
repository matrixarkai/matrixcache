# Contributing to MatrixCache

Thanks for your interest in improving MatrixCache — the standalone Rust multi-tier
cache library extracted from TemporalStore.

## Development

The default feature set builds RocksDB from source, so you need a C/C++ toolchain
plus `clang`/`libclang` and `cmake`:

```bash
# Ubuntu / Debian
sudo apt-get install -y build-essential pkg-config libssl-dev clang libclang-dev cmake
```

Then:

```bash
cargo build
cargo test                       # default: RocksDB SSD backend
cargo test --no-default-features # file-backed compatibility store
cargo fmt --all
cargo clippy --all-targets
cargo doc --no-deps
```

The Minimum Supported Rust Version (MSRV) is **1.74**. CI runs formatting, a
warnings-as-errors build, tests for both the default (RocksDB) and
`--no-default-features` backends, docs, the MSRV check, Clippy (`-D warnings`), and
`cargo-deny` (license + advisory) — all must pass.

## Pull requests

1. Keep changes focused and add tests for behavior changes. Cover both the RocksDB
   default backend and the `--no-default-features` compatibility store where the
   change affects storage behavior.
2. Ensure `cargo fmt --all -- --check`, `cargo test`, and `cargo doc --no-deps` are
   green.
3. Update [`CHANGELOG.md`](CHANGELOG.md) under **Unreleased**.
4. Keep the public source free of references to internal or proprietary systems.

## Reporting issues

Use the issue templates. For security-sensitive reports, follow
[`SECURITY.md`](SECURITY.md) instead of opening a public issue.

## License

By contributing, you agree that your contributions are licensed under the
Apache-2.0 license (see [`LICENSE`](LICENSE)).
