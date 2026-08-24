# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **`L2CachePolicy` buffers access records again instead of applying them on
  the calling path.** `set_async_on_access` picks between the two modes.
  Buffering keeps the caller off the migration-order update path at the cost of
  that order lagging the workload, and a record arriving once the buffer is
  full is dropped and counted by `access_drop_count`. Previously every record
  was enqueued and drained in the same call, so the buffer bound was
  unreachable and the shedding behaviour could never trigger.
- **Queueing an evicted buffer for the lower tier is opt-in.**
  `set_use_eviction_handler` controls it and it is off by default, so an
  eviction drops the data and the tail passes stay the only path into the lower
  tier. It previously queued unconditionally, which wrote every eviction down a
  tier whether or not that was wanted.
- **A failed migration write no longer stalls the queue behind it.**
  `L2CachePolicy::write_task_internal` counts the failure and keeps draining
  rather than returning early on the first error, so one bad key cannot block
  every entry queued after it. Failures remain visible through
  `write_fail_count`.
- **`L2CachePolicyFactory` sizes from the documented defaults** — 100,000
  migration-order items, 100,000 buffered access records, 1,000 keys per tail
  batch and 10,000 queued writes — rather than the placeholder 1,024 / 1,024 /
  64 / 1,024 it used before.

- **First-in-first-out order no longer resets on overwrite.** `ReplacementFIFO`
  now keeps a key at its original queue position when `put` overwrites it,
  updating only the payload and the byte accounting. Previously the key was
  re-queued at the back, so rewriting a value made an old entry look newly
  inserted and pushed its eviction arbitrarily far out — a cache that is
  written in place never evicted in insertion order at all.
- **A segmented-LRU read no longer re-fronts its entry.** `ReplacementSLRU::get`
  records the access flag and nothing else; list position is decided by the
  maintainer and by eviction. A key that is read once no longer jumps ahead of
  one that was read twice, and the read is now a single index lookup instead of
  two plus a list splice.
- **Constant-time list maintenance across every policy.** `ReplacementFIFO` and
  `BaseLRUList` — and through it `GhostLRUList`, `ArcList` and `ReplacementArc`
  — now share the intrusive doubly-linked list and node arena introduced for
  the segmented LRU, replacing the `VecDeque` rescans that ran on every get,
  delete and overwrite. No policy scans a key list any more. Measured with
  `examples/policy_bench.rs` (64-byte values, best of three):

  | entries | FIFO churn before | after | ARC `get` before | after |
  | ------: | ----------------: | ----: | ---------------: | ----: |
  |   1,024 |           5.69 us | 199 ns |         4.69 us | 340 ns |
  |   4,096 |           20.2 us | 362 ns |         15.9 us | 501 ns |
  |  16,384 |            153 us | 597 ns |         91.5 us | 1.02 us |

  The before column grows linearly with the entry count while the after column
  stays flat, which is the point; the absolute ratios vary with machine noise.
  Segmented-LRU timings are unchanged within that noise, having already moved
  to intrusive lists.

- **Segmented (sharded) `ReplacementSLRU`.** The policy now hash-partitions its
  index and its hot/warm/cold lists into `num_segments` segments (256 by default,
  configurable with `ReplacementSLRU::with_num_segments`), each with an
  independent byte budget of `capacity / num_segments`. Inserts, lookups and
  eviction only touch the segment owning the key, so eviction scans a small
  segment-local list instead of one global list, and no single hot list can
  consume the whole budget. Segment counts are rounded up to a power of two, and
  a capacity below the segment count collapses to a single segment.
- **Constant-time list maintenance in `ReplacementSLRU`.** The hot, warm and cold
  lists are now intrusive doubly-linked lists over a shared node arena instead of
  key deques that were rescanned on every access, so `get`, `delete` and an
  overwriting `put` unlink in O(1) rather than scanning every cached key.
  Measured with `examples/policy_bench.rs` (64-byte values, best of three):

  | entries | `get` before | `get` after | `delete`+re-`put` before | `delete`+re-`put` after |
  | ------: | -----------: | ----------: | -----------------------: | ----------------------: |
  |   1,024 |      28.8 us |      151 ns |                  17.4 us |                  191 ns |
  |   4,096 |       110 us |      260 ns |                  52.5 us |                  247 ns |
  |  16,384 |      1.08 ms |      536 ns |                   503 us |                  563 ns |

  Under eviction pressure the previous implementation could rescan a whole list
  once per eviction attempt and did not finish a 16,384-entry run in 15 minutes;
  the segmented policy sustains roughly 1.2 us per insert at 65,536 entries.
- **Standalone open-source library.** Removed internal-only build and validation
  tooling under `tools/` and every reference to retired internal system names from
  the source, the example, the README, and the crate metadata. The crate is now a
  standalone Rust multi-tier cache library with no references to internal systems.
- Enforce `unsafe_code = "forbid"` via the `[lints]` table — the library is
  entirely unsafe-free.

### Added

