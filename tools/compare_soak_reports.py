#!/usr/bin/env python3
"""Compare two MatrixCache soak JSON reports for scale regressions."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="Known-good matrixcache_soak_v1 report")
    parser.add_argument("current", type=Path, help="Report from the run being checked")
    parser.add_argument(
        "--max-get-p99-regression",
        type=float,
        default=1.25,
        help="Maximum allowed current/baseline get p99 ratio",
    )
    parser.add_argument(
        "--max-put-p99-regression",
        type=float,
        default=1.25,
        help="Maximum allowed current/baseline put p99 ratio",
    )
    parser.add_argument(
        "--max-operation-max-regression",
        type=float,
        default=2.0,
        help=(
            "Maximum allowed current/baseline ratio for read-through/refill/"
            "writeback/eviction/compaction max latency fields"
        ),
    )
    parser.add_argument(
        "--max-operation-p99-regression",
        type=float,
        default=1.50,
        help=(
            "Maximum allowed current/baseline ratio for read-through/refill/"
            "writeback/eviction/compaction p99 latency fields"
        ),
    )
    parser.add_argument(
        "--max-memory-growth",
        type=float,
        default=1.10,
        help="Maximum allowed current/baseline peak memory ratio",
    )
    parser.add_argument(
        "--min-throughput-ratio",
        type=float,
        default=0.90,
        help="Minimum allowed current/baseline best Kops/s ratio",
    )
    parser.add_argument(
        "--min-hit-rate-delta",
        type=float,
        default=-2.0,
        help="Minimum allowed current minus baseline hit-rate percentage points",
    )
    parser.add_argument(
        "--min-sample-ratio",
        type=float,
        default=0.90,
        help=(
            "Minimum allowed current/baseline ratio for latency sample counts. "
            "This prevents a short or partial run from passing only because it "
            "skipped read-through/refill/writeback/eviction/compaction work."
        ),
    )
    parser.add_argument(
        "--min-interval-sample-ratio",
        type=float,
        default=0.90,
        help="Minimum allowed current/baseline ratio for archived interval samples",
    )
    parser.add_argument(
        "--min-worst-interval-throughput-ratio",
        type=float,
        default=0.75,
        help="Minimum allowed current/baseline ratio for the worst sampled interval Kops/s",
    )
    parser.add_argument(
        "--max-final-interval-memory-growth",
        type=float,
        default=1.10,
        help="Maximum allowed current/baseline final interval resident-memory ratio",
    )
    parser.add_argument(
        "--min-efficiency-ratio",
        type=float,
        default=0.90,
        help="Minimum allowed current/baseline throughput-per-resident-memory ratio",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache soak comparison failed: {message}", file=sys.stderr)
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
    if data.get("report_version") != "matrixcache_soak_v1":
        fail(f"{path} is not a matrixcache_soak_v1 report")
    if data.get("passed") is not True:
        fail(f"{path} is not a passing report")
    return data


def number(data: dict[str, Any], field: str) -> float:
    value = data.get(field)
    if not isinstance(value, (int, float)):
        fail(f"field {field!r} must be numeric")
    return float(value)


def latency(data: dict[str, Any], field: str) -> float:
    latencies = data.get("latency")
    if not isinstance(latencies, dict):
        fail("missing latency object")
    value = latencies.get(field)
    if not isinstance(value, (int, float)):
        fail(f"latency field {field!r} must be numeric")
    return float(value)


def interval_samples(data: dict[str, Any]) -> list[dict[str, Any]]:
    samples = data.get("interval_samples")
    if not isinstance(samples, list) or not samples:
        fail("missing non-empty interval_samples")
    for index, sample in enumerate(samples):
        if not isinstance(sample, dict):
            fail(f"interval_samples[{index}] must be an object")
    return samples


def sample_number(sample: dict[str, Any], field: str) -> float:
    value = sample.get(field)
    if not isinstance(value, (int, float)):
        fail(f"interval sample field {field!r} must be numeric")
    return float(value)


def worst_interval_kops(data: dict[str, Any]) -> float:
    return min(sample_number(sample, "kops") for sample in interval_samples(data))


def final_interval_memory_bytes(data: dict[str, Any]) -> float:
    return sample_number(interval_samples(data)[-1], "memory_bytes")


def memory_pressure(data: dict[str, Any], field: str) -> float:
    pressure = data.get("memory_pressure")
    if not isinstance(pressure, dict):
        fail("missing memory_pressure object")
    value = pressure.get(field)
    if not isinstance(value, (int, float)):
        fail(f"memory_pressure field {field!r} must be numeric")
    return float(value)


def efficiency(data: dict[str, Any], field: str) -> float:
    values = data.get("efficiency")
    if not isinstance(values, dict):
        fail("missing efficiency object")
    value = values.get(field)
    if not isinstance(value, (int, float)):
        fail(f"efficiency field {field!r} must be numeric")
    return float(value)


def ratio(current: float, baseline: float) -> float:
    if baseline <= 0:
        return 1.0 if current <= 0 else float("inf")
    return current / baseline


def check_ratio(name: str, value: float, limit: float, direction: str) -> None:
    if direction == "max" and value > limit:
        fail(f"{name} ratio {value:.4f} exceeds {limit:.4f}")
    if direction == "min" and value < limit:
        fail(f"{name} ratio {value:.4f} below {limit:.4f}")


def main() -> int:
    args = parse_args()
    baseline = load(args.baseline)
    current = load(args.current)

    get_p99_ratio = ratio(latency(current, "get_p99_us"), latency(baseline, "get_p99_us"))
    put_p99_ratio = ratio(latency(current, "put_p99_us"), latency(baseline, "put_p99_us"))
    memory_ratio = ratio(number(current, "peak_memory_bytes"), number(baseline, "peak_memory_bytes"))
    memory_pressure_peak_ratio = ratio(
        memory_pressure(current, "peak_utilization_percent"),
        memory_pressure(baseline, "peak_utilization_percent"),
    )
    memory_pressure_final_ratio = ratio(
        memory_pressure(current, "final_utilization_percent"),
        memory_pressure(baseline, "final_utilization_percent"),
    )
    throughput_ratio = ratio(
        number(current, "interval_best_kops"),
        number(baseline, "interval_best_kops"),
    )
    hit_rate_delta = number(current, "observed_hit_rate_percent") - number(
        baseline, "observed_hit_rate_percent"
    )
    operation_max_ratios = {
        field: ratio(latency(current, field), latency(baseline, field))
        for field in (
            "read_through_max_us",
            "refill_max_us",
            "writeback_max_us",
            "eviction_max_us",
            "compaction_max_us",
        )
    }
    operation_p99_ratios = {
        field: ratio(latency(current, field), latency(baseline, field))
        for field in (
            "read_through_p99_us",
            "refill_p99_us",
            "writeback_p99_us",
            "eviction_p99_us",
            "compaction_p99_us",
        )
    }
    sample_ratios = {
        field: ratio(latency(current, field), latency(baseline, field))
        for field in (
            "get_count",
            "put_count",
            "read_through_count",
            "refill_count",
            "writeback_count",
            "eviction_count",
            "compaction_count",
        )
    }
    interval_sample_ratio = ratio(
        float(len(interval_samples(current))),
        float(len(interval_samples(baseline))),
    )
    worst_interval_throughput_ratio = ratio(
        worst_interval_kops(current),
        worst_interval_kops(baseline),
    )
    final_interval_memory_ratio = ratio(
        final_interval_memory_bytes(current),
        final_interval_memory_bytes(baseline),
    )
    total_ops_efficiency_ratio = ratio(
        efficiency(current, "total_ops_per_peak_mib"),
        efficiency(baseline, "total_ops_per_peak_mib"),
    )
    best_kops_efficiency_ratio = ratio(
        efficiency(current, "best_kops_per_peak_mib"),
        efficiency(baseline, "best_kops_per_peak_mib"),
    )

    check_ratio("get p99", get_p99_ratio, args.max_get_p99_regression, "max")
    check_ratio("put p99", put_p99_ratio, args.max_put_p99_regression, "max")
    for field, field_ratio in operation_max_ratios.items():
        check_ratio(
            field.replace("_", " "),
            field_ratio,
            args.max_operation_max_regression,
            "max",
        )
    for field, field_ratio in operation_p99_ratios.items():
        check_ratio(
            field.replace("_", " "),
            field_ratio,
            args.max_operation_p99_regression,
            "max",
        )
    for field, field_ratio in sample_ratios.items():
        check_ratio(
            field.replace("_", " "),
            field_ratio,
            args.min_sample_ratio,
            "min",
        )
    check_ratio("peak memory", memory_ratio, args.max_memory_growth, "max")
    check_ratio(
        "peak memory utilization",
        memory_pressure_peak_ratio,
        args.max_memory_growth,
        "max",
    )
    check_ratio(
        "final memory utilization",
        memory_pressure_final_ratio,
        args.max_final_interval_memory_growth,
        "max",
    )
    check_ratio("best throughput", throughput_ratio, args.min_throughput_ratio, "min")
    check_ratio(
        "interval sample count",
        interval_sample_ratio,
        args.min_interval_sample_ratio,
        "min",
    )
    check_ratio(
        "worst interval throughput",
        worst_interval_throughput_ratio,
        args.min_worst_interval_throughput_ratio,
        "min",
    )
    check_ratio(
        "final interval memory",
        final_interval_memory_ratio,
        args.max_final_interval_memory_growth,
        "max",
    )
    check_ratio(
        "total ops per peak MiB",
        total_ops_efficiency_ratio,
        args.min_efficiency_ratio,
        "min",
    )
    check_ratio(
        "best Kops per peak MiB",
        best_kops_efficiency_ratio,
        args.min_efficiency_ratio,
        "min",
    )
    if hit_rate_delta < args.min_hit_rate_delta:
        fail(
            f"hit-rate delta {hit_rate_delta:.4f}pp below "
            f"{args.min_hit_rate_delta:.4f}pp"
        )

    print(
        "OK matrixcache soak comparison: "
        f"get_p99_ratio={get_p99_ratio:.3f} "
        f"put_p99_ratio={put_p99_ratio:.3f} "
        f"throughput_ratio={throughput_ratio:.3f} "
        f"memory_ratio={memory_ratio:.3f} "
        f"memory_pressure_peak_ratio={memory_pressure_peak_ratio:.3f} "
        f"memory_pressure_final_ratio={memory_pressure_final_ratio:.3f} "
        f"total_ops_efficiency_ratio={total_ops_efficiency_ratio:.3f} "
        f"best_kops_efficiency_ratio={best_kops_efficiency_ratio:.3f} "
        f"interval_sample_ratio={interval_sample_ratio:.3f} "
        f"worst_interval_throughput_ratio={worst_interval_throughput_ratio:.3f} "
        f"final_interval_memory_ratio={final_interval_memory_ratio:.3f} "
        f"hit_rate_delta={hit_rate_delta:.2f}pp "
        "operation_p99_ratios="
        + ",".join(f"{name}={value:.3f}" for name, value in operation_p99_ratios.items())
        + " "
        "sample_ratios="
        + ",".join(f"{name}={value:.3f}" for name, value in sample_ratios.items())
        + " "
        "operation_max_ratios="
        + ",".join(f"{name}={value:.3f}" for name, value in operation_max_ratios.items())
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
