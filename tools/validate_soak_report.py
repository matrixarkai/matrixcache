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
    "interval_samples": list,
    "throughput": dict,
    "latency": dict,
    "latency_budgets": dict,
    "checks": dict,
    "passed": bool,
}

REQUIRED_CHECKS = {
    "bounded_entries",
    "bounded_memory",
    "steady_throughput_ceiling",
    "get_p99_within_budget",
    "put_p99_within_budget",
    "read_through_p99_within_budget",
    "refill_p99_within_budget",
    "writeback_p99_within_budget",
    "eviction_p99_within_budget",
    "compaction_p99_within_budget",
    "hit_rate_within_budget",
    "total_qps_within_budget",
    "read_qps_within_budget",
    "write_qps_within_budget",
}

REQUIRED_THROUGHPUT = {
    "total_qps",
    "read_qps",
    "write_qps",
    "duration_seconds",
}

QPS_RELATIVE_TOLERANCE = 0.01
FLOAT_ABSOLUTE_TOLERANCE = 0.01

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

REQUIRED_LATENCY_BUDGETS = {
    "get": ("get_p99_us", "max_get_p99_us", "get_p99_within_budget"),
    "put": ("put_p99_us", "max_put_p99_us", "put_p99_within_budget"),
    "read_through": (
        "read_through_p99_us",
        "max_read_through_p99_us",
        "read_through_p99_within_budget",
    ),
    "refill": ("refill_p99_us", "max_refill_p99_us", "refill_p99_within_budget"),
    "writeback": (
        "writeback_p99_us",
        "max_writeback_p99_us",
        "writeback_p99_within_budget",
    ),
    "eviction": (
        "eviction_p99_us",
        "max_eviction_p99_us",
        "eviction_p99_within_budget",
    ),
    "compaction": (
        "compaction_p99_us",
        "max_compaction_p99_us",
        "compaction_p99_within_budget",
    ),
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
    parser.add_argument("--max-read-through-p99-us", type=int)
    parser.add_argument("--max-refill-p99-us", type=int)
    parser.add_argument("--max-writeback-p99-us", type=int)
    parser.add_argument("--max-eviction-p99-us", type=int)
    parser.add_argument("--max-compaction-p99-us", type=int)
    parser.add_argument("--min-reads", type=int, default=1)
    parser.add_argument("--min-writes", type=int, default=0)
    parser.add_argument("--min-total-qps", type=float)
    parser.add_argument("--min-read-qps", type=float)
    parser.add_argument("--min-write-qps", type=float)
    parser.add_argument("--min-memory-evictions", type=int, default=0)
    parser.add_argument("--min-interval-samples", type=int, default=1)
    parser.add_argument("--max-peak-memory-bytes", type=int)
    parser.add_argument("--max-final-interval-memory-bytes", type=int)
    parser.add_argument("--min-get-samples", type=int, default=1)
    parser.add_argument("--min-put-samples", type=int, default=0)
    parser.add_argument("--min-read-through-samples", type=int, default=0)
    parser.add_argument("--min-refill-samples", type=int, default=0)
    parser.add_argument("--min-writeback-samples", type=int, default=0)
    parser.add_argument("--min-eviction-samples", type=int, default=0)
    parser.add_argument("--min-compaction-samples", type=int, default=0)
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


def require_numeric_at_least(data: dict[str, Any], field: str, minimum: float | None) -> None:
    if minimum is None:
        return
    value = data.get(field)
    if not isinstance(value, (int, float)):
        fail(f"throughput field {field!r} must be numeric")
    if float(value) < minimum:
        fail(f"{field}={float(value):.4f} below minimum {minimum:.4f}")


def require_latency_budget_consistency(data: dict[str, Any]) -> None:
    latency = data["latency"]
    checks = data["checks"]
    budgets = data["latency_budgets"]
    missing = REQUIRED_LATENCY_BUDGETS.keys() - budgets.keys()
    if missing:
        fail(f"missing latency budget paths: {', '.join(sorted(missing))}")
    for path, (latency_field, budget_field, check_field) in REQUIRED_LATENCY_BUDGETS.items():
        budget = budgets.get(path)
        if not isinstance(budget, dict):
            fail(f"latency_budgets.{path} must be an object")
        observed = budget.get("observed_p99_us")
        if not isinstance(observed, int):
            fail(f"latency_budgets.{path}.observed_p99_us must be an integer")
        if observed != latency[latency_field]:
            fail(
                f"latency_budgets.{path}.observed_p99_us={observed} disagrees with "
                f"latency.{latency_field}={latency[latency_field]}"
            )
        configured = budget.get("max_p99_us")
        if configured is not None and not isinstance(configured, int):
            fail(f"latency_budgets.{path}.max_p99_us must be an integer or null")
        if configured != data[budget_field]:
            fail(
                f"latency_budgets.{path}.max_p99_us={configured!r} disagrees with "
                f"{budget_field}={data[budget_field]!r}"
            )
        within_budget = budget.get("within_budget")
        if within_budget is not checks[check_field]:
            fail(
                f"latency_budgets.{path}.within_budget={within_budget!r} disagrees with "
                f"checks.{check_field}={checks[check_field]!r}"
            )


def require_interval_sample_consistency(data: dict[str, Any], args: argparse.Namespace) -> None:
    samples = data["interval_samples"]
    if not samples:
        fail("interval_samples must not be empty")
    if len(samples) < args.min_interval_samples:
        fail(f"interval_samples={len(samples)} below minimum {args.min_interval_samples}")
    last_elapsed = -1
    best_kops = 0.0
    worst_kops = float("inf")
    min_hit_rate = float("inf")
    max_hit_rate = 0.0
    peak_entries = 0
    peak_memory_bytes = 0
    for index, sample in enumerate(samples):
        if not isinstance(sample, dict):
            fail(f"interval_samples[{index}] must be an object")
        for field in ("elapsed_seconds", "entries", "memory_bytes", "cumulative_writes"):
            if not isinstance(sample.get(field), int):
                fail(f"interval_samples[{index}].{field} must be an integer")
        for field in ("kops", "hit_rate_percent"):
            if not isinstance(sample.get(field), (int, float)):
                fail(f"interval_samples[{index}].{field} must be numeric")
        elapsed = sample["elapsed_seconds"]
        if elapsed <= last_elapsed:
            fail("interval_samples elapsed_seconds must be strictly increasing")
        last_elapsed = elapsed
        hit_rate = float(sample["hit_rate_percent"])
        if not 0.0 <= hit_rate <= 100.0:
            fail(f"interval_samples[{index}].hit_rate_percent={hit_rate:.4f} is outside 0..100")
        kops = float(sample["kops"])
        if kops < 0.0:
            fail(f"interval_samples[{index}].kops must be non-negative")
        best_kops = max(best_kops, kops)
        worst_kops = min(worst_kops, kops)
        min_hit_rate = min(min_hit_rate, hit_rate)
        max_hit_rate = max(max_hit_rate, hit_rate)
        peak_entries = max(peak_entries, sample["entries"])
        peak_memory_bytes = max(peak_memory_bytes, sample["memory_bytes"])
    if abs(best_kops - float(data["interval_best_kops"])) > FLOAT_ABSOLUTE_TOLERANCE:
        fail("interval_best_kops disagrees with interval_samples")
    if abs(worst_kops - float(data["interval_worst_kops"])) > FLOAT_ABSOLUTE_TOLERANCE:
        fail("interval_worst_kops disagrees with interval_samples")
    if abs(min_hit_rate - float(data["interval_min_hit_rate_percent"])) > FLOAT_ABSOLUTE_TOLERANCE:
        fail("interval_min_hit_rate_percent disagrees with interval_samples")
    if abs(max_hit_rate - float(data["interval_max_hit_rate_percent"])) > FLOAT_ABSOLUTE_TOLERANCE:
        fail("interval_max_hit_rate_percent disagrees with interval_samples")
    if peak_entries != data["peak_entries"]:
        fail("peak_entries disagrees with interval_samples")
    if peak_memory_bytes != data["peak_memory_bytes"]:
        fail("peak_memory_bytes disagrees with interval_samples")
    if args.max_peak_memory_bytes is not None and peak_memory_bytes > args.max_peak_memory_bytes:
        fail(
            f"peak_memory_bytes={peak_memory_bytes} exceeds "
            f"{args.max_peak_memory_bytes}"
        )
    final_memory_bytes = samples[-1]["memory_bytes"]
    if (
        args.max_final_interval_memory_bytes is not None
        and final_memory_bytes > args.max_final_interval_memory_bytes
    ):
        fail(
            f"final interval memory_bytes={final_memory_bytes} exceeds "
            f"{args.max_final_interval_memory_bytes}"
        )


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

    throughput = data["throughput"]
    missing_throughput = REQUIRED_THROUGHPUT.difference(throughput)
    if missing_throughput:
        fail(f"missing throughput fields: {', '.join(sorted(missing_throughput))}")
    for field in REQUIRED_THROUGHPUT:
        if not isinstance(throughput.get(field), (int, float)):
            fail(f"throughput field {field!r} must be numeric")
    duration_seconds = float(throughput["duration_seconds"])
    if duration_seconds <= 0:
        fail("throughput.duration_seconds must be positive")
    expected_read_qps = data["reads"] / duration_seconds
    expected_write_qps = data["writes"] / duration_seconds
    expected_total_qps = (data["reads"] + data["writes"]) / duration_seconds
    for field, expected in (
        ("read_qps", expected_read_qps),
        ("write_qps", expected_write_qps),
        ("total_qps", expected_total_qps),
    ):
        reported = float(throughput[field])
        if abs(reported - expected) > max(0.01, expected * QPS_RELATIVE_TOLERANCE):
            fail(f"throughput.{field}={reported:.4f} disagrees with report counts")

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
    sample_floors = (
        ("put_count", args.min_put_samples),
        ("read_through_count", args.min_read_through_samples),
        ("refill_count", args.min_refill_samples),
        ("writeback_count", args.min_writeback_samples),
        ("eviction_count", args.min_eviction_samples),
        ("compaction_count", args.min_compaction_samples),
    )
    for field, minimum in sample_floors:
        if latency.get(field, 0) < minimum:
            fail(f"{field}={latency.get(field, 0)} below minimum {minimum}")

    if not args.allow_failed and data["passed"] is not True:
        fail("report passed=false")
    if args.min_hit_rate_percent is not None:
        observed = float(data["observed_hit_rate_percent"])
        if observed < args.min_hit_rate_percent:
            fail(f"observed_hit_rate_percent={observed:.4f} below {args.min_hit_rate_percent}")
    require_numeric_at_least(throughput, "total_qps", args.min_total_qps)
    require_numeric_at_least(throughput, "read_qps", args.min_read_qps)
    require_numeric_at_least(throughput, "write_qps", args.min_write_qps)

    require_numeric_at_most(latency, "get_p99_us", args.max_get_p99_us)
    require_numeric_at_most(latency, "put_p99_us", args.max_put_p99_us)
    require_numeric_at_most(latency, "read_through_p99_us", args.max_read_through_p99_us)
    require_numeric_at_most(latency, "refill_p99_us", args.max_refill_p99_us)
    require_numeric_at_most(latency, "writeback_p99_us", args.max_writeback_p99_us)
    require_numeric_at_most(latency, "eviction_p99_us", args.max_eviction_p99_us)
    require_numeric_at_most(latency, "compaction_p99_us", args.max_compaction_p99_us)
    require_latency_budget_consistency(data)
    require_interval_sample_consistency(data, args)
    return data


def main() -> int:
    args = parse_args()
    data = validate(args)
    print(
        "OK matrixcache soak report: "
        f"reads={data['reads']} writes={data['writes']} "
        f"total_qps={data['throughput']['total_qps']:.2f} "
        f"read_qps={data['throughput']['read_qps']:.2f} "
        f"write_qps={data['throughput']['write_qps']:.2f} "
        f"evictions={data.get('memory_evictions', 0)} "
        f"hit_rate={float(data['observed_hit_rate_percent']):.2f}% "
        f"get_p99={data['latency']['get_p99_us']}us "
        f"put_p99={data['latency']['put_p99_us']}us"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
