#!/usr/bin/env python3
"""Validate a MatrixCache read-scaling JSON report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_TOP_LEVEL = {
    "report_version": str,
    "max_entries": int,
    "value_bytes": int,
    "repeats": int,
    "read_trials": int,
    "per_thread_ops": int,
    "shards": int,
    "hit_costs": list,
    "thread_scaling": list,
    "summary": dict,
    "checks": dict,
    "passed": bool,
}

REQUIRED_HIT_COST_FIELDS = {"entries", "ns_per_op"}
REQUIRED_THREAD_FIELDS = {
    "threads",
    "single_lock_ns_per_op",
    "sharded_ns_per_op",
    "single_lock_mops",
    "sharded_mops",
    "speedup",
}
REQUIRED_CHECKS = {
    "has_hit_costs",
    "has_thread_scaling",
    "sharded_speedup_within_budget",
    "single_thread_hit_within_budget",
}
REQUIRED_SUMMARY = {
    "first_hit_ns_per_op",
    "last_hit_ns_per_op",
    "best_sharded_mops",
    "worst_sharded_speedup",
    "output_path",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="Path to matrixcache_read_scaling_v1 JSON")
    parser.add_argument(
        "--allow-failed",
        action="store_true",
        help="Validate report shape but do not require passed=true",
    )
    parser.add_argument("--min-hit-costs", type=int, default=1)
    parser.add_argument("--min-thread-rows", type=int, default=4)
    parser.add_argument("--min-max-threads", type=int)
    parser.add_argument("--min-best-sharded-mops", type=float)
    parser.add_argument("--min-worst-sharded-speedup", type=float)
    parser.add_argument("--max-first-hit-ns", type=float)
    parser.add_argument("--min-repeats", type=int, default=1)
    parser.add_argument("--min-read-trials", type=int, default=1)
    parser.add_argument("--min-per-thread-ops", type=int, default=1)
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache read-scaling report invalid: {message}", file=sys.stderr)
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


def require_number(data: dict[str, Any], field: str) -> float:
    value = data.get(field)
    if not isinstance(value, (int, float)):
        fail(f"field {field!r} must be numeric")
    return float(value)


def validate_hit_cost(row: Any, index: int) -> None:
    if not isinstance(row, dict):
        fail(f"hit_costs[{index}] must be an object")
    missing = REQUIRED_HIT_COST_FIELDS.difference(row)
    if missing:
        fail(f"hit_costs[{index}] missing fields: {', '.join(sorted(missing))}")
    if not isinstance(row["entries"], int) or row["entries"] <= 0:
        fail(f"hit_costs[{index}].entries must be a positive integer")
    if require_number(row, "ns_per_op") <= 0:
        fail(f"hit_costs[{index}].ns_per_op must be positive")


def validate_thread_row(row: Any, index: int) -> None:
    if not isinstance(row, dict):
        fail(f"thread_scaling[{index}] must be an object")
    missing = REQUIRED_THREAD_FIELDS.difference(row)
    if missing:
        fail(f"thread_scaling[{index}] missing fields: {', '.join(sorted(missing))}")
    if not isinstance(row["threads"], int) or row["threads"] <= 0:
        fail(f"thread_scaling[{index}].threads must be a positive integer")
    for field in sorted(REQUIRED_THREAD_FIELDS - {"threads"}):
        if require_number(row, field) <= 0:
            fail(f"thread_scaling[{index}].{field} must be positive")


def validate(args: argparse.Namespace) -> dict[str, Any]:
    data = load_report(args.report)
    for field, expected in REQUIRED_TOP_LEVEL.items():
        require_type(data, field, expected)
    if data["report_version"] != "matrixcache_read_scaling_v1":
        fail(f"unexpected report_version {data['report_version']!r}")
    if data["max_entries"] <= 0 or data["value_bytes"] <= 0:
        fail("max_entries and value_bytes must be positive")
    if data["repeats"] < args.min_repeats:
        fail(f"repeats={data['repeats']} below minimum {args.min_repeats}")
    if data["read_trials"] < args.min_read_trials:
        fail(f"read_trials={data['read_trials']} below minimum {args.min_read_trials}")
    if data["per_thread_ops"] < args.min_per_thread_ops:
        fail(
            f"per_thread_ops={data['per_thread_ops']} below minimum "
            f"{args.min_per_thread_ops}"
        )
    if data["shards"] <= 0:
        fail("shards must be positive")
    if len(data["hit_costs"]) < args.min_hit_costs:
        fail(f"hit_cost count {len(data['hit_costs'])} below {args.min_hit_costs}")
    if len(data["thread_scaling"]) < args.min_thread_rows:
        fail(f"thread row count {len(data['thread_scaling'])} below {args.min_thread_rows}")
    for index, row in enumerate(data["hit_costs"]):
        validate_hit_cost(row, index)
    max_threads = 0
    for index, row in enumerate(data["thread_scaling"]):
        validate_thread_row(row, index)
        max_threads = max(max_threads, row["threads"])
    if args.min_max_threads is not None and max_threads < args.min_max_threads:
        fail(f"max thread count {max_threads} below {args.min_max_threads}")

    checks = data["checks"]
    missing_checks = REQUIRED_CHECKS.difference(checks)
    if missing_checks:
        fail(f"missing checks: {', '.join(sorted(missing_checks))}")
    false_checks = [name for name in sorted(REQUIRED_CHECKS) if checks.get(name) is not True]
    if false_checks and not args.allow_failed:
        fail(f"failing built-in checks: {', '.join(false_checks)}")

    summary = data["summary"]
    missing_summary = REQUIRED_SUMMARY.difference(summary)
    if missing_summary:
        fail(f"missing summary fields: {', '.join(sorted(missing_summary))}")
    best_sharded = require_number(summary, "best_sharded_mops")
    worst_speedup = require_number(summary, "worst_sharded_speedup")
    first_hit = require_number(summary, "first_hit_ns_per_op")
    if best_sharded <= 0 or worst_speedup <= 0 or first_hit <= 0:
        fail("summary performance fields must be positive")
    if args.min_best_sharded_mops is not None and best_sharded < args.min_best_sharded_mops:
        fail(f"best_sharded_mops={best_sharded:.4f} below {args.min_best_sharded_mops:.4f}")
    if args.min_worst_sharded_speedup is not None and worst_speedup < args.min_worst_sharded_speedup:
        fail(f"worst_sharded_speedup={worst_speedup:.4f} below {args.min_worst_sharded_speedup:.4f}")
    if args.max_first_hit_ns is not None and first_hit > args.max_first_hit_ns:
        fail(f"first_hit_ns_per_op={first_hit:.4f} exceeds {args.max_first_hit_ns:.4f}")
    if data["passed"] is not True and not args.allow_failed:
        fail("report passed=false")
    return data


def main() -> int:
    args = parse_args()
    data = validate(args)
    summary = data["summary"]
    print(
        "OK matrixcache read-scaling report: "
        f"hit_costs={len(data['hit_costs'])} thread_rows={len(data['thread_scaling'])} "
        f"max_threads={max(row['threads'] for row in data['thread_scaling'])} "
        f"repeats={data['repeats']} "
        f"read_trials={data['read_trials']} "
        f"per_thread_ops={data['per_thread_ops']} "
        f"best_sharded_mops={float(summary['best_sharded_mops']):.2f} "
        f"worst_speedup={float(summary['worst_sharded_speedup']):.2f}x "
        f"first_hit_ns={float(summary['first_hit_ns_per_op']):.1f}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
