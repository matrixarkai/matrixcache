#!/usr/bin/env python3
"""Emit a compact MatrixCache backend benchmark operator log line."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_OPERATOR_FIELDS = (
    "passed",
    "put_qps",
    "resident_hot_get_qps",
    "hot_get_qps",
    "cold_refill_qps",
    "put_avg_us",
    "hot_get_avg_us",
    "cold_refill_avg_us",
    "put_p95_us",
    "hot_get_p95_us",
    "cold_refill_p95_us",
    "put_p99_us",
    "hot_get_p99_us",
    "cold_refill_p99_us",
    "memory_evictions",
    "pmem_evictions",
    "ssd_evictions",
    "disk_fills",
    "cold_ssd_refills",
    "async_writeback_backpressure",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="Path to matrixcache_rocksdb_backend_v1 JSON")
    parser.add_argument(
        "--prefix",
        default="matrixcache_backend_operator",
        help="Log line prefix",
    )
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache backend operator log invalid: {message}", file=sys.stderr)
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


def logfmt_value(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return f"{value:.2f}"
    text = str(value)
    if text and all(ch.isalnum() or ch in "._:-/" for ch in text):
        return text
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main() -> int:
    args = parse_args()
    data = load_report(args.report)
    if data.get("report_version") != "matrixcache_rocksdb_backend_v1":
        fail(f"unexpected report_version {data.get('report_version')!r}")
    operator_log = data.get("operator_log")
    if not isinstance(operator_log, dict):
        fail("missing operator_log object")
    missing = [field for field in REQUIRED_OPERATOR_FIELDS if field not in operator_log]
    if missing:
        fail("operator_log missing fields: " + ", ".join(missing))

    fields: list[tuple[str, Any]] = [
        ("report_version", data["report_version"]),
        ("backend", data.get("backend")),
        ("iterations", data.get("iterations")),
        ("replacement_soak_iterations", data.get("replacement_soak_iterations")),
    ]
    fields.extend((field, operator_log[field]) for field in REQUIRED_OPERATOR_FIELDS)
    print(args.prefix + " " + " ".join(f"{key}={logfmt_value(value)}" for key, value in fields))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
