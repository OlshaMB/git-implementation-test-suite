# Git pack implementation test suite

## Goal

Build a language-independent test harness for Git pack generators. A test gives
an implementation a bare repository and one or more head commit IDs. The
implementation writes a complete pack containing exactly the objects reachable
from those heads.

The first scope is pack generation and delta compression. Revision negotiation,
thin packs, fetch, push, and other Git protocol behavior are deliberately
deferred.

## Test boundary

The Rust runner invokes an implementation-specific wrapper with these
environment variables:

| Variable | Meaning |
| --- | --- |
| `PACKTEST_REPO_PATH` | Absolute path to the input bare repository |
| `PACKTEST_HEADS_PATH` | Absolute path to a newline-delimited file of full commit IDs |
| `PACKTEST_REQUEST_PATH` | Absolute path to the versioned JSON request |
| `PACKTEST_OUTPUT_PATH` | Absolute path where the wrapper must atomically publish a raw pack |

Heads are full object IDs rather than ref names. This keeps symbolic-ref and ref
resolution behavior outside the pack test while still making the implementation
perform its own object graph walk.

For each head, the output contains its commit, all parent commits, and all trees
and blobs recursively reachable from those commits. Gitlinks are not traversed.
Unrelated refs, reflog-only objects, and dangling objects are not included.
Duplicate reachability does not duplicate objects in the pack.

The V1 request is:

```json
{
  "version": 1,
  "objectFormat": "sha1",
  "deltaCompression": "enabled"
}
```

`deltaCompression` is either `enabled` or `disabled`. More controls, such as
window size and maximum depth, will be added only with tests that need them.

The output is a complete, non-thin pack. The wrapper must not mutate the input
repository, use another packer as a fallback, add objects, or omit reachable
objects.

## Architecture

### Python fixture and corpus tools

Python creates deterministic bare repositories without using the implementation
under test. A fixture contains:

- `repository.git/`;
- `manifest.json`, including its heads and object format;
- `expected-objects.txt`, the independently known reachable object set.

Small fixtures use a standard-library loose-object writer. External corpus
preparation may use canonical Git to enumerate objects from a pinned commit.

### Rust `packtest` CLI

Rust owns:

- wrapper configuration and execution;
- timeout and process diagnostics;
- direct structural pack scanning;
- pack checksum and entry-size validation;
- libgit2 indexing, delta resolution, and object-ID extraction;
- exact comparison with the expected reachable set;
- canonical `git index-pack --strict` interoperability validation;
- human-readable and JSON reporting.

Libgit2 is intentionally reused instead of implementing a second complete pack
and delta engine in the first milestone. A small direct scanner still reports
base objects, `OFS_DELTA` objects, and `REF_DELTA` objects and verifies the pack
container independently.

This is an interoperability suite, not an independent formal specification.
When libgit2 itself is the implementation under test, another resolving oracle
will be required to avoid circular validation.

### Implementation wrappers

Wrappers translate the stable environment contract to an implementation's CLI
or API. They may be written in any language, but must remain thin: they cannot
repair, normalize, or regenerate output through canonical Git.

## Validation

Every run checks:

1. wrapper exit status and output existence;
2. `PACK` signature, supported version, and declared object count;
3. each entry header and zlib stream;
4. SHA-1 trailer checksum;
5. exact end of object data with no unparsed or trailing bytes;
6. libgit2 acceptance, indexing, and delta resolution;
7. exact equality between indexed IDs and the expected reachable IDs;
8. canonical Git acceptance through `git index-pack --strict`.

Delta-disabled runs must contain no delta entries. Fixtures marked as requiring
deltas must contain deltas in enabled mode. Running both modes also verifies
that both packs contain the same object set and records their size ratio.

Exact pack bytes, object order, chosen bases, and compressed streams are not
compared. These are valid implementation choices.

## Fixtures

### Milestone 1

- `tiny-mixed`: basic commits, trees, text, binary, executable, symlink, and an
  unreachable object that must not be packed.
- `linear-text-history`: many similar revisions of a moderately large text
  file, designed to require useful delta compression.

### Milestone 2

- `branching-history`: three branches evolving the same large source from a
  shared root.
- `competing-bases`: many closely related blobs offering several plausible
  delta bases.
- `depth-pressure`: a long sequence of small edits intended to encourage delta
  chains.
- `delta-boundaries`: paired blobs immediately below, at, and above important
  variable-length and copy-size boundaries.

### Subsequent fixtures

- overlapping and redundant heads;
- deterministic incompressible data;
- malformed or missing source objects;
- SHA-256 repositories.

## Large corpora

The Linux repository is opt-in and cached outside normal test artifacts. Its
manifest pins a full commit ID; tests never target a moving branch.

Two modes are planned:

- a deterministic object/history sample for routine testing;
- the full closure of the pinned commit for scheduled or local stress testing.

The suite will not silently download a large repository. Corpus acquisition is
an explicit Python command.

## Milestones

### 1. End-to-end pack validation

- Write this plan and the wrapper contract.
- Scaffold the Rust `run` and `inspect` commands.
- Generate `tiny-mixed` and `linear-text-history` in Python.
- Add a canonical Git example wrapper.
- Scan pack structure and checksums in Rust.
- Validate and index packs with libgit2.
- Compare the indexed and expected object sets.
- Validate with canonical Git.
- Compare delta-enabled and delta-disabled results.
- Add automated tests and run the full local workflow.

### 2. Delta coverage

- Add branching, competing-base, depth-pressure, and boundary fixtures.
- Add requested delta-base encoding and maximum-depth controls.
- Keep libgit2 as the authority for delta resolution and pack correctness.
- Record libgit2 indexing progress, indexing/finalization time, full-object
  resolution time, and resolved logical object bytes as the first efficiency
  metrics.
- Combine raw pack base references with generated-index object offsets to report
  chain depth, depth distribution, chain roots, and base reuse without
  implementing a second delta resolver.
- Enforce maximum depth only when the request specifies a limit.
- Compare final pack size, delta savings, generation time, and resolution time
  across implementations on identical fixtures and settings.
- Keep exact base selection, object ordering, and chain shape as implementation
  choices rather than correctness requirements.

### 3. Corpus and performance

- Define repositories through versioned corpus TOML files; explicitly acquire
  Linux `v6.12` into an external cache and verify its peeled commit against a
  fixed object ID.
- Deterministically hash-rank reachable blobs into a standalone routine sample;
  expose the complete pinned closure as an opt-in, cache-backed fixture.
- Keep expected and indexed object IDs sorted and compare them with a linear
  merge, avoiding tree-set duplication for full Linux history.
- Record generation time, byte and object throughput, wrapper-reported peak
  memory, and the existing libgit2 consumer timings.
- Compare reports from identical fixtures and implementations against explicit,
  host-specific regression baselines with independently configurable limits.

### 4. Protocol

- Add thin-pack validation.
- Add revision negotiation and wants/haves.
- Add local and network fetch/push interoperability.

## Completion criteria for milestone 1

Milestone 1 is complete when the included Git wrapper produces enabled and
disabled packs for both fixtures, Rust and libgit2 validate them, canonical Git
accepts them, reachable sets match exactly, and the linear-history enabled pack
contains deltas and is smaller than its disabled counterpart.

## Completion criteria for milestone 3

Milestone 3 is complete when corpus acquisition is explicit and verifies the
pinned Linux commit, repeated sample generation selects identical objects, the
full fixture can reference the external cache without copying it, large object
sets are compared in linear time, and JSON reports can be checked against a
performance baseline including throughput and peak memory.
