# packtest

`packtest` tests Git pack-generating implementations through a small wrapper
contract. See [PLAN.md](PLAN.md) for the design and roadmap.

## Requirements

- Rust and Cargo
- Python 3.11 or newer
- Git, for the included reference wrapper and interoperability check
- a C build toolchain, used when `git2` builds its bundled libgit2

## Generate fixtures

```sh
python3 scripts/generate_fixtures.py --all --output target/fixtures
```

## Build and run

```sh
cargo build

cargo run -- run \
  --fixture target/fixtures/linear-text-history \
  --implementation implementations/git.toml \
  --delta both
```

Inspect an existing pack without invoking a wrapper:

```sh
cargo run -- inspect path/to/result.pack
```

Use `--json path/to/report.json` with either command for a machine-readable
report.

## Linux corpus and performance

Linux workloads are opt-in. Acquisition is never part of fixture generation or
the test suite:

```sh
python3 scripts/prepare_corpus.py \
  --config corpora/linux-v6.12.toml acquire
python3 scripts/prepare_corpus.py \
  --config corpora/linux-v6.12.toml sample \
  --output target/fixtures/linux-sample
python3 scripts/prepare_corpus.py \
  --config corpora/linux-v6.12.toml full \
  --output target/fixtures/linux-full
```

Corpus TOML files define the repository URL, pinned ref and commit, and routine
sample defaults. The included Linux definition fetches the `v6.12` tag and
refuses to continue unless it peels to the pinned commit
`adc218676eef25575469234709c2d87185ca223a`. Samples use a versioned seed to
hash-rank reachable blobs, then copy the selected objects into a standalone
fixture. Full fixtures intentionally symlink the external cache rather than
duplicating the Linux object database.

Run with `--json` to record generation throughput, wrapper-reported peak memory,
and libgit2 ingestion, finalization, and resolution timings. A report from the
same machine and fixture can serve as a regression baseline:

```sh
python3 scripts/check_performance_baseline.py \
  baselines/linux-sample.json target/current-report.json
```

The checker uses separate tolerances for pack size, generation time, resolution
time, throughput, and peak memory. Run it with `--help` to tune those limits for
the stability of the benchmark host.

## Test

```sh
cargo test
python3 -m unittest discover -s python/tests
```
