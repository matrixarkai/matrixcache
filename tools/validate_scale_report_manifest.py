#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 MatrixArkAI
"""Validate a MatrixCache scale report manifest and its referenced artifacts."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


REPORT_VERSION = "matrixcache_scale_report_manifest_v1"
EXPECTED_REPORTS = {
    "batch_read_cost",
    "batch_concurrency",
    "hit_concurrency",
    "read_path_cost",
}
REQUIRED_REPORT_FIELDS = {"name", "description", "path", "validator"}


def fail(message: str) -> None:
    print(f"matrixcache scale report manifest invalid: {message}", file=sys.stderr)
    raise SystemExit(1)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Repository root used for validator paths.",
    )
    parser.add_argument("--require-release-profile", action="store_true")
    parser.add_argument("--require-markdown", action="store_true")
    parser.add_argument(
        "--expect-reports",
        nargs="*",
        default=sorted(EXPECTED_REPORTS),
        help="Report names that must be present. Pass an empty list to skip.",
    )
    return parser.parse_args()


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


def require_string(data: dict[str, Any], field: str) -> str:
    value = data.get(field)
    if not isinstance(value, str) or not value:
        fail(f"{field!r} must be a non-empty string")
    return value


def run_validator(repo: Path, validator: str, report_path: Path) -> None:
    validator_path = repo / validator
    if not validator_path.exists():
        fail(f"validator {validator!r} does not exist")
    try:
        subprocess.run(
            [sys.executable, str(validator_path), str(report_path)],
            cwd=repo,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except subprocess.CalledProcessError as error:
        if error.stdout:
            print(error.stdout, file=sys.stderr)
        if error.stderr:
            print(error.stderr, file=sys.stderr)
        fail(f"validator {validator!r} failed for {report_path}")


def validate_markdown(manifest: dict[str, Any], manifest_path: Path) -> None:
    markdown = manifest.get("markdown_path")
    if not isinstance(markdown, str) or not markdown:
        fail("markdown_path must be present when --require-markdown is used")
    markdown_path = Path(markdown)
    if not markdown_path.exists():
        fail(f"markdown report {markdown_path} does not exist")
    text = markdown_path.read_text()
    required = [
        "# MatrixCache Read/Retrieval Scale Report",
        str(manifest_path),
        "## batch_read_cost",
        "## batch_concurrency",
        "## hit_concurrency",
        "## read_path_cost",
    ]
    missing = [needle for needle in required if needle not in text]
    if missing:
        fail(f"markdown report missing text: {', '.join(missing)}")


def main() -> int:
    args = parse_args()
    manifest = load_json(args.manifest)
    if manifest.get("report_version") != REPORT_VERSION:
        fail(f"unexpected report_version={manifest.get('report_version')!r}")
    require_string(manifest, "generated_at")
    profile = require_string(manifest, "profile")
    if args.require_release_profile and profile != "release":
        fail(f"profile={profile!r}, expected 'release'")
    reports = manifest.get("reports")
    if not isinstance(reports, list) or not reports:
        fail("reports must be a non-empty array")

    names = set()
    for index, report in enumerate(reports):
        if not isinstance(report, dict):
            fail(f"reports[{index}] must be an object")
        missing = REQUIRED_REPORT_FIELDS.difference(report)
        if missing:
            fail(f"reports[{index}] missing fields: {', '.join(sorted(missing))}")
        name = require_string(report, "name")
        if name in names:
            fail(f"duplicate report name {name!r}")
        names.add(name)
        description = require_string(report, "description")
        if len(description) < 8:
            fail(f"report {name!r} description is too short")
        report_path = Path(require_string(report, "path"))
        if not report_path.exists():
            fail(f"report {name!r} path does not exist: {report_path}")
        validator = require_string(report, "validator")
        run_validator(args.repo, validator, report_path)

    expected = set(args.expect_reports)
    if expected and names != expected:
        fail(f"report names differ: expected={sorted(expected)} actual={sorted(names)}")
    if args.require_markdown:
        validate_markdown(manifest, args.manifest)

    print(
        "OK matrixcache scale report manifest: "
        f"profile={profile} reports={len(reports)} names={','.join(sorted(names))}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
