# Research: Incremental Dense Commits

Read on 2026-09-18: `crates/xtriever-dense/src/{lib,bytes}.rs`, `src/index/{format,mod,search}.rs`,
the dense tests (`index_golden`, `index_mutation`, `index_persist`, `index_prop`,
`index_errors`, `support/`), `crates/xtriever-pipeline/src/{index,ids,descriptor,types}.rs`,
`crates/xtriever-ffi/src/ffi/types.rs`, `crates/xtriever-ffi/examples/fixture_index.rs`,
ADR-0007 / ADR-0008, `roaring` 0.10.12 (`src/bitmap/{inherent,serialization}.rs`),
`memmap2` 0.9 as used; the survey of alternatives from the conversation (`vecstore`'s
`Cargo.toml` and `src/store/disk.rs` at `main`).

## D1 — Why a new format, and why not a crate

Version 1 (`index.bin`) is **columnar**: `ids[n]`, then `norms[n]`, then `vectors[n×dim]`
as three contiguous sections after a JSON header. Nothing can be appended to that without
rewriting the sections, which is exactly what `commit` does today (`committed ⊕ pending` →
`index.bin.tmp` → rename). The fix must change the layout.

Alternatives surveyed (Principle I): tantivy bytes fast fields (dictionary-encoded,
per-segment, non-contiguous), `usearch` / `hnsw_rs` / `arroy` (approximate — change results;
whole-index saves; C or LMDB deps), `lance` (append-only but `tokio` + Arrow + object_store),
`parquet`/`arrow` (files not appendable → segments anyway), `redb` / `sled` / `heed` (page
B-trees — no contiguous scan), `vecstore` 0.1.0 (`DiskLayout::save_all` rewrites
`vectors.bin` as JSON via `atomic_write`; HNSW; alpha). None gives an exact, contiguous,
mappable, appendable f32 matrix. What is reused: `roaring` (the tombstones), `memmap2`
(the mapping), `serde_json` (the header).

## D2 — Files: `manifest.bin` and `vectors.<generation>.bin`

- **`vectors.<g>.bin`**: fixed-size rows appended in commit order:
  `id u32 LE · norm f32 LE · vector dim × f32 LE` (1,544 bytes at dim 384). No header — the
  manifest describes it. Row *r* starts at `r × row_bytes`.
- **`manifest.bin`**: `magic b"XTDENSE2"` · `hdr_len u64 LE` · JSON header
  `{"format_version":2,"dim":…,"metric":"…","fingerprint":"…","generation":g,"rows":n,"live":m,"tombstones_len":t}`
  · the tombstone set, `t` bytes of `RoaringBitmap::serialize_into` (row indices that are
  dead — deleted or superseded). Written to `manifest.bin.tmp`, synced, renamed: **the
  manifest is the truth** — `rows` is the committed length of the row file; bytes beyond it
  are not part of the index.
- Interleaved rows rather than three appendable column files: one append per commit, one
  length to reason about for crash safety, and the unfiltered scan reads one stream. A
  filtered scan touches the id at the row's head and skips the rest by offset; the bench
  measures the unfiltered case (SC-004) and reports the filtered one.
- Generation-named row files make compaction atomic: the compact file is written under
  `g+1`, the manifest switch is the rename, the old file is removed afterwards (best effort;
  a writable open sweeps `vectors.*.bin` not named by the manifest). A mapping of the old
  file stays valid until dropped — the inode outlives the unlink.

## D3 — The protocols

**commit** (pending non-empty; otherwise a no-op):
1. Resolve pending against the committed state: for each pending id, if a live row exists
   for it (`rows_by_id`, D6), that row index goes into `dead`; if the change is `Some(v)`,
   the row is appended (id, norm, v).
2. Append the new rows to `vectors.<g>.bin` (`OpenOptions::append`), `sync_all`.
3. Encode the manifest with `rows += appended`, `live = rows − dead.len()`, the new
   tombstone set; write `manifest.bin.tmp`, `sync_all`, rename over `manifest.bin`.
