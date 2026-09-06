#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 MatrixArkAI
"""Run MatrixCache read/retrieval scale reports and validate their JSON output."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


REPORTS = [
    {
        "name": "batch_read_cost",
        "example": "batch_read_cost",
        "output": "matrixcache-batch-read-cost.json",
        "validator": "tools/validate_batch_read_report.py",
        "validator_args": [],
        "description": "tiered batch read cost across copied, shared, no-promotion, and pinned paths",
    },
    {
        "name": "batch_concurrency",
        "example": "batch_concurrency_bench",
        "output": "matrixcache-batch-concurrency.json",
        "validator": "tools/validate_batch_concurrency_report.py",
        "validator_args": [],
        "description": "concurrent batch retrieval throughput",
    },
    {
        "name": "hit_concurrency",
        "example": "hit_concurrency_bench",
        "output": "matrixcache-hit-concurrency.json",
        "validator": "tools/validate_hit_concurrency_report.py",
        "validator_args": [],
        "description": "single-key memory-hit throughput and pinned-handle scale",
    },
    {
        "name": "read_path_cost",
        "example": "read_path_cost",
        "output": "matrixcache-read-path.json",
        "validator": "tools/validate_read_path_report.py",
        "validator_args": [
            "--max-full-ns",
            "10000",
            "--max-overhead-percent",
            "95",
            "--max-spread-percent",
            "100",
        ],
        "example_args": [
            "--require-passed",
            "--max-full-ns",
            "10000",
            "--max-overhead-percent",
            "95",
            "--max-spread-percent",
            "100",
        ],
        "description": "memory-hit bookkeeping and lock overhead",
    },
]


def run(command: list[str], *, cwd: Path) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("/tmp/matrixcache-scale-reports"),
        help="Directory for benchmark reports and manifest.",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        help="Manifest path. Defaults to OUTPUT_DIR/matrixcache-scale-report-manifest.json.",
    )
    parser.add_argument(
        "--debug-build",
        action="store_true",
        help="Run examples without --release for quick local diagnostics.",
    )
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[1]
    output_dir = args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = args.manifest or output_dir / "matrixcache-scale-report-manifest.json"
    profile = "debug" if args.debug_build else "release"
    cargo_base = ["cargo", "run"]
    if not args.debug_build:
        cargo_base.append("--release")
    cargo_base.extend(["--no-default-features", "--example"])

    report_entries = []
    for spec in REPORTS:
        report_path = output_dir / spec["output"]
        example_args = spec.get("example_args", [])
        run(
            [
                *cargo_base,
                spec["example"],
                "--",
                "--json-output",
                str(report_path),
                *example_args,
            ],
            cwd=repo,
        )
        run(
            [
                sys.executable,
                spec["validator"],
                str(report_path),
                *spec.get("validator_args", []),
            ],
            cwd=repo,
        )
        report_entries.append(
            {
                "name": spec["name"],
                "description": spec["description"],
                "path": str(report_path),
                "validator": spec["validator"],
            }
        )

    manifest = {
        "report_version": "matrixcache_scale_report_manifest_v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "profile": profile,
        "reports": report_entries,
    }
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"matrixcache scale report manifest written to {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
