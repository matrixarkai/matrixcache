#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 MatrixArkAI
"""Validate the MatrixCache batch-read-cost JSON report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


PRIVATE_TIERED_ROW_FIELDS = [
    "batch_size",
    "mem_get_ns",
    "mem_no_promotion_ns",
    "mem_shared_ns",
    "mem_acquire_ns",
    "pmem_get_ns",
    "pmem_no_promotion_ns",
    "pmem_shared_ns",
    "pmem_acquire_ns",
    "ssd_get_ns",
    "ssd_no_promotion_ns",
    "ssd_shared_ns",
    "ssd_acquire_ns",
]

PUBLIC_SHARDED_ROW_FIELDS = [
    "batch_size",
    "get_batch_ns",
    "sharded_get_ns",
    "sharded_colocated_ns",
    "shared_batch_ns",
    "get_batch_no_promotion_ns",
    "acquire_no_promotion_ns",
    "sharded_acquire_colocated_ns",
    "sharded_acquire_no_promotion_ns",
]

KNOWN_ROW_SHAPES = [
    PRIVATE_TIERED_ROW_FIELDS,
    PUBLIC_SHARDED_ROW_FIELDS,
]


def fail(message: str) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--min-rows", type=int, default=2)
    args = parser.parse_args()

    try:
        report = json.loads(args.report.read_text())
    except FileNotFoundError:
        fail(f"report not found: {args.report}")
    except json.JSONDecodeError as error:
        fail(f"invalid JSON: {error}")

    if report.get("report_version") != "matrixcache_batch_read_cost_v1":
        fail("unexpected report_version")
    rows = report.get("rows")
    if not isinstance(rows, list) or len(rows) < args.min_rows:
        fail(f"expected at least {args.min_rows} rows")
    seen_batch_sizes = set()
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
        batch_size = row["batch_size"]
        if not isinstance(batch_size, int) or batch_size <= 0:
            fail(f"row {index} has invalid batch_size")
        if batch_size in seen_batch_sizes:
            fail(f"duplicate batch_size: {batch_size}")
        seen_batch_sizes.add(batch_size)
        for field in required_fields[1:]:
            value = row[field]
            if not isinstance(value, (int, float)) or value <= 0:
                fail(f"row {index} field {field} must be a positive number")

    print(
        "OK matrixcache batch read report: "
        f"rows={len(rows)} batches={','.join(str(size) for size in sorted(seen_batch_sizes))}"
    )


if __name__ == "__main__":
    main()
