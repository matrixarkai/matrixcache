#!/usr/bin/env python3
"""Validate a MatrixCache eviction JSON report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_TOP_LEVEL = {
    "report_version": str,
    "value_bytes": int,
    "write_pressure_writes": int,
    "read_pressure_reads": int,
    "summary": dict,
    "steady_state": list,
    "hit_rates": list,
    "checks": dict,
    "passed": bool,
}
REQUIRED_SUMMARY = {
    "max_ns_per_write": (int, float),
    "max_groups_per_eviction": (int, float),
    "min_hit_rate_percent": (int, float),
    "total_promotions": int,
}
REQUIRED_CHECKS = {
    "positive_timings",
    "candidate_groups_within_budget",
    "hit_rate_within_budget",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="Path to matrixcache_eviction_v1 JSON")
    parser.add_argument("--allow-failed", action="store_true")
    parser.add_argument("--min-steady-rows", type=int, default=1)
    parser.add_argument("--min-hit-rate-rows", type=int, default=1)
    parser.add_argument("--max-ns-per-write", type=float)
    parser.add_argument("--max-groups-per-eviction", type=float)
    parser.add_argument("--min-hit-rate-percent", type=float)
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache eviction report invalid: {message}", file=sys.stderr)
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


def validate_row_set(rows: list[Any], required: dict[str, Any], name: str) -> None:
    entries = set()
    for row in rows:
        if not isinstance(row, dict):
            fail(f"{name} rows must be objects")
        for field, expected in required.items():
            require_type(row, field, expected)
        entry = row["entries"]
        if entry <= 0:
            fail(f"{name} entries must be positive")
        if entry in entries:
            fail(f"duplicate {name} entry count {entry}")
        entries.add(entry)


def validate(args: argparse.Namespace) -> dict[str, Any]:
    data = load(args.report)
    for field, expected in REQUIRED_TOP_LEVEL.items():
        require_type(data, field, expected)
    if data["report_version"] != "matrixcache_eviction_v1":
        fail(f"unexpected report_version {data['report_version']!r}")
    if data["value_bytes"] <= 0 or data["write_pressure_writes"] <= 0 or data["read_pressure_reads"] <= 0:
        fail("value_bytes and workload counts must be positive")

    summary = data["summary"]
    for field, expected in REQUIRED_SUMMARY.items():
        require_type(summary, field, expected)
    if number(summary, "max_ns_per_write") <= 0:
        fail("max_ns_per_write must be positive")
    if number(summary, "max_groups_per_eviction") <= 0:
        fail("max_groups_per_eviction must be positive")
    if not 0 <= number(summary, "min_hit_rate_percent") <= 100:
        fail("min_hit_rate_percent must be between 0 and 100")

    if len(data["steady_state"]) < args.min_steady_rows:
        fail("too few steady_state rows")
    if len(data["hit_rates"]) < args.min_hit_rate_rows:
        fail("too few hit_rates rows")
    validate_row_set(
        data["steady_state"],
        {"entries": int, "ns_per_write": (int, float), "groups_per_eviction": (int, float)},
        "steady_state",
    )
    validate_row_set(
        data["hit_rates"],
        {"entries": int, "hit_rate_percent": (int, float), "promotions": int},
        "hit_rates",
    )

    checks = data["checks"]
    missing = REQUIRED_CHECKS.difference(checks)
    if missing:
        fail(f"missing checks: {', '.join(sorted(missing))}")
    false_checks = [name for name in sorted(REQUIRED_CHECKS) if checks.get(name) is not True]
    if false_checks and not args.allow_failed:
        fail(f"failing built-in checks: {', '.join(false_checks)}")
    if data["passed"] is not True and not args.allow_failed:
        fail("report passed=false")

    if args.max_ns_per_write is not None and number(summary, "max_ns_per_write") > args.max_ns_per_write:
        fail("max_ns_per_write exceeds limit")
    if (
        args.max_groups_per_eviction is not None
        and number(summary, "max_groups_per_eviction") > args.max_groups_per_eviction
    ):
        fail("max_groups_per_eviction exceeds limit")
    if args.min_hit_rate_percent is not None and number(summary, "min_hit_rate_percent") < args.min_hit_rate_percent:
        fail("min_hit_rate_percent below limit")
    return data


def main() -> int:
    args = parse_args()
    data = validate(args)
    summary = data["summary"]
    print(
        "OK matrixcache eviction report: "
        f"steady_rows={len(data['steady_state'])} hit_rows={len(data['hit_rates'])} "
        f"max_ns={float(summary['max_ns_per_write']):.1f} "
        f"groups={float(summary['max_groups_per_eviction']):.1f} "
        f"min_hit={float(summary['min_hit_rate_percent']):.2f}%"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
