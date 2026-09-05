#!/usr/bin/env python3
"""Compare MatrixCache backend benchmark reports for cache regression gates."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REPORT_VERSION = "matrixcache_rocksdb_backend_v1"
TIMING_FIELDS = (
    "put",
    "resident_hot_get",
    "hot_get",
    "cold_ssd_refill_get",
)
WORKLOAD_INVARIANTS = (
    "value_bytes",
    "dram_capacity_bytes",
    "pmem_capacity_bytes",
    "ssd_capacity_bytes",
    "placement_threshold_bytes",
    "replacement_soak_iterations",
)
TOP_LEVEL_INVARIANTS = ("backend", "iterations", "replacement_soak_iterations")
COUNTER_FIELDS = (
    "cold_ssd_refills",
    "memory_hits",
    "pmem_hits",
    "ssd_hits",
    "memory_evictions",
    "pmem_evictions",
    "disk_fills",
    "async_writeback_backpressure",
)
REPLACEMENT_SOAK_MAX_FIELDS = (
    "read_through_latency_max_micros",
    "refill_latency_max_micros",
    "writeback_latency_max_micros",
    "eviction_latency_max_micros",
    "compaction_latency_max_micros",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("current", type=Path)
    parser.add_argument("--max-put-p99-regression", type=float, default=1.35)
    parser.add_argument("--max-resident-hot-get-p99-regression", type=float, default=1.35)
    parser.add_argument("--max-hot-get-p99-regression", type=float, default=1.35)
    parser.add_argument("--max-cold-refill-p99-regression", type=float, default=1.50)
    parser.add_argument("--min-put-qps-ratio", type=float, default=0.80)
    parser.add_argument("--min-hot-get-qps-ratio", type=float, default=0.80)
    parser.add_argument("--min-cold-refill-qps-ratio", type=float, default=0.75)
    parser.add_argument("--min-counter-ratio", type=float, default=0.90)
    parser.add_argument("--max-refill-failure-growth", type=int, default=0)
    parser.add_argument("--max-replacement-max-regression", type=float, default=1.50)
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache backend report comparison failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_report(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"{path} does not exist")
    except json.JSONDecodeError as exc:
        fail(f"{path} is not valid JSON: {exc}")
    if not isinstance(data, dict):
        fail(f"{path} top-level JSON value must be an object")
    if data.get("report_version") != REPORT_VERSION:
        fail(f"{path} has report_version={data.get('report_version')!r}")
    contract = data.get("matrixcache_contract")
    if not isinstance(contract, dict):
        fail(f"{path} missing matrixcache_contract")
    if contract.get("passed") is not True:
        fail(f"{path} has matrixcache_contract.passed={contract.get('passed')!r}")
    return data


def number_at(data: dict[str, Any], path: str) -> float:
    value: Any = data
    for part in path.split("."):
        if not isinstance(value, dict) or part not in value:
            fail(f"missing numeric field {path!r}")
        value = value[part]
    if not isinstance(value, (int, float)):
        fail(f"field {path!r} must be numeric, got {type(value).__name__}")
    return float(value)


def value_at(data: dict[str, Any], path: str) -> Any:
    value: Any = data
    for part in path.split("."):
        if not isinstance(value, dict) or part not in value:
            fail(f"missing field {path!r}")
        value = value[part]
    return value


def positive_ratio(current: float, baseline: float, path: str) -> float:
    if baseline <= 0:
        fail(f"baseline {path!r} must be positive")
    return current / baseline


def compare_equal(baseline: dict[str, Any], current: dict[str, Any], path: str) -> None:
    left = value_at(baseline, path)
    right = value_at(current, path)
    if left != right:
        fail(f"{path} changed: baseline={left!r} current={right!r}")


def compare_latency(
    baseline: dict[str, Any],
    current: dict[str, Any],
    field: str,
    limit: float,
) -> tuple[str, float]:
    ratio = positive_ratio(
        number_at(current, f"{field}.p99_us"),
        number_at(baseline, f"{field}.p99_us"),
        f"{field}.p99_us",
    )
    if ratio > limit:
        fail(f"{field}.p99_us regressed {ratio:.3f}x above limit {limit:.3f}x")
    return field, ratio


def compare_qps(
    baseline: dict[str, Any],
    current: dict[str, Any],
    field: str,
    limit: float,
) -> tuple[str, float]:
    ratio = positive_ratio(
        number_at(current, f"{field}.qps"),
        number_at(baseline, f"{field}.qps"),
        f"{field}.qps",
    )
    if ratio < limit:
        fail(f"{field}.qps ratio {ratio:.3f} below limit {limit:.3f}")
    return field, ratio


def compare_counter(
    baseline: dict[str, Any],
    current: dict[str, Any],
    field: str,
    limit: float,
) -> tuple[str, float]:
    base_value = number_at(baseline, field)
    current_value = number_at(current, field)
    if base_value < 0 or current_value < 0:
        fail(f"{field} counters must be non-negative")
    if base_value == 0:
        if current_value < base_value:
            fail(f"{field} regressed below zero baseline")
        return field, 1.0
    ratio = current_value / base_value
    if ratio < limit:
        fail(f"{field} ratio {ratio:.3f} below limit {limit:.3f}")
    return field, ratio


def compare_replacement_max(
    baseline: dict[str, Any],
    current: dict[str, Any],
    field: str,
    limit: float,
) -> tuple[str, float]:
    path = f"matrixcache_contract_evidence.replacement_soak.{field}"
    ratio = positive_ratio(number_at(current, path), number_at(baseline, path), path)
    if ratio > limit:
        fail(f"{path} regressed {ratio:.3f}x above limit {limit:.3f}x")
    return field, ratio


def main() -> int:
    args = parse_args()
    baseline = load_report(args.baseline)
    current = load_report(args.current)

    for field in TOP_LEVEL_INVARIANTS:
        compare_equal(baseline, current, field)
    for field in WORKLOAD_INVARIANTS:
        compare_equal(baseline, current, f"workload.{field}")

    latency_limits = {
        "put": args.max_put_p99_regression,
        "resident_hot_get": args.max_resident_hot_get_p99_regression,
        "hot_get": args.max_hot_get_p99_regression,
        "cold_ssd_refill_get": args.max_cold_refill_p99_regression,
    }
    qps_limits = {
        "put": args.min_put_qps_ratio,
        "resident_hot_get": args.min_hot_get_qps_ratio,
        "hot_get": args.min_hot_get_qps_ratio,
        "cold_ssd_refill_get": args.min_cold_refill_qps_ratio,
    }

    latency_ratios = [
        compare_latency(baseline, current, field, latency_limits[field])
        for field in TIMING_FIELDS
    ]
    qps_ratios = [
        compare_qps(baseline, current, field, qps_limits[field])
        for field in TIMING_FIELDS
    ]
    counter_ratios = [
        compare_counter(baseline, current, field, args.min_counter_ratio)
        for field in COUNTER_FIELDS
    ]

    refill_failure_growth = int(number_at(current, "refill_failures")) - int(
        number_at(baseline, "refill_failures")
    )
    if refill_failure_growth > args.max_refill_failure_growth:
        fail(
            "refill_failures grew by "
            f"{refill_failure_growth}, above {args.max_refill_failure_growth}"
        )

    replacement_ratios = [
        compare_replacement_max(
            baseline,
            current,
            field,
            args.max_replacement_max_regression,
        )
        for field in REPLACEMENT_SOAK_MAX_FIELDS
    ]

    worst_latency = max(latency_ratios, key=lambda item: item[1])
    weakest_qps = min(qps_ratios, key=lambda item: item[1])
    weakest_counter = min(counter_ratios, key=lambda item: item[1])
    worst_replacement = max(replacement_ratios, key=lambda item: item[1])
    print(
        "OK matrixcache backend comparison: "
        f"backend={current['backend']} iterations={current['iterations']} "
        f"worst_p99={worst_latency[0]}:{worst_latency[1]:.3f}x "
        f"weakest_qps={weakest_qps[0]}:{weakest_qps[1]:.3f}x "
        f"weakest_counter={weakest_counter[0]}:{weakest_counter[1]:.3f}x "
        f"worst_replacement_max={worst_replacement[0]}:{worst_replacement[1]:.3f}x "
        f"refill_failure_growth={refill_failure_growth}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
