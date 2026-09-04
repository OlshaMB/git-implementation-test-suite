"""Explicit acquisition and deterministic sampling of Git corpora."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

from .repository import Repository, write_fixture


@dataclass(frozen=True)
class CorpusSpec:
    name: str
    url: str
    ref: str
    commit: str
    sample_seed: str
    sample_blobs: int
    sample_delta_required: bool = False
    full_delta_required: bool = False


def load_spec(path: Path) -> CorpusSpec:
    """Load and validate a versioned corpus definition."""
    with path.open("rb") as input_file:
        document = tomllib.load(input_file)
    if document.get("version") != 1:
        raise ValueError(f"{path} has an unsupported corpus config version")
    corpus = document.get("corpus")
    sample = document.get("sample")
    expectations = document.get("expectations", {})
    if not isinstance(corpus, dict) or not isinstance(sample, dict):
        raise ValueError(f"{path} must contain [corpus] and [sample] tables")
    if not isinstance(expectations, dict):
        raise ValueError(f"[expectations] in {path} must be a table")
    try:
        spec = CorpusSpec(
            name=corpus["name"],
            url=corpus["url"],
            ref=corpus["ref"],
            commit=corpus["commit"],
            sample_seed=sample["seed"],
            sample_blobs=sample["blobs"],
            sample_delta_required=expectations.get("sample_delta_required", False),
            full_delta_required=expectations.get("full_delta_required", False),
        )
    except KeyError as error:
        raise ValueError(f"{path} is missing {error.args[0]!r}") from error
    _validate_spec(spec, path)
    return spec


def acquire(spec: CorpusSpec, cache: Path) -> None:
    """Fetch a pinned corpus into a reusable bare repository."""
    if not (cache / "HEAD").is_file():
        if cache.exists():
            shutil.rmtree(cache)
        _run(["git", "init", "--bare", "--quiet", cache])
    remotes = _git(cache, "remote").decode("utf-8").splitlines()
    if "origin" in remotes:
        _git(cache, "remote", "set-url", "origin", spec.url)
    else:
        _git(cache, "remote", "add", "origin", spec.url)
    _git(
        cache,
        "fetch",
        "--no-tags",
        "--force",
        "origin",
        f"{spec.ref}:refs/packtest/source",
    )
    actual = _git(cache, "rev-parse", "refs/packtest/source^{commit}").decode().strip()
    if actual != spec.commit:
        raise RuntimeError(
            f"{spec.name} ref {spec.ref} resolved to {actual}, expected {spec.commit}"
        )
    _git(cache, "update-ref", "refs/packtest/pinned", spec.commit)
    (cache / "packtest-corpus.json").write_text(
        json.dumps({"version": 1, **_spec_metadata(spec)}, indent=2) + "\n",
        encoding="utf-8",
    )


def create_sample(
    spec: CorpusSpec,
    cache: Path,
    destination: Path,
    *,
    blob_count: int | None = None,
    seed: str | None = None,
) -> None:
    """Create a standalone fixture from a stable hash-ranked sample of blobs."""
    _verify_cache(spec, cache)
    blob_count = spec.sample_blobs if blob_count is None else blob_count
    seed = spec.sample_seed if seed is None else seed
    if blob_count <= 0:
        raise ValueError("blob_count must be positive")
    candidates = _reachable_blobs(cache, spec.commit)
    if blob_count > len(candidates):
        raise ValueError(
            f"requested {blob_count} blobs, but the corpus has only {len(candidates)}"
        )
    seed_bytes = seed.encode("utf-8")
    selected = sorted(
        candidates,
        key=lambda item: (
            hashlib.sha256(seed_bytes + bytes.fromhex(item[0])).digest(),
            item[0],
        ),
    )[:blob_count]
    selected.sort()

    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    repository = Repository.create(destination / "repository.git")
    entries = []
    for index, (object_id, _size) in enumerate(selected):
        data = _git(cache, "cat-file", "blob", object_id)
        copied_id = repository.blob(data)
        if copied_id != object_id:
            raise RuntimeError(f"copied blob {object_id} became {copied_id}")
        entries.append(("100644", f"{index:06d}-{object_id}.blob", copied_id))
    tree = repository.tree(entries)
    head = repository.commit(
        tree,
        f"Deterministic sample of {spec.name}",
        timestamp=1_731_888_000,
    )
    repository.update_main(head)
    write_fixture(
        destination,
        repository,
        name=f"{spec.name}-sample-{blob_count}",
        heads=[head],
        delta_required=spec.sample_delta_required,
    )
    _write_selection_metadata(
        destination,
        spec,
        mode="sample",
        blob_count=blob_count,
        seed=seed,
        selected=selected,
    )


def create_full(spec: CorpusSpec, cache: Path, destination: Path) -> None:
    """Create a lightweight full-history fixture referring to the corpus cache."""
    _verify_cache(spec, cache)
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    os.symlink(cache.resolve(), destination / "repository.git", target_is_directory=True)
    write_fixture(
        destination,
        Repository(cache),
        name=f"{spec.name}-full",
        heads=[spec.commit],
        delta_required=spec.full_delta_required,
    )
    _write_selection_metadata(destination, spec, mode="full")


def _reachable_blobs(repository: Path, commit: str) -> list[tuple[str, int]]:
    object_ids = _git(
        repository, "rev-list", "--objects", "--no-object-names", commit
    ).splitlines()
    process = subprocess.run(
        [
            "git",
            f"--git-dir={repository}",
            "cat-file",
            "--batch-check=%(objectname) %(objecttype) %(objectsize)",
        ],
        input=b"\n".join(object_ids) + b"\n",
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=_environment(),
    )
    blobs = []
    for line in process.stdout.decode("ascii").splitlines():
        object_id, kind, size = line.split()
        if kind == "blob":
            blobs.append((object_id, int(size)))
    return blobs


def _verify_cache(spec: CorpusSpec, cache: Path) -> None:
    try:
        actual = _git(cache, "rev-parse", "refs/packtest/pinned^{commit}").decode().strip()
    except subprocess.CalledProcessError as error:
        raise RuntimeError(
            f"corpus cache {cache} is not prepared; run the acquire command first"
        ) from error
    if actual != spec.commit:
        raise RuntimeError(
            f"corpus cache contains {actual}, expected pinned commit {spec.commit}"
        )


def _validate_spec(spec: CorpusSpec, path: Path) -> None:
    string_fields = {
        "corpus.name": spec.name,
        "corpus.url": spec.url,
        "corpus.ref": spec.ref,
        "sample.seed": spec.sample_seed,
    }
    for field, value in string_fields.items():
        if not isinstance(value, str) or not value:
            raise ValueError(f"{field} in {path} must be a nonempty string")
    if (
        not isinstance(spec.commit, str)
        or len(spec.commit) != 40
        or not all(character in "0123456789abcdef" for character in spec.commit)
    ):
        raise ValueError(f"corpus.commit in {path} must be a full SHA-1 object ID")
    if type(spec.sample_blobs) is not int or spec.sample_blobs <= 0:
        raise ValueError(f"sample.blobs in {path} must be a positive integer")
    if type(spec.sample_delta_required) is not bool or type(
        spec.full_delta_required
    ) is not bool:
        raise ValueError(f"delta expectations in {path} must be booleans")


def _write_selection_metadata(
    destination: Path,
    spec: CorpusSpec,
    *,
    mode: str,
    blob_count: int | None = None,
    seed: str | None = None,
    selected: list[tuple[str, int]] | None = None,
) -> None:
    metadata: dict[str, object] = {
        "version": 1,
        **_spec_metadata(spec),
        "mode": mode,
    }
    if blob_count is not None:
        metadata["blobCount"] = blob_count
    if seed is not None:
        metadata["seed"] = seed
    if selected is not None:
        metadata["selectedBlobs"] = [
            {"objectId": object_id, "size": size} for object_id, size in selected
        ]
    (destination / "corpus.json").write_text(
        json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
    )


def _spec_metadata(spec: CorpusSpec) -> dict[str, object]:
    return {
        "corpus": {
            "name": spec.name,
            "url": spec.url,
            "ref": spec.ref,
            "commit": spec.commit,
        },
        "sample": {
            "seed": spec.sample_seed,
            "blobs": spec.sample_blobs,
        },
        "expectations": {
            "sampleDeltaRequired": spec.sample_delta_required,
            "fullDeltaRequired": spec.full_delta_required,
        },
    }


def _git(repository: Path, *arguments: str) -> bytes:
    return _run(["git", f"--git-dir={repository}", *arguments])


def _run(command: list[str | Path]) -> bytes:
    return subprocess.run(
        command,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=_environment(),
    ).stdout


def _environment() -> dict[str, str]:
    environment = os.environ.copy()
    environment.update(
        {
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_TERMINAL_PROMPT": "0",
            "LC_ALL": "C",
        }
    )
    return environment
