# PR text for Feature 002 — the lexical stage

Hand-written line counts for the Rule 3 split (fixture JSON is generated data, ~50k lines,
counted separately): `src/` 1,165 · `tests/` 1,950 · `examples/` 81 · Python 918 · docs ~330.
One PR would be ~4,400 hand-written lines; the suggested commit/PR boundaries below keep each
under ~800 except the test suite, which is a single red-checkpoint commit by design (Rule 4) and
is data-heavy (fixture loaders and goldens comparisons rather than logic).

| PR | commits | hand-written lines |
|---|---|---|
| 1 | `reference/xtref` extraction + 001 byte-neutrality; `gen_002_fixtures.py` + fixtures; scaffold; **all tests (red)**; `check-no-stubs.sh` | ~2,900 (of which tests 1,950) |
| 2 | `error.rs`, `schema.rs`, `index.rs` create/open/add/commit, `search.rs`, `query.rs` | ~700 |
| 3 | `filter.rs`, `gen_ranking` example, minted goldens + `--verify-ranking`/`--refresh-manifest` | ~350 |
| 4 | `merge`, `delete`, `stats.rs`, keyed collector (F-001), docs, report, ADR-0005 resolution | ~450 |

---

## Title

`feat(lexical): implement LexicalIndex over tantivy 0.26.2 (Feature 002)`

## Body

Implements `xtriever_core::LexicalIndex` in full as `xtriever_lexical::TantivyIndex` — BM25
retrieval for all six `LexicalQuery` shapes, the eight-variant `Filter` algebra resolved to
`DocSet`, live-only corpus statistics, replace/delete semantics, an on-disk descriptor with a
format version, and a deterministic `(score DESC, DocId ASC)` tie-break that holds across segment
layouts, at the k-boundary, and across process restarts.

Spec: `specs/002-lexical-stage/spec.md` · Plan: `plan.md` · Report: `report.md`

**Tests** (`cargo nextest run -p xtriever-lexical`): 64 / 64 pass, 1 ignored measurement (4 tests added in review round 1).
Workspace: 73 / 73. The suite was committed red first (PR 1: 57 tests, 2 pass, 55 fail on the
`NotImplemented` scaffold, 0 fixture-caused failures — SC-010).

**Oracles**: 18 / 18 ranking goldens agree with the independent Python BM25 transcription
(`reference/gen_002_fixtures.py --verify-ranking`: ids and order exact, scores within 1e-5);
22 / 22 filter sets and all statistics exact. Feature 001's fixtures regenerate byte-identically
through the extracted `reference/xtref/bm25.py`.

**Property tests**: filter algebra (5 properties × 1,000 cases), analyzer determinism, index
round-trip.

**Gate**: fmt ✓ · clippy `-D warnings` ✓ · nextest ✓ · deny ✓ (no new ignore) ·
`cargo check` aarch64-apple-ios / -sim / aarch64-linux-android ✓ · wasm32 ✗ tracked
(`tantivy → fs4 → rustix → errno`) · no stubs ✓ · zero C/C++ deps ✓

**eval delta: N/A — ADR-0006.** `xtriever-eval` is a placeholder; this feature is the baseline.
The merge commit of this branch is the baseline for the first real eval run (report.md).

**`xtriever-core`: unchanged.** `git diff --stat crates/xtriever-core/` is empty.

### FR-025 divergence (measured, not adjusted)

| phase | same ids | max \|Δscore\| | verdict |
|---|---|---|---|
| before `merge()` | yes | 5.7338095e-1 | diverge |
| after `merge()` | yes | 0 | agree |

The backend scores with deletion-inclusive statistics until a merge (research D8); live-only
`term_stats`/`stats` are exact throughout (SC-011). Recorded for the LTR spec.

### Findings (report.md)

- **F-001** the backend's k-boundary tie order is random per process (`HashMap`-ordered equal-size
  segments) → resolved by keying `TopDocs::tweak_score` on `(score, Reverse(DocId))`; ADR-0005's
  amendment superseded, no core change needed.
- **F-002** filtered vs unfiltered collection paths differed by 1 ulp → subsumed by F-001's fix.
- **F-003** `Range` on `Bool` unsupported by the backend → `InvalidQuery`.
- **F-004** `merge()` raced the background merge thread → bounded retry.
- **F-005** FR-004 over-promised "no threads" → corrected at planning.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
