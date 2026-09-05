#!/usr/bin/env python3
"""Validate MatrixCache Grafana panels against exported Prometheus metrics."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


METRIC_RE = re.compile(r"\bmatrixcache_[a-zA-Z0-9_]+\b")
DEFAULT_REQUIRED = (
    "matrixcache_memory_hits",
    "matrixcache_misses",
    "matrixcache_memory_bytes",
    "matrixcache_pmem_bytes",
    "matrixcache_disk_bytes",
    "matrixcache_async_writeback_queue_depth",
    "matrixcache_async_writeback_backpressure_rejections",
    "matrixcache_get_latency_p95_seconds",
    "matrixcache_get_latency_p99_seconds",
    "matrixcache_put_latency_p95_seconds",
    "matrixcache_put_latency_p99_seconds",
    "matrixcache_read_through_latency_p95_seconds",
    "matrixcache_refill_latency_p95_seconds",
    "matrixcache_eviction_latency_p95_seconds",
    "matrixcache_writeback_latency_p95_seconds",
    "matrixcache_sharded_batch_latency_p95_seconds",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--metrics-source",
        type=Path,
        default=Path("src/core/metrics.rs"),
        help="Generated Rust Prometheus exporter to inspect",
    )
    parser.add_argument(
        "--dashboard",
        type=Path,
        default=Path("docs/grafana/matrixcache-dashboard.json"),
        help="Grafana dashboard JSON to validate",
    )
    parser.add_argument(
        "--require",
        action="append",
        default=[],
        help="Metric that must appear in at least one dashboard expression",
    )
    parser.add_argument(
        "--no-default-required",
        action="store_true",
        help="Only require metrics explicitly supplied with --require",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache dashboard invalid: {message}", file=sys.stderr)
    raise SystemExit(1)


def exported_metrics(path: Path) -> set[str]:
    try:
        text = path.read_text()
    except FileNotFoundError:
        fail(f"{path} does not exist")
    names = set(METRIC_RE.findall(text))
    expanded = set(names)
    for name in names:
        if name.endswith("_seconds"):
            expanded.add(f"{name}_bucket")
            expanded.add(f"{name}_sum")
            expanded.add(f"{name}_count")
    if not expanded:
        fail(f"{path} did not contain exported matrixcache metrics")
    return expanded


def iter_targets(value: Any) -> list[str]:
    expressions: list[str] = []
    if isinstance(value, dict):
        expr = value.get("expr")
        if isinstance(expr, str) and expr.strip():
            expressions.append(expr)
        for child in value.values():
            expressions.extend(iter_targets(child))
    elif isinstance(value, list):
        for child in value:
            expressions.extend(iter_targets(child))
    return expressions


def dashboard_metrics(path: Path) -> tuple[set[str], int]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"{path} does not exist")
    except json.JSONDecodeError as exc:
        fail(f"{path} is not valid JSON: {exc}")
    expressions = iter_targets(data)
    metrics: set[str] = set()
    for expr in expressions:
        metrics.update(METRIC_RE.findall(expr))
    if not expressions:
        fail(f"{path} has no Prometheus expressions")
    return metrics, len(expressions)


def main() -> int:
    args = parse_args()
    exported = exported_metrics(args.metrics_source)
    referenced, expression_count = dashboard_metrics(args.dashboard)
    unknown = sorted(referenced - exported)
    if unknown:
        fail("dashboard references metrics not exported by prometheus_text: " + ", ".join(unknown))

    required = set(args.require)
    if not args.no_default_required:
        required.update(DEFAULT_REQUIRED)
    missing_required = sorted(required - referenced)
    if missing_required:
        fail("dashboard is missing required cache panels/queries: " + ", ".join(missing_required))

    print(
        "OK matrixcache Grafana dashboard: "
        f"expressions={expression_count} referenced_metrics={len(referenced)} "
        f"exported_metrics={len(exported)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
