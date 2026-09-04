from __future__ import annotations

from pathlib import Path

from ..repository import Repository, write_fixture


def generate(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    repository = Repository.create(destination / "repository.git")

    original = [
        f"{index:05d}: competing delta base candidate {'x' * 56}\n"
        for index in range(3072)
    ]
    entries: list[tuple[str, str, str]] = []
    for candidate in range(30):
        lines = original.copy()
        edit_count = 1 + candidate % 10
        family = candidate // 10
        for edit in range(edit_count):
            line = (family * 887 + edit * 251) % len(lines)
            lines[line] = (
                f"{line:05d}: family {family} candidate {candidate:02d} edit {edit:02d} "
                f"{chr(ord('a') + family) * 34}\n"
            )
        blob = repository.blob("".join(lines).encode())
        entries.append(("100644", f"candidate-{candidate:02d}.txt", blob))

    tree = repository.tree(entries)
    head = repository.commit(
        tree,
        "Create competing delta-base candidates",
        timestamp=1_711_000_000,
    )
    repository.update_main(head)
    write_fixture(
        destination,
        repository,
        name="competing-bases",
        heads=[head],
        delta_required=True,
    )
