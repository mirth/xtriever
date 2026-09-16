# Implementation Plan: Re-rank Depth Study

**Branch**: `014-rerank-depth-study` | **Date**: 2026-09-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/014-rerank-depth-study/spec.md`

## Summary

A measurement, no engine change: one end-to-end re-rank run per dataset at depth 50 with the
explain export, from which the replace-order lists at depths 5 / 10 / 20 and the interpolation
variants (rank fusion; linear at α 0.25 / 0.5 / 0.75) are derived offline — exactly, because
the pinned re-ranker scores every pair alone (research D2) — and scored by the 003 reference.
Depth 20 must reproduce the 013 baseline and depth 5 an end-to-end SciFact run. The decision
follows the rule fixed in the spec (mean ≥ 0.4768 + 0.005 and no dataset > 0.005 below depth
0). Rust change: a `--rerank-depth` flag and a `fused_scores` key in `examples/beir.rs`;
the derivation and the table in `reference/rerank_study.py` with its tests.

## Technical Context

**Language/Version**: Rust 1.91.1 (edition 2024, `rust-toolchain.toml`) for the example flag; Python 3.12 (`reference/.venv-012` (012's environment: `numpy 2.5.3`, `pytrec_eval 0.5`, `pytest`; `.venv-003` lacks pytest)) for the derivation and scoring

**Primary Dependencies**: the pinned re-ranker (`ms-marco-MiniLM-L-6-v2`, 006 manifest), the 004 vector cache, the 013 `hybrid-baseline-v2` / `hybrid-rerank-v2` baselines and the exported depth-20 runs; `reference/gen_003_fixtures.py` (scorer) and `reference/gen_006_fixtures.py` (`order_reranked`) imported

**Storage**: explain exports and derived runs under `target/xt-rerank-study/<dataset>/` (git-ignored); reports and the table under `specs/014-rerank-depth-study/runs/`

**Testing**: `pytest` on `reference/tests_014/` (derivation rules, 006 order cases, ties, constant columns, round trip), `cargo nextest` unchanged; the oracles are the 013 baselines and the end-to-end runs

**Target Platform**: host only (a study); nothing under the pure crates changes

**Project Type**: evaluation study (example binary flag + reference script)

**Performance Goals**: none claimed; wall time ≈ 2 h for the depth-50 runs (research D8, spec SC-006)

**Constraints**: no model, engine, format, FFI, default or baseline change (spec FR-007, FR-008, SC-005); the decision rule fixed before the runs (FR-006)

**Scale/Scope**: 3 datasets × (1 depth-50 run + 1 depth-5 check) end to end; 3 × 5 variants × 4 depths derived; ~15 lines of Rust, ~350 lines of Python + tests

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | Nothing is built: the pipeline's re-ranker and fusion are reused; the derivation reuses the engine's own order rule (006 reference) rather than a second one. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | Derivation tests committed first, failing; every cell verified by the 003 reference to 1e-6; derived depth 20 must equal the 013 baseline exactly and depth 5 an end-to-end run; deltas for every row on the three sets in the report and PR. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | **PASS** | No crate under Principle III changes; the only Rust change is in `xtriever-eval`'s example binary (a flag and one JSON key), which links the stage crates as dev-dependencies already. Target checks re-run in the gate. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | The feature is the measurement: pinned model revision, pinned datasets, reproducible derivation from an exported run; cost stated per row as cross-encoder calls per query (the depth), backed by 006's per-pair timing. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | No trait, format or error change; `SearchOptions::rerank_depth` is used as designed; the explain export gains a key, readers unaffected (research D3). |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | Determinism is what makes the study exact: per-pair scoring, ties by ascending `DocId` in both the engine and the derivation; the interpolation tie rule is stated and tested. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | Example code only (`anyhow` allowed); no new crate dependency (`cargo add` not needed); fmt / clippy / nextest / deny in the gate. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | `SearchOptions::rerank_depth` (`pipeline/src/search.rs:167–172`), `order_reranked` (`pipeline/src/rerank.rs:13–35`), `fused_terms` (`fusion.rs:23–45`), the explain export (`examples/beir.rs:546–599`), the scorer's per-pair contract (`rerank/src/scorer.rs:1–9`), `gen_006_fixtures.order_reranked` (`:311`), `gen_003_fixtures.reference` (`:70`) — all read and cited in research D1–D5. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither `xtriever-core` nor `deny.toml` is touched; SC-005's gate is `git diff --stat main -- crates/ ':!crates/xtriever-eval'` empty. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | ~15 lines of Rust, ~350 of Python and tests, reports and documents; one PR. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | Phase 2 commits `reference/tests_014/` failing (no `rerank_study` module); the script follows. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 4: the Rust gate, the pytest suite, every cell's verification, the two end-to-end derivation checks, the byte-identity of every earlier baseline; the table and the decision in the PR. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | Stop-points: a derivation mismatch (run every depth end to end instead), a Recall@100 that differs from depth 0 (a defect), a depth-20 row that does not reproduce 013. The decision rule is fixed before the runs and not revisited after. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | A flag, a JSON key, a script with four small functions and a table printer. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-16, Claude (plan author); no N/A rows, no Complexity Tracking entries.

**Post-design gate (post-Phase 1)**: PASS — 2026-09-16, Claude, after research D1–D9 and the Phase 1 artifacts: the design adds no dependency, touches no crate but the eval example, and keeps every oracle.

## Project Structure

### Documentation (this feature)

```text
specs/014-rerank-depth-study/
├── plan.md              # This file
├── research.md          # D1–D9
├── data-model.md        # variants, cells, the decision
├── quickstart.md        # the runs, the derivation, the gate
├── contracts/study-cli.md
├── runs/                # the table (JSON + markdown) and per-cell reports — committed
├── report.md            # the table, the decision, findings
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/xtriever-eval/examples/beir.rs     # --rerank-depth N (overrides the configuration's depth; report name @dN);
                                          # "fused_scores" key in the explain export
reference/rerank_study.py                 # derive | score | table | decide (contracts/study-cli.md)
reference/tests_014/{conftest,test_order,test_variants,test_table}.py
target/xt-rerank-study/<dataset>/         # explain exports, derived runs (git-ignored)
```

**Structure Decision**: one example-binary change in `xtriever-eval` (dev-dependencies on
the stage crates already exist there — the dependency direction is unchanged) and a
reference script beside the 003 / 006 generators it imports. No library code changes anywhere.

## Complexity Tracking

None: every gate row is PASS without deviation.
