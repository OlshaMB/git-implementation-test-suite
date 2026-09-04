# Corpus configuration

Repository-scale tests are defined independently from the acquisition and
fixture-generation code. A V1 corpus TOML file has this shape:

```toml
version = 1

[corpus]
name = "linux-v6.12"
url = "https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git"
ref = "refs/tags/v6.12"
commit = "adc218676eef25575469234709c2d87185ca223a"

[sample]
seed = "packtest-linux-v6.12-v1"
blobs = 2048

[expectations]
sample_delta_required = false
full_delta_required = true
```

`ref` tells acquisition what to fetch. After fetching, it must peel to the exact
full SHA-1 in `commit`; a moved or incorrectly configured ref is rejected.

`sample.seed` versions deterministic selection. Reachable blobs are ranked by
the SHA-256 of the UTF-8 seed followed by their binary object ID. The first
`sample.blobs` objects are copied into a standalone fixture. Changing either
field intentionally defines a different workload.

The optional `[expectations]` table controls whether enabled packs from each
generated fixture must contain at least one delta. Both settings default to
`false`, allowing the same machinery to describe small or incompressible
repositories without Linux-specific assumptions.

The same configuration can produce a full fixture. Full fixtures symlink their
external bare cache and therefore remain local benchmark inputs rather than
portable fixture archives.

Use any V1 definition with the generic command:

```sh
python3 scripts/prepare_corpus.py --config path/to/corpus.toml acquire
python3 scripts/prepare_corpus.py --config path/to/corpus.toml sample \
  --output target/fixtures/sample
python3 scripts/prepare_corpus.py --config path/to/corpus.toml full \
  --output target/fixtures/full
```