- **Paced collection checks.** `StorageGCController::poll` runs a collection
  round when one is due, at an interval set by `set_gc_check_interval_ms` (1
  second by default). The controller has no thread of its own, so a caller
  drives this from its own loop, and the interval keeps the fragmentation check
  off the hot path rather than rescanning on every call. Adds `set_enable_gc`,
  which toggles collection without the drain that `stop` performs, and
  `GC_DEFAULT_CHECK_INTERVAL_MS`.

- **`ConcurrentReplacementSLRU`, a segmented LRU with one lock per segment.**
  `ReplacementSLRU` partitions its lists but is driven through `&mut self`, so a
  caller sharing it had to wrap the whole policy in one lock and the
  partitioning bought nothing — the reason segmenting exists went unrealised.
  The new type gives each segment its own lock, so operations on keys that hash
  to different segments do not wait on each other. Key-to-segment mapping,
  segment budgets and the maintainer behaviour are identical to the
  single-threaded form; it is additive, so existing callers are unaffected.

  Measured with `examples/policy_bench.rs` on 8 cores, one shared workload,
  best of three:

  | threads | one global lock | per-segment locks | speedup |
  | ------: | --------------: | ----------------: | ------: |
  |       1 |          529 ns |            727 ns |   0.73x |
  |       2 |          974 ns |            436 ns |   2.24x |
  |       4 |        1,659 ns |            329 ns |   5.04x |
  |       8 |        2,094 ns |            305 ns |   6.87x |

  Note the single-threaded row: with no contention to relieve, the per-segment
  form is *slower*, paying for the extra indirection and 256 mutexes. Reach for
  it when the policy is genuinely shared; `ReplacementSLRU` remains the better
  choice for single-threaded use.
- A contention sweep in `examples/policy_bench.rs` comparing the two forms
  across thread counts, and tests covering segment-layout agreement between
  them, four threads sharing one policy without an outer lock, eviction
  reporting from every segment, and value round-trips through the shared form.

- **Migration pacing.** `L2CachePolicy::poll` runs whichever of the access,
  tail and write passes are due, at intervals set by `set_access_interval_ms`,
  `set_tail_interval_ms` and `set_write_interval_ms` (1 ms, 1 s and 1 s by
  default). `flush_once` still runs all three unconditionally; `poll` paces
  them the way independent timers would, so a caller driving one loop does not
  write to the lower tier faster than the write interval allows. Throttling
  those writes is the reason the policy exists: migration must not crowd out
  reads on the device.
- `L2CachePolicy` accessors for the new configuration and counters
  (`async_on_access`, `use_eviction_handler`, the three intervals, and
  `access_drop_count`), plus the `L2_DEFAULT_*` constants naming every default.
- Tests pinning the adaptive replacement state machine: promotion out of the
  fetch data list on a hit, both ghost-list hits shifting capacity between the
  fetch and active sides, a total miss dropping the fetch tail outright when
  its ghost list is empty, and delete clearing a key from whichever list holds
  it. The algorithm needed no change; these make that checkable.
- Tests for the migration policy: both access-record modes and the drop on a
  full buffer, the eviction handler defaulting off, interval pacing versus
  `flush_once`, and the factory sizing.

- `examples/policy_bench.rs` replaces `examples/slru_shard_bench.rs` and now
  probes the segmented LRU, FIFO and ARC through the same fill / read / churn
  phases, so the cost of a policy can be tracked as the working set grows.
- `ReplacementFIFO::queue_len`, which reports queued entries. It equals
  `get_item_num`: the queue holds no tombstones for deleted keys.
- Tests for the changed semantics: FIFO keeping its queue position across an
  overwrite and leaving no tombstone behind a delete, a segmented-LRU read
  marking an entry without moving it and the maintainer then promoting it from
  the tail, and node recycling in `BaseLRUList` across repeated churn.

- **Segmented-LRU maintainer.** `ReplacementSLRU` keeps each segment's hot and
  warm lists within a configurable share of the segment budget (`set_hot_lru_pct`
  and `set_warm_lru_pct`, defaulting to 20% and 40%), promoting entries touched
  twice into the warm list and demoting the rest into the cold list.
  `run_lru_maintainer_pass` sweeps every segment, each `put` maintains the
  segment it touched, and `test_config_lru_maintainer` disables both.
- Segment introspection on `ReplacementSLRU`: `num_segments`,
  `segment_byte_limit`, `segment_used_size`, `list_used_size`, `list_item_num`
  and `segment_for_key`, with `GetSegmentUsedSize`, `GetSegmentByteLimit`,
  `GetListUsedSize`, `PickSegment` and `LRUMaintainerTask` aliases.
- `examples/policy_bench.rs`, a throughput probe that sweeps the working-set
  size at a fixed segment count and the segment count under eviction pressure,
  reporting both a table and a JSON summary.
- Unit tests covering segment-count resolution, key distribution and per-segment
  budgets, byte and item accounting across overwrite-delete-reuse churn, and the
  maintainer's promote, demote and cold-eviction paths.

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
