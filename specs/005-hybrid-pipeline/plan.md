# Implementation Plan: The Hybrid Pipeline

**Branch**: `005-hybrid-pipeline` | **Date**: 2026-09-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/005-hybrid-pipeline/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Implement `xtriever-pipeline`: `HybridIndex`, the composition root that owns a Feature 002
`TantivyIndex`, a Feature 004 `FlatIndex` and a caller-supplied `Embedder` under one directory
with a versioned descriptor and a persisted external-id ↔ `DocId` map. Ingest assigns internal
ids (never reused), embeds the configured text fields, feeds both stages and commits them in a
fixed order whose only possible partial states an open-time four-count check refuses. Search
resolves a filter once into a `DocSet` applied to both stages, runs lexical then dense to a
candidate depth, fuses by reciprocal rank fusion (`k = 60`, `f64`, `(score DESC, DocId ASC)`),
and degrades to the lexical list when the dense stage fails or a caller-supplied elapsed-time
source says the budget is spent — or errors in strict mode. Every hit can explain itself under
the core's feature names. The 003 harness gains a stage-agnostic closure runner, a
`hybrid-baseline-v1` configuration that reuses the 004 embedding cache by value (0 documents
embedded), a cross-configuration `compare`, and the three fused baselines that the regression
rule guards from now on. No new external dependency; no `unsafe`; no clock in the crate.

## Technical Context

**Language/Version**: Rust, edition 2024, toolchain 1.91.1

**Primary Dependencies**: `xtriever-pipeline` — `xtriever-core`, `xtriever-lexical`,
`xtriever-dense` (path), `serde`, `serde_json`, `thiserror` (all already in the workspace, added
with `cargo add`); feature `mmap = ["xtriever-dense/mmap"]`. Dev: `tempfile`, `proptest`,
`serde_json`, `sha2`. `xtriever-eval` — dev-dependency on `xtriever-pipeline` for the example;
library graph unchanged. Python oracle — standard library + NumPy in the existing
`reference/.venv-003` (research D9).

**Storage**: one directory per hybrid index: `xtriever-pipeline.json`, `ids.json`, `lexical/`
(002 format), `dense/` (004 format v1). Baselines under `specs/005-hybrid-pipeline/baselines/`.

**Testing**: `cargo nextest`; offline suite with the real stages underneath and a table-driven
stub embedder (fusion goldens, round-trip, reopen, filters, degradation with a stub clock,
explanation, partial-commit detection, property tests: fusion order total and permutation-safe,
id map round-trip); one `#[ignore]` model-backed round-trip; ranking via `xtriever-eval` on the
three datasets with `--verify-run` (metrics) and `--verify-fusion` (fusion order on real data).

**Target Platform**: host for everything that runs; `cargo check` on the three mobile targets;
wasm32 best-effort (unchanged failure point, `getrandom` via candle).

**Project Type**: library crate + additive harness change + example subcommands + Python
generator.

**Performance Goals**: none claimed. SC-010 records FiQA ingest and per-query timings, directory
and id-map sizes, peak RSS; the `Filter::Ids` cost is measured once (research D6/D12).

**Constraints**: `xtriever-pipeline` pure and `std`-only — no C/C++, no async, no threads of its
own, **no `Instant`/`SystemTime`** (time comes from the caller, research D7); `xtriever-core`,
`xtriever-lexical`, `xtriever-dense`, eval `metrics.rs`/`dataset.rs`, `deny.toml` untouched
(FR-027); dependencies downward (`pipeline → {lexical, dense} → core`; example `eval → pipeline`
dev-only).