4. Re-read the committed state (the row file re-mapped or re-read at its new committed
   length; the bitmap; `rows_by_id` updated incrementally), clear pending.
5. If a compaction threshold is set and `dead.len() / rows > threshold` (rows > 0): compact.

A crash before step 3's rename leaves the old manifest: the appended tail is beyond `rows`
and is ignored (and truncated at the next writable open, D4). A crash after it is a
complete commit. Nothing in steps 1–3 modifies a byte a reader could have mapped.

**compact** (inherent `FlatIndex::compact`; a no-op when `dead` is empty *and* ids are
already ascending — i.e. the file was produced by a compaction and only appended to with
fresh, higher ids, which is the common build case): write the live rows in ascending id
order to `vectors.<g+1>.bin` (`sync_all`), write the manifest with `generation g+1`,
`rows = live`, an empty tombstone set (rename), remove `vectors.<g>.bin`, re-read. This is
the current `commit` algorithm, moved. The pipeline's `merge` calls it after `commit` and
before the lexical merge.

**open** (writable): read the manifest (version check → the existing `Corrupt` error for
any other version, naming both); if `vectors.<g>.bin` is longer than `rows × row_bytes`,
`set_len` it to that (a crashed append — before anything is mapped); if shorter →
`Corrupt`; sweep stale `vectors.*.bin`; read or map the row file; build `rows_by_id`.
**open** (read-only / mapped): the same without the truncation and the sweep — a longer
file is tolerated (only `rows` are read); a shorter one is `Corrupt`.

## D4 — The mmap SAFETY argument, amended not weakened

ADR-0007 condition 2 says the crate never writes `index.bin` in place, only replaces it.
Version 2 **extends** the row file past every live mapping's end and never modifies a mapped
byte: a mapping covers `[0, len at map time)`; appends write beyond that; the truncation of
a crashed tail happens at writable open *before* this handle maps anything, and the
single-writer precondition (documented on `LoadPath::Mmap`, unchanged) excludes another
writer. Compaction replaces by rename as before. The SAFETY comment in `bytes.rs` and
ADR-0013 state the new invariant: *a mapped byte is never modified or truncated; the file is
only extended beyond every mapping or replaced by rename*. The writer's own mapping is
refreshed after each commit (as `read_generation` does today).

## D5 — Tombstones with `roaring`

`RoaringBitmap` (0.10.12): `insert(u32) -> bool`, `contains(u32) -> bool`, `len() -> u64`,
`serialize_into(W)`, `deserialize_from(R)`, `serialized_size()`. Row indices are `u32`
(the manifest's `rows` is `u64` for the header but bounded by `u32::MAX` rows, checked at
open — 1,544 bytes × 4 G rows is far beyond any device). The bitmap is held in memory and
consulted per row in the scan (`contains` on a bitmap container is a bounded search;
measured by the bench — if it costs more than the budget allows, the fallback is a
`Vec<u64>` word bitset built at open from the roaring set, still serialised as roaring).
The tombstone set is rewritten whole each commit: at 428k rows the worst case is ~53 KB,
typically far less (roaring compresses runs and sparse sets).

## D6 — `vector(id)` and `len()`

`rows_by_id: Vec<u32>` indexed by `DocId`, `u32::MAX` for none, sized `max id + 1`, built
at open by one pass over the rows (later rows win; dead rows skipped) — 1.7 MB for the
Wikipedia index, O(rows) at open (~milliseconds). `vector(id)` reads that row; `len()` is
the manifest's `live`. Superseded rows are found the same way at commit (D3 step 1).

## D7 — Search, unchanged in substance

The loop in `FlatIndex::search` becomes: for each row `r` in `0..rows`, skip if
`dead.contains(r)`, read the id at the row's head, apply `allowed`, score
(`search::score`, `f64` accumulation), push `(score, id)`; `search::top_k` as now. Per-row
scores are position-independent and the final order is total, so results are bit-identical
to version 1 for the same live content — the property test and the goldens assert it.

## D8 — Measurement: the workspace's first `criterion` bench

