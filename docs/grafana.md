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

The exporter drives a small skewed workload so a dashboard has moving series.
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
  `matrixcache_sharded_batch_local_operations`, and `matrixcache_sharded_batch_fanout_shards`
- p50, p95, and average latency for get, put, read-through, refill, writeback,
  and eviction

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
