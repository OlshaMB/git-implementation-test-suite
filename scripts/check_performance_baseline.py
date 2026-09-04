#!/usr/bin/env python3
"""Compare a packtest JSON report with a previously recorded baseline."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "python"))

from packtest_fixtures.performance import RegressionLimits, compare_reports


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("report", type=Path)
    parser.add_argument("--pack-size", type=float, default=0.01)
    parser.add_argument("--generation-time", type=float, default=0.25)
    parser.add_argument("--resolution-time", type=float, default=0.25)
    parser.add_argument("--peak-memory", type=float, default=0.15)
    parser.add_argument("--throughput", type=float, default=0.20)
    arguments = parser.parse_args()
    limits = RegressionLimits(
        pack_size=arguments.pack_size,
        generation_time=arguments.generation_time,
        resolution_time=arguments.resolution_time,
        peak_memory=arguments.peak_memory,
        throughput=arguments.throughput,
    )
    baseline = json.loads(arguments.baseline.read_text(encoding="utf-8"))
    report = json.loads(arguments.report.read_text(encoding="utf-8"))
    errors = compare_reports(baseline, report, limits)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("performance report is within baseline limits")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