`crates/xtriever-dense/benches/scan.rs` (`criterion::{criterion_group, criterion_main,
Criterion, BenchmarkId}`; `cargo add --dev criterion -p xtriever-dense`; `[[bench]] harness
= false`): 100k rows × 384 dims from a fixed-seed generator; (a) the unfiltered scan on a
version-2 index with no dead rows vs. a **version-1-shaped scan** kept as a bench-only
function over the columnar layout (the old `search` loop, so the comparison survives the
format's removal); (b) the 10-row commit into 100k rows: bytes written (from file sizes)
and time — version 2 vs. the version-1 rewrite (the same bench-only encoder). Budgets:
scan within 5 %; commit under 100 KB and 50 ms (SC-001, SC-004). Results are pasted into the
PR and committed under `runs/`.

## D9 — Regeneration and the proofs

- **Fixture index** (`swift/Xtriever/Tests/Fixtures/index`, untracked; `expected.json`
  tracked): `cargo run --release -p xtriever-ffi --example fixture_index -- swift/Xtriever/Tests/Fixtures`
  regenerates both; `git diff --stat swift/Xtriever/Tests/Fixtures/expected.json` must be
  empty — the goldens reproduce bit for bit. The same for the FFI/Python/Swift test suites
  that open it.
- **The shipped Wikipedia artefact** (`target/xt-wiki`, 427,947 rows): its vectors are the
  pinned embedder's output; a throwaway converter (`reference/convert_dense_v1_to_v2.py`
  under the 024 spec's `reference/`, not shipped in the engine — or a `#[ignore]`d Rust
  test that reads the v1 layout, kept out of the library) copies rows into version 2; then
  `wikidemo measure` (host goldens, 800/800 bits) and the iOS device rule prove it. If the
  owner prefers a from-scratch rebuild (11 h), the result is the same and the proof the same.
- **Demo slices** under `target/` are rebuilt when needed (not committed).
- **SciFact hybrid baseline** re-run once with `xtriever-eval` (2 s lexical + the dense
  embedding of 5,183 docs ~9 min): expected identical to
  `specs/013-lexical-quality/baselines/hybrid-baseline-v2.scifact.json` to 1e-6 — a
  belt-and-braces check that the eval path (which builds an index) is unchanged. No
  three-dataset run: nothing ranking-affecting changed, and bit-identity is asserted directly.
- **RSS**: `wikidemo measure` on the regenerated artefact records peak resident bytes;
  compared with the 019 record (1,029 MB) — the same order (one row file mapped).

## D10 — The threshold knob

`HybridConfig.dense_compact_dead_share: Option<f32>` (validated `0.0..=1.0`, else
`Error::Schema`); recorded in the descriptor with `#[serde(default)]` (pipeline format
version unchanged, as `rerank_mode` in Feature 015); passed to `FlatIndex` at create /
writable open (`FlatIndex::set_compaction_threshold(Option<f32>)`); the FFI `IndexConfig`
gains the same field with `#[uniffi(default = None)]` — uniffi regenerates the Python and
Swift records, so no hand-written binding changes. Semantics: after a commit's manifest
rename, `dead / rows > threshold` → `compact()` in the same call. A read-only open ignores it.

## Alternatives considered

- **Segments per commit** (a file per commit + a manifest listing them): equivalent
  outcome, more machinery (per-segment tombstones, many mappings); rejected (Rule 7).
- **Convert version 1 on first commit**: rejected by the owner (Q1 = C) — one format, one
  reader.
- **Automatic compaction only**: rejected by the owner (Q2 = C) — explicit `merge` stays;
  the threshold is opt-in.
- **Tombstones in a separate generation-named file**: the manifest already needs a rename
  per commit; embedding the bitmap in it removes a crash window (a new tombstone file with an
  old manifest would hide replaced documents).
- **Truncating the crashed tail lazily at the next append**: would overwrite bytes a
  read-only mapping opened in between could cover; truncating at writable open, before this
  handle maps, keeps the SAFETY invariant simple.
