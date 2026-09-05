#!/usr/bin/env python3
"""Compare two MatrixCache batch-control JSON reports for regressions."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


TIMING_FIELDS = (
    "put_colocated_ns_per_entry",
    "put_fanout_ns_per_entry",
    "insert_pinned_release_colocated_ns_per_entry",
    "insert_pinned_release_fanout_ns_per_entry",
    "acquire_release_colocated_ns_per_entry",
    "acquire_release_fanout_ns_per_entry",
)

COUNTER_FIELDS = (
    "sharded_batch_local_operations",
    "sharded_batch_fanout_operations",
    "sharded_batch_fanout_shards",
    "sharded_batch_latency_samples",
    "disk_hits",
    "zero_copy_handle_hits",
    "refill_latency_samples",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="Known-good batch-control report")
    parser.add_argument("current", type=Path, help="Report from the run being checked")
    parser.add_argument(
        "--max-ns-regression",
        type=float,
        default=1.35,
        help="Maximum allowed current/baseline ratio for each ns-per-entry field",
    )
    parser.add_argument(
        "--min-counter-ratio",
        type=float,
        default=0.95,
        help="Minimum allowed current/baseline ratio for evidence counters",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache batch-control comparison failed: {message}", file=sys.stderr)
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
    if data.get("report_version") != "matrixcache_batch_control_v1":
        fail(f"{path} is not a matrixcache_batch_control_v1 report")
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


def batches_by_size(data: dict[str, Any]) -> dict[int, dict[str, Any]]:
    batches = data.get("batches")
    if not isinstance(batches, list):
        fail("missing batches list")
    out: dict[int, dict[str, Any]] = {}
    for row in batches:
        if not isinstance(row, dict):
            fail("batch rows must be objects")
        batch = row.get("batch")
        if not isinstance(batch, int) or batch <= 0:
            fail("batch row has invalid batch size")
        if batch in out:
            fail(f"duplicate batch size {batch}")
        out[batch] = row
    return out


def stats(data: dict[str, Any]) -> dict[str, Any]:
    value = data.get("stats")
    if not isinstance(value, dict):
        fail("missing stats object")
    return value


def main() -> int:
    args = parse_args()
    baseline = load(args.baseline)
    current = load(args.current)
    baseline_batches = batches_by_size(baseline)
    current_batches = batches_by_size(current)
    if set(baseline_batches) != set(current_batches):
        fail(
            "batch sizes differ: "
            f"baseline={sorted(baseline_batches)} current={sorted(current_batches)}"
        )

    timing_ratios: list[tuple[str, float]] = []
    for batch in sorted(baseline_batches):
        base_row = baseline_batches[batch]
        current_row = current_batches[batch]
        for field in TIMING_FIELDS:
            field_ratio = ratio(number(current_row, field), number(base_row, field))
            if field_ratio > args.max_ns_regression:
                fail(
                    f"batch {batch} {field} ratio {field_ratio:.4f} "
                    f"exceeds {args.max_ns_regression:.4f}"
                )
            timing_ratios.append((f"{batch}.{field}", field_ratio))

    base_stats = stats(baseline)
    current_stats = stats(current)
    counter_ratios: list[tuple[str, float]] = []
    for field in COUNTER_FIELDS:
        base = number(base_stats, field)
        current_value = number(current_stats, field)
        field_ratio = ratio(current_value, base)
        if field_ratio < args.min_counter_ratio:
            fail(
                f"stats.{field} ratio {field_ratio:.4f} "
                f"below {args.min_counter_ratio:.4f}"
            )
        counter_ratios.append((field, field_ratio))

    worst_timing = max(timing_ratios, key=lambda item: item[1])
    weakest_counter = min(counter_ratios, key=lambda item: item[1])
    print(
        "OK matrixcache batch-control comparison: "
        f"worst_timing={worst_timing[0]}:{worst_timing[1]:.3f} "
        f"weakest_counter={weakest_counter[0]}:{weakest_counter[1]:.3f}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
