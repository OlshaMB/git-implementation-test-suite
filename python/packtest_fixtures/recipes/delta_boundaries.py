from __future__ import annotations

from pathlib import Path

from ..repository import Repository, write_fixture


SIZES = (127, 128, 129, 16_383, 16_384, 16_385, 65_535, 65_536, 65_537)


def generate(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    repository = Repository.create(destination / "repository.git")

    entries: list[tuple[str, str, str]] = []
    for size in SIZES:
        base = _content(size)
        variant = bytearray(base)
        variant[size // 2] = ord("y")
        entries.extend(
            (
                ("100644", f"size-{size:05d}-base.bin", repository.blob(base)),
                (
                    "100644",
                    f"size-{size:05d}-variant.bin",
                    repository.blob(bytes(variant)),
                ),
            )
        )

    tree = repository.tree(entries)
    head = repository.commit(
        tree,
        "Add objects around delta size boundaries",
        timestamp=1_713_000_000,
    )
    repository.update_main(head)
    write_fixture(
        destination,
        repository,
        name="delta-boundaries",
        heads=[head],
        delta_required=True,
    )


def _content(size: int) -> bytes:
    pattern = b"0123456789abcdef"
    return (pattern * ((size + len(pattern) - 1) // len(pattern)))[:size]
