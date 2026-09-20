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
  file's `generation`, the committed `rows`, the `live` count, whether the row ids are
  `ordered` (strictly ascending) and the tombstone bytes' length; the tombstone set (dead
  row indices — deleted, or superseded by a replacement) is a `roaring` bitmap embedded after
  the header. It is written to `manifest.bin.tmp`, synced and renamed through the workspace's
  one atomic-write definition (`xtriever_core::fs`), the directory handle for the post-rename
  sync opened *before* the rename.
- **`commit`** appends the pending rows to the current row file (sync), then replaces the
  manifest with `rows += appended` and the tombstones of the rows the changes superseded or
  deleted. No committed byte is modified. Cost: O(change) plus the manifest (tens of KB at
  most — the bitmap is compressed).
- **`compact`** (inherent on `FlatIndex`) streams the live rows — pending changes folded in,
  a kept row copied as its committed bytes — in ascending id order to `vectors.<g+1>.bin`,
  replaces the manifest (`generation g+1`, no tombstones, `ordered`), and removes the old
  file; an index larger than memory compacts on the mapped path because nothing is
  materialised on the heap beyond one row. The pipeline's `merge` will call it (PR B). When
  the configured dead-row share (`HybridConfig::dense_compact_dead_share`, PR B, default
  `None`) would be exceeded,
  `commit` *is* this rewrite rather than an append: one protocol with one rename, so a commit
  fails whole or succeeds whole — never a durable append followed by a compaction that fails
  on its own.
- **Manifest failures are two kinds**: before the rename nothing on disk changed and the
  handle rolls back (the appended rows cut, or the new generation removed; pending kept);
  after it — only an `fsync` failure on the directory handle opened before the rename, so an
  unopenable directory fails *before* the switch — the manifest is the new one, the handle
  adopts that state, the error names it as a durability-unconfirmed success, and no later
  `commit` or `compact` on that handle succeeds until a sync has (`is_sync_pending`). The
  obligation is per handle: a dropped handle drops it. The pipeline's `commit` treats that
  outcome as switched — it finishes its own protocol so the directory is consistent, returns
  the error, and its next `commit` retries the sync through the stage's empty commit.
- **One writer at a time, checked**: a writable handle verifies at every `commit` and
  `compact` — no-ops included — that the manifest on disk is the one it last saw (generation,
  rows, live count and tombstone set) and refuses with `Corrupt` otherwise, so a stale handle
  cannot cut another writer's rows in place, resurrect its deletes or silently undo its
  commit. The no-concurrent-writer precondition of every open is still the caller's.
- **`ordered` is trusted by readers, verified by writers**: a read-only open takes the
  manifest's word for it like every other field (nothing beyond the manifest is read — the
  point of the flag); a writable handle scans the ids once before its first write and
  refuses (`Corrupt`) to build on a manifest that lied. A lying flag is corruption of the
  same class as a lying row count; the format carries no checksums.
