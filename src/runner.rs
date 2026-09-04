use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use wait_timeout::ChildExt;

use crate::model::{DeltaMode, DeltaSelection, FixtureManifest, Implementation, PackRequest};
use crate::pack::{self, PackStats};
use crate::validation;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationReport {
    mode: DeltaMode,
    pack_path: PathBuf,
    generation_milliseconds: u128,
    indexed_object_count: usize,
    canonical_git_validated: bool,
    performance: PerformanceMetrics,
    libgit2: validation::Libgit2Metrics,
    pack: PackStats,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PerformanceMetrics {
    generation_microseconds: u128,
    pack_bytes_per_second: f64,
    objects_per_second: f64,
    peak_memory_bytes: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapperMetrics {
    peak_memory_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Comparison {
    enabled_to_disabled_size_ratio: f64,
    bytes_saved: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunReport {
    fixture: String,
    implementation: String,
    runs: Vec<ValidationReport>,
    comparison: Option<Comparison>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectReport {
    pack_path: PathBuf,
    indexed_object_count: usize,
    canonical_git_validated: bool,
    libgit2: validation::Libgit2Metrics,
    pack: PackStats,
}

pub fn run(
    fixture_path: &Path,
    implementation_path: &Path,
    delta: DeltaSelection,
    output_dir: Option<&Path>,
    json_path: Option<&Path>,
    validate_git: bool,
) -> Result<()> {
    let fixture_path = fixture_path
        .canonicalize()
        .with_context(|| format!("could not find fixture {}", fixture_path.display()))?;
    let manifest_path = fixture_path.join("manifest.json");
    let manifest: FixtureManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .with_context(|| format!("could not read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("invalid fixture manifest {}", manifest_path.display()))?;
    validate_manifest(&manifest)?;

    let implementation_file = implementation_path.canonicalize().with_context(|| {
        format!(
            "could not find implementation config {}",
            implementation_path.display()
        )
    })?;
    let implementation: Implementation = toml::from_str(
        &fs::read_to_string(&implementation_file)
            .with_context(|| format!("could not read {}", implementation_file.display()))?,
    )
    .with_context(|| {
        format!(
            "invalid implementation config {}",
            implementation_file.display()
        )
    })?;
    validate_implementation(&implementation, &manifest, delta)?;

    let expected_path = fixture_path.join(&manifest.expected_objects);
    let expected = validation::read_expected_objects(&expected_path)?;
    for head in &manifest.heads {
        ensure!(
            expected.binary_search(&head.to_ascii_lowercase()).is_ok(),
            "fixture head {head} is absent from the expected object set"
        );
    }

    let temporary;
    let run_directory = if let Some(path) = output_dir {
        fs::create_dir_all(path)
            .with_context(|| format!("could not create output directory {}", path.display()))?;
        path.canonicalize()?
    } else {
        temporary = tempfile::tempdir().context("could not create run directory")?;
        temporary.path().to_path_buf()
    };

    let config_directory = implementation_file
        .parent()
        .context("implementation config has no parent directory")?;
    let wrapper_directory = implementation
        .working_directory
        .as_deref()
        .map(|path| config_directory.join(path))
        .unwrap_or_else(|| config_directory.to_path_buf())
        .canonicalize()
        .context("could not resolve wrapper working directory")?;

    let mut reports = Vec::new();
    for &mode in delta.modes() {
        reports.push(run_one(
            &fixture_path,
            &manifest,
            &implementation,
            &wrapper_directory,
            &expected,
            &run_directory,
            mode,
            validate_git,
        )?);
    }

    let comparison = compare_modes(&manifest, &reports)?;
    let report = RunReport {
        fixture: manifest.name,
        implementation: implementation.name,
        runs: reports,
        comparison,
    };
    print_run_report(&report);
    write_json(json_path, &report)
}

pub fn inspect(pack_path: &Path, json_path: Option<&Path>, validate_git: bool) -> Result<()> {
    let pack_path = pack_path
        .canonicalize()
        .with_context(|| format!("could not find pack {}", pack_path.display()))?;
    let stats = pack::scan(&pack_path)?;
    let index = validation::index_with_libgit2(&pack_path)?;
    ensure!(
        index.object_ids.len() == stats.object_count as usize,
        "pack header declares {} objects, libgit2 indexed {}",
        stats.object_count,
        index.object_ids.len()
    );
    if validate_git {
        validation::validate_with_git(&pack_path)?;
    }
    let report = InspectReport {
        pack_path,
        indexed_object_count: index.object_ids.len(),
        canonical_git_validated: validate_git,
        libgit2: index.metrics,
        pack: stats,
    };
    print_inspect_report(&report);
    write_json(json_path, &report)
}

#[allow(clippy::too_many_arguments)]
fn run_one(
    fixture_path: &Path,
    manifest: &FixtureManifest,
    implementation: &Implementation,
    wrapper_directory: &Path,
    expected: &[String],
    run_directory: &Path,
    mode: DeltaMode,
    validate_git: bool,
) -> Result<ValidationReport> {
    let prefix = mode.as_str();
    let heads_path = run_directory.join(format!("{prefix}-heads.txt"));
    let request_path = run_directory.join(format!("{prefix}-request.json"));
    let output_path = run_directory.join(format!("{prefix}.pack"));
    let stdout_path = run_directory.join(format!("{prefix}-wrapper.stdout"));
    let stderr_path = run_directory.join(format!("{prefix}-wrapper.stderr"));
    let metrics_path = run_directory.join(format!("{prefix}-wrapper-metrics.json"));
    if output_path.exists() {
        fs::remove_file(&output_path)
            .with_context(|| format!("could not remove old output {}", output_path.display()))?;
    }
    if metrics_path.exists() {
        fs::remove_file(&metrics_path)
            .with_context(|| format!("could not remove old metrics {}", metrics_path.display()))?;
    }

    fs::write(&heads_path, manifest.heads.join("\n") + "\n")?;
    let request = PackRequest {
        version: 1,
        object_format: manifest.object_format.clone(),
        delta_compression: mode,
    };
    fs::write(&request_path, serde_json::to_vec_pretty(&request)?)?;

    let repository = fixture_path.join("repository.git").canonicalize()?;
    let heads_path = heads_path.canonicalize()?;
    let request_path = request_path.canonicalize()?;
    let output_path = absolute_output_path(&output_path)?;
    let metrics_path = absolute_output_path(&metrics_path)?;
    let stdout = File::create(&stdout_path)?;
    let stderr = File::create(&stderr_path)?;

    let executable = implementation
        .command
        .first()
        .context("implementation command is empty")?;
    let mut command = Command::new(executable);
    command
        .args(&implementation.command[1..])
        .current_dir(wrapper_directory)
        .env("PACKTEST_REPO_PATH", repository)
        .env("PACKTEST_HEADS_PATH", &heads_path)
        .env("PACKTEST_REQUEST_PATH", &request_path)
        .env("PACKTEST_OUTPUT_PATH", &output_path)
        .env("PACKTEST_METRICS_PATH", &metrics_path)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    let started = Instant::now();
    let mut child = command
        .spawn()
        .with_context(|| format!("could not start implementation wrapper {executable}"))?;
    let timeout = Duration::from_secs(implementation.timeout_seconds);
    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            child.kill().context("could not kill timed-out wrapper")?;
            child.wait()?;
            bail!(
                "implementation wrapper timed out after {} seconds; stderr: {}",
                implementation.timeout_seconds,
                read_diagnostic(&stderr_path)
            );
        }
    };
    if !status.success() {
        bail!(
            "implementation wrapper failed in {} mode (status {}); stderr: {}",
            mode.as_str(),
            status,
            read_diagnostic(&stderr_path)
        );
    }
    ensure!(
        output_path.is_file(),
        "implementation wrapper succeeded but did not create {}",
        output_path.display()
    );
    let elapsed = started.elapsed();
    let wrapper_metrics = if metrics_path.exists() {
        serde_json::from_slice(&fs::read(&metrics_path).with_context(|| {
            format!("could not read wrapper metrics {}", metrics_path.display())
        })?)
        .with_context(|| format!("invalid wrapper metrics {}", metrics_path.display()))?
    } else {
        WrapperMetrics::default()
    };

    let stats = pack::scan(&output_path)?;
    let elapsed_seconds = elapsed.as_secs_f64().max(f64::EPSILON);
    let performance = PerformanceMetrics {
        generation_microseconds: elapsed.as_micros(),
        pack_bytes_per_second: stats.pack_bytes as f64 / elapsed_seconds,
        objects_per_second: stats.object_count as f64 / elapsed_seconds,
        peak_memory_bytes: wrapper_metrics.peak_memory_bytes,
    };
    if mode == DeltaMode::Disabled {
        ensure!(
            stats.delta_count() == 0,
            "delta-disabled pack contains {} delta objects",
            stats.delta_count()
        );
    }
    if mode == DeltaMode::Enabled && manifest.expectations.delta_required {
        ensure!(
            stats.delta_count() > 0,
            "fixture requires delta compression, but generated pack has no deltas"
        );
    }

    let index = validation::index_with_libgit2(&output_path)?;
    ensure!(
        index.object_ids.len() == stats.object_count as usize,
        "pack header declares {} objects, libgit2 indexed {}",
        stats.object_count,
        index.object_ids.len()
    );
    validation::compare_object_sets(expected, &index.object_ids)?;
    if validate_git {
        validation::validate_with_git(&output_path)?;
    }

    Ok(ValidationReport {
        mode,
        pack_path: output_path,
        generation_milliseconds: elapsed.as_millis(),
        indexed_object_count: index.object_ids.len(),
        canonical_git_validated: validate_git,
        performance,
        libgit2: index.metrics,
        pack: stats,
    })
}

fn compare_modes(
    manifest: &FixtureManifest,
    reports: &[ValidationReport],
) -> Result<Option<Comparison>> {
    let enabled = reports
        .iter()
        .find(|report| report.mode == DeltaMode::Enabled);
    let disabled = reports
        .iter()
        .find(|report| report.mode == DeltaMode::Disabled);
    let (Some(enabled), Some(disabled)) = (enabled, disabled) else {
        return Ok(None);
    };

    let ratio = enabled.pack.pack_bytes as f64 / disabled.pack.pack_bytes as f64;
    if let Some(maximum) = manifest.expectations.max_delta_ratio {
        ensure!(
            ratio <= maximum,
            "delta pack size ratio {ratio:.3} exceeds fixture maximum {maximum:.3}"
        );
    }
    Ok(Some(Comparison {
        enabled_to_disabled_size_ratio: ratio,
        bytes_saved: disabled
            .pack
            .pack_bytes
            .saturating_sub(enabled.pack.pack_bytes),
    }))
}

fn validate_manifest(manifest: &FixtureManifest) -> Result<()> {
    ensure!(
        manifest.version == 1,
        "unsupported fixture manifest version"
    );
    ensure!(
        manifest.object_format == "sha1",
        "milestone 1 supports only SHA-1 fixtures"
    );
    ensure!(!manifest.heads.is_empty(), "fixture has no heads");
    for head in &manifest.heads {
        ensure!(
            head.len() == 40 && head.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "fixture contains invalid SHA-1 head {head}"
        );
    }
    Ok(())
}

fn validate_implementation(
    implementation: &Implementation,
    manifest: &FixtureManifest,
    delta: DeltaSelection,
) -> Result<()> {
    ensure!(
        !implementation.command.is_empty(),
        "implementation command is empty"
    );
    if manifest.object_format == "sha1" {
        ensure!(
            implementation.capabilities.sha1,
            "implementation does not advertise SHA-1 support"
        );
    }
    if matches!(delta, DeltaSelection::Disabled | DeltaSelection::Both) {
        ensure!(
            implementation.capabilities.delta_disable,
            "implementation does not advertise delta-disable support"
        );
    }
    Ok(())
}

fn absolute_output_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .context("output path has no parent")?
        .canonicalize()?;
    let name = path.file_name().context("output path has no file name")?;
    Ok(parent.join(name))
}

fn read_diagnostic(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|_| "<could not read diagnostic output>".to_owned())
        .trim()
        .to_owned()
}

fn print_run_report(report: &RunReport) {
    println!(
        "fixture: {}  implementation: {}",
        report.fixture, report.implementation
    );
    for run in &report.runs {
        println!(
            "  {:8} {:>7} bytes  {:>5} objects  {:>5} deltas \
             (ofs {}, ref {})  generation {} ms",
            run.mode.as_str(),
            run.pack.pack_bytes,
            run.pack.object_count,
            run.pack.delta_count(),
            run.pack.ofs_deltas,
            run.pack.ref_deltas,
            run.generation_milliseconds
        );
        println!(
            "           libgit2: ingest {:.3} ms  finalize {:.3} ms  resolve {:.3} ms  \
             logical/pack {:.2}x",
            run.libgit2.ingest_microseconds as f64 / 1_000.0,
            run.libgit2.finalize_microseconds as f64 / 1_000.0,
            run.libgit2.object_resolution_microseconds as f64 / 1_000.0,
            run.libgit2.logical_to_pack_size_ratio
        );
        println!(
            "           throughput: {:.2} MiB/s  {:.0} objects/s  peak memory {}",
            run.performance.pack_bytes_per_second / (1024.0 * 1024.0),
            run.performance.objects_per_second,
            run.performance
                .peak_memory_bytes
                .map(|bytes| format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0)))
                .unwrap_or_else(|| "not reported".to_owned())
        );
    }
    if let Some(comparison) = &report.comparison {
        println!(
            "  enabled/disabled size ratio: {:.3} ({} bytes saved)",
            comparison.enabled_to_disabled_size_ratio, comparison.bytes_saved
        );
    }
}

fn print_inspect_report(report: &InspectReport) {
    println!("pack: {}", report.pack_path.display());
    println!(
        "  version {}  {} bytes  {} objects",
        report.pack.version, report.pack.pack_bytes, report.pack.object_count
    );
    println!(
        "  base entries: commits {}  trees {}  blobs {}  tags {}; deltas: ofs {}  ref {}",
        report.pack.base_commit_objects,
        report.pack.base_tree_objects,
        report.pack.base_blob_objects,
        report.pack.base_tag_objects,
        report.pack.ofs_deltas,
        report.pack.ref_deltas
    );
    println!(
        "  libgit2: {} deltas  ingest {:.3} ms  finalize {:.3} ms  resolve {:.3} ms; \
         {} logical bytes ({:.2}x pack size)",
        report.libgit2.total_deltas,
        report.libgit2.ingest_microseconds as f64 / 1_000.0,
        report.libgit2.finalize_microseconds as f64 / 1_000.0,
        report.libgit2.object_resolution_microseconds as f64 / 1_000.0,
        report.libgit2.logical_object_bytes,
        report.libgit2.logical_to_pack_size_ratio
    );
}

fn write_json<T: Serialize>(path: Option<&Path>, report: &T) -> Result<()> {
    if let Some(path) = path {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(report)?)
            .with_context(|| format!("could not write JSON report {}", path.display()))?;
    }
    Ok(())
}
