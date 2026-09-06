#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 MatrixArkAI
"""Compare MatrixCache read/retrieval scale report manifests."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REPORT_VERSION = "matrixcache_scale_report_manifest_v1"
LOWER_IS_BETTER_SUFFIXES = (
    "_ns",
    "_ns_per_op",
)
HIGHER_IS_BETTER_SUFFIXES = (
    "_mkeys_per_s",
    "_mops_per_s",
)


def fail(message: str) -> None:
    print(f"matrixcache scale report comparison failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"{path} does not exist")
    except json.JSONDecodeError as error:
        fail(f"{path} is not valid JSON: {error}")
    if not isinstance(data, dict):
        fail(f"{path} top-level JSON value must be an object")
    return data


def load_manifest(path: Path) -> dict[str, Any]:
    data = load_json(path)
    if data.get("report_version") != REPORT_VERSION:
        fail(f"{path} has unexpected report_version={data.get('report_version')!r}")
    reports = data.get("reports")
    if not isinstance(reports, list) or not reports:
        fail(f"{path} has no reports")
    return data


def report_index(manifest: dict[str, Any], manifest_path: Path) -> dict[str, Path]:
    indexed = {}
    for item in manifest["reports"]:
        if not isinstance(item, dict):
            fail(f"{manifest_path} contains a non-object report entry")
        name = item.get("name")
        path = item.get("path")
        if not isinstance(name, str) or not name:
            fail(f"{manifest_path} contains a report without a name")
        if not isinstance(path, str) or not path:
            fail(f"{manifest_path} report {name!r} has no path")
        indexed[name] = Path(path)
    return indexed


def row_key(row: dict[str, Any]) -> tuple[str, int]:
    if "batch_size" in row:
        return "batch_size", int(row["batch_size"])
    if "threads" in row:
        return "threads", int(row["threads"])
    fail("row has neither batch_size nor threads")


def compare_lower(
    report_name: str,
    field: str,
    baseline: float,
    current: float,
    limit: float,
) -> tuple[str, float]:
    if baseline <= 0.0 or current <= 0.0:
        fail(f"{report_name}.{field} must be positive")
    ratio = current / baseline
    if ratio > limit:
        fail(
            f"{report_name}.{field} regressed {ratio:.3f}x above "
            f"limit {limit:.3f}x"
        )
    return field, ratio


def compare_higher(
    report_name: str,
    field: str,
    baseline: float,
    current: float,
    limit: float,
) -> tuple[str, float]:
    if baseline <= 0.0 or current <= 0.0:
        fail(f"{report_name}.{field} must be positive")
    ratio = current / baseline
    if ratio < limit:
        fail(
            f"{report_name}.{field} ratio {ratio:.3f} below "
            f"limit {limit:.3f}"
        )
    return field, ratio


def numeric_fields(row: dict[str, Any]) -> list[str]:
    return [
        field
        for field, value in row.items()
        if field not in {"batch_size", "threads"} and isinstance(value, (int, float))
    ]


def compare_rows(
    report_name: str,
    baseline_rows: list[Any],
    current_rows: list[Any],
    max_latency_regression: float,
    min_throughput_ratio: float,
) -> tuple[list[tuple[str, float]], list[tuple[str, float]]]:
    baseline = {}
    for row in baseline_rows:
        if not isinstance(row, dict):
            fail(f"{report_name} baseline contains a non-object row")
        baseline[row_key(row)] = row
    current = {}
    for row in current_rows:
        if not isinstance(row, dict):
            fail(f"{report_name} current contains a non-object row")
        current[row_key(row)] = row
    if set(baseline) != set(current):
        fail(
            f"{report_name} row keys differ: "
            f"baseline={sorted(baseline)} current={sorted(current)}"
        )

    latency_ratios = []
    throughput_ratios = []
    for key in sorted(baseline):
        left = baseline[key]
        right = current[key]
        common_fields = sorted(set(numeric_fields(left)).intersection(numeric_fields(right)))
        for field in common_fields:
            left_value = float(left[field])
            right_value = float(right[field])
            if field.endswith(LOWER_IS_BETTER_SUFFIXES):
                latency_ratios.append(
                    compare_lower(
                        f"{report_name}[{key[0]}={key[1]}]",
                        field,
                        left_value,
                        right_value,
                        max_latency_regression,
                    )
                )
            elif field.endswith(HIGHER_IS_BETTER_SUFFIXES):
                throughput_ratios.append(
                    compare_higher(
                        f"{report_name}[{key[0]}={key[1]}]",
                        field,
                        left_value,
                        right_value,
                        min_throughput_ratio,
                    )
                )
    return latency_ratios, throughput_ratios


def scalar_metric_fields(report: dict[str, Any]) -> list[str]:
    return [
        field
        for field, value in report.items()
        if field not in {"report_version", "checks", "passed"}
        and isinstance(value, (int, float))
        and (field.endswith(LOWER_IS_BETTER_SUFFIXES) or field.endswith(HIGHER_IS_BETTER_SUFFIXES))
    ]


def compare_report(
    name: str,
    baseline_path: Path,
    current_path: Path,
    max_latency_regression: float,
    min_throughput_ratio: float,
) -> tuple[list[tuple[str, float]], list[tuple[str, float]]]:
    baseline = load_json(baseline_path)
    current = load_json(current_path)
    if baseline.get("report_version") != current.get("report_version"):
        fail(
            f"{name} report versions differ: "
            f"{baseline.get('report_version')!r} vs {current.get('report_version')!r}"
        )

    latency_ratios = []
    throughput_ratios = []
    if isinstance(baseline.get("rows"), list) or isinstance(current.get("rows"), list):
        if not isinstance(baseline.get("rows"), list) or not isinstance(current.get("rows"), list):
            fail(f"{name} must have rows in both reports")
        row_latency, row_throughput = compare_rows(
            name,
            baseline["rows"],
            current["rows"],
            max_latency_regression,
            min_throughput_ratio,
        )
        latency_ratios.extend(row_latency)
        throughput_ratios.extend(row_throughput)

    common_scalars = sorted(set(scalar_metric_fields(baseline)).intersection(scalar_metric_fields(current)))
    for field in common_scalars:
        left = float(baseline[field])
        right = float(current[field])
        if field.endswith(LOWER_IS_BETTER_SUFFIXES):
            latency_ratios.append(
                compare_lower(name, field, left, right, max_latency_regression)
            )
        elif field.endswith(HIGHER_IS_BETTER_SUFFIXES):
            throughput_ratios.append(
                compare_higher(name, field, left, right, min_throughput_ratio)
            )
    return latency_ratios, throughput_ratios


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("current", type=Path)
    parser.add_argument("--max-latency-regression", type=float, default=1.50)
    parser.add_argument("--min-throughput-ratio", type=float, default=0.65)
    args = parser.parse_args()

    baseline_manifest = load_manifest(args.baseline)
    current_manifest = load_manifest(args.current)
    baseline_reports = report_index(baseline_manifest, args.baseline)
    current_reports = report_index(current_manifest, args.current)
    if set(baseline_reports) != set(current_reports):
        fail(
            "manifest report names differ: "
            f"baseline={sorted(baseline_reports)} current={sorted(current_reports)}"
        )

    latency_ratios = []
    throughput_ratios = []
    for name in sorted(baseline_reports):
        report_latency, report_throughput = compare_report(
            name,
            baseline_reports[name],
            current_reports[name],
            args.max_latency_regression,
            args.min_throughput_ratio,
        )
        latency_ratios.extend(report_latency)
        throughput_ratios.extend(report_throughput)

    worst_latency = max((ratio for _, ratio in latency_ratios), default=1.0)
    worst_throughput = min((ratio for _, ratio in throughput_ratios), default=1.0)
    print(
        "OK matrixcache scale report comparison: "
        f"reports={len(baseline_reports)} "
        f"latency_fields={len(latency_ratios)} "
        f"throughput_fields={len(throughput_ratios)} "
        f"worst_latency={worst_latency:.3f}x "
        f"worst_throughput={worst_throughput:.3f}x"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
