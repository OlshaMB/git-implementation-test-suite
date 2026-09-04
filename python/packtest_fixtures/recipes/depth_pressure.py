from __future__ import annotations

from pathlib import Path

from ..repository import Repository, write_fixture


def generate(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    repository = Repository.create(destination / "repository.git")

    lines = [
        f"{index:05d}: depth pressure content with stable padding {'x' * 40}\n"
        for index in range(2048)
    ]
    parent: str | None = None
    for revision in range(96):
        line = (revision * 181) % len(lines)
        lines[line] = (
            f"{line:05d}: depth revision {revision:03d} "
            f"{chr(ord('a') + revision % 26) * 42}\n"
        )
        source = repository.blob("".join(lines).encode())
        tree = repository.tree([("100644", "evolving-source.txt", source)])
        parents = (parent,) if parent is not None else ()
        parent = repository.commit(
            tree,
            f"Depth revision {revision}",
            parents=parents,
            timestamp=1_712_000_000 + revision,
        )

    assert parent is not None
    repository.update_main(parent)
    write_fixture(
        destination,
        repository,
        name="depth-pressure",
        heads=[parent],
        delta_required=True,
    )
