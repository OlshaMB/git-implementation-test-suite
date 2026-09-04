# Wrapper contract

An implementation wrapper is an executable configured by an implementation
TOML file. The runner starts it with no packtest-specific positional arguments.

## Environment

The runner sets five absolute paths:

- `PACKTEST_REPO_PATH`: existing bare input repository;
- `PACKTEST_HEADS_PATH`: text file containing one full commit ID per line;
- `PACKTEST_REQUEST_PATH`: JSON request;
- `PACKTEST_OUTPUT_PATH`: destination for the raw pack;
- `PACKTEST_METRICS_PATH`: optional destination for producer metrics.

The runner creates the output parent and ensures that the output does not exist
before invocation.

After publishing a pack, a wrapper may atomically write a JSON object to
`PACKTEST_METRICS_PATH`. The only V1 metric is `peakMemoryBytes`, the peak
resident memory used while generating the pack. Wrappers that cannot measure it
may omit the file or the field.

## Request V1

```json
{
  "version": 1,
  "objectFormat": "sha1",
  "deltaCompression": "enabled"
}
```

`objectFormat` is currently `sha1`. `deltaCompression` is `enabled` or
`disabled`.

## Required behavior

- Treat the repository as read-only.
- Resolve every supplied ID as a commit.
- Pack the complete commit/tree/blob closure of all supplied heads.
- Do not traverse gitlinks.
- Do not include unrelated or unreachable objects.
- Produce a complete, non-thin pack.
- Write only raw pack data to the requested output.
- Exit zero only after the output has been completely published.
- Exit nonzero on unsupported requests or generation failures.

Wrappers should write a temporary sibling and atomically rename it to the output
path. Stdout and stderr are captured only as diagnostics.

The wrapper must not use a different pack implementation as a fallback or repair
the implementation's output.
