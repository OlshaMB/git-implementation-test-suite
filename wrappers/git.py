#!/usr/bin/env python3
"""Reference wrapper for canonical Git's pack-objects command."""

from __future__ import annotations

import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import tempfile


REQUIRED_ENV = (
    "PACKTEST_REPO_PATH",
    "PACKTEST_HEADS_PATH",
    "PACKTEST_REQUEST_PATH",
    "PACKTEST_OUTPUT_PATH",
)


def main() -> int:
    missing = [name for name in REQUIRED_ENV if not os.environ.get(name)]
    if missing:
        print(f"missing environment variables: {', '.join(missing)}", file=sys.stderr)
        return 2

    repository = Path(os.environ["PACKTEST_REPO_PATH"])
    heads_path = Path(os.environ["PACKTEST_HEADS_PATH"])
    request_path = Path(os.environ["PACKTEST_REQUEST_PATH"])
    output_path = Path(os.environ["PACKTEST_OUTPUT_PATH"])
    metrics_path_value = os.environ.get("PACKTEST_METRICS_PATH")
    metrics_path = Path(metrics_path_value) if metrics_path_value else None

    request = json.loads(request_path.read_text(encoding="utf-8"))
    if request.get("version") != 1:
        print("unsupported request version", file=sys.stderr)
        return 2
    if request.get("objectFormat") != "sha1":
        print("only SHA-1 repositories are supported by this wrapper", file=sys.stderr)
        return 2

    delta_mode = request.get("deltaCompression")
    if delta_mode not in {"enabled", "disabled"}:
        print("deltaCompression must be 'enabled' or 'disabled'", file=sys.stderr)
        return 2

    command = [
        "git",
        f"--git-dir={repository}",
        "-c",
        "core.useReplaceRefs=false",
        "pack-objects",
        "--stdout",
        "--revs",
        "--no-reuse-delta",
        "--no-reuse-object",
    ]
    if delta_mode == "enabled":
        command.append("--delta-base-offset")
    else:
        command.append("--window=0")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path: Path | None = None
    try:
        with heads_path.open("rb") as heads:
            with tempfile.NamedTemporaryFile(
                prefix=f".{output_path.name}.",
                suffix=".tmp",
                dir=output_path.parent,
                delete=False,
            ) as output:
                temporary_path = Path(output.name)
                process = subprocess.run(
                    command,
                    stdin=heads,
                    stdout=output,
                    stderr=subprocess.PIPE,
                    check=False,
                    env=_git_environment(),
                )

        if process.returncode != 0:
            sys.stderr.buffer.write(process.stderr)
            return process.returncode

        os.replace(temporary_path, output_path)
        temporary_path = None
        if metrics_path is not None:
            usage = resource.getrusage(resource.RUSAGE_CHILDREN)
            peak_memory_bytes = int(usage.ru_maxrss)
            if sys.platform != "darwin":
                peak_memory_bytes *= 1024
            temporary_metrics = metrics_path.with_name(
                f".{metrics_path.name}.{os.getpid()}.tmp"
            )
            temporary_metrics.write_text(
                json.dumps({"peakMemoryBytes": peak_memory_bytes}) + "\n",
                encoding="utf-8",
            )
            os.replace(temporary_metrics, metrics_path)
        return 0
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)


def _git_environment() -> dict[str, str]:
    environment = os.environ.copy()
    environment.update(
        {
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_TERMINAL_PROMPT": "0",
            "LC_ALL": "C",
        }
    )
    return environment


if __name__ == "__main__":
    raise SystemExit(main())
