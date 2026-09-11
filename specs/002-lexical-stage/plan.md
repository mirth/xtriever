# Implementation Plan: The Lexical Stage

**Branch**: `002-lexical-stage` | **Date**: 2026-09-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-lexical-stage/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Implement `xtriever_core::LexicalIndex` in `xtriever-lexical` as one type, `TantivyIndex`, over
tantivy 0.26.2 with Feature 001's C-free feature set. The design is deliberately boring: the backend
answers every question it answers natively (terms, ranges, existence, top-k, fast-field filtering),
and the crate adds only what the contract needs and the backend lacks — a caller-supplied `DocId`
kept in a hidden fast column, exact live-only statistics kept in per-field length columns, a
`DocId` re-sort after collection (ADR-0005), filter algebra in `roaring`, and an on-disk descriptor
that makes schema or format mismatch a hard error at open. Every clarification the spec recorded
maps to one research decision below, and every backend item is cited from the pinned source.

Two gates needed the repository owner's decision and got it on 2026-09-12: the constitution's BEIR
eval clause (no `xtriever-eval` exists — [ADR-0006](../../docs/adr/0006-defer-beir-eval-gate.md),
accepted) and the one-sentence doc-comment change in `xtriever-core` that FR-014 requires (ADR-0005
amendment, accepted). The Constitution Check passes on all 14 rows.

## Technical Context

**Language/Version**: Rust, edition 2024, toolchain pinned 1.91.1 (`rust-toolchain.toml`)

**Primary Dependencies**: `tantivy 0.26.2` (`default-features = false`; `mmap`, `stopwords`,
`lz4-compression`, `stemmer` — research D18), `roaring` (workspace), `serde` + `serde_json`
(descriptor), `thiserror` via `xtriever-core`. Dev: `proptest`, `tempfile`. All added with
`cargo add`; no version written from memory.

**Storage**: on-disk, memory-mapped index directory + one JSON descriptor
(research D13, data-model "On-disk entities"). No in-RAM constructor is exposed; tests use
`tempfile`.

**Testing**: `cargo nextest` acceptance tests committed failing first (Rule 4); goldens from
`reference/gen_002_fixtures.py` with the BM25 oracle extracted from 001 into `reference/xtref/`
(D17); `proptest` for analyzer determinism, index round-trip and filter algebra (FR-034); one
`#[ignore]`d measurement test that prints the FR-025 `DivergenceRecord`.

**Target Platform**: host (macOS/Linux/Windows CI) for all tests; `cargo check` on
`aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`; wasm32 expected to fail
(tantivy `mmap`/threads) and tracked (R5).

**Project Type**: library crate (`xtriever-lexical`), stage crate under Principle V's layering.

**Performance Goals**: none stated — the spec sets no budget (Assumptions). No `criterion` bench is
added; the plan records the known costs instead: O(df) `term_stats`, a second analyzer pass per text
field at index time for exact lengths (D7), 15 MB writer arena from the first mutation (D1/D2).

**Constraints**: pure Rust, no C/C++ (measured by 001; re-asserted in quickstart Step 6), no
`async`/`tokio`, no `std::time::Instant`; **threads are the backend's and pinned to one worker +
one merger** (FR-004 as corrected during planning — see "Spec corrections" below); `xtriever-core`
unchanged except the FR-014 doc comment; determinism engineered per D1/D3/D12.

**Scale/Scope**: one crate touched (`xtriever-lexical`), one doc comment in `xtriever-core`, the
`reference/` refactor, 1,000-document fixture corpus with 11 fields. Index memory scaling explicitly
deferred (FR-038). Four PRs (Rule 3 split below).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.1.0, ratified 2026-09-10, amended 2026-09-11).
Every row below MUST be marked **PASS**, **FAIL**, or **N/A** with a one-line justification — an
empty verdict is a FAIL.

- Any **FAIL** blocks the phase. Do not proceed, and do not weaken the gate to make it pass.
- Any **N/A** MUST state why the principle cannot apply to this feature.
- A violation that is genuinely required is recorded in Complexity Tracking below and REQUIRES an
  accepted ADR in `docs/adr/`; the row stays **FAIL** until that ADR is merged.
- These are plan-time gates. They do not replace the blocking CI gates in the constitution's
  "Quality Gates (CI)" table.

### Core Principles

