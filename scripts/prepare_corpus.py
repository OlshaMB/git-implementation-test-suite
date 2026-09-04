#!/usr/bin/env python3
"""Acquire a pinned Git corpus and create sample or full fixtures."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "python"))

from packtest_fixtures.corpus import acquire, create_full, create_sample, load_spec


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument(
        "--cache",
        type=Path,
        help="bare repository cache; defaults to ~/.cache/packtest/<corpus>.git",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("acquire", help="explicitly fetch the configured pinned ref")
    sample = subparsers.add_parser("sample", help="create a standalone blob sample")
    sample.add_argument("--output", type=Path, required=True)
    sample.add_argument("--blobs", type=int, help="override the configured sample size")
    full = subparsers.add_parser("full", help="create a full-history cache-backed fixture")
    full.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()

    spec = load_spec(arguments.config)
    cache = arguments.cache or (
        Path.home() / ".cache" / "packtest" / f"{spec.name}.git"
    )
    if arguments.command == "acquire":
        acquire(spec, cache)
    elif arguments.command == "sample":
        create_sample(spec, cache, arguments.output, blob_count=arguments.blobs)
    else:
        create_full(spec, cache, arguments.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
