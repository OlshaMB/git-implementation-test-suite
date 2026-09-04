from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from packtest_fixtures.corpus import (
    CorpusSpec,
    acquire,
    create_full,
    create_sample,
    load_spec,
)
from packtest_fixtures.repository import Repository


class CorpusTests(unittest.TestCase):
    def test_acquisition_and_sample_are_pinned_and_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = Repository.create(root / "source.git")
            entries = [
                ("100644", f"file-{index}.txt", source.blob(f"content {index}\n".encode()))
                for index in range(12)
            ]
            tree = source.tree(entries)
            commit = source.commit(tree, "Corpus source", timestamp=1_700_000_000)
            source._git("update-ref", "refs/tags/test", commit)
            spec = CorpusSpec(
                "test-corpus",
                str(source.path),
                "refs/tags/test",
                commit,
                "configured-seed",
                5,
            )

            cache = root / "cache.git"
            acquire(spec, cache)
            first = root / "first"
            second = root / "second"
            create_sample(spec, cache, first)
            create_sample(spec, cache, second)

            self.assertEqual(
                (first / "manifest.json").read_bytes(),
                (second / "manifest.json").read_bytes(),
            )
            self.assertEqual(
                (first / "expected-objects.txt").read_bytes(),
                (second / "expected-objects.txt").read_bytes(),
            )
            self.assertEqual(
                (first / "corpus.json").read_bytes(),
                (second / "corpus.json").read_bytes(),
            )
            metadata = json.loads((first / "corpus.json").read_text())
            self.assertEqual(len(metadata["selectedBlobs"]), 5)

    def test_full_fixture_uses_verified_cache(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = Repository.create(root / "source.git")
            tree = source.tree([("100644", "file", source.blob(b"content\n"))])
            commit = source.commit(tree, "Corpus source", timestamp=1_700_000_000)
            source._git("update-ref", "refs/tags/test", commit)
            spec = CorpusSpec(
                "test-corpus",
                str(source.path),
                "refs/tags/test",
                commit,
                "configured-seed",
                5,
            )
            cache = root / "cache.git"
            acquire(spec, cache)

            destination = root / "full"
            create_full(spec, cache, destination)

            self.assertTrue((destination / "repository.git").is_symlink())
            manifest = json.loads((destination / "manifest.json").read_text())
            self.assertEqual(manifest["heads"], [commit])

    def test_loads_corpus_definition_from_toml(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary) / "corpus.toml"
            config.write_text(
                """
version = 1
[corpus]
name = "example"
url = "https://example.invalid/repository.git"
ref = "refs/tags/v1"
commit = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
[sample]
seed = "example-v1"
blobs = 32
""",
                encoding="utf-8",
            )

            spec = load_spec(config)

            self.assertEqual(spec.name, "example")
            self.assertEqual(spec.sample_blobs, 32)


if __name__ == "__main__":
    unittest.main()
