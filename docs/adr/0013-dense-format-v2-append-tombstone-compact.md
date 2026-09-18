# ADR-0013: Dense on-disk format version 2 — append-only rows, tombstones, explicit compaction

- **Status**: Accepted — 2026-09-18 (the repository owner chose the format's shape: version 1 is dropped and every artefact regenerated, Q1 = C; compaction on `merge` and on a configurable dead-row share, Q2 = C)
- **Date**: 2026-09-18
- **Deciders**: mirth (repository owner), 2026-09-18
- **Spec**: [024-incremental-dense-commits](../../specs/024-incremental-dense-commits/spec.md) FR-001–FR-013
- **Amends**: [ADR-0007](0007-unsafe-readonly-mmap-in-dense.md) condition 2 (the wording of what the crate never does to a mapped file)
- **Blocks**: Principle V row in [plan.md](../../specs/024-incremental-dense-commits/plan.md)

## Context

Principle V: "the on-disk format … unchanged — or the change has human review plus an ADR".
Feature 004 defined the dense stage's format version 1: one `dense/index.bin` holding a JSON
header and three columns — `ids[n]`, `norms[n]`, `vectors[n × dim]`. Because the columns are
contiguous sections, nothing can be appended: `FlatIndex::commit` rebuilt the whole file
from the committed rows plus the pending changes and renamed it into place. Crash-safe and
simple, and O(index) per commit — adding ten passages to the full Simple English Wikipedia
index (427,947 rows, ~660 MB of vectors) rewrote ~660 MB. That rules the engine out for
on-device ingestion of a user's documents as they arrive, which is what the stage exists for.

Alternatives to a format of our own were surveyed (Principle I): tantivy's bytes fast fields
(dictionary-encoded, per segment — no contiguous scan), `usearch` / `hnsw_rs` / `arroy`
(approximate — results change; whole-index saves; C or LMDB dependencies), `lance`
(append-only but `tokio`, Arrow and `object_store`), `parquet` / `arrow` (files are not
appendable — one file per commit, i.e. segments anyway), `redb` / `sled` / `heed` (page
B-trees — no contiguous scan), `vecstore` 0.1.0 (`DiskLayout::save_all` rewrites
`vectors.bin`, as JSON, on every save; HNSW; alpha). None offers an exact, contiguous,
memory-mappable *and* appendable f32 matrix. A flat f32 file is not on the principle's
"do not build" list (inverted index, ANN graph, tensor runtime); what can be reused is
reused: `roaring` for the tombstone set, `memmap2` for the mapping, `serde_json` for the
header.

## Decision

**Dense format version 2** (`xtriever_dense::FORMAT_VERSION = 2`):

```text
dense/
├── manifest.bin        magic "XTDENSE2" · hdr_len u64 · JSON header · roaring tombstones
└── vectors.<g>.bin     rows in commit order: id u32 · norm f32 · vector dim × f32
```

- The manifest is the truth. Its header carries `dim`, `metric`, `fingerprint`, the row
  file's `generation`, the committed `rows`, the `live` count and the tombstone bytes'
  length; the tombstone set (dead row indices — deleted, or superseded by a replacement) is a
  `roaring` bitmap. It is written to `manifest.bin.tmp`, synced and renamed.
- **`commit`** appends the pending rows to the current row file (sync), then replaces the
  manifest with `rows += appended` and the tombstones of the rows the changes superseded or
  deleted. No committed byte is modified. Cost: O(change) plus the manifest (tens of KB at
  most — the bitmap is compressed).
- **`compact`** (inherent on `FlatIndex`) writes the live rows in ascending id order to
  `vectors.<g+1>.bin`, replaces the manifest (`generation g+1`, no tombstones), and removes
  the old file. The pipeline's `merge` calls it; `commit` calls it when the configured
  dead-row share (`HybridConfig::dense_compact_dead_share`, default `None`) is exceeded.
- **Crash safety**: a crash before the manifest rename leaves the previous manifest, so the
  previous state; a partial append's tail beyond the committed rows is ignored and cut at the
  next open (best effort — a read-only directory keeps it, and only committed rows are read);
  a stale generation or manifest temporary is swept the same way.
- **Results are unchanged by construction**: the scan visits every live row, each row's score
  is computed independently in `f64` and rounded once, and the final order is total
  (`score DESC, id ASC`), so the arrangement of rows in the file cannot change a bit. The
  version-1 oracle (`tests/support/v1_oracle.json`, minted on the version-1 implementation)
  and the 004 goldens assert it.
- **Version 1 is not read.** A directory holding `index.bin` is refused at open with the
  existing `Corrupt` version error naming both versions. Every artefact the repository
  relies on is regenerated in version 2 with unchanged vectors: the fixture index (its
  committed `expected.json` goldens must still pass — the proof), the shipped Wikipedia
  artefact (its host goldens, 800/800 bits), the demo slices as needed.

### The memory-mapping invariant, amended

ADR-0007 condition 2 read: the crate never writes `index.bin` in place, it only replaces it
by `rename`. Version 2 extends that sentence rather than weakening the guarantee:

> A mapped byte is never modified or truncated by this crate. The row file is only
> **extended** beyond every live mapping's end (`commit`) or **replaced** by `rename`
> (`compact`). The one truncation the crate performs — cutting a crashed append's tail —
> happens at open, before the opening handle maps anything, and only on bytes beyond every
> manifest's committed length.

The `SAFETY` comment in `bytes::map_readonly` and the docs of `LoadPath::Mmap` state this;
the single-writer precondition the caller owns is unchanged.

## Consequences

- A 10-row commit into a 100k-row index writes the ten rows plus a manifest under 1 KB
  instead of ~150 MB (the bench in `crates/xtriever-dense/benches/scan.rs`; numbers in the
  spec's `runs/`).
- Dead rows cost scan time until a compaction; the default leaves compaction to `merge`
  (predictable on a phone), the threshold makes it automatic for callers who prefer that.
- Rows are interleaved (`id · norm · vector`) rather than columnar: one append per commit,
  one length to reason about, one stream for the unfiltered scan. A filtered scan reads the
  id at a row's head and skips the rest by offset.
- `FlatIndex` keeps an in-memory id → live row table (`Vec<u32>`, 4 bytes per id) built at
  open by one pass over the rows; `vector(id)` and the superseded-row detection use it.
- The pipeline's descriptor gains `dense_compact_dead_share` with a serde default; the
  pipeline format version is unchanged (as `rerank_mode` in Feature 015). The FFI
  `IndexConfig` gains the same optional field with a uniffi default.
- Existing version-1 indexes must be rebuilt (or converted once; the Wikipedia artefact was
  converted by `reference/convert_dense_v1_to_v2.py`, a record of the step rather than a
  supported tool).

## Alternatives considered

- **Segments per commit** (a row file per commit and a manifest listing them): the same
  outcome with more machinery — per-segment tombstones, many small mappings after many small
  commits. Rejected (Agent Operating Rule 7).
- **Read version 1 and convert on the first writable commit**: rejected by the owner (Q1 = C)
  — one format, one reader.
- **Automatic compaction only**: rejected by the owner (Q2 = C) — the explicit `merge` stays,
  the threshold is opt-in.
- **Tombstones in a separate generation-named file**: the manifest already needs a rename
  per commit; embedding the bitmap in it removes a crash window in which a new tombstone file
  paired with an old manifest would hide replaced documents.
- **Cutting a crashed tail lazily at the next append**: would overwrite bytes a read-only
  mapping opened in between could cover; cutting at open, before the handle maps, keeps the
  invariant simple.

## Revisit when

- An index outgrows a linear scan on device: the answer is an ANN crate behind its own ADR
  (`usearch` was the candidate in the survey), not a change to this format.
- Multi-writer access is ever needed: this format, like version 1, assumes one writer.
