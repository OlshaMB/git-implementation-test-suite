"""Deterministic bare repository construction using canonical Git plumbing."""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess


class Repository:
    def __init__(self, path: Path) -> None:
        self.path = path

    @classmethod
    def create(cls, path: Path) -> "Repository":
        if path.exists():
            shutil.rmtree(path)
        subprocess.run(
            ["git", "init", "--bare", "--quiet", "--object-format=sha1", path],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        repository = cls(path)
        repository._git("symbolic-ref", "HEAD", "refs/heads/main")
        return repository

    def _git(
        self,
        *arguments: str,
        input: bytes | None = None,
        environment: dict[str, str] | None = None,
    ) -> bytes:
        command_environment = os.environ.copy()
        command_environment["LC_ALL"] = "C"
        if environment is not None:
            command_environment.update(environment)
        result = subprocess.run(
            ["git", "--git-dir", self.path, *arguments],
            input=input,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=command_environment,
        )
        return result.stdout

    def write_object(self, kind: str, data: bytes) -> str:
        return self._git("hash-object", "-w", "-t", kind, "--stdin", input=data).decode(
            "ascii"
        ).strip()

    def blob(self, data: bytes) -> str:
        return self.write_object("blob", data)

    def tree(self, entries: list[tuple[str, str, str]]) -> str:
        data = b"".join(
            mode.encode("ascii")
            + b" "
            + (b"tree" if mode == "40000" else b"blob")
            + b" "
            + object_id.encode("ascii")
            + b"\t"
            + name.encode("utf-8")
            + b"\0"
            for mode, name, object_id in entries
        )
        return self._git("mktree", "-z", input=data).decode("ascii").strip()

    def commit(
        self,
        tree: str,
        message: str,
        *,
        parents: tuple[str, ...] = (),
        timestamp: int,
    ) -> str:
        identity = {
            "GIT_AUTHOR_NAME": "Pack Test",
            "GIT_AUTHOR_EMAIL": "packtest@example.invalid",
            "GIT_AUTHOR_DATE": f"@{timestamp} +0000",
            "GIT_COMMITTER_NAME": "Pack Test",
            "GIT_COMMITTER_EMAIL": "packtest@example.invalid",
            "GIT_COMMITTER_DATE": f"@{timestamp} +0000",
        }
        arguments = ["commit-tree", tree]
        for parent in parents:
            arguments.extend(("-p", parent))
        return self._git(
            *arguments, input=f"{message}\n".encode("utf-8"), environment=identity
        ).decode("ascii").strip()

    def update_main(self, commit: str) -> None:
        self._git("update-ref", "refs/heads/main", commit)

    def reachable(self, heads: list[str]) -> set[str]:
        output = self._git("rev-list", "--objects", "--no-object-names", *heads)
        return set(output.decode("ascii").splitlines())


def write_fixture(
    destination: Path,
    repository: Repository,
    *,
    name: str,
    heads: list[str],
    delta_required: bool,
    max_delta_ratio: float | None = None,
) -> None:
    expected = sorted(repository.reachable(heads))
    (destination / "expected-objects.txt").write_text(
        "".join(f"{object_id}\n" for object_id in expected), encoding="ascii"
    )
    expectations: dict[str, object] = {"deltaRequired": delta_required}
    if max_delta_ratio is not None:
        expectations["maxDeltaRatio"] = max_delta_ratio
    manifest = {
        "version": 1,
        "name": name,
        "objectFormat": "sha1",
        "heads": heads,
        "expectedObjects": "expected-objects.txt",
        "expectations": expectations,
    }
    (destination / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
