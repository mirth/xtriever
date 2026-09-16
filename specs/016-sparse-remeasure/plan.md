# Implementation Plan: Sparse Stage Re-measurement

**Branch**: `016-sparse-remeasure` | **Date**: 2026-09-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/016-sparse-remeasure/spec.md`

## Summary

An offline measurement from artefacts already on disk: the engine's own lexical-v2 and dense
candidate lists (014's explain exports) fused with 012's sparse dot list under the engine's
RRF (ties by corpus position), then re-ranked under the engine's interpolating rule (014's
derivation code, verified in 015) using the engine's cross-encoder scores where the 014
exports cover the pair and the 006 torch reference where they do not — counted and its
agreement measured. Two reproduction checks anchor it: `rrf(lex2, dense)` = `hybrid-baseline-v2`
and its re-ranking = `hybrid-rerank-v3`, per query and list for list. The decision rule
(re-ranked three-way mean ≥ 0.4963, no dataset > 0.005 below v3) is applied mechanically and
recorded. Python only; no Rust, no model change, no new environment.

## Technical Context

**Language/Version**: Python 3.12 in `reference/.venv-012` (`numpy 2.5.3`, `pytrec_eval 0.5`, `torch 2.14.0`, `transformers 5.17.0`, `pytest`)

**Primary Dependencies**: `reference/rerank_study.py` (the interpolating rule, `compare_runs`, `corpus_positions`), `reference/gen_003_fixtures.py` (scorer), `reference/gen_006_fixtures.py` (`Reference` cross-encoder), the pinned `ms-marco-MiniLM-L-6-v2`

**Storage**: runs under `target/xt-sparse-remeasure/<d>/` (git-ignored); cells, table, decision under `specs/016-sparse-remeasure/runs/`

**Testing**: `pytest reference/tests_016/` (fusion ties, score sourcing, decision rule, synthetic end-to-end); the reproduction checks against the real artefacts in the quickstart

**Target Platform**: host only

**Project Type**: evaluation study (reference script)

**Performance Goals**: none claimed; reference scoring of uncovered pairs is the only model work (minutes to an hour)

**Constraints**: nothing under `crates/` changes; no baseline changes; the decision rule fixed before the runs (FR-007)

**Scale/Scope**: 4 variants × 2 × 3 datasets = 24 cells; ~350 lines of Python + ~150 of tests; report, table, decision

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.4.0, ratified 2026-09-10, last amended 2026-09-13). Every row below MUST be
marked **PASS**, **FAIL**, or **N/A** with a one-line justification — an empty verdict is a FAIL.

- Any **FAIL** blocks the phase. Do not proceed, and do not weaken the gate to make it pass.
- Any **N/A** MUST state why the principle cannot apply to this feature.
- A violation that is genuinely required is recorded in Complexity Tracking below and REQUIRES an
  accepted ADR in `docs/adr/`; the row stays **FAIL** until that ADR is merged.
- These are plan-time gates. They do not replace the blocking CI gates in the constitution's
  "Quality Gates (CI)" table.

### Core Principles

| # | Principle | Gate | Verdict | Justification |
|---|---|---|---|---|
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | Nothing is built: the engine's fusion and re-rank rules are re-implemented only to measure a list the engine cannot yet fuse, and both re-implementations must reproduce the engine's baselines exactly. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | Tests first (fusion ties, score sourcing, rule); the two reproduction checks are executable oracles on the real baselines; every cell scored by the 003 reference; deltas per dataset in the report and PR. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | **PASS** | No crate changes at all; `git diff --stat main -- crates/` empty is the gate. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | The feature is the measurement; the reference scorer's contribution is counted and its agreement with the engine measured on this corpus, so the estimate's error is stated, not assumed. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | No trait, format or default change. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | Deterministic offline computation; the engine's tie rules are applied and checked by reproduction. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | No Rust; the Rust gate still runs unchanged. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | `fused_terms` (`pipeline/src/fusion.rs:23–45`), the explain export keys (`eval/examples/beir.rs`), `rerank_study.{head,rest,order_lin,minmax,compare_runs,corpus_positions}`, `gen_006_fixtures.Reference.score` (`:117–150`, `TOLERANCE_ABS = 1e-3`), `gen_003_fixtures.reference` — read and cited in research D1–D4. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Nothing under `crates/`; `deny.toml` untouched. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | ~500 lines of Python and tests, 24 small cells, the report; one PR. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | Phase 2 commits `reference/tests_016/` failing (no `sparse_remeasure` module). |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 4: the Rust gate (unchanged code), `pytest reference/tests_016`, the reproduction checks, `git diff --stat main -- crates/ specs/*/baselines specs/014-*/runs` empty; deltas in the PR. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | Stop-points: a reproduction mismatch (fix the fusion / re-rank code, never the anchor); a Recall@100 that differs between a variant and its re-ranked form; the rule is not revisited after the numbers. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | One script with five functions and a table printer, reusing two existing modules. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-16, Claude (plan author).

**Post-design gate (post-Phase 1)**: PASS — 2026-09-16, Claude, after research D1–D8 and the Phase 1 artifacts.

## Project Structure

### Documentation (this feature)

```text
specs/016-sparse-remeasure/
├── plan.md · research.md · data-model.md · quickstart.md
├── contracts/remeasure-cli.md
├── runs/                # 24 cells, table.{json,md}, decision.json — committed
├── report.md · pr-description.md
└── tasks.md
```

### Source Code (repository root)

```text
reference/sparse_remeasure.py         # fuse | rerank | score | check | table | decide | all
reference/tests_016/{conftest,test_fuse,test_scores,test_decide,test_end_to_end}.py
target/xt-sparse-remeasure/<d>/       # fused and re-ranked runs, the reference-score cache (git-ignored)
```

**Structure Decision**: one reference script beside the 012 spike and the 014 study it
imports from; nothing under `crates/`.

## Complexity Tracking

None.
