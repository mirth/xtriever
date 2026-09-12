# Feature 002 Report: The Lexical Stage

**Branch**: `002-lexical-stage` | **Closed**: 2026-09-12 | **Spec**: [spec.md](./spec.md) |
**Plan**: [plan.md](./plan.md)

## Verdict

`xtriever_core::LexicalIndex` is implemented in full by `xtriever_lexical::TantivyIndex` over
tantivy 0.26.2 with Feature 001's C-free feature set. All 13 success criteria are met by named
tests; the full local gate passes; `xtriever-core` is untouched.

| | |
|---|---|
| acceptance suite | **60 / 60** pass (`cargo nextest run -p xtriever-lexical`), 1 `#[ignore]`d measurement |
| workspace suite | 69 / 69 |
| goldens vs the independent Python oracle | **18 / 18** rankings agree — ids and order exact, scores within 1e-5 relative — 22 / 22 filter sets exact, all statistics exact |
| property tests | filter algebra 5 properties × 1,000 cases; analyzer determinism 48 cases; round-trip 32 cases |
| gate | fmt ✓ · clippy `-D warnings` ✓ · deny ✓ (no new ignore; `onig_sys` ban intact) · iOS / iOS-sim / Android `cargo check` ✓ · wasm32 ✗ tracked (below) · no stubs ✓ · zero C/C++ in the graph ✓ |
| `xtriever-core` changes | **none** (`git diff --stat crates/xtriever-core/` empty) |
| eval delta | **N/A — [ADR-0006](../../docs/adr/0006-defer-beir-eval-gate.md)** (condition 1: baseline below) |

## ADR-0006 baseline (condition 1)

The first `xtriever-eval` run must use as its baseline **the merge commit of branch
`002-lexical-stage` into `main`**. At the time this report was written the branch base was
`b36f430` (`main` after Feature 001); the merge commit hash is to be filled in by whoever merges:

> **Baseline commit**: `________` *(merge of 002-lexical-stage → main)*

## Findings

### F-001 — The backend's k-boundary ordering is random per process (resolved)

**Item**: FR-014 as first accepted (clarification Q1, option B); ADR-0005 amendment.

**Observed**: `determinism::k_boundary_tie_membership_and_order` failed on one run and passed on
the next with identical inputs. In a four-batch index the planted tie group (`Term(tags, "alpha")`,
99 documents, `k = 10`) returned `[751, 783, 789, …]` on one run and `[1, 3, 5, …]` on another.

**Cause** (read from source): at every commit tantivy sorts the searcher's segments by descending
`max_doc` using a *stable* sort over a `Vec` collected from a `HashMap`
(`src/indexer/segment_updater.rs:406-407`, `src/indexer/segment_register.rs:18,66-70`). Equal-size
segments therefore take `HashMap` iteration order, which std randomises per process. `DocAddress`
is segment-ordinal-major, so the backend's tie-break at the boundary was random across restarts —
a Principle VI violation, and one that FR-015 ("identical across process restarts") forbids.

**Resolution** (repository owner, 2026-09-12): key the backend's top-k collector on
`(score, Reverse(DocId))` via `TopDocs::tweak_score` (research D12 revised). The `DocId` tie-break
now holds inside the collector, across segments and at the boundary, with no over-fetch and one
fast-field read per candidate. The ADR-0005 amendment is superseded and the core doc comment
stays as written. **Smallest reproduction**: build the fixture corpus in 4 batches of 250, run
`term_tag_tie` in two processes, compare membership.

### F-002 — Filtered and unfiltered searches took different collection paths (resolved)

**Item**: FR-022 ("scores unchanged by a filter"); `filter_algebra_prop::filtered_search_agrees_with_resolve`.

**Observed**: a one-ulp score difference (`4.227677` vs `4.2276764`) for document 777 under
`Match(body, "quantum lattice photon")` with `Exists(tags)`.

**Cause**: `TopDocs::order_by_score` collects through block-WAND pruning
(`src/collector/sort_key/sort_by_score.rs:41-50`); a wrapping `FilterCollector` cannot, and falls
back to `weight.for_each` (`src/collector/mod.rs:186-220`). The BM25 clause sum is accumulated in a
different order.

**Resolution**: subsumed by F-001's fix — a `tweak_score` collector takes the plain path on both
routes, so filtered and unfiltered scores are bit-identical by construction. Re-minting the goldens
after the change produced byte-identical `queries.json`.

### F-003 — `Range` on `Bool` is rejected by the backend (resolved)

`FastFieldRangeWeight` accepts u64/i64/f64/date terms only (`InvalidArgument("Expected term with
u64, i64, f64 or date, but got … type=Bool")`). The data model had admitted it. Now
`Error::InvalidQuery` ("use Eq"), tested by `filters::range_on_bool_is_invalid_query`; the
`range_bool_false` golden was dropped.

### F-004 — `merge()` raced the backend's own merge thread (resolved)

