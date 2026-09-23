# Contract: the artefact, its staging, and the read-only open

**Feature**: `008-wiki-corpus`

## Layout of `target/xt-wiki/` (and of `XtrieverData/wikipedia/` when staged)

```text
index/                      # a pipeline format-v2 hybrid index, opened unchanged by 007's surface
├── xtriever-pipeline.json  # descriptor (untouched format)
├── lexical/                # one segment after merge
├── dense/manifest.bin           # since Feature 024: manifest + vectors.<g>.bin, not index.bin
├── dense/vectors.<g>.bin        # since Feature 026: dense format 3, eight-bit rows of 396 bytes (was 1,540)
├── passages.bin
├── ids.json
└── corpus.json             # 008 sidecar (identity + counts); the pipeline ignores it
wiki-build.json             # build record (committed copy: specs/008-wiki-corpus/build-record.json)
ATTRIBUTION.txt
queries.json                # the 20 measurement queries (copied from reference/fixtures/008/)
expected.json               # host goldens for the device parity check
```

## What a hit means

- `Hit.text` = `"{title}\n\n{passage body}"`. The app splits at the first `"\n\n"`; the part
  before is the article title, the part after the passage as chunked.
- `Hit.chunk.parent` = the article's page id; `Hit.chunk.ordinal` = 0-based passage index in
  the article; `Hit.chunk.byte_range` = `(start, end)` in the article body (not in `Hit.text`).
- `Hit.external_id` = `"{page_id}#{ordinal}"`.
- Article URL = `"https://simple.wikipedia.org/wiki/"` + `percent_encode(title)` where every
  byte of the title's UTF-8 outside `A–Z a–z 0–9 - _ . ~ /` becomes `%XX` (upper-case hex).
  Verified exact against the snapshot for every article at build time; the Swift package
  exposes it as `Hit.wikipediaURL` (a pure function of `Hit.text`'s first line — no lookup).

## Read-only open (D11)

- `xtriever_pipeline::HybridIndex::open_with(dir, embedder, OpenOptions { mapped, read_only })`.
  With `read_only: true` the lexical backend takes no lock and creates no file; `add`, `commit`,
  `merge` on such an index return `Error::Io` with message `"read-only index"`. `open` and
  `open_mapped` are unchanged and equal `open_with(.., OpenOptions { mapped, read_only: false })`.
- `xtriever_ffi::IndexHandle::open` opens **with the lock** when the directory permits (the
  reader's protection against a concurrent writer's garbage collection stays wherever it can
  exist) and retries `read_only: true` only when the directory refuses the lock file
  (`Error::Io` `PermissionDenied` — an app bundle, a read-only mount). `xtriever-lexical` maps
  that lock failure to `Error::Io` (a busy lock stays `Backend`, 002 D14). Opening an index
  inside a read-only directory therefore succeeds; the 007 "must be writable" caveat and
  `XtrieverIndex.writableCopy` are removed; 007 report F-001 is marked resolved by 008.
- Tests: `xtriever-lexical` opens a `chmod 0o555` copy read-only and searches; mutation errors;
  the directory's file list and mtimes are unchanged after open + search (the 007 snapshot
  assertion, now on a read-only directory). `xtriever-ffi` `readonly.rs` gains the same on a
  read-only directory.

## Staging (`scripts/build-ios-package.sh --with-wiki`)

- Requires `target/xt-wiki/index/xtriever-pipeline.json`; prints the build command if absent.
- Copies the layout above into `swift/Xtriever/Sources/Xtriever/XtrieverData/wikipedia/`.
- Prints the staged size of `XtrieverData` and fails (exit 1) if it exceeds **2,000,000,000
  bytes** — the bundle budget; the number is revisited in the report against the measured
  artefact and, if changed, changed in the script and here together. Measured: 1,076,413,167
  bytes for the format-2 artefact (Feature 008), 585,130,234 bytes for the same 427,947 passages
  in format 3 (Feature 026) — the vectors fell from 660,750,511 to 169,467,012 bytes and the
  other three files did not change.
- `DeviceMeasurementTests` reads `TEST_RUNNER_XTRIEVER_CORPUS` (`scifact` default, `wikipedia`)
  and resolves `XtrieverData/<corpus>/{index,queries.json,expected.json}`; the record carries
  `corpus`, `index.bytes`, `openedInPlace: true`.
