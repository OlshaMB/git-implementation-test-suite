from __future__ import annotations

from pathlib import Path

from ..repository import Repository, write_fixture


def generate(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    repository = Repository.create(destination / "repository.git")

    original = [
        f"{index:05d}: shared branching fixture content {'x' * 48}\n"
        for index in range(2048)
    ]
    root = _commit(repository, original, None, "Branching root", 1_710_000_000)

    heads: list[str] = []
    for branch in range(3):
        lines = original.copy()
        parent = root
        for revision in range(12):
            line = (branch * 509 + revision * 137) % len(lines)
            lines[line] = (
                f"{line:05d}: branch {branch} revision {revision:02d} "
                f"{chr(ord('a') + branch) * 48}\n"
            )
            parent = _commit(
                repository,
                lines,
                parent,
                f"Branch {branch} revision {revision}",
                1_710_000_001 + branch * 100 + revision,
            )
        heads.append(parent)

    repository.update_main(heads[0])
    write_fixture(
        destination,
        repository,
        name="branching-history",
        heads=heads,
        delta_required=True,
    )


def _commit(
    repository: Repository,
    lines: list[str],
    parent: str | None,
    message: str,
    timestamp: int,
) -> str:
    source = repository.blob("".join(lines).encode())
    tree = repository.tree([("100644", "shared-source.txt", source)])
    parents = (parent,) if parent is not None else ()
    return repository.commit(tree, message, parents=parents, timestamp=timestamp)