**Scale/Scope**: one crate implemented (~900 lines), one crate extended (~250), example (~200),
generator (~300), three fixture files, three baselines. **Over Rule 3's ~800 lines — four PRs.**

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.2.0, ratified 2026-09-10, last amended 2026-09-12).
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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | Nothing commodity is built: the stages are 002 and 004 unchanged, `roaring` sets flow through `DocSet`. What is written — id map, descriptor, RRF, degradation, explanation — is exactly the innovation budget the constitution names ("pipeline orchestration, fusion"). |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | PR 1 is goldens + red suite. Fusion has an independent Python reference (`gen_005_fixtures.py`, research D5) with 10 named cases at 1e-9 and a real-data `--verify-fusion` check; the dense oracle in `hybrid.json` proves the pipeline handed the stage the right vectors. Ranking: three hybrid baselines, `--verify-run`-checked, with `compare` tables against **both** stage baselines and the SC-011 verdict — the first delta-guarded number. Properties: fusion order total and permutation-invariant, id map round-trip, filtered ⊆ set. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 300 MB for a 100k-chunk index including loaded models). | **PASS** | `xtriever-pipeline` adds no dependency outside the workspace; lexical and dense are pure Rust. **No clock**: time budgets use a caller-supplied `Fn() -> Duration` (D7) — the reason `core::Budget` is a `Duration`; quickstart Step 6 greps the crate for `Instant`/`SystemTime`/`std::thread`. `xtriever-eval`'s library graph is unchanged; the pipeline is a dev-dependency of the example only. **RSS clause**: FiQA numbers recorded (SC-010); no on-device claim. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | No performance claim; observations with method (D12). Reproducibility: the same pins as 003/004 plus the 004 cache key (fingerprint + corpus hash) guarding the reused vectors; generator seeded and hashed. **`explain()` implemented for the first time**: `HitExplain` carries both stages' scores and 1-based ranks and the fused score under `features::{BM25_SCORE, BM25_RANK, DENSE_SCORE, DENSE_RANK, FUSED_SCORE}`, `NaN` for absent per the core's convention (D8). |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | Core untouched (FR-027); no new `Error` variant (contract "Error mapping"). Two **new** on-disk files (descriptor, id map), both versioned, both the pipeline's own — the stage formats are not touched. **The id boundary is implemented here**: the map lives in the pipeline, both stages receive `DocId`s only (D4), and the harness now hands the pipeline BEIR string ids instead of doing the mapping itself. Direction: `pipeline → {lexical, dense} → core`; the harness reaches the pipeline through a closure runner that names no pipeline type (D10). Synchronous throughout. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | **Degradation implemented for the first time** (D7): dense failure or a spent time budget ⇒ the lexical list, marked on the response; strict ⇒ the error (or `BudgetExhausted`); lexical failure ⇒ error in every mode. Determinism: RRF in `f64` in a fixed term order, `(fused DESC, DocId ASC)`, stage results already deterministic (002/004), tests across reopening. Descriptor carries `format_version`, `schema`, `embedder_fingerprint`; open refuses version, schema, fingerprint and partial-commit states naming both sides (D3). |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | Only workspace dependencies, added with `cargo add`; core `Error` for library errors; `anyhow` only in the example; **zero `unsafe`** in the pipeline (the `mmap` feature merely forwards to the dense crate's already-admitted block); `missing_docs` on; `deny.toml` unchanged. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | No external crate item is new; every workspace item the pipeline calls is cited by `crate/path:line` in research D1, D3, D5–D8 (stage constructors and methods, `DocSet::iter`, `Budget`, `features::*`, `BudgetExhausted`). |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither is touched. The one place a core change would have helped — a `LexicalIndex::search_in(&DocSet)` to avoid materialising `Filter::Ids` — is explicitly **not** done; the cost is measured instead (D6). |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Branch `005-hybrid-pipeline`. **PR 1** goldens, generator, scaffold, red suite (~900 incl. tests); **PR 2** descriptor, id map, ingest, commit, open — US1 green (~400); **PR 3** search, fusion, degradation, explain — US2–US4 green (~450); **PR 4** harness runner, `compare`, example, baselines, report (~500). |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | PR 1 ends at the red checkpoint on the `NotImplemented` scaffold, `check-no-stubs.sh` extended to the crate. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 6 plus the purity greps; the PR carries the three fused baselines, six `compare` tables and the SC-011 verdict — the first PR in this repository with a real delta section. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | SC-011 (fused ≥ better stage on ≥ 2 of 3) is a ⛔ stop-and-report by the spec's own words (user decision Q3 = A); the fusion tolerance is 1e-9 on `f64` values that are bit-identical by construction; `--verify-fusion` and `--verify-run` are cross-checks, not tunables. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | Concrete stage types, one boxed trait object, JSON id map, a three-line RRF, a closure for the clock and a closure for the harness runner instead of new traits; sequential ids with no reuse instead of a free-list. |

**Initial gate (pre-Phase 0)**: **PASS** on all 14 rows — 2026-09-13, Claude (agent). No ADR
required: no core change, no on-disk change to an existing format, no `unsafe`, no new
dependency, no commodity component built.

**Post-design gate (post-Phase 1)**: **PASS** on all 14 rows — 2026-09-13, Claude. Design added
two new pipeline-owned files (versioned) and the `add_embedded` ingest path for the cached
vectors; neither touches a row. `/speckit-tasks` may run.

## Project Structure

### Documentation (this feature)

```text
specs/005-hybrid-pipeline/
├── plan.md              # This file
├── research.md          # Phase 0 — D1–D12 + risks
├── data-model.md        # Phase 1 — index, descriptor, id map, options, response, goldens, harness additions
├── quickstart.md        # Phase 1 — Steps 0–7
├── contracts/
│   └── hybrid-pipeline.md
├── baselines/           # PR 4 — hybrid-baseline-v1.{scifact,nfcorpus,fiqa}.json
├── checklists/requirements.md
├── tasks.md             # /speckit-tasks
└── report.md            # closing report
```

### Source Code (repository root)

```text
crates/xtriever-pipeline/
├── Cargo.toml                 # xtriever-{core,lexical,dense}, serde, serde_json, thiserror; [features] default = [], mmap = ["xtriever-dense/mmap"]
├── src/
│   ├── lib.rs                 # crate docs; pub use; FORMAT_VERSION
│   ├── error.rs               # helpers → core Error::{Schema, Corrupt} (002/004 pattern)
│   ├── descriptor.rs          # Descriptor (serde), read/write via .tmp + rename, identity + version checks
│   ├── ids.rs                 # IdMap: Vec<Option<String>> + HashMap<String, u32>; assign/reuse/delete/live; read/write
│   ├── index.rs               # HybridIndex: create/open/open_mapped, add/add_embedded/delete/commit, four-count check
│   ├── fusion.rs              # rrf(): f64 sum in fixed order, (score DESC, id ASC), truncate
│   └── search.rs              # SearchOptions, Response family, the 8-step search with degradation and explain
└── tests/
    ├── support/mod.rs         # fixtures; TableEmbedder; FailingEmbedder; build_from_fixture(); direct stage openers
    ├── fixtures_valid.rs      # manifest hashes of reference/fixtures/005/*
    ├── fusion_golden.rs       # fusion.json: every case exact (SC-001); rrf() public fn
    ├── fusion_prop.rs         # proptest: order total, permutation of list contents ≠ order change, one-list-only ⊆
    ├── ingest.rs              # US1: round-trip 1,000 docs, replace, delete, unknown delete, empty id, batch dup, chunk provenance (SC-002)
    ├── persist.rs             # reopen identical (SC-003); descriptor version; schema/fingerprint mismatch; partial-commit → Corrupt with four counts; stale handle
    ├── search.rs              # US2: composition check vs direct stage searches; dense oracle via explain; filters (SC-004); k=0; empty query; empty set; depth<k
    ├── degrade.rs             # US3: failing embedder default/strict; stub clock over budget at A and at B; lexical failure errors; time limit ignored w/o clock; max_items caps depth (SC-005)
    ├── explain.rs             # US4: features() names and NaN; explain never changes hits (SC-006); degraded explain
    └── model_roundtrip.rs     # #[ignore]: MiniLmEmbedder-backed tiny index; real fingerprint mismatch

crates/xtriever-eval/
├── src/run.rs                 # + HybridConfig, hybrid_baseline_v1, build_external, execute_external
├── src/report.rs              # + Comparison, compare()
├── examples/beir.rs           # + --config hybrid-baseline-v1 (cache → add_embedded), --export-explain, compare
└── tests/{hybrid_run,report}.rs   # + runner with a closure, build_external goldens, compare has no trigger and names both configs

reference/
├── gen_005_fixtures.py        # fusion.json, hybrid.json, manifest.json; --verify-fusion
└── fixtures/005/{fusion,hybrid,manifest}.json

scripts/check-no-stubs.sh      # + xtriever-pipeline
```

**Structure Decision**: `xtriever-pipeline` is implemented in place (placeholder crate exists);
it depends on both stage crates and on core, nothing depends on it except the harness example
(dev-only). The harness library stays stage-agnostic through `execute_external`'s closure.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| *(none)* | — | — | — | — |

## Decisions this plan takes that the spec left open (recorded for the human)

1. **`HybridHit.score` is `f64`** (D5) so the spec's 1e-9 fusion tolerance is meaningful; core
   `Hit.score` stays `f32`; the FFI feature decides the wire type.
2. **Ids are never reused after delete** (D4) — safety over compactness at 4.29 billion ids.
3. **Partial commits are detected, not repaired** (D3) — a four-count check at open.
4. **`Filter::Ids` materialisation** for the lexical stage (D6) instead of a core trait change;
   its cost is measured, not assumed.
5. **Bring-your-own-embedding ingest** (`add_embedded`, D10) — how the harness reuses the 004
   cache with 0 documents embedded; a legitimate path for offline embedding, not a test hook.
6. **`compare` beside `delta`** (D10) — cross-configuration comparison without an ADR trigger;
   `delta`'s refusal (004 FR-021) is untouched.
7. **A time budget exceeded *after* the dense stage discards its candidates** (D7) — the budget
   is a promise about the result, not the attempt.
8. **Strict mode + spent time budget ⇒ `BudgetExhausted`** (D7), the core's existing variant.
