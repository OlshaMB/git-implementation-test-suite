from __future__ import annotations

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from packtest_fixtures.performance import RegressionLimits, compare_reports


def report(*, pack_bytes: int = 100, generation: int = 100) -> dict[str, object]:
    return {
        "fixture": "fixture",
        "implementation": "implementation",
        "runs": [
            {
                "mode": "enabled",
                "pack": {"packBytes": pack_bytes},
                "performance": {
                    "generationMicroseconds": generation,
                    "packBytesPerSecond": 1000.0,
                    "peakMemoryBytes": 1000,
                },
                "libgit2": {"objectResolutionMicroseconds": 100},
            }
        ],
    }


class PerformanceTests(unittest.TestCase):
    def test_accepts_results_within_limits(self) -> None:
        self.assertEqual(
            compare_reports(report(), report(pack_bytes=101), RegressionLimits()),
            [],
        )

    def test_reports_regressions(self) -> None:
        errors = compare_reports(
            report(), report(pack_bytes=102, generation=126), RegressionLimits()
        )
        self.assertTrue(any("pack bytes" in error for error in errors))
        self.assertTrue(any("generation time" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
