#!/usr/bin/env python3
"""Compare two MatrixCache read-scaling JSON reports for regressions."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


SUMMARY_MAX_RATIO_FIELDS = (
    "first_hit_ns_per_op",
    "last_hit_ns_per_op",
)
SUMMARY_MIN_RATIO_FIELDS = (
    "best_sharded_mops",
    "worst_sharded_speedup",
)
THREAD_MAX_RATIO_FIELDS = (
    "single_lock_ns_per_op",
    "sharded_ns_per_op",
)
THREAD_MIN_RATIO_FIELDS = (
    "single_lock_mops",
    "sharded_mops",
    "speedup",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="Known-good read-scaling report")
    parser.add_argument("current", type=Path, help="Report from the run being checked")
    parser.add_argument(
        "--max-latency-regression",
        type=float,
        default=1.35,
        help="Maximum allowed current/baseline ratio for ns-per-op fields",
    )
    parser.add_argument(
        "--min-throughput-ratio",
        type=float,
        default=0.80,
        help="Minimum allowed current/baseline ratio for Mops/s and speedup fields",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache read-scaling comparison failed: {message}", file=sys.stderr)
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
    if data.get("report_version") != "matrixcache_read_scaling_v1":
        fail(f"{path} is not a matrixcache_read_scaling_v1 report")
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


def summary(data: dict[str, Any]) -> dict[str, Any]:
    value = data.get("summary")
    if not isinstance(value, dict):
        fail("missing summary object")
    return value


def thread_rows(data: dict[str, Any]) -> dict[int, dict[str, Any]]:
    rows = data.get("thread_scaling")
    if not isinstance(rows, list):
        fail("missing thread_scaling list")
    out: dict[int, dict[str, Any]] = {}
    for row in rows:
        if not isinstance(row, dict):
            fail("thread_scaling rows must be objects")
        threads = row.get("threads")
        if not isinstance(threads, int) or threads <= 0:
            fail("thread_scaling row has invalid thread count")
        if threads in out:
            fail(f"duplicate thread count {threads}")
        out[threads] = row
    return out


def hit_costs(data: dict[str, Any]) -> dict[int, dict[str, Any]]:
    rows = data.get("hit_costs")
    if not isinstance(rows, list):
        fail("missing hit_costs list")
    out: dict[int, dict[str, Any]] = {}
    for row in rows:
        if not isinstance(row, dict):
            fail("hit_cost rows must be objects")
        entries = row.get("entries")
        if not isinstance(entries, int) or entries <= 0:
            fail("hit_cost row has invalid entry count")
        if entries in out:
            fail(f"duplicate entry count {entries}")
        out[entries] = row
    return out


def main() -> int:
    args = parse_args()
    baseline = load(args.baseline)
    current = load(args.current)
    for field in ("max_entries", "value_bytes", "repeats", "read_trials", "per_thread_ops", "shards"):
        if baseline.get(field) != current.get(field):
            fail(f"{field} differs: baseline={baseline.get(field)!r} current={current.get(field)!r}")

    baseline_hit_costs = hit_costs(baseline)
    current_hit_costs = hit_costs(current)
    if set(baseline_hit_costs) != set(current_hit_costs):
        fail(
            "hit-cost entry sets differ: "
            f"baseline={sorted(baseline_hit_costs)} current={sorted(current_hit_costs)}"
        )
    baseline_threads = thread_rows(baseline)
    current_threads = thread_rows(current)
    if set(baseline_threads) != set(current_threads):
        fail(
            "thread sets differ: "
            f"baseline={sorted(baseline_threads)} current={sorted(current_threads)}"
        )

    max_ratios: list[tuple[str, float]] = []
    min_ratios: list[tuple[str, float]] = []
    base_summary = summary(baseline)
    current_summary = summary(current)
    for field in SUMMARY_MAX_RATIO_FIELDS:
        field_ratio = ratio(number(current_summary, field), number(base_summary, field))
        if field_ratio > args.max_latency_regression:
            fail(
                f"summary.{field} ratio {field_ratio:.4f} "
                f"exceeds {args.max_latency_regression:.4f}"
            )
        max_ratios.append((f"summary.{field}", field_ratio))
    for field in SUMMARY_MIN_RATIO_FIELDS:
        field_ratio = ratio(number(current_summary, field), number(base_summary, field))
        if field_ratio < args.min_throughput_ratio:
            fail(
                f"summary.{field} ratio {field_ratio:.4f} "
                f"below {args.min_throughput_ratio:.4f}"
            )
        min_ratios.append((f"summary.{field}", field_ratio))

    for entries in sorted(baseline_hit_costs):
        field_ratio = ratio(
            number(current_hit_costs[entries], "ns_per_op"),
            number(baseline_hit_costs[entries], "ns_per_op"),
        )
        if field_ratio > args.max_latency_regression:
            fail(
                f"hit_costs[{entries}].ns_per_op ratio {field_ratio:.4f} "
                f"exceeds {args.max_latency_regression:.4f}"
            )
        max_ratios.append((f"hit_costs.{entries}.ns_per_op", field_ratio))

    for threads in sorted(baseline_threads):
        for field in THREAD_MAX_RATIO_FIELDS:
            field_ratio = ratio(number(current_threads[threads], field), number(baseline_threads[threads], field))
            if field_ratio > args.max_latency_regression:
                fail(
                    f"thread_scaling[{threads}].{field} ratio {field_ratio:.4f} "
                    f"exceeds {args.max_latency_regression:.4f}"
                )
            max_ratios.append((f"thread_scaling.{threads}.{field}", field_ratio))
        for field in THREAD_MIN_RATIO_FIELDS:
            field_ratio = ratio(number(current_threads[threads], field), number(baseline_threads[threads], field))
            if field_ratio < args.min_throughput_ratio:
                fail(
                    f"thread_scaling[{threads}].{field} ratio {field_ratio:.4f} "
                    f"below {args.min_throughput_ratio:.4f}"
                )
            min_ratios.append((f"thread_scaling.{threads}.{field}", field_ratio))

    worst_latency = max(max_ratios, key=lambda item: item[1])
    weakest_throughput = min(min_ratios, key=lambda item: item[1])
    print(
        "OK matrixcache read-scaling comparison: "
        f"worst_latency={worst_latency[0]}:{worst_latency[1]:.3f} "
        f"weakest_throughput={weakest_throughput[0]}:{weakest_throughput[1]:.3f}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
