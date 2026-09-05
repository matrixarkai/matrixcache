#!/usr/bin/env python3
"""Compare two MatrixCache eviction JSON reports for regressions."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


SUMMARY_MAX_RATIO_FIELDS = (
    "max_ns_per_write",
    "max_groups_per_eviction",
)
SUMMARY_MIN_RATIO_FIELDS = (
    "min_hit_rate_percent",
)
STEADY_MAX_RATIO_FIELDS = (
    "ns_per_write",
    "groups_per_eviction",
)
HIT_MIN_RATIO_FIELDS = (
    "hit_rate_percent",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="Known-good eviction report")
    parser.add_argument("current", type=Path, help="Report from the run being checked")
    parser.add_argument(
        "--max-latency-regression",
        type=float,
        default=1.35,
        help="Maximum allowed current/baseline ratio for ns-per-write fields",
    )
    parser.add_argument(
        "--max-candidate-regression",
        type=float,
        default=1.10,
        help="Maximum allowed current/baseline ratio for candidate-group fields",
    )
    parser.add_argument(
        "--min-hit-rate-ratio",
        type=float,
        default=0.95,
        help="Minimum allowed current/baseline ratio for hit-rate fields",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache eviction comparison failed: {message}", file=sys.stderr)
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
    if data.get("report_version") != "matrixcache_eviction_v1":
        fail(f"{path} is not a matrixcache_eviction_v1 report")
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


def rows_by_entries(data: dict[str, Any], field: str) -> dict[int, dict[str, Any]]:
    rows = data.get(field)
    if not isinstance(rows, list):
        fail(f"missing {field} list")
    out: dict[int, dict[str, Any]] = {}
    for row in rows:
        if not isinstance(row, dict):
            fail(f"{field} rows must be objects")
        entries = row.get("entries")
        if not isinstance(entries, int) or entries <= 0:
            fail(f"{field} row has invalid entry count")
        if entries in out:
            fail(f"duplicate {field} entry count {entries}")
        out[entries] = row
    return out


def require_same_shape(baseline: dict[str, Any], current: dict[str, Any]) -> None:
    for field in ("value_bytes", "write_pressure_writes", "read_pressure_reads"):
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


def compare_max(name: str, current: float, baseline: float, limit: float, observed: list[tuple[str, float]]) -> None:
    field_ratio = ratio(current, baseline)
    if field_ratio > limit:
        fail(f"{name} ratio {field_ratio:.4f} exceeds {limit:.4f}")
    observed.append((name, field_ratio))


def compare_min(name: str, current: float, baseline: float, limit: float, observed: list[tuple[str, float]]) -> None:
    field_ratio = ratio(current, baseline)
    if field_ratio < limit:
        fail(f"{name} ratio {field_ratio:.4f} below {limit:.4f}")
    observed.append((name, field_ratio))


def main() -> int:
    args = parse_args()
    baseline = load(args.baseline)
    current = load(args.current)
    require_same_shape(baseline, current)

    base_steady = rows_by_entries(baseline, "steady_state")
    current_steady = rows_by_entries(current, "steady_state")
    if set(base_steady) != set(current_steady):
        fail(f"steady_state entry sets differ: baseline={sorted(base_steady)} current={sorted(current_steady)}")
    base_hits = rows_by_entries(baseline, "hit_rates")
    current_hits = rows_by_entries(current, "hit_rates")
    if set(base_hits) != set(current_hits):
        fail(f"hit_rates entry sets differ: baseline={sorted(base_hits)} current={sorted(current_hits)}")

    max_ratios: list[tuple[str, float]] = []
    min_ratios: list[tuple[str, float]] = []
    base_summary = summary(baseline)
    current_summary = summary(current)
    compare_max(
        "summary.max_ns_per_write",
        number(current_summary, "max_ns_per_write"),
        number(base_summary, "max_ns_per_write"),
        args.max_latency_regression,
        max_ratios,
    )
    compare_max(
        "summary.max_groups_per_eviction",
        number(current_summary, "max_groups_per_eviction"),
        number(base_summary, "max_groups_per_eviction"),
        args.max_candidate_regression,
        max_ratios,
    )
    compare_min(
        "summary.min_hit_rate_percent",
        number(current_summary, "min_hit_rate_percent"),
        number(base_summary, "min_hit_rate_percent"),
        args.min_hit_rate_ratio,
        min_ratios,
    )

    for entries in sorted(base_steady):
        base_row = base_steady[entries]
        current_row = current_steady[entries]
        compare_max(
            f"steady_state.{entries}.ns_per_write",
            number(current_row, "ns_per_write"),
            number(base_row, "ns_per_write"),
            args.max_latency_regression,
            max_ratios,
        )
        compare_max(
            f"steady_state.{entries}.groups_per_eviction",
            number(current_row, "groups_per_eviction"),
            number(base_row, "groups_per_eviction"),
            args.max_candidate_regression,
            max_ratios,
        )

    for entries in sorted(base_hits):
        compare_min(
            f"hit_rates.{entries}.hit_rate_percent",
            number(current_hits[entries], "hit_rate_percent"),
            number(base_hits[entries], "hit_rate_percent"),
            args.min_hit_rate_ratio,
            min_ratios,
        )

    worst = max(max_ratios, key=lambda item: item[1])
    weakest = min(min_ratios, key=lambda item: item[1])
    print(
        "OK matrixcache eviction comparison: "
        f"worst={worst[0]}:{worst[1]:.3f} "
        f"weakest_hit={weakest[0]}:{weakest[1]:.3f} "
        f"max_ns={number(current_summary, 'max_ns_per_write'):.1f} "
        f"groups={number(current_summary, 'max_groups_per_eviction'):.1f} "
        f"min_hit={number(current_summary, 'min_hit_rate_percent'):.2f}%"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
