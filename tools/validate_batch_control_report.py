#!/usr/bin/env python3
"""Validate a MatrixCache batch-control benchmark JSON report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_TOP_LEVEL = {
    "report_version": str,
    "shards": int,
    "value_bytes": int,
    "passes": int,
    "passed": bool,
    "batches": list,
    "stats": dict,
}

REQUIRED_BATCH_FIELDS = {
    "batch",
    "put_colocated_ns_per_entry",
    "put_fanout_ns_per_entry",
    "insert_pinned_release_colocated_ns_per_entry",
    "insert_pinned_release_fanout_ns_per_entry",
    "acquire_release_colocated_ns_per_entry",
    "acquire_release_fanout_ns_per_entry",
}

REQUIRED_STATS = {
    "sharded_batch_local_operations",
    "sharded_batch_fanout_operations",
    "sharded_batch_fanout_shards",
    "sharded_batch_latency_samples",
    "sharded_batch_latency_max_micros",
    "disk_hits",
    "zero_copy_handle_hits",
    "refill_latency_samples",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "report", type=Path, help="Path to matrixcache_batch_control_v1 JSON"
    )
    parser.add_argument(
        "--allow-failed",
        action="store_true",
        help="Validate shape but do not require passed=true",
    )
    parser.add_argument("--min-batches", type=int, default=2)
    parser.add_argument("--min-passes", type=int, default=1)
    parser.add_argument("--min-disk-hits", type=int, default=1)
    parser.add_argument("--min-zero-copy-hits", type=int, default=1)
    parser.add_argument("--min-refill-samples", type=int, default=1)
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache batch-control report invalid: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_report(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"{path} does not exist")
    except json.JSONDecodeError as exc:
        fail(f"{path} is not valid JSON: {exc}")
    if not isinstance(data, dict):
        fail("top-level JSON value must be an object")
    return data


def require_type(data: dict[str, Any], field: str, expected: Any) -> None:
    if field not in data:
        fail(f"missing field {field!r}")
    if not isinstance(data[field], expected):
        fail(
            f"field {field!r} has type {type(data[field]).__name__}, not {expected}"
        )


def require_positive_number(data: dict[str, Any], field: str) -> float:
    value = data.get(field)
    if not isinstance(value, (int, float)):
        fail(f"field {field!r} must be numeric")
    value = float(value)
    if value <= 0:
        fail(f"field {field!r} must be positive")
    return value


def validate_batch(row: Any, index: int) -> None:
    if not isinstance(row, dict):
        fail(f"batches[{index}] must be an object")
    missing = REQUIRED_BATCH_FIELDS.difference(row)
    if missing:
        fail(f"batches[{index}] missing fields: {', '.join(sorted(missing))}")
    if not isinstance(row.get("batch"), int) or row["batch"] <= 0:
        fail(f"batches[{index}].batch must be a positive integer")
    for field in sorted(REQUIRED_BATCH_FIELDS - {"batch"}):
        require_positive_number(row, field)


def validate(args: argparse.Namespace) -> dict[str, Any]:
    data = load_report(args.report)
    for field, expected in REQUIRED_TOP_LEVEL.items():
        require_type(data, field, expected)
    if data["report_version"] != "matrixcache_batch_control_v1":
        fail(f"unexpected report_version {data['report_version']!r}")
    if data["shards"] <= 0:
        fail("shards must be positive")
    if data["value_bytes"] <= 0:
        fail("value_bytes must be positive")
    if data["passes"] <= 0:
        fail("passes must be positive")
    if data["passes"] < args.min_passes:
        fail(f"passes={data['passes']} below {args.min_passes}")
    if len(data["batches"]) < args.min_batches:
        fail(f"batch count {len(data['batches'])} below {args.min_batches}")
    for index, row in enumerate(data["batches"]):
        validate_batch(row, index)

    stats = data["stats"]
    missing_stats = REQUIRED_STATS.difference(stats)
    if missing_stats:
        fail(f"stats missing fields: {', '.join(sorted(missing_stats))}")
    for field in REQUIRED_STATS:
        if not isinstance(stats[field], int):
            fail(f"stats.{field} must be an integer")
    if stats["sharded_batch_local_operations"] <= 0:
        fail("stats.sharded_batch_local_operations must be positive")
    if stats["sharded_batch_fanout_operations"] <= 0:
        fail("stats.sharded_batch_fanout_operations must be positive")
    if stats["sharded_batch_latency_samples"] <= 0:
        fail("stats.sharded_batch_latency_samples must be positive")
    if stats["disk_hits"] < args.min_disk_hits:
        fail(f"stats.disk_hits={stats['disk_hits']} below {args.min_disk_hits}")
    if stats["zero_copy_handle_hits"] < args.min_zero_copy_hits:
        fail(
            "stats.zero_copy_handle_hits="
            f"{stats['zero_copy_handle_hits']} below {args.min_zero_copy_hits}"
        )
    if stats["refill_latency_samples"] < args.min_refill_samples:
        fail(
            "stats.refill_latency_samples="
            f"{stats['refill_latency_samples']} below {args.min_refill_samples}"
        )
    if data["passed"] is not True and not args.allow_failed:
        fail("report passed=false")
    return data


def main() -> int:
    args = parse_args()
    data = validate(args)
    stats = data["stats"]
    print(
        "OK matrixcache batch-control report: "
        f"batches={len(data['batches'])} passes={data['passes']} "
        f"local={stats['sharded_batch_local_operations']} "
        f"fanout={stats['sharded_batch_fanout_operations']} "
        f"disk_hits={stats['disk_hits']} "
        f"zero_copy_hits={stats['zero_copy_handle_hits']} "
        f"refill_samples={stats['refill_latency_samples']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
