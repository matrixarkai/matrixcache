# MatrixCache Grafana Dashboard

This directory contains a minimal Grafana/Prometheus setup for watching
MatrixCache memory pressure, tier movement, eviction, writeback backpressure,
and latency while a soak or service workload runs.

## Export Metrics

Run the built-in exporter:

```bash
cargo run --release --no-default-features --example metrics_server
```

It exposes:

- `http://127.0.0.1:9184/metrics`
- `http://127.0.0.1:9184/healthz`

The exporter drives a small skewed workload plus small and large sharded batch
reads so the dashboard has moving hit-rate, latency, and batch fan-out series.
Production services should call `matrixcache::prometheus_text` from their own
metrics endpoint and attach service-specific labels such as shard, tier, table,
or process.

## Prometheus

Use [docs/prometheus/matrixcache-scrape.yml](prometheus/matrixcache-scrape.yml)
as a starting point. If Prometheus is not running in Docker, replace
`host.docker.internal:9184` with `127.0.0.1:9184`.

```bash
docker run --rm -p 9090:9090 \
  -v "$PWD/docs/prometheus/matrixcache-scrape.yml:/etc/prometheus/prometheus.yml:ro" \
  prom/prometheus
```

## Grafana

Import [docs/grafana/matrixcache-dashboard.json](grafana/matrixcache-dashboard.json)
and select the Prometheus datasource. The dashboard uses the generated metric
families directly, including:

- memory hit rate from `matrixcache_memory_hits` and `matrixcache_misses`
- tier residency from `matrixcache_memory_bytes`,
  `matrixcache_pmem_bytes`, and `matrixcache_disk_bytes`
- eviction pressure from memory, PMEM, SSD, and pinned-skip counters
- async writeback queue depth, drain rate, and backpressure rejections
- sharded batch fan-out/local decisions from `matrixcache_sharded_batch_fanout_operations`,
  `matrixcache_sharded_batch_local_operations`, `matrixcache_sharded_batch_fanout_shards`,
  `matrixcache_sharded_batch_latency_p95_seconds`, and `matrixcache_sharded_batch_latency_p99_seconds`
- p50, p95, p99, and average latency for get, put, read-through, refill, writeback,
  and eviction


The `soak` example can also emit JSON with optional scale gates for p99 get/put
latency and hit rate. The archived `latency` object includes p50/p95/p99 plus
max observed latency for read-through, refill, writeback, eviction, and
compaction so tail spikes stay visible outside Prometheus:

```text
cargo run --release --no-default-features --example soak -- 10 8 --json --sample-seconds 10 --max-get-p99-us 5000 --max-put-p99-us 8000 --min-hit-rate-percent 80
```

Add `--require-passed` when an automated scale gate should exit nonzero after
the JSON report names the failing check. Add `--json-output <path>` when the
same report should be archived without scraping the console stream:

```bash
cargo run --release --no-default-features --example soak -- 10 8 --json-output /tmp/matrixcache-soak.json --require-passed --sample-seconds 10 --max-get-p99-us 5000 --max-put-p99-us 8000 --min-hit-rate-percent 80
```

Validate archived reports before publishing or comparing them. The validator
requires the operational max-latency fields so old archives cannot silently pass
as current soak evidence:

```bash
tools/validate_soak_report.py /tmp/matrixcache-soak.json --max-get-p99-us 5000 --max-put-p99-us 8000 --min-hit-rate-percent 80
```

Compare a current archive with a known-good baseline before accepting a scale
run as an optimization result. In addition to get/put p99, the comparator checks
read-through, refill, writeback, eviction, and compaction max-latency regression:

```bash
tools/compare_soak_reports.py /tmp/matrixcache-baseline.json /tmp/matrixcache-soak.json --max-get-p99-regression 1.10 --max-put-p99-regression 1.10 --max-operation-max-regression 1.50 --min-throughput-ratio 0.95
```

The JSON report keeps memory-bound checks separate from optional latency and hit-rate
budgets so a scale run can fail for the exact reason that moved.

For the batch control path TemporalStore uses to warm, pin, release, and rewrite
groups of block entries, archive `batch_write_cost` too. The report captures
colocated and fan-out batch costs for `put_batch`, `insert_pinned_batch_sized`
plus release, and zero-copy `acquire_batch` plus release, along with sharded
batch and refill counters:

```bash
cargo run --release --no-default-features --example batch_write_cost -- --json-output /tmp/matrixcache-batch-control.json --require-passed
tools/validate_batch_control_report.py /tmp/matrixcache-batch-control.json --min-batches 2 --min-disk-hits 1 --min-zero-copy-hits 1 --min-refill-samples 1
tools/compare_batch_control_reports.py /tmp/matrixcache-batch-control-baseline.json /tmp/matrixcache-batch-control.json --max-ns-regression 1.35 --min-counter-ratio 0.95
```

For memory-read scale checks, archive `cache_scaling_bench` too. This report
keeps the single-lock and sharded read paths in one schema so CI and Grafana
archives can track whether sharding is still buying lower latency under
concurrency:

```bash
cargo run --release --no-default-features --example cache_scaling_bench -- 4096 --json-output /tmp/matrixcache-read-scaling.json --require-passed --min-sharded-speedup 0.25 --max-single-thread-ns 1000000
tools/validate_read_scaling_report.py /tmp/matrixcache-read-scaling.json --min-hit-costs 1 --min-thread-rows 4 --min-worst-sharded-speedup 0.25 --max-first-hit-ns 1000000
```

For RocksDB-backed SSD-cache scale checks, archive the backend report too:

```bash
cargo run --release --example rocksdb_backend_bench -- --iterations 5000 --json-output /tmp/matrixcache-rocksdb-backend.json --require-passed
tools/validate_backend_report.py /tmp/matrixcache-rocksdb-backend.json --expect-backend rocksdb --min-iterations 5000 --min-cold-ssd-refills 1 --max-refill-failures 0
```

CI also runs the same report contract against the file-backed compatibility
backend with `--no-default-features`. That smoke test proves the report schema,
eviction/refill evidence fields, and fail-closed contract without paying the
full RocksDB native build cost on every tiny backend-report iteration. The
replacement-soak evidence also records max latency for read-through, refill,
writeback, eviction, and compaction so backend pressure runs carry the same
tail-latency signal as the Prometheus dashboard.

## Scale Report Pairing

For non-Grafana archives, run:

```bash
cargo run --release --no-default-features --example soak -- 60 4 --json
```

Use shorter runs for CI smoke checks:

```bash
cargo run --no-default-features --example soak -- 1 1 --duration-seconds 1 --sample-seconds 1 --json
```

The JSON report, Prometheus exporter, and Grafana dashboard intentionally watch
the same families: bounded memory, hit rate, eviction pressure, writeback
pressure, and latency stability.
