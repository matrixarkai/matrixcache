#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 MatrixArkAI
"""Validate a MatrixCache scale report comparison JSON archive."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REPORT_VERSION = "matrixcache_scale_report_comparison_v1"
REQUIRED_TOP_LEVEL = {
    "report_version": str,
    "baseline_manifest": str,
    "current_manifest": str,
    "reports": int,
    "latency_fields": int,
    "throughput_fields": int,
    "worst_latency_ratio": (int, float),
    "worst_throughput_ratio": (int, float),
    "max_latency_regression": (int, float),
    "min_throughput_ratio": (int, float),
    "latency_ratios": list,
    "throughput_ratios": list,
    "passed": bool,
    "operator_log": dict,
}
REQUIRED_OPERATOR_LOG = {
    "passed",
    "reports",
    "latency_fields",
    "throughput_fields",
    "worst_latency_ratio",
    "weakest_throughput_ratio",
    "max_latency_regression",
    "min_throughput_ratio",
}
RELATIVE_TOLERANCE = 0.001


def fail(message: str) -> None:
    print(f"matrixcache scale comparison report invalid: {message}", file=sys.stderr)
    raise SystemExit(1)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--min-reports", type=int, default=1)
    parser.add_argument("--min-latency-fields", type=int, default=1)
    parser.add_argument("--min-throughput-fields", type=int, default=1)
    parser.add_argument("--max-latency-ratio", type=float)
    parser.add_argument("--min-throughput-ratio", type=float)
    parser.add_argument("--allow-failed", action="store_true")
    return parser.parse_args()


def load_report(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"{path} does not exist")
    except json.JSONDecodeError as error:
        fail(f"{path} is not valid JSON: {error}")
    if not isinstance(data, dict):
        fail("top-level JSON value must be an object")
    return data


def require_type(data: dict[str, Any], field: str, expected: Any) -> None:
    if field not in data:
        fail(f"missing field {field!r}")
    if not isinstance(data[field], expected):
        fail(f"{field!r} has type {type(data[field]).__name__}, not {expected}")


def require_positive_number(data: dict[str, Any], field: str) -> float:
    value = data.get(field)
    if not isinstance(value, (int, float)):
        fail(f"{field!r} must be numeric")
    value = float(value)
    if value <= 0.0:
        fail(f"{field!r} must be positive")
    return value


def require_non_negative_int(data: dict[str, Any], field: str) -> int:
    value = data.get(field)
    if not isinstance(value, int):
        fail(f"{field!r} must be an integer")
    if value < 0:
        fail(f"{field!r} must be non-negative")
    return value


def ratio_items(data: dict[str, Any], field: str) -> list[dict[str, Any]]:
    items = data[field]
    for index, item in enumerate(items):
        if not isinstance(item, dict):
            fail(f"{field}[{index}] must be an object")
        metric = item.get("metric")
        ratio = item.get("ratio")
        if not isinstance(metric, str) or not metric:
            fail(f"{field}[{index}].metric must be a non-empty string")
        if not isinstance(ratio, (int, float)) or float(ratio) <= 0.0:
            fail(f"{field}[{index}].ratio must be positive")
    return items


def close_enough(left: float, right: float) -> bool:
    return abs(left - right) <= max(RELATIVE_TOLERANCE, abs(right) * RELATIVE_TOLERANCE)


def validate_operator_log(data: dict[str, Any]) -> None:
    operator_log = data["operator_log"]
    missing = REQUIRED_OPERATOR_LOG.difference(operator_log)
    if missing:
        fail(f"operator_log missing fields: {', '.join(sorted(missing))}")
    expected = {
        "passed": data["passed"],
        "reports": data["reports"],
        "latency_fields": data["latency_fields"],
        "throughput_fields": data["throughput_fields"],
        "worst_latency_ratio": data["worst_latency_ratio"],
        "weakest_throughput_ratio": data["worst_throughput_ratio"],
        "max_latency_regression": data["max_latency_regression"],
        "min_throughput_ratio": data["min_throughput_ratio"],
    }
    for field, expected_value in expected.items():
        observed = operator_log.get(field)
        if isinstance(expected_value, float):
            if not isinstance(observed, (int, float)):
                fail(f"operator_log.{field} must be numeric")
            if not close_enough(float(observed), expected_value):
                fail(
                    f"operator_log.{field}={float(observed):.4f} disagrees "
                    f"with detailed value {expected_value:.4f}"
                )
        elif observed != expected_value:
            fail(
                f"operator_log.{field}={observed!r} disagrees "
                f"with detailed value {expected_value!r}"
            )


def validate(data: dict[str, Any], args: argparse.Namespace) -> None:
    for field, expected in REQUIRED_TOP_LEVEL.items():
        require_type(data, field, expected)
    if data["report_version"] != REPORT_VERSION:
        fail(f"unexpected report_version {data['report_version']!r}")
    if not data["baseline_manifest"]:
        fail("baseline_manifest must be non-empty")
    if not data["current_manifest"]:
        fail("current_manifest must be non-empty")

    reports = require_non_negative_int(data, "reports")
    latency_fields = require_non_negative_int(data, "latency_fields")
    throughput_fields = require_non_negative_int(data, "throughput_fields")
    if reports < args.min_reports:
        fail(f"reports={reports} below {args.min_reports}")
    if latency_fields < args.min_latency_fields:
        fail(f"latency_fields={latency_fields} below {args.min_latency_fields}")
    if throughput_fields < args.min_throughput_fields:
        fail(f"throughput_fields={throughput_fields} below {args.min_throughput_fields}")

    max_latency_regression = require_positive_number(data, "max_latency_regression")
    min_throughput_ratio = require_positive_number(data, "min_throughput_ratio")
    worst_latency = require_positive_number(data, "worst_latency_ratio")
    worst_throughput = require_positive_number(data, "worst_throughput_ratio")
    if args.max_latency_ratio is not None and worst_latency > args.max_latency_ratio:
        fail(f"worst_latency_ratio={worst_latency:.4f} exceeds {args.max_latency_ratio:.4f}")
    if args.min_throughput_ratio is not None and worst_throughput < args.min_throughput_ratio:
        fail(
            f"worst_throughput_ratio={worst_throughput:.4f} below "
            f"{args.min_throughput_ratio:.4f}"
        )
    if worst_latency > max_latency_regression and not args.allow_failed:
        fail(
            f"worst_latency_ratio={worst_latency:.4f} exceeds "
            f"max_latency_regression={max_latency_regression:.4f}"
        )
    if worst_throughput < min_throughput_ratio and not args.allow_failed:
        fail(
            f"worst_throughput_ratio={worst_throughput:.4f} below "
            f"min_throughput_ratio={min_throughput_ratio:.4f}"
        )

    latency = ratio_items(data, "latency_ratios")
    throughput = ratio_items(data, "throughput_ratios")
    if len(latency) != latency_fields:
        fail(f"latency_ratios has {len(latency)} rows, expected {latency_fields}")
    if len(throughput) != throughput_fields:
        fail(f"throughput_ratios has {len(throughput)} rows, expected {throughput_fields}")
    if latency:
        observed_worst_latency = max(float(item["ratio"]) for item in latency)
        if not close_enough(observed_worst_latency, worst_latency):
            fail("worst_latency_ratio disagrees with latency_ratios")
    if throughput:
        observed_worst_throughput = min(float(item["ratio"]) for item in throughput)
        if not close_enough(observed_worst_throughput, worst_throughput):
            fail("worst_throughput_ratio disagrees with throughput_ratios")
    if data["passed"] is not True and not args.allow_failed:
        fail("report passed=false")
    validate_operator_log(data)


def main() -> int:
    args = parse_args()
    data = load_report(args.report)
    validate(data, args)
    print(
        "OK matrixcache scale comparison report: "
        f"reports={data['reports']} "
        f"latency_fields={data['latency_fields']} "
        f"throughput_fields={data['throughput_fields']} "
        f"worst_latency={float(data['worst_latency_ratio']):.3f}x "
        f"worst_throughput={float(data['worst_throughput_ratio']):.3f}x"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
