#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 MatrixArkAI
"""Validate the MatrixCache batch-concurrency JSON report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


PRIVATE_ROW_FIELDS = [
    "threads",
    "get_refresh_distance_zero_mkeys_per_s",
    "get_no_move_mkeys_per_s",
    "no_promotion_get_mkeys_per_s",
    "no_promotion_acquire_mkeys_per_s",
]

PUBLIC_ROW_FIELDS = [
    "threads",
    "get_refresh_distance_zero_mkeys_per_s",
    "get_no_move_mkeys_per_s",
]

KNOWN_ROW_SHAPES = [PRIVATE_ROW_FIELDS, PUBLIC_ROW_FIELDS]


def fail(message: str) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--min-rows", type=int, default=4)
    parser.add_argument("--min-throughput-mkeys", type=float, default=0.0)
    args = parser.parse_args()

    try:
        report = json.loads(args.report.read_text())
    except FileNotFoundError:
        fail(f"report not found: {args.report}")
    except json.JSONDecodeError as error:
        fail(f"invalid JSON: {error}")

    if report.get("report_version") != "matrixcache_batch_concurrency_v1":
        fail("unexpected report_version")
    for field in ["resident_values", "value_bytes", "batch_size", "batches_per_thread", "passes"]:
        value = report.get(field)
        if not isinstance(value, int) or value <= 0:
            fail(f"{field} must be a positive integer")

    rows = report.get("rows")
    if not isinstance(rows, list) or len(rows) < args.min_rows:
        fail(f"expected at least {args.min_rows} rows")
    seen_threads = set()
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            fail(f"row {index} is not an object")
        required_fields = next(
            (
                fields
                for fields in KNOWN_ROW_SHAPES
                if all(field in row for field in fields)
            ),
            None,
        )
        if required_fields is None:
            expected = " or ".join("/".join(fields[1:3]) for fields in KNOWN_ROW_SHAPES)
            fail(f"row {index} does not match a known report shape: {expected}")
        threads = row["threads"]
        if not isinstance(threads, int) or threads <= 0:
            fail(f"row {index} has invalid threads")
        if threads in seen_threads:
            fail(f"duplicate thread count: {threads}")
        seen_threads.add(threads)
        for field in required_fields[1:]:
            value = row[field]
            if not isinstance(value, (int, float)) or value <= args.min_throughput_mkeys:
                fail(
                    f"row {index} field {field} must be greater than "
                    f"{args.min_throughput_mkeys}"
                )

    print(
        "OK matrixcache batch concurrency report: "
        f"rows={len(rows)} threads={','.join(str(size) for size in sorted(seen_threads))}"
    )


if __name__ == "__main__":
    main()
