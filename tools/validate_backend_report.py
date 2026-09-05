#!/usr/bin/env python3
"""Validate a MatrixCache RocksDB/backend benchmark JSON report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_TOP_LEVEL = {
    "report_version": str,
    "backend": str,
    "iterations": int,
    "workload": dict,
    "put": dict,
    "resident_hot_get": dict,
    "hot_get": dict,
    "cold_ssd_refill_get": dict,
    "resident_hot_key_count": int,
    "cold_ssd_refills": int,
    "memory_hits": int,
    "pmem_hits": int,
    "ssd_hits": int,
    "memory_evictions": int,
    "pmem_evictions": int,
    "ssd_evictions": int,
    "refill_failures": int,
    "disk_fills": int,
    "pmem_fills": int,
    "main_pressure_passed": bool,
    "replacement_soak_iterations": int,
    "replacement_soak_passed": bool,
    "async_writeback_backpressure": int,
    "restart_disk_refill_ready": bool,
    "matrixcache_contract": dict,
    "matrixcache_contract_evidence": dict,
}

REQUIRED_TIMING = {
    "count",
    "total_ms",
    "total_us",
    "qps",
    "p50_us",
    "p95_us",
    "p99_us",
    "p50_ns",
    "p95_ns",
    "p99_ns",
}

REQUIRED_CONTRACT = {
    "dram_to_pmem_eviction",
    "pmem_to_ssd_eviction",
    "ssd_read_through_refill",
    "replacement_soak",
    "async_writeback_backpressure",
    "restart_disk_refill",
    "passed",
}

REQUIRED_REPLACEMENT_SOAK_EVIDENCE = {
    "read_through_latency_max_micros",
    "refill_latency_max_micros",
    "writeback_latency_max_micros",
    "eviction_latency_max_micros",
    "compaction_latency_max_micros",
}

REQUIRED_WORKLOAD = {
    "value_bytes",
    "dram_capacity_bytes",
    "pmem_capacity_bytes",
    "ssd_capacity_bytes",
    "placement_threshold_bytes",
    "replacement_soak_iterations",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="Path to matrixcache_rocksdb_backend_v1 JSON")
    parser.add_argument(
        "--allow-failed",
        action="store_true",
        help="Validate shape but do not require matrixcache_contract.passed=true",
    )
    parser.add_argument(
        "--expect-backend",
        choices=("rocksdb", "file-compat"),
        help="Require a specific backend value",
    )
    parser.add_argument("--min-iterations", type=int)
    parser.add_argument("--min-replacement-soak-iterations", type=int)
    parser.add_argument("--min-cold-ssd-refills", type=int)
    parser.add_argument("--min-memory-evictions", type=int)
    parser.add_argument("--min-pmem-evictions", type=int)
    parser.add_argument("--min-disk-fills", type=int)
    parser.add_argument("--min-async-writeback-backpressure", type=int)
    parser.add_argument("--max-refill-failures", type=int)
    parser.add_argument("--max-hot-get-p99-us", type=int)
    parser.add_argument("--max-cold-refill-p99-us", type=int)
    return parser.parse_args()


def fail(message: str) -> None:
    print(f"matrixcache backend report invalid: {message}", file=sys.stderr)
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


def require_numeric_field(data: dict[str, Any], field: str) -> float:
    value = data.get(field)
    if not isinstance(value, (int, float)):
        fail(f"field {field!r} must be numeric")
    return float(value)


def require_int_field(data: dict[str, Any], field: str) -> int:
    value = data.get(field)
    if not isinstance(value, int):
        fail(f"field {field!r} must be an integer")
    return value


def validate_timing(data: dict[str, Any], field: str, expected_count: int) -> None:
    timing = data.get(field)
    if not isinstance(timing, dict):
        fail(f"{field!r} must be an object")
    missing = REQUIRED_TIMING.difference(timing)
    if missing:
        fail(f"{field!r} missing timing fields: {', '.join(sorted(missing))}")
    count = require_numeric_field(timing, "count")
    if count <= 0:
        fail(f"{field!r}.count must be positive")
    if int(count) != expected_count:
        fail(f"{field!r}.count={int(count)} does not match expected {expected_count}")
    if require_numeric_field(timing, "qps") <= 0:
        fail(f"{field!r}.qps must be positive")
    if require_numeric_field(timing, "total_us") <= 0:
        fail(f"{field!r}.total_us must be positive")
    if require_numeric_field(timing, "p99_us") < require_numeric_field(timing, "p95_us"):
        fail(f"{field!r}.p99_us must be >= p95_us")
    if require_numeric_field(timing, "p95_us") < require_numeric_field(timing, "p50_us"):
        fail(f"{field!r}.p95_us must be >= p50_us")


def validate_contract(data: dict[str, Any], allow_failed: bool) -> None:
    contract = data["matrixcache_contract"]
    missing = REQUIRED_CONTRACT.difference(contract)
    if missing:
        fail(f"matrixcache_contract missing fields: {', '.join(sorted(missing))}")
    false_fields = [
        name
        for name in sorted(REQUIRED_CONTRACT - {"passed"})
        if contract.get(name) is not True
    ]
    if contract.get("passed") is not True:
        false_fields.append("passed")
    if false_fields and not allow_failed:
        fail(f"failing contract fields: {', '.join(false_fields)}")

    evidence = data["matrixcache_contract_evidence"]
    for name in sorted(REQUIRED_CONTRACT - {"passed"}):
        item = evidence.get(name)
        if not isinstance(item, dict):
            fail(f"missing evidence object for {name!r}")
        if item.get("observed") is not contract.get(name):
            fail(f"evidence observed value disagrees with contract field {name!r}")
        if "source" not in item or "metric" not in item:
            fail(f"evidence object for {name!r} must include source and metric")
        if name == "replacement_soak":
            missing = REQUIRED_REPLACEMENT_SOAK_EVIDENCE.difference(item)
            if missing:
                fail(
                    "replacement_soak evidence missing fields: "
                    + ", ".join(sorted(missing))
                )
            for field in REQUIRED_REPLACEMENT_SOAK_EVIDENCE:
                value = item[field]
                if not isinstance(value, int):
                    fail(f"replacement_soak.{field} must be an integer")
                if value <= 0:
                    fail(f"replacement_soak.{field} must be positive")
            if item.get("iterations") != data["replacement_soak_iterations"]:
                fail(
                    "replacement_soak evidence iterations="
                    f"{item.get('iterations')!r} does not match "
                    f"replacement_soak_iterations={data['replacement_soak_iterations']}"
                )
        elif name == "dram_to_pmem_eviction":
            require_evidence_counter(item, "memory_evictions", data["memory_evictions"])
            require_evidence_counter(item, "pmem_fills", data["pmem_fills"])
        elif name == "pmem_to_ssd_eviction":
            require_evidence_counter(item, "pmem_evictions", data["pmem_evictions"])
            require_evidence_counter(item, "disk_fills", data["disk_fills"])
        elif name == "ssd_read_through_refill":
            require_evidence_counter(item, "cold_ssd_refills", data["cold_ssd_refills"])
            require_evidence_counter(item, "refill_failures", data["refill_failures"])
        elif name == "async_writeback_backpressure":
            require_evidence_counter(
                item,
                "observed_async_writeback_backpressure",
                data["async_writeback_backpressure"],
            )
        elif name == "restart_disk_refill":
            observed = item.get("restart_disk_refill_ready")
            if observed is not data["restart_disk_refill_ready"]:
                fail(
                    "restart_disk_refill evidence="
                    f"{observed!r} does not match restart_disk_refill_ready="
                    f"{data['restart_disk_refill_ready']!r}"
                )


def require_evidence_counter(item: dict[str, Any], field: str, expected: int) -> None:
    value = item.get(field)
    if value != expected:
        fail(f"evidence {field}={value!r} does not match top-level {expected}")


def validate(args: argparse.Namespace) -> dict[str, Any]:
    data = load_report(args.report)
    for field, expected in REQUIRED_TOP_LEVEL.items():
        require_type(data, field, expected)

    if data["report_version"] != "matrixcache_rocksdb_backend_v1":
        fail(f"unexpected report_version {data['report_version']!r}")
    if data["backend"] not in {"rocksdb", "file-compat"}:
        fail(f"unexpected backend {data['backend']!r}")
    if args.expect_backend and data["backend"] != args.expect_backend:
        fail(f"backend {data['backend']!r} does not match expected {args.expect_backend!r}")
    if data["iterations"] <= 0:
        fail("iterations must be positive")
    workload = data["workload"]
    missing_workload = REQUIRED_WORKLOAD.difference(workload)
    if missing_workload:
        fail(f"workload missing fields: {', '.join(sorted(missing_workload))}")
    for field in REQUIRED_WORKLOAD:
        if not isinstance(workload.get(field), int):
            fail(f"workload.{field} must be an integer")
        if workload[field] <= 0:
            fail(f"workload.{field} must be positive")
    if workload["replacement_soak_iterations"] != data["replacement_soak_iterations"]:
        fail(
            "workload.replacement_soak_iterations="
            f"{workload['replacement_soak_iterations']} does not match "
            f"replacement_soak_iterations={data['replacement_soak_iterations']}"
        )
    if args.min_iterations is not None and data["iterations"] < args.min_iterations:
        fail(f"iterations={data['iterations']} below {args.min_iterations}")
    if (
        args.min_replacement_soak_iterations is not None
        and data["replacement_soak_iterations"] < args.min_replacement_soak_iterations
    ):
        fail(
            "replacement_soak_iterations="
            f"{data['replacement_soak_iterations']} below "
            f"{args.min_replacement_soak_iterations}"
        )
    if args.min_cold_ssd_refills is not None and data["cold_ssd_refills"] < args.min_cold_ssd_refills:
        fail(f"cold_ssd_refills={data['cold_ssd_refills']} below {args.min_cold_ssd_refills}")
    if args.min_memory_evictions is not None and data["memory_evictions"] < args.min_memory_evictions:
        fail(f"memory_evictions={data['memory_evictions']} below {args.min_memory_evictions}")
    if args.min_pmem_evictions is not None and data["pmem_evictions"] < args.min_pmem_evictions:
        fail(f"pmem_evictions={data['pmem_evictions']} below {args.min_pmem_evictions}")
    if args.min_disk_fills is not None and data["disk_fills"] < args.min_disk_fills:
        fail(f"disk_fills={data['disk_fills']} below {args.min_disk_fills}")
    if (
        args.min_async_writeback_backpressure is not None
        and data["async_writeback_backpressure"] < args.min_async_writeback_backpressure
    ):
        fail(
            "async_writeback_backpressure="
            f"{data['async_writeback_backpressure']} below "
            f"{args.min_async_writeback_backpressure}"
        )
    if args.max_refill_failures is not None and data["refill_failures"] > args.max_refill_failures:
        fail(f"refill_failures={data['refill_failures']} exceeds {args.max_refill_failures}")

    validate_timing(data, "put", data["iterations"])
    validate_timing(data, "hot_get", data["iterations"])
    validate_timing(data, "cold_ssd_refill_get", data["iterations"])
    resident_hot_key_count = require_int_field(data, "resident_hot_key_count")
    validate_timing(data, "resident_hot_get", resident_hot_key_count)

    if data["cold_ssd_refills"] > data["iterations"]:
        fail("cold_ssd_refills cannot exceed iterations")
    if data["main_pressure_passed"] != (
        data["memory_evictions"] > 0
        and data["pmem_evictions"] > 0
        and data["cold_ssd_refills"] > 0
        and data["refill_failures"] == 0
    ):
        fail("main_pressure_passed disagrees with pressure/refill counters")
    if data["replacement_soak_passed"] != data["matrixcache_contract"]["replacement_soak"]:
        fail("replacement_soak_passed disagrees with matrixcache_contract.replacement_soak")
    if data["restart_disk_refill_ready"] != data["matrixcache_contract"]["restart_disk_refill"]:
        fail("restart_disk_refill_ready disagrees with matrixcache_contract.restart_disk_refill")
    if args.max_hot_get_p99_us is not None:
        hot_p99 = require_numeric_field(data["hot_get"], "p99_us")
        if hot_p99 > args.max_hot_get_p99_us:
            fail(f"hot_get.p99_us={hot_p99:.0f} exceeds {args.max_hot_get_p99_us}")
    if args.max_cold_refill_p99_us is not None:
        cold_p99 = require_numeric_field(data["cold_ssd_refill_get"], "p99_us")
        if cold_p99 > args.max_cold_refill_p99_us:
            fail(
                f"cold_ssd_refill_get.p99_us={cold_p99:.0f} "
                f"exceeds {args.max_cold_refill_p99_us}"
            )

    validate_contract(data, args.allow_failed)
    return data


def main() -> int:
    args = parse_args()
    data = validate(args)
    print(
        "OK matrixcache backend report: "
        f"backend={data['backend']} iterations={data['iterations']} "
        f"hot_get_p99={data['hot_get']['p99_us']}us "
        f"replacement_soak_iterations={data['replacement_soak_iterations']} "
        f"cold_refill_p99={data['cold_ssd_refill_get']['p99_us']}us "
        f"memory_evictions={data['memory_evictions']} "
        f"pmem_evictions={data['pmem_evictions']} "
        f"disk_fills={data['disk_fills']} "
        f"async_writeback_backpressure={data['async_writeback_backpressure']} "
        f"cold_refills={data['cold_ssd_refills']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
