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
    throughput_ratio = ratio(
        number(current, "interval_best_kops"),
        number(baseline, "interval_best_kops"),
    )
    hit_rate_delta = number(current, "observed_hit_rate_percent") - number(
        baseline, "observed_hit_rate_percent"
    )

    check_ratio("get p99", get_p99_ratio, args.max_get_p99_regression, "max")
    check_ratio("put p99", put_p99_ratio, args.max_put_p99_regression, "max")
    check_ratio("peak memory", memory_ratio, args.max_memory_growth, "max")
    check_ratio("best throughput", throughput_ratio, args.min_throughput_ratio, "min")
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
        f"hit_rate_delta={hit_rate_delta:.2f}pp"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
