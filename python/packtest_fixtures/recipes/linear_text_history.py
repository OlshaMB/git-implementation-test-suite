from __future__ import annotations

from pathlib import Path

from ..repository import Repository, write_fixture


def generate(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    repository = Repository.create(destination / "repository.git")

    lines = [
        f"{index:05d}: packtest deterministic source line with stable padding {'x' * 24}\n"
        for index in range(4096)
    ]
    parent: str | None = None
    for revision in range(40):
        line = (revision * 97) % len(lines)
        lines[line] = (
            f"{line:05d}: revision {revision:03d} changed this localized source line "
            f"{'y' * 25}\n"
        )
        source = repository.blob("".join(lines).encode())
        metadata = repository.blob(
            f"fixture revision {revision:03d}\n".encode("ascii")
        )
        tree = repository.tree(
            [
                ("100644", "large-source.txt", source),
                ("100644", "revision.txt", metadata),
            ]
        )
        parents = (parent,) if parent is not None else ()
        parent = repository.commit(
            tree,
            f"Fixture revision {revision}",
            parents=parents,
            timestamp=1_700_000_000 + revision,
        )

    assert parent is not None
    repository.update_main(parent)
    write_fixture(
        destination,
        repository,
        name="linear-text-history",
        heads=[parent],
        delta_required=True,
        max_delta_ratio=0.75,
    )
