from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from packtest_fixtures.recipes import RECIPES


class FixtureTests(unittest.TestCase):
    def test_all_recipes_are_deterministic(self) -> None:
        for name, recipe in RECIPES.items():
            with self.subTest(recipe=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                first = root / "first"
                second = root / "second"
                recipe(first)
                recipe(second)

                self.assertEqual(
                    (first / "manifest.json").read_bytes(),
                    (second / "manifest.json").read_bytes(),
                )
                self.assertEqual(
                    (first / "expected-objects.txt").read_bytes(),
                    (second / "expected-objects.txt").read_bytes(),
                )

                manifest = json.loads((first / "manifest.json").read_text())
                expected = set(
                    (first / manifest["expectedObjects"]).read_text().splitlines()
                )
                self.assertTrue(set(manifest["heads"]) <= expected)


if __name__ == "__main__":
    unittest.main()