- **Read-only opens** (`open_read_only*`, the pipeline's `OpenOptions { read_only }`) alter
  nothing — no tail cut, no sweep — and refuse every mutation with the workspace's one
  read-only error (`Error::read_only`).
- **No id table for the shipped case**: an `ordered`, tombstone-free generation answers
  `vector(id)` by binary search over the row ids, so a mapped read-only open touches nothing
  beyond the manifest — the id → row map (`BTreeMap`, one entry per live row) exists only
  when a file has tombstones or out-of-order ids.
- **Crash safety**: a crash before the manifest rename leaves the previous manifest, so the
  previous state (any byte boundary — the test enumerates them). On the Unix targets the
  engine ships to (macOS, iOS, Android, Linux) the directory is also fsynced after a new row
  file is created and after every manifest rename, so a power loss cannot keep a manifest
  naming a row file whose entry never reached disk; on other targets (Windows is a CI check
  only) that ordering is the filesystem's, not the crate's; a partial append's tail beyond the committed rows is ignored and cut at the
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

> A mapped byte is never modified or truncated by this crate. A row file is mapped over
> exactly its committed rows (the manifest's `rows × row_bytes`, never the file's length), and
> the crate only ever **extends** it past that committed length (`commit`) or **replaces** it
> by `rename` (`compact`). The truncations it performs — cutting a crashed append's tail at
> open or before an append — touch only bytes beyond the committed length, which no mapping
> covers.

The `SAFETY` comment in `bytes::map_readonly` and the docs of `LoadPath::Mmap` state this;
the single-writer precondition the caller owns is unchanged.

## Consequences

- An append never reallocates or copies the matrix already in memory: on the buffered path
  the committed rows stay in the buffer they were read into and the appended rows live in a
  separate in-memory tail (every row lies wholly in one segment — the tail starts at a row
  boundary), folded back at the next reopen or compaction; the mapped path re-maps the
  committed prefix (a mapping is lazy). The first commit after an open costs the same as any
  other (the bench's cold case).
- A 10-row commit into a 100k-row index writes the ten rows plus a manifest under 1 KB
  instead of the `rows × (8 + 4·dim)` bytes a version-1 rewrite wrote (154 MB at 100k × 384 —
  arithmetic, not a measurement; the bench in `crates/xtriever-dense/benches/scan.rs`
  measures the version-2 append and compares the scan with the version-1 shape; numbers in
  the spec's `runs/`).
- Dead rows cost scan time until a compaction; the default leaves compaction to `merge`
  (predictable on a phone), the threshold makes it automatic for callers who prefer that.
- Rows are interleaved (`id · norm · vector`) rather than columnar: one append per commit,
  one length to reason about, one stream for the unfiltered scan. A filtered scan reads the
  id at a row's head and skips the rest by offset.
- `FlatIndex` keeps an id → live row map (`BTreeMap<u32, u32>`, one entry per live row)
  only for a generation with tombstones or out-of-order ids, built by one pass over the rows
  at open (or, for a writable handle, at the first commit that creates a tombstone); an
  ordered, tombstone-free generation — every shipped or freshly compacted index — needs none
  and looks ids up by binary search. A map rather than a table indexed by id, because `add`
  accepts any `u32`: memory follows the row count, not the largest id (~15 MB for the
  Wikipedia index's 428k rows when one is needed; a sparse `u32::MAX` costs one entry).
- The pipeline's descriptor gains `dense_compact_dead_share` with a serde default; the
  pipeline format version is unchanged (as `rerank_mode` in Feature 015). The FFI
  `IndexConfig` gains the same optional field with a uniffi default.
- The oracle that pins version 2 to version 1's results bit for bit
  (`crates/xtriever-dense/tests/support/v1_oracle.json`) is reproducible from
  `reference/gen_024_fixtures.py`, which recomputes every expectation from the contract's
  arithmetic in Python (Principle II). *Superseded by Feature 026 (ADR-0015): format 3 changes
  the scores by design, so that oracle and its generator are gone; the format-3 oracle
  (`v3_oracle.json`) is written and checked by `reference/gen_026_fixtures.py` from
  `reference/dense_format3.py`.*
- An observation from PR B's tests, about the *lexical* stage: the backend's BM25
  statistics are deletion-inclusive until a merge physically drops the deleted or replaced
  documents (Feature 002 FR-025), so any `merge` that drops documents — after plain deletes
  or replacements, whenever the index holds more than one segment — moves BM25 and hence
  fused bits; a single-segment merge drops nothing and keeps every bit. (PR B's first
  reading, "replacements move bits, plain deletes do not", was an artefact of probing the
  deletes on a one-segment fixture.) Pinned by the lexical crate's
  `merge_after_deletes_moves_bm25_bits_only_when_it_drops_documents` (one batch → every bit
  kept; three batches → bits move), the only lexical change on the branch: pre-existing
  behaviour, not something this format changes. The dense stage's scores are bit-identical
  across a compaction by construction and the tests compare every live row's score; spec
  FR-005 was revised to say exactly this, and the lexical behaviour is left for a lexical
  spec.
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
