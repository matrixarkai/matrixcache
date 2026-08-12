## Summary

<!-- What does this change, and why? -->

## Storage impact

<!--
Does this affect cache placement, eviction, writeback, refill, or persistence?
If so, note which backends are affected (RocksDB default and/or the
`--no-default-features` compatibility store). If not, write "none".
-->

## Checklist

- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo test` passes (default / RocksDB)
- [ ] `cargo test --no-default-features` passes where relevant
- [ ] `cargo clippy --all-targets` is clean
- [ ] `cargo doc --no-deps` builds
- [ ] Added or updated tests for behavior changes
- [ ] Updated `CHANGELOG.md` (Unreleased)
- [ ] No references to internal or proprietary systems added
