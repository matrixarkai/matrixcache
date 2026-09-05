#!/usr/bin/env python3
"""Validate a MatrixCache read-path overhead JSON report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_FIELDS = {
    "report_version": str,
    "entries": int,
    "value_bytes": int,
    "passes": int,
    "peek_ns_per_op": (int, float),
    "no_promotion_ns_per_op": (int, float),
    "full_ns_per_op": (int, float),
    "overhead_ns_per_op": (int, float),
    "overhead_median_percent": (int, float),
    "overhead_low_percent": (int, float),
    "overhead_high_percent": (int, float),
    "spread_percent": (int, float),
    "checks": dict,
    "passed": bool,
}
REQUIRED_CHECKS = {
    "positive_timings",
    "full_hit_within_budget",
    "overhead_within_budget",
    "spread_within_budget",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="Path to matrixcache_read_path_v1 JSON")
    parser.add_argument("--allow-failed", action="store_true")
    parser.add_argument("--max-full-ns", type=float)
    parser.add_argument("--max-overhead-percent", type=float)
    parser.add_argument("--max-spread-percent", type=float)
    parser.add_argument("--min-passes", type=int, default=1)
    parser.add_argument("--min-entries", type=int, default=1)
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache read-path report invalid: {message}", file=sys.stderr)
    raise SystemExit(1)


def load(path: Path) -> dict[str, Any]:
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


def number(data: dict[str, Any], field: str) -> float:
    require_type(data, field, (int, float))
    return float(data[field])


def validate(args: argparse.Namespace) -> dict[str, Any]:
    data = load(args.report)
    for field, expected in REQUIRED_FIELDS.items():
        require_type(data, field, expected)
    if data["report_version"] != "matrixcache_read_path_v1":
        fail(f"unexpected report_version {data['report_version']!r}")
    if data["entries"] <= 0 or data["value_bytes"] <= 0:
        fail("entries and value_bytes must be positive")
    if data["entries"] < args.min_entries:
        fail(f"entries={data['entries']} below minimum {args.min_entries}")
    if data["passes"] < args.min_passes:
        fail(f"passes={data['passes']} below minimum {args.min_passes}")
    for field in ("peek_ns_per_op", "no_promotion_ns_per_op", "full_ns_per_op"):
        if number(data, field) <= 0:
            fail(f"{field} must be positive")
    checks = data["checks"]
    missing = REQUIRED_CHECKS.difference(checks)
    if missing:
        fail(f"missing checks: {', '.join(sorted(missing))}")
    false_checks = [name for name in sorted(REQUIRED_CHECKS) if checks.get(name) is not True]
    if false_checks and not args.allow_failed:
        fail(f"failing built-in checks: {', '.join(false_checks)}")
    if args.max_full_ns is not None and number(data, "full_ns_per_op") > args.max_full_ns:
        fail(f"full_ns_per_op={number(data, 'full_ns_per_op'):.4f} exceeds {args.max_full_ns:.4f}")
    if args.max_overhead_percent is not None and number(data, "overhead_median_percent") > args.max_overhead_percent:
        fail(f"overhead_median_percent={number(data, 'overhead_median_percent'):.4f} exceeds {args.max_overhead_percent:.4f}")
    if args.max_spread_percent is not None and number(data, "spread_percent") > args.max_spread_percent:
        fail(f"spread_percent={number(data, 'spread_percent'):.4f} exceeds {args.max_spread_percent:.4f}")
    if data["passed"] is not True and not args.allow_failed:
        fail("report passed=false")
    return data


def main() -> int:
    args = parse_args()
    data = validate(args)
    print(
        "OK matrixcache read-path report: "
        f"entries={data['entries']} passes={data['passes']} "
        f"full_ns={float(data['full_ns_per_op']):.1f} "
        f"overhead_ns={float(data['overhead_ns_per_op']):.1f} "
        f"overhead={float(data['overhead_median_percent']):.1f}% "
        f"spread={float(data['spread_percent']):.1f}%"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
