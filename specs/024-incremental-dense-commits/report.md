# Report: Incremental Dense Commits

**Status**: in progress — red checkpoint reached (PR A).

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
| unfiltered scan | 32.8 ms | **31.6 ms** (−4 %) | within 5 % |
| scan, `allowed` = every other id | — | 16.2 ms | reported |
| 10-row commit, bytes written | 154,400,101 | **15,440** (+ a manifest under 1 KB) | < 100 KB |
| 10-row commit, time | 175 ms (134 ms in the first run) | **12.9 ms** (two `fsync`s) | < 50 ms |

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
