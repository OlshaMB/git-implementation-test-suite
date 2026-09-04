# Fixture format

A generated fixture directory contains:

```text
repository.git/
manifest.json
expected-objects.txt
```

`manifest.json` has this V1 shape:

```json
{
  "version": 1,
  "name": "linear-text-history",
  "objectFormat": "sha1",
  "heads": ["<full commit ID>"],
  "expectedObjects": "expected-objects.txt",
  "expectations": {
    "deltaRequired": true,
    "maxDeltaRatio": 0.75
  }
}
```

Paths are relative to the fixture directory. `expected-objects.txt` is a
sorted, newline-delimited set of every object reachable from the heads.

Fixture repositories may contain deliberately unreachable objects. They are not
listed in `expected-objects.txt` and must not appear in generated packs.