With eight batches, `LogMergePolicy` merged segments in the background between `commit` and
`writer.merge(&ids)`, and the backend rejected the stale ids (`InvalidArgument("The segments that
were merged could not be found …")`). `merge()` now re-reads the segment list and retries up to 8
times on exactly that error (`index.rs`), and `concurrency::search_during_merge_is_consistent`
passes.

### F-005 — FR-004 over-promised "no threads" (spec corrected during planning)

Recorded in plan.md "Spec corrections". tantivy's writer spawns an indexing worker and two rayon
pools structurally; the constitution never required a stage crate to be thread-free. FR-004 now
pins every backend thread count to its minimum and forbids the crate's own threads.

## FR-015 / FR-025 divergence measurement (SC-012)

`stats::divergence_measurement` (`--run-ignored only`), history pair = fixture corpus vs. fixture
corpus + 100 extra documents then deleting those 100; query `phrase_exact`
(`Phrase(body, "quantum lattice", 0)`, k = 10):

| phase | same ids | max \|Δscore\| | verdict |
|---|---|---|---|
| before `merge()` | yes | **5.7338095e-1** | **diverge** |
| after `merge()` | yes | 0 | agree |

Live statistics on the variant (`term_stats(body, "quantum")` = `{doc_freq: 286, total_term_freq:
347}`, `num_docs` = 1000) equal the baseline's exactly (SC-011 green). The divergence is entirely
in what the backend *scored* with before the merge: `max_doc` = 1100, term-dictionary `df` = 386
(the 100 deleted extras all contain "quantum lattice"), and `total_num_tokens` including the deleted
bodies — research D8's closed-form prediction. After the merge physically drops the deleted
documents the two histories agree to the bit.

**Consequence for the `Ranker`**: a feature vector that pairs `bm25.score` with an IDF derived from
`term_stats` is internally inconsistent on an index with pending deletions, by up to this
magnitude, until a merge. That is a real constraint; it is recorded here for the LTR spec rather
than papered over (FR-025), and `merge()` is the operator's remedy. No ADR is opened by this
feature — the LTR spec owns the decision of whether to require a merge before feature extraction or
to read statistics through the backend's own (deletion-inclusive) provider.

## Known costs (stated, not claimed small — no performance budget is set)

- `term_stats` is O(df): it walks the term's postings in every segment to skip deleted documents
  and sum term frequencies (the backend stores no total term frequency).
- Every text field is analyzed twice at index time — once by the backend, once for the exact
  length column (D7).
- The first `add`/`delete` allocates the backend's 15 MB arena and spawns its threads (D1/D2).
- The keyed collector forgoes block-WAND pruning and reads one fast-field value per scored
  candidate (D12 revised).

## wasm32 (best-effort, tracked)

`cargo check -p xtriever-lexical --target wasm32-unknown-unknown` fails at
`errno v0.3.14` — *"The target OS is "unknown" or "none", so it's unsupported by the errno crate"* —
reached via `tantivy 0.26.2 → fs4 1.4.1 → rustix 1.1.4 → errno`, i.e. the `mmap` feature. Expected
(research R5); non-blocking per the constitution.

## Success criteria → tests

| SC | proved by |
|---|---|
| SC-001 all eight methods | `index_query`, `mutation`, `filters`, `stats` together exercise every method |
| SC-002 every query and filter variant has a golden | `index_query::every_query_golden_matches_exactly` (18 entries, 6 variants); `filters::every_filter_golden_resolves_exactly` (22 entries, 8 variants) |
| SC-003 one batch = several batches | `determinism::single_and_multi_batch_layouts_rank_identically` |
| SC-004 repeat / restart identical | `determinism::repeated_search_is_identical`, `mutation::reopen_exposes_only_committed_state` |
| SC-005 reference agreement | `gen_002_fixtures.py --verify-ranking`: 18/18 within 1e-5 |
| SC-006 algebra properties ≥ 1,000 cases | `filter_algebra_prop` (5 properties, `cases: 1000`) |
| SC-007 core types + constructor only | every test imports only `xtriever_core` and `xtriever_lexical::TantivyIndex` |
| SC-008 targets, zero C/C++ | gate: iOS, iOS-sim, Android `cargo check`; `cargo tree` grep → pure |
| SC-009 error paths, 0 panics | the `*_is_invalid_query` / `*_fails_at_create` / `unknown_field_*` tests; `short_phrases_do_not_panic`; `k_zero_is_an_empty_ok` |
| SC-010 red checkpoint | T024: 57 tests, 2 pass (`fixtures_valid`), 55 fail on `NotImplemented`, 0 fixture-caused |
| SC-011 history-pair statistics | `stats::history_pair_has_identical_live_statistics` |
| SC-012 divergence recorded | table above |
| SC-013 shared under a lock, one type | `concurrency::*` (5 tests); public surface = `TantivyIndex` + `ANALYZERS` |
