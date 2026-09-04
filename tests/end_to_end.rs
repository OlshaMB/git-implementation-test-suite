use std::fs;
use std::path::Path;
use std::process::Command;

fn run(command: &mut Command, description: &str) {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("could not {description}: {error}"));
    assert!(
        output.status.success(),
        "{description} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn canonical_git_validates_all_included_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temporary = tempfile::tempdir().expect("could not create test directory");
    let fixtures = temporary.path().join("fixtures");

    run(
        Command::new("python3")
            .arg(root.join("scripts/generate_fixtures.py"))
            .args(["--all", "--output"])
            .arg(&fixtures)
            .current_dir(root),
        "generate fixtures",
    );

    for fixture in [
        "branching-history",
        "competing-bases",
        "delta-boundaries",
        "depth-pressure",
        "linear-text-history",
        "tiny-mixed",
    ] {
        let output_directory = temporary.path().join(format!("run-{fixture}"));
        let report = output_directory.join("report.json");
        run(
            Command::new(env!("CARGO_BIN_EXE_packtest"))
                .args(["run", "--fixture"])
                .arg(fixtures.join(fixture))
                .arg("--implementation")
                .arg(root.join("implementations/git.toml"))
                .args(["--delta", "both", "--output-dir"])
                .arg(&output_directory)
                .arg("--json")
                .arg(&report)
                .current_dir(root),
            &format!("validate {fixture}"),
        );

        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(report).expect("missing JSON report"))
                .expect("invalid JSON report");
        assert_eq!(report["fixture"], fixture);
        let runs = report["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2);
        for run in runs {
            assert_eq!(
                run["libgit2"]["receivedBytes"], run["pack"]["packBytes"],
                "libgit2 byte count differs from scanned pack size"
            );
            assert!(
                run["libgit2"]["logicalObjectBytes"].as_u64().unwrap() > 0,
                "libgit2 did not report resolved object bytes"
            );
            assert!(
                run["performance"]["packBytesPerSecond"].as_f64().unwrap() > 0.0,
                "runner did not report generation throughput"
            );
            assert!(
                run["performance"]["peakMemoryBytes"].as_u64().unwrap() > 0,
                "canonical Git wrapper did not report peak memory"
            );
        }
    }
}
