# Report: Incremental Dense Commits

**Status**: PR A (the format, `xtriever-dense`) complete and green — gate, bench, fixture regenerated, eleven Copilot rounds and one `/code-review` (16 findings) applied — awaiting merge; PR B (the pipeline's `merge` → `compact`, the threshold knob through config/descriptor/FFI, the Wikipedia artefact regeneration) not started.

## The oracle (T002)

`crates/xtriever-dense/tests/index_oracle.rs::mint` run on the version-1 implementation:
three sequences (cosine / dot / euclidean, seeds `0x5EED0024..26`, dim 8, ~300 steps each —
143–161 adds, 55–62 replaces, 21–29 deletes, 48–56 commits, 9–17 reopens), five queries after
every commit (two with an `allowed` set), every hit's id and score bits →
`tests/support/v1_oracle.json` (289 KB). Minted twice: byte-identical. `replay` green on
version 1.

## Red checkpoint (C1, 2026-09-18)

`cargo nextest run -p xtriever-dense`: the crate's test targets `index_append`,
`index_compact`, `index_crash` and `index_prop` do not compile (`DenseStats`, `stats`,
`compact`, `set_compaction_threshold` absent); `index_persist` does not compile under
`--features mmap` (`compact`). Of the targets that compile: `index_errors` **3 failed, 9
passed** (the version-3 header, the version-1 refusal, the manifest truncations);
`index_persist` **2 failed, 5 passed** (the file names, the stale-generation sweep);
`index_oracle::replay`, `index_golden` (3), `index_mutation` (4) pass — they drive the
public API and are the oracle that must keep passing.

## PR A — the format (2026-09-18)

**Green**: `cargo nextest run -p xtriever-dense` 53 passed (20 skipped: model-backed);
`cargo test -p xtriever-dense --features mmap` 56 passed; `index_oracle::replay` reproduces
the version-1 oracle **bit for bit** on version 2 (three metrics, 159 commits, ~800 queries);
`index_prop::compact_and_reopen_preserve_every_bit` with `PROPTEST_CASES=1000` passes;
`index_crash` enumerates every byte boundary of a 7-row commit (168 lengths) and of a
compaction (~1,250 lengths) — each reopens to the previous committed state. Workspace: 294
passed; the Python surface 33 passed over the regenerated fixture.

**The fixture** regenerated (`fixture_index` example): the minted `expected.json` differs
from the committed file in one line only — the generator's commit hash in `generated_by`;
every hit and score bit is identical, so the committed file is kept (restored; the diff is
empty) and the regenerated `index/dense/` (`manifest.bin` + `vectors.0.bin`) passes it.
(`XtrieverData/` is staged from it by `scripts/build-ios-package.sh` at package build time;
no device job here.)

**Bench** (`runs/bench-scan-…txt`, `MacBookPro18,3`, 100,000 × 384, k = 10):

| | version 1 (shape) | **version 2** | budget |
|---|---|---|---|
| unfiltered scan | 32.8 ms (v1 shape) | **31.6 ms** (−4 %); 40.4 vs 40.1 ms in the contended re-run after the review fixes | within 5 % |
| scan, `allowed` = every other id | — | 16.2 ms | reported |
| 10-row commit, bytes written | 154,400,101 | **15,596** (ten rows + one manifest, by the handle's own counter; net growth 15,440) | < 100 KB |
| 10-row commit, time | 145–176 ms across runs (a `rows × row_bytes` rewrite by construction) | **18.5 ms** idle after the `/code-review` fixes (17.5–18.2 before them; 12.9 before the directory fsyncs; three `fsync`s and a manifest read for the stale-writer check now) | < 50 ms |

The first commit run measured 37.6 ms: `commit` re-read the whole row file after each
append. Fixed (`absorb`: the in-memory buffer grows by the appended bytes; a mapping is
re-made without a read) before the numbers were recorded.

**Gate**: fmt, clippy (`--all-targets`, `--all-features` on the dense crate), deny,
`cargo check` on the three cross-targets, `cargo nextest run --workspace`; no change under
`crates/xtriever-core`, `deny.toml`, `apps/`, baselines; no identifiers in the records.

**Size**: 18 files under `crates/` and `docs/`, 2,038 + / 302 − — of which the crate's
source is ~900 lines (`format.rs`, `index/mod.rs`, `lib.rs`, `bytes.rs`), tests ~1,000, the
bench 228, ADR-0013 123; plus the 289 KB oracle JSON. Over Rule 3's ~800 as stated in the
plan's split: PR A is the format with its oracle and tests, PR B the pipeline and artefacts.

### Review round 1 (Copilot, six comments — all applied)

1. `decode_manifest` validates the full row layout with checked arithmetic
   (`Rows::checked`: `8 + dim × 4` and `rows × row_bytes`), so an absurd `dim` is `Corrupt`,
   never an overflow; unit-tested with three overflowing pairs.
2. `reload` rejects two live rows for one id (`Corrupt` naming the id and both rows) instead
   of overwriting the table entry; `index_errors::two_live_rows_for_one_id_are_corrupt`.
3. The 024 property block uses `ProptestConfig::default()`, so `PROPTEST_CASES=1000` is
   honoured: re-run, 1,000 cases in 253 s, green.
4. `compact`'s contract and doc no longer promise a "read-only index" error — `FlatIndex`
   has no read-only mode; the pipeline refuses `merge` first; a read-only directory surfaces
   the underlying `Error::Io`.
5. The threshold acceptance test covers 20 % → exactly 25 % (no compaction — strict `>`) →
   26 % (compaction); the spec's US4 wording matches.
6. `expected.json`: the committed golden is restored (the regeneration changed only the
   provenance line); research D9, the quickstart and T018 now say so explicitly.

### Review round 2 (Copilot, two comments — both applied)

1. The generation read from disk advances with `checked_add`: at `u64::MAX`, appends still
   work and `compact` returns `Corrupt` ("cannot advance") instead of overflowing;
   `index_errors::a_generation_that_cannot_advance_is_corrupt_not_an_overflow` also checks
   the handle stays coherent after the refusal.
2. Every fallible step now precedes the manifest switch, in both protocols: `commit` grows
   the in-memory buffer (undone by a truncate on failure) or makes the new mapping *before*
   renaming the manifest, and the state swap after the rename is infallible; `compact`
   writes the new generation, brings it into memory, then renames, then swaps — a failure
   anywhere leaves disk and handle on the previous state (the partial file removed). The
   `absorb`/`reload`-after-rename paths are gone; `reload` is used at open only.

### Review round 3 (Copilot, four comments — all applied)

1. A mapping now covers exactly the committed rows (`bytes::read_prefix` maps
   `rows × row_bytes`), so the invariant no longer depends on the best-effort truncation at
   open: a crashed tail is never mapped, and the truncations touch only bytes beyond the
   committed length. ADR-0013's amendment, research D4 and the SAFETY comment restated;
   `index_persist::a_mapping_covers_only_the_committed_rows` (the row file made read-only so the crashed tail survives the open — round 6).
2. Row-space exhaustion (`u32` row indices) is checked before any I/O (`row_space`, unit
   tests) — an `Error::Io` naming the limit and the remedy (compact).
3. The generation error names the value and the condition; no review reference.
4. The append path is selected by `load_path`, not by the current buffer variant: an
   `open_mapped` handle maps after its first append and after compacting to zero rows and
   back (`is_mapped()` probe, feature `mmap`;
   `index_persist::a_mapped_handle_maps_after_its_first_append_and_after_compacting_to_empty`).

### Review round 4 (Copilot, four comments — all applied)

1. The live-row table is a `BTreeMap<id, row>`: memory follows the row count whatever ids
   the caller chooses (a sparse `u32::MAX` costs one entry), and it iterates in ascending id
   order — the order a compaction writes. `index_persist::a_sparse_id_costs_one_entry_not_a_table`.
   (The pipeline's ids are dense by contract; the fix is for `FlatIndex`'s public surface.)
2. Directory syncs at the ordering points (`sync_dir`, POSIX `fsync` on the directory):
   after creating the row file at `create` and the new generation at `compact` (before a
   manifest names it), and after every manifest rename (inside `write_manifest`) — so the old
   generation is removed only once the rename is durable. A removal itself need not be
   durable: a survivor is swept at open.
3. On the mapped path the temporary mapping over the appended bytes is dropped before a failed
   manifest write truncates them.
4. The buffered prefix read takes exactly `len` bytes (`Read::take`): a crashed tail is never
   read; `index_persist::a_buffered_open_reads_only_the_committed_bytes` uses a sparse 64 MB
   tail on a read-only directory.

The commit bench re-run after the directory syncs: **17.5 ms** per 10-row commit (was 12.9 ms), still 15.4 KB written; `runs/bench-scan-…txt` carries both runs.

### Review round 5 (Copilot, six comments — all applied)

1. The read-only-directory test and its `PermissionsExt` import are `#[cfg(unix)]` (CI runs
   the crate on Windows too).
2. The power-loss ordering guarantee is scoped to the Unix targets the engine ships to
   (`sync_dir` doc, ADR-0013, the contract); elsewhere `sync_dir` is a documented no-op and the
   crash-at-any-byte guarantee stands on the manifest rename alone.
3–6. ADR-0013's consequences, the data model, research D6 and task T011 now record the
   `BTreeMap<u32, u32>` live-row map (memory follows rows; ascending order for compaction)
   instead of the superseded `Vec<u32>` table.

### Review round 6 (Copilot, three suppressed comments — all applied)

1. A commit over the compaction threshold is now *one* protocol: decided before any I/O, it
   is performed as the rewrite (live rows with the pending changes folded in, one manifest
   rename) instead of an append followed by a separate `compact()`. It fails whole — pending
   kept, nothing on disk — or succeeds whole. `compact()` itself folds pending in the same
   way (no separate commit first). `index_compact::a_threshold_commit_is_one_protocol_that_fails_whole`
   (at the last generation the rewrite refuses; the same pending changes then commit as an
   append with the threshold off).
2. `write_manifest` reports *where* it failed: before the rename (callers roll back — rows
   cut or the new generation removed, pending kept) or after it (only the directory sync;
   the manifest is switched, the handle adopts the new state, and the returned `Error::Io`
   says the state is the new one with its entry's durability unconfirmed).
3. The crashed-tail tests make the row file read-only (`0o400`) so the open cannot cut the
   tail: the file is asserted to stay extended and the mapped (and buffered) index exposes
   exactly the committed rows.

### Review round 7 (Copilot, one comment — applied)

`FlatIndex` gains a read-only mode after all: `open_read_only[_for]` and, under `mmap`,
`open_mapped_read_only[_for]` skip the truncation and the sweep (`settle`) — a logical
read-only open holds no writer's role, so it alters nothing — the no-concurrent-writer
precondition of every open stands, see round 8 — read exactly the committed rows, and refuse `commit` / `compact`
with the lexical stage's "read-only index" `Error::Io`. The pipeline's `open_with(...,
read_only: true)` now uses them (a four-line change in `crates/xtriever-pipeline/src/index.rs`,
pulled into PR A because the reachable path is the pipeline's). The round-1 "no read-only
mode" wording in the contract is superseded. Tests: a read-only open leaves a crashed tail, a
manifest temporary and a stale generation in place and refuses mutations; a mapped read-only
open exposes the committed rows only; the next writable open cleans up.

### Review round 8 (Copilot, three comments — all applied)

1. The rewrite protocol computes the new generation's row count (committed live − superseded
   + pending inserts) and checks it against the `u32` row-space limit before any I/O, as the
   append path does; the loop counter is bounded by it (`debug_assert`).
2. An empty index's open — read-only included — stats the row file the manifest names: a
   missing generation is `Error::Io` in every mode (`index_errors::an_empty_index_still_needs_its_row_file`).
3. The read-only docs no longer suggest sharing the directory with a writer: the
   no-concurrent-writer precondition of every open (spec edge cases, the pipeline's
   `OpenOptions`) stands; a read-only open merely alters nothing.

### Review round 9 (Copilot, five comments — all applied)

1. `create` validates the row layout (`Rows::checked(0, dim)`) before any write, so a refused
   `dim` leaves no populated, unusable directory behind.
2. The `FlatIndex` doc and FR-006 now state the transaction as it is: a crash reopens to the
   previous committed state *or* the fully committed new one (the manifest rename is the
   switch), never a partial state — which is what `index_crash` enumerates.
3. The crate docs no longer say the pipeline's `merge` compacts — that lands in PR B (T021).
4. T011 and T013's completed descriptions record the protocols as landed (rows in memory
   before the rename; the threshold decided up front as one rewrite; pending folded into
   `compact`), so an audit cannot recreate the superseded crash window.

### Review round 10 (Copilot, five comments — all applied)

1. A post-rename directory-sync failure is remembered (`sync_pending`): every later `commit`
   or `compact` — an empty one included — retries the sync first and succeeds only once it
   has, so a later success never hides an unconfirmed switch (`is_sync_pending()` tells).
   Test: a directory without read permission makes the sync fail; the commit reports the
   unconfirmed switch, an empty commit and a compact fail the same way, and once the
   permission is back the next commit succeeds.
2. The manifest decoder recognises versioned magic (`XTDENSE<digit>`) before the generic
   bad-magic rejection, so a genuine future format names both versions; the tests write the
   actual `XTDENSE3` magic (and, separately, a bumped JSON header).
3. The 024 property test now compares every query — unfiltered and with an `allowed` set,
   `k` ∈ {3, 64} — against an independent reference scorer over the model (the contract's
   arithmetic, written in the test), ids and score bits, before compaction, after, and after
   a reopen; re-run with `PROPTEST_CASES=1000`.
4. The spec's manifest entity describes the embedded tombstone set (no tombstone file).
5. The FFI `LoadPath::Mmap` doc names `dense/vectors.<g>.bin` (a doc-only change in
   `crates/xtriever-ffi`, pulled into PR A); T024 notes it.

### Review round 11 (Copilot, six comments — all applied)

1–2. Write volume is now measured, not inferred from directory growth: `FlatIndex::bytes_written()`
   counts every byte the handle hands to the row files and manifests (appended rows, every
   manifest, a compaction's new generation — nothing else is ever written). The append test
   asserts the counter's delta is exactly ten rows plus one manifest (a delete-only commit:
   one manifest) and reports net growth beside it; the bench prints both figures.
3. A read-only handle refuses every mutation — `add`, `delete`, `set_compaction_threshold`
   as well as `commit` and `compact` — so the doc's promise is true; the test covers all five.
4. The contract's guarantee 3 states the old-or-new boundary (no rollback after the switch).
5. This report's status line distinguishes PR A (complete, green) from PR B (not started).
6. Research D3's commit *and* compact steps, and T013, record the landed protocol (the new
   rows or generation in memory before the rename, the directory sync, the two failure
   kinds, infallible adoption after) — two further comments in the same round.

### `/code-review` (eight finder agents; 16 findings, all applied)

| # | Finding | Fix |
|---|---|---|
| 1 | a stale writable handle's `commit` truncated rows another handle committed (probed) | every writable `commit` / `compact` verifies the manifest on disk is the one it last saw (generation, rows) → `Corrupt` "changed by another writer"; test |
| 2 | `Rows::new` silently degraded an overflowing layout to empty on the write path | deleted; `Rows::for_count` (u32 rows + checked bytes) is the one write-path constructor, `decode_manifest` returns the validated layout |
| 3 | the after-switch outcome stranded the pipeline's commit; per-handle retry; `create` failed after a complete directory | the directory handle for the post-rename sync is opened *before* the rename (`xtriever_core::fs::open_dir_for_sync`), so an unopenable directory fails before the switch; the pipeline finishes its protocol on an after-switch error and returns it, and its empty `commit` retries the stage's sync; `create` opens the complete index with `sync_pending` set |
| 4 | three `unreachable!` arms in library code | the switch tail is one `match` (roll back / adopt), the merge is an exhaustive four-arm `match` on a `Merge` iterator — no panic arm anywhere |
| 5 | a mapped read-only open faulted in the whole row file to build the id map | the manifest records `ordered`; an ordered, tombstone-free generation has no table (`vector(id)` binary-searches the row ids), so a read-only mapped open touches nothing beyond the manifest; the map exists only for files with tombstones or unordered ids |
| 6 | compaction materialised the whole live set on the heap and re-read it | `rewrite` streams merged rows through a `BufWriter` (a kept row is a byte copy), keeps a copy in memory only on the buffered path (the state's own memory), maps on the mapped path |
| 7 | `create` left an unusable directory on a manifest failure | the row file and temporary are removed on a before-switch failure; test (permission case) |
| 8 | the eval cache silently re-embedded a version-1 cache | `EmbeddingCacheKey.format_version` is 2 (three literals, the test's variant bumped to 3); the open error is printed before a wipe |
| 9 | `set_len` on an append-only handle (Windows) | the append uses a read+write handle, `set_len` only when longer, `seek` to the committed length |
| 10 | `bytes_written` counted bytes before the write succeeded | counted after each successful write; documented as such; test |
| 11 | no test of the append-then-manifest-failure rollback, of read-only opens of a short row file, of owned-vs-mapped over sequences | a directory named `manifest.bin.tmp` provokes the rollback (buffered and mapped); the read-only constructors on a short file; the property test runs the same sequence on a writable mapped handle and compares after every commit, after compaction and after reopen |
| 12 | four hand copies of write-tmp-sync-rename with unequal durability; three read-only errors | `xtriever_core::fs::{write_atomically, sync_dir, open_dir_for_sync}` and `Error::read_only()` / `READ_ONLY_MESSAGE`: the pipeline's descriptor and id map, the lexical descriptor and the dense manifest all sync the directory now; one error text |
| 13 | ADR described PR B behaviour as landed | marked PR B |
| 14 | the oracle had no `reference/` generator | `reference/gen_024_fixtures.py` recomputes every expectation from the contract's arithmetic in Python: **0 mismatches** over 159 commits (`--write` regenerates) |
| 15 | derivable state (`ascending`, `layout`), duplicated prologues, the bench's v1 fossil pinned bit for bit | `ordered` lives in the manifest; the layout is switched only through `adopt`; one `pending_counts`; `Lcg`, `vec_for`, `hit_bits`, `row_bytes`, `row_file`, `dir_bytes` in `tests/support`, shared by the bench; the v1 scan shape stays as a comparison, its assertion and the v1 rewrite timing are gone (the v1 write volume is arithmetic) |
| 16 | PR A is ~3.5× Rule 3's ~800 lines | acknowledged; the owner decided the PR A / PR B split — a further split (format + manifest tests / append commit / tombstones + compaction) remains possible at the owner's call |

**Gate after the `/code-review` fixes**: fmt, clippy (workspace; dense with `--all-features`),
deny, the three cross-target checks; `cargo nextest run --workspace` 311 passed; dense 65
buffered / 72 with `mmap`; the oracle bit-identical; the reference-scorer property with
`PROPTEST_CASES=1000` on the buffered *and* the writable mapped handle; the Python surface 33;
`reference/gen_024_fixtures.py --check` 0 mismatches; the fixture regenerated (`ordered`
manifest, goldens reproduce, the committed file kept); the bench re-recorded.

### Review round 12 (Copilot, two comments — both applied)

1. The stale-writer check also compares the live count and the tombstone set, so a
   delete-only commit by another handle (generation and rows unchanged) is caught too; the
   test covers a delete after an add and two delete-only commits with equal live counts.
2. The pipeline's commit marker is written atomically and the directory synced before any
   stage commits, and synced again after its removal — the marker's presence and absence are
   both durable, as `xtriever_core::fs`'s module doc claimed.

### Review round 13 (Copilot, two comments — one applied, one applied in part)

1. The stale-writer check runs before the no-op shortcuts of `commit` and `compact`, so an
   idle stale handle never reports success over another writer's state; tested.
2. Validating `ordered` at every open would be the full id scan the table-free path exists to
   avoid, so the flag stays trusted by read-only handles (like `rows` or the tombstone set —
   the format has no checksums); a writable handle verifies it once, before its first write,
   and refuses to build on a lying manifest (`Corrupt` "not ordered"); tested both ways.
