#!/usr/bin/env python3
"""Validate a MatrixCache soak JSON report.

The soak example can fail closed while it runs, but scale jobs also archive the
JSON report for later comparison. This validator keeps those archives honest:
it checks the schema, the built-in gate booleans, and any stricter thresholds a
caller wants to enforce after the run.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_TOP_LEVEL = {
    "report_version": str,
    "reads": int,
    "writes": int,
    "peak_entries": int,
    "peak_memory_bytes": int,
    "observed_hit_rate_percent": (int, float),
    "interval_best_kops": (int, float),
    "latency": dict,
    "checks": dict,
    "passed": bool,
}

REQUIRED_CHECKS = {
    "bounded_entries",
    "bounded_memory",
    "steady_throughput_ceiling",
    "get_p99_within_budget",
    "put_p99_within_budget",
    "hit_rate_within_budget",
}

REQUIRED_LATENCY = {
    "get_p50_us",
    "get_p95_us",
    "get_p99_us",
    "put_p50_us",
    "put_p95_us",
    "put_p99_us",
    "read_through_p99_us",
    "read_through_max_us",
    "refill_p99_us",
    "refill_max_us",
    "writeback_p99_us",
    "writeback_max_us",
    "eviction_p99_us",
    "eviction_max_us",
    "compaction_p99_us",
    "compaction_max_us",
    "histogram_ready",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="Path to matrixcache_soak_v1 JSON")
    parser.add_argument(
        "--allow-failed",
        action="store_true",
        help="Validate the report shape but do not require passed=true",
    )
    parser.add_argument("--min-hit-rate-percent", type=float)
    parser.add_argument("--max-get-p99-us", type=int)
    parser.add_argument("--max-put-p99-us", type=int)
    parser.add_argument("--max-eviction-p99-us", type=int)
    parser.add_argument("--min-reads", type=int, default=1)
    parser.add_argument("--min-writes", type=int, default=0)
    parser.add_argument("--min-memory-evictions", type=int, default=0)
    parser.add_argument("--min-get-samples", type=int, default=1)
    parser.add_argument("--min-put-samples", type=int, default=0)
    parser.add_argument("--min-eviction-samples", type=int, default=0)
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache soak report invalid: {message}", file=sys.stderr)
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
        fail(f"field {field!r} has type {type(data[field]).__name__}, not {expected}")


def require_numeric_at_most(data: dict[str, Any], field: str, limit: int | None) -> None:
    if limit is None:
        return
    value = data.get(field)
    if not isinstance(value, int):
        fail(f"latency field {field!r} must be an integer")
    if value > limit:
        fail(f"{field}={value} exceeds limit {limit}")


def validate(args: argparse.Namespace) -> dict[str, Any]:
    data = load_report(args.report)
    for field, expected in REQUIRED_TOP_LEVEL.items():
        require_type(data, field, expected)

    if data["report_version"] != "matrixcache_soak_v1":
        fail(f"unexpected report_version {data['report_version']!r}")
    if data["reads"] < args.min_reads:
        fail(f"reads={data['reads']} below minimum {args.min_reads}")
    if data["writes"] < args.min_writes:
        fail(f"writes={data['writes']} below minimum {args.min_writes}")
    if data.get("memory_evictions", 0) < args.min_memory_evictions:
        fail(
            f"memory_evictions={data.get('memory_evictions', 0)} below minimum "
            f"{args.min_memory_evictions}"
        )
    if data["peak_entries"] < 0 or data["peak_memory_bytes"] < 0:
        fail("peak entry and memory counts must be non-negative")

    checks = data["checks"]
    missing_checks = REQUIRED_CHECKS.difference(checks)
    if missing_checks:
        fail(f"missing checks: {', '.join(sorted(missing_checks))}")
    false_checks = [name for name in sorted(REQUIRED_CHECKS) if checks.get(name) is not True]
    if false_checks and not args.allow_failed:
        fail(f"failing built-in checks: {', '.join(false_checks)}")

    latency = data["latency"]
    missing_latency = REQUIRED_LATENCY.difference(latency)
    if missing_latency:
        fail(f"missing latency fields: {', '.join(sorted(missing_latency))}")
    if latency["histogram_ready"] is not True:
        fail("latency histogram is not ready")
    if latency.get("get_count", 0) < args.min_get_samples:
        fail(f"get_count={latency.get('get_count', 0)} below minimum {args.min_get_samples}")
    if latency.get("put_count", 0) < args.min_put_samples:
        fail(f"put_count={latency.get('put_count', 0)} below minimum {args.min_put_samples}")
    if latency.get("eviction_count", 0) < args.min_eviction_samples:
        fail(
            f"eviction_count={latency.get('eviction_count', 0)} below minimum "
            f"{args.min_eviction_samples}"
        )

    if not args.allow_failed and data["passed"] is not True:
        fail("report passed=false")
    if args.min_hit_rate_percent is not None:
        observed = float(data["observed_hit_rate_percent"])
        if observed < args.min_hit_rate_percent:
            fail(f"observed_hit_rate_percent={observed:.4f} below {args.min_hit_rate_percent}")

    require_numeric_at_most(latency, "get_p99_us", args.max_get_p99_us)
    require_numeric_at_most(latency, "put_p99_us", args.max_put_p99_us)
    require_numeric_at_most(latency, "eviction_p99_us", args.max_eviction_p99_us)
    return data


def main() -> int:
    args = parse_args()
    data = validate(args)
    print(
        "OK matrixcache soak report: "
        f"reads={data['reads']} writes={data['writes']} "
        f"evictions={data.get('memory_evictions', 0)} "
        f"hit_rate={float(data['observed_hit_rate_percent']):.2f}% "
        f"get_p99={data['latency']['get_p99_us']}us "
        f"put_p99={data['latency']['put_p99_us']}us"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
