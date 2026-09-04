#!/usr/bin/env python3
"""Run the fixture generator without installing its Python package."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "python"))

from packtest_fixtures.__main__ import main  # noqa: E402


if __name__ == "__main__":
    raise SystemExit(main())
