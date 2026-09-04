from __future__ import annotations

import argparse
from pathlib import Path

from .recipes import RECIPES


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate packtest fixtures")
    parser.add_argument("recipe", nargs="?", choices=sorted(RECIPES))
    parser.add_argument("--all", action="store_true", help="generate every fixture")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if args.all == (args.recipe is not None):
        parser.error("choose exactly one recipe or --all")

    selected = sorted(RECIPES) if args.all else [args.recipe]
    for name in selected:
        destination = args.output / name if args.all else args.output
        RECIPES[name](destination)
        print(f"generated {name}: {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