| # | Principle | Gate | Verdict | Justification |
|---|---|---|---|---|
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | Inverted index, BM25, term/range/exists/fuzzy/phrase queries, top-k, fast-field filtering and both analyzer chains all come from tantivy — including `FilterCollector` (D11) and the `default`/`en_stem` tokenizers (D5), so the crate registers no tokenizer and writes no collector. Doc sets are `roaring` via core's `DocSet` (D10). The only "built" pieces are glue the backend cannot supply: `DocId` identity column, exact live-only length columns (D7), the ADR-0005 re-sort, and the descriptor. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** (ADR-0006 accepted 2026-09-12) | Tests-first: PR 1 is fixtures + red tests only (FR-032, SC-010). Goldens: 001's BM25 transcription extended and byte-neutrally refactored (D17, quickstart Step 2), tolerances in spec Assumptions (1e-5 relative vs Python; exact vs own goldens), coverage per shape stated honestly (slop > 0 is membership-only). Property tests: all three named invariants (FR-034, quickstart Step 5). **The eval clause cannot be met as written**: this feature creates ranking, `xtriever-eval` is a placeholder, and no baseline exists. Marking it N/A as 001 did would be dishonest here. [ADR-0006](../../docs/adr/0006-defer-beir-eval-gate.md), **accepted 2026-09-12**, defers it for this one feature, and the row passes under its three conditions — baseline commit recorded in `report.md`, the mandatory "eval delta: N/A — ADR-0006" line in every PR, and no later feature may cite it. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 300 MB for a 100k-chunk index including loaded models). | **PASS** | The five pure crates are untouched (core gets one doc comment). `xtriever-lexical` is not in the thread-free list and the constitution puts C/C++ only in three leaf crates — tantivy with 001's feature set has none (001 verdict matrix: 8/8, zero `-sys`), re-asserted by quickstart Step 6's `cargo tree` grep. Thread counts are pinned to the backend minimum (D1) and no thread is spawned by the crate (FR-004). `cargo check` on the three mobile targets is in the gate; 001 already proved tantivy compiles for both iOS triples. wasm32 will fail on `mmap`/threads — best-effort, tracked (R5). **RSS clause**: index memory scaling is explicitly deferred by FR-038 with reasoning; no on-device configuration is claimed. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | No performance claim is made anywhere in spec or plan, so the `criterion`-vs-budget clause is vacuous by design, as in 001; known costs are *stated* (Technical Context) rather than claimed to be small. Reproducibility: fixed seed, tamper-evident manifest, fixture EOL protection already in `.gitattributes`. **`explain()` is N/A**: `LexicalIndex` has no `explain` method and adding one is a core change (FR-002); per-hit BM25 is the `Hit.score` the pipeline will feed `explain()` later. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** (ADR-0005 amended 2026-09-12) | Traits, types and error variants unchanged; error mapping uses six existing variants (D16). Public surface is three inherent methods + one const beyond the trait ([contracts](./contracts/lexical-index.md)); no backend type leaks. `DocId` is core's `u32`; `ChunkInfo.parent` (an external id) is neither indexed nor stored (FR-008b, clarification Q7). Dependencies: `xtriever-lexical → xtriever-core` only. Synchronous throughout. **The one change**: FR-014 narrows the `search` doc comment. Documentation-only, spec-authorised, but the constitution's ADR-and-review gate applies to *any* contract change — the amendment to ADR-0005 was **accepted 2026-09-12**, so the doc-comment edit has its ADR and human review; it lands in PR 4 alongside the ADR text. The on-disk descriptor is a *new* format, version 1, so it defines rather than changes one. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | Determinism is engineered at four points: one indexing worker (D1, tantivy's own reason), manual reader reload so nothing changes between calls (D3), explicit re-sort into `(score DESC, DocId ASC)` (D12), and the k-boundary decided rather than left implicit (FR-014). FR-015 additionally *measures* the one case expected to break — different mutation histories — and records it (D8) instead of asserting it away. Format version + full schema (incl. analyzer ids, the lexical fingerprint) in the descriptor; mismatch ⇒ `Corrupt` at open (D13). **Degradation N/A**: no ML stage and no stage chain here. **Embedder fingerprint N/A**: no embedder in a lexical index. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. `unsafe` only in `xtriever-dense` SIMD kernels, each block preceded by `// SAFETY:` and tested against the safe path. All public items documented (`missing_docs` on). | **PASS** | Deps via `cargo add` (D18); tantivy pinned to the version 001 measured. Workspace lints deny `unwrap`/`expect`/`panic`; the three **backend panics** this plan found — `PhraseQuery::new*` on < 2 terms, `TopDocs::with_limit(0)`, `RangeQuery` with no bound (D9, D11, D10) — are not caught by clippy and are each guarded with a test. `f32::total_cmp` avoids the `partial_cmp().unwrap()` idiom (D12). Errors are core's `thiserror` enum. Zero `unsafe`. `cargo deny` unchanged (no new ignore, `onig_sys` ban intact). `missing_docs` satisfied on the five public items. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | Every backend item in [research.md](./research.md) is cited by file and line from the `tantivy-0.26.2` checkout 001 built (and `tantivy-common-0.11.0`, `tantivy-columnar` for `DateTime`/`Column`). Four traps were found only by reading source: date precision hard-coded to seconds (D6), deletion-inclusive BM25 statistics via `max_doc` (D8), constant-score fuzzy (D9), and the three panicking constructors. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | `deny.toml` untouched. The single core edit is a doc comment that spec FR-014 explicitly requires and FR-002/FR-036 explicitly scope to that one sentence; the ADR gate is Principle V's row, not this one. |
| 3 | One spec, small PRs | Work is scoped to this spec on its branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Four PRs (below), each under ~800 hand-written lines; generated fixture JSON is data produced by a committed script and counted separately, as in 001. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | PR 1 is the oracle refactor, fixtures and every acceptance test, ending at the red checkpoint (quickstart Step 4). No `src/` beyond the placeholder `lib.rs` until PR 2. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 6 is the gate; Step 7 fixes the PR-description contents, including the mandatory "eval delta: N/A — ADR-0006" line and the FR-025 divergence table. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | Three places this plan pre-commits to stopping: a non-empty diff in the oracle refactor (quickstart Step 2), a stemmer disagreement on the `standard_en` golden (R1), and the FR-015/FR-025 divergence — which is *expected* and is recorded with its magnitude, never made to agree (D8, data-model `DivergenceRecord.verdict`). |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | One struct, one trait impl, three inherent methods, one const. No generics over backends, no custom collector, no custom `Query` impl, no analyzer registry. The most "clever" thing is a closure over an `Arc<RoaringBitmap>` handed to a backend-provided collector. |

**Initial gate (pre-Phase 0)**: **FAIL** — 2026-09-11, evaluated during `/speckit-plan`. Two rows
blocked on ADRs: **II** (eval clause → ADR-0006, proposed) and **V** (doc-comment change → ADR-0005
amendment, proposed). All other rows pass.

**Post-design gate (post-Phase 1)**: **FAIL (same two rows)** — 2026-09-11. Phase 1 introduced no
new violation. *(Historical: superseded by the re-evaluation below.)*

**Gate re-evaluation**: **PASS** — 2026-09-12. Both ADRs were accepted by the repository owner:

- [ADR-0006](../../docs/adr/0006-defer-beir-eval-gate.md) — Accepted. The eval clause is deferred
  for this one feature under three binding conditions.
- [ADR-0005 amendment](../../docs/adr/0005-tie-breaking-contract.md) — Accepted. The k-boundary is
  backend-ordered; the `search` doc-comment change is authorised.

**All 14 rows now PASS.** Two obligations carry into implementation: PR 4 lands the core doc
comment with the amendment text, and every PR description carries the "eval delta: N/A — ADR-0006"
line (quickstart Step 7).

## Spec corrections made during planning

One spec defect surfaced while reading the backend source and was corrected in `spec.md` with an
inline note rather than silently:

- **FR-004** had copied the pure-crate rule "no unconditional threads" onto `xtriever-lexical`. The
  constitution does not require that of stage crates (it names five pure crates; lexical is not
  one), and tantivy's `IndexWriter` structurally spawns an indexing worker plus rayon pools
  (`index_writer.rs:424`, `segment_updater.rs:281-305`). FR-004 now requires what the constitution
  requires — no C/C++, no async, no `Instant` — plus pinning every backend thread count to its
  minimum and spawning none of the crate's own.

## Project Structure

### Documentation (this feature)

```text
specs/002-lexical-stage/
├── plan.md              # This file
├── research.md          # Phase 0 — D1–D18, R1–R8, all backend items cited by file:line
├── data-model.md        # Phase 1 — runtime, on-disk, fixture and report entities
├── quickstart.md        # Phase 1 — validation contract, gate, PR-description contents
├── contracts/
│   └── lexical-index.md # Phase 1 — the whole public surface + trait semantics + error mapping
├── checklists/requirements.md
├── report.md            # written during implementation: DivergenceRecord, findings
└── tasks.md             # /speckit-tasks — NOT created here
```

### Source Code (repository root)

```text
crates/
├── xtriever-core/                 # ONE doc-comment edit (traits.rs: LexicalIndex::search, VectorIndex::search) — FR-014
└── xtriever-lexical/
    ├── Cargo.toml                 # + tantivy (001's feature set), serde, serde_json; dev: proptest, tempfile
    ├── src/
    │   ├── lib.rs                 # pub use TantivyIndex, ANALYZERS; crate docs
    │   ├── index.rs               # TantivyIndex: create/open/merge, LexicalIndex impl (add/delete/commit/schema)
    │   ├── schema.rs              # FieldMap, analyzer table, descriptor, validation (D4, D5, D13)
    │   ├── query.rs               # LexicalQuery → backend Query (D9)
    │   ├── filter.rs              # Filter → DocSet (D10), FilterCollector wiring (D11)
    │   ├── search.rs              # top-k, DocAddress→DocId, re-sort (D12)
    │   ├── stats.rs               # term_stats, stats (D7, D8)
    │   └── error.rs               # backend error → core Error mapping (D16)
    ├── examples/
    │   └── gen_ranking.rs         # mints queries.json expected rankings (D17)
    └── tests/
        ├── support/mod.rs         # fixture loading, tempdir index builder
        ├── fixtures_valid.rs      # manifest hashes (runs first)
        ├── index_query.rs         # Story 1
        ├── determinism.rs         # Story 2
        ├── mutation.rs            # Story 3
        ├── filters.rs             # Story 4 goldens
        ├── filter_algebra_prop.rs # Story 4 properties (FR-023)
        ├── stats.rs               # Story 5 + #[ignore] divergence measurement (FR-025)
        ├── concurrency.rs         # Story 6
        ├── analyzer_prop.rs       # invariant: analyzer determinism
        └── roundtrip_prop.rs      # invariant: add → commit → reopen

reference/
├── xtref/
│   ├── __init__.py
│   └── bm25.py                    # analyzer chains, FIELD_NORMS_TABLE, idf, BM25 — extracted from 001
├── gen_001_fixtures.py            # now imports xtref; fixtures regenerate byte-identically (quickstart Step 2)
├── gen_002_fixtures.py            # corpus, schema, queries, filters, stats, mutations, manifest; --verify-ranking
├── requirements-002.{in,txt}      # 001's pins + snowballstemmer
└── fixtures/002/                  # schema.json corpus.json queries.json filters.json stats.json mutations.json manifest.json

docs/adr/
├── 0005-tie-breaking-contract.md  # + Amendment section (proposed)
└── 0006-defer-beir-eval-gate.md   # proposed
```

**Structure Decision**: One stage crate, `xtriever-lexical`, depending on `xtriever-core` alone —
direction `core ← lexical`, nothing upward. `xtriever-ffi` is not touched (FR-037). The seven
`src/` modules follow the trait's own seams (schema, query, filter, search, stats) so a reader can
find the code for a requirement by name; none is a trait or generic — they are plain functions over
`&FieldMap` and `&Searcher`.

### PR split (Rule 3)

| PR | contents | ends at |
|---|---|---|
| 1 | `reference/xtref/` extraction + 001 byte-neutrality proof; `gen_002_fixtures.py` + fixtures; `Cargo.toml` deps; **all** `tests/` files; `examples/gen_ranking.rs` (compiles against the trait, fails at runtime) | red checkpoint (quickstart Step 4) |
| 2 | `schema.rs`, `error.rs`, `index.rs` (create/open/add/delete/commit/schema), `search.rs`, `query.rs` for `Match`/`Term` only | Story 1 (Match/Term), Story 3, round-trip property green |
| 3 | `query.rs` remainder (`Phrase`/`Fuzzy`/`Bool`/`Boost`), `filter.rs`, `FilterCollector` wiring; mint `queries.json` rankings and cross-check | Stories 1, 4 fully green; filter algebra property green |
| 4 | `stats.rs`, `merge`, Story 2 + 6 tests green, FR-025 measurement, `report.md`, ADR-0005 amendment accepted + core doc comment, ADR-0006 status | all 13 SCs; full gate |

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| BEIR eval clause not met for a ranking-creating change | II | `xtriever-eval` is a placeholder and there is no baseline to delta against | Building a SciFact smoke here doubles scope and ships an unreviewed harness; N/A-ing it (001's route) is dishonest for a change that creates ranking | [ADR-0006](../../docs/adr/0006-defer-beir-eval-gate.md) — Accepted 2026-09-12 |
| Doc-comment narrowing of `LexicalIndex::search` / `VectorIndex::search` | V | FR-014 accepts the backend's k-boundary; the unqualified promise in core would otherwise be false | Leaving the comment as-is documents a promise the implementation cannot keep; over-fetching to keep it was rejected in clarification Q1 | [ADR-0005 amendment](../../docs/adr/0005-tie-breaking-contract.md) — Accepted 2026-09-12 |
