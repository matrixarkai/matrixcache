#!/usr/bin/env python3
"""Compare two MatrixCache read-path overhead JSON reports for regressions."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


LATENCY_FIELDS = (
    "peek_ns_per_op",
    "no_promotion_ns_per_op",
    "full_ns_per_op",
    "overhead_ns_per_op",
)
PERCENT_FIELDS = (
    "overhead_median_percent",
    "overhead_low_percent",
    "overhead_high_percent",
    "spread_percent",
)
IDENTITY_FIELDS = (
    "entries",
    "value_bytes",
    "passes",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="Known-good read-path report")
    parser.add_argument("current", type=Path, help="Report from the run being checked")
    parser.add_argument(
        "--max-latency-regression",
        type=float,
        default=1.35,
        help="Maximum allowed current/baseline ratio for ns-per-op fields",
    )
    parser.add_argument(
        "--max-overhead-regression",
        type=float,
        default=1.35,
        help="Maximum allowed current/baseline ratio for overhead percentage fields",
    )
    parser.add_argument(
        "--max-spread-regression",
        type=float,
        default=1.50,
        help="Maximum allowed current/baseline ratio for pass-to-pass spread",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache read-path comparison failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def load(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"{path} does not exist")
    except json.JSONDecodeError as exc:
        fail(f"{path} is not valid JSON: {exc}")
    if not isinstance(data, dict):
        fail(f"{path} must contain a JSON object")
    if data.get("report_version") != "matrixcache_read_path_v1":
        fail(f"{path} is not a matrixcache_read_path_v1 report")
    if data.get("passed") is not True:
        fail(f"{path} is not a passing report")
    return data


def number(data: dict[str, Any], field: str) -> float:
    value = data.get(field)
    if not isinstance(value, (int, float)):
        fail(f"field {field!r} must be numeric")
    return float(value)


def ratio(current: float, baseline: float) -> float:
    if baseline <= 0:
        return 1.0 if current <= 0 else float("inf")
    return current / baseline


def require_same_shape(baseline: dict[str, Any], current: dict[str, Any]) -> None:
    for field in IDENTITY_FIELDS:
        if baseline.get(field) != current.get(field):
            fail(f"{field} differs: baseline={baseline.get(field)!r} current={current.get(field)!r}")
    baseline_checks = baseline.get("checks")
    current_checks = current.get("checks")
    if not isinstance(baseline_checks, dict) or not isinstance(current_checks, dict):
        fail("both reports must contain a checks object")
    if set(baseline_checks) != set(current_checks):
        fail(
            "check sets differ: "
            f"baseline={sorted(baseline_checks)} current={sorted(current_checks)}"
        )


def main() -> int:
    args = parse_args()
    baseline = load(args.baseline)
    current = load(args.current)
    require_same_shape(baseline, current)

    observed: list[tuple[str, float]] = []
    for field in LATENCY_FIELDS:
        field_ratio = ratio(number(current, field), number(baseline, field))
        if field_ratio > args.max_latency_regression:
            fail(
                f"{field} ratio {field_ratio:.4f} "
                f"exceeds {args.max_latency_regression:.4f}"
            )
        observed.append((field, field_ratio))

    for field in PERCENT_FIELDS:
        limit = args.max_spread_regression if field == "spread_percent" else args.max_overhead_regression
        field_ratio = ratio(number(current, field), number(baseline, field))
        if field_ratio > limit:
            fail(f"{field} ratio {field_ratio:.4f} exceeds {limit:.4f}")
        observed.append((field, field_ratio))

    worst = max(observed, key=lambda item: item[1])
    print(
        "OK matrixcache read-path comparison: "
        f"worst={worst[0]}:{worst[1]:.3f} "
        f"full_ns={number(current, 'full_ns_per_op'):.1f} "
        f"overhead={number(current, 'overhead_median_percent'):.1f}% "
        f"spread={number(current, 'spread_percent'):.1f}%"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
