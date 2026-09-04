from __future__ import annotations

from pathlib import Path

from ..repository import Repository, write_fixture


def generate(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    repository = Repository.create(destination / "repository.git")

    readme = repository.blob(b"# Tiny fixture\n\nPack interoperability.\n")
    binary = repository.blob(bytes(range(256)) + b"\0packtest\0")
    executable = repository.blob(b"#!/bin/sh\nprintf 'packtest\\n'\n")
    symlink = repository.blob(b"README.md")
    nested_blob = repository.blob("snowman: \N{SNOWMAN}\n".encode())
    nested_tree = repository.tree([("100644", "unicode.txt", nested_blob)])
    root = repository.tree(
        [
            ("100644", "README.md", readme),
            ("100644", "binary.dat", binary),
            ("100755", "run.sh", executable),
            ("120000", "readme-link", symlink),
            ("40000", "directory", nested_tree),
        ]
    )
    head = repository.commit(
        root, "Create mixed fixture", timestamp=1_700_000_000
    )
    repository.update_main(head)

    # The pack walk must not include this object merely because it exists.
    repository.blob(b"deliberately unreachable\n")

    write_fixture(
        destination,
        repository,
        name="tiny-mixed",
        heads=[head],
        delta_required=False,
    )
