"""Performance regression checks for packtest JSON reports."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class RegressionLimits:
    pack_size: float = 0.01
    generation_time: float = 0.25
    resolution_time: float = 0.25
    peak_memory: float = 0.15
    throughput: float = 0.20


def compare_reports(
    baseline: dict[str, object],
    current: dict[str, object],
    limits: RegressionLimits,
) -> list[str]:
    """Return human-readable regressions between reports for identical work."""
    errors = []
    for field in ("fixture", "implementation"):
        if baseline.get(field) != current.get(field):
            errors.append(
                f"{field} differs: baseline {baseline.get(field)!r}, "
                f"current {current.get(field)!r}"
            )
    if errors:
        return errors

    baseline_runs = {run["mode"]: run for run in baseline["runs"]}
    current_runs = {run["mode"]: run for run in current["runs"]}
    if baseline_runs.keys() != current_runs.keys():
        return ["baseline and current reports contain different delta modes"]

    for mode, baseline_run in baseline_runs.items():
        current_run = current_runs[mode]
        _maximum(
            errors,
            mode,
            "pack bytes",
            baseline_run["pack"]["packBytes"],
            current_run["pack"]["packBytes"],
            limits.pack_size,
        )
        _maximum(
            errors,
            mode,
            "generation time",
            baseline_run["performance"]["generationMicroseconds"],
            current_run["performance"]["generationMicroseconds"],
            limits.generation_time,
        )
        _maximum(
            errors,
            mode,
            "resolution time",
            baseline_run["libgit2"]["objectResolutionMicroseconds"],
            current_run["libgit2"]["objectResolutionMicroseconds"],
            limits.resolution_time,
        )
        _minimum(
            errors,
            mode,
            "pack throughput",
            baseline_run["performance"]["packBytesPerSecond"],
            current_run["performance"]["packBytesPerSecond"],
            limits.throughput,
        )
        baseline_memory = baseline_run["performance"].get("peakMemoryBytes")
        current_memory = current_run["performance"].get("peakMemoryBytes")
        if baseline_memory is not None and current_memory is not None:
            _maximum(
                errors,
                mode,
                "peak memory",
                baseline_memory,
                current_memory,
                limits.peak_memory,
            )
    return errors


def _maximum(
    errors: list[str],
    mode: str,
    metric: str,
    baseline: float,
    current: float,
    tolerance: float,
) -> None:
    limit = baseline * (1.0 + tolerance)
    if current > limit:
        errors.append(
            f"{mode} {metric} regressed: {current:g} exceeds {limit:g} "
            f"(baseline {baseline:g}, tolerance {tolerance:.0%})"
        )


def _minimum(
    errors: list[str],
    mode: str,
    metric: str,
    baseline: float,
    current: float,
    tolerance: float,
) -> None:
    limit = baseline * (1.0 - tolerance)
    if current < limit:
        errors.append(
            f"{mode} {metric} regressed: {current:g} is below {limit:g} "
            f"(baseline {baseline:g}, tolerance {tolerance:.0%})"
        )
