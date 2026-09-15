# Implementation Plan: Lexical Quality — One Field for BM25

**Branch**: `013-lexical-quality` | **Date**: 2026-09-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/013-lexical-quality/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

The evaluation's lexical baseline indexes `title` boosted 2.0 beside `text`; the 012 spike
showed, by rebuilding that exact shape in Python, that this layout — not the analyzer, the BM25
parameters or stop words — costs 6.0 nDCG points on SciFact and 1.1 on NFCorpus against one
joined `contents` field (BEIR's own reference layout). This feature adds `Source::TitleAndText`
and the v2 configurations (`lexical-baseline-v2`, `hybrid-baseline-v2`, `hybrid-rerank-v2`),
records their baselines on the three sets with verified deltas against v1, moves the CI smoke
to the v2 lexical baseline, keeps v1 runnable and byte-identical, and documents the
recommendation. No engine change.

## Technical Context

**Language/Version**: Rust 1.91.1, edition 2024. **Primary Dependencies**: none new.
**Storage**: none new (baseline JSON files). **Testing**: `cargo nextest` (eval crate), the
003 reference scorer (`--verify-run`), `beir compare`. **Target Platform**: host; CI smoke on
ubuntu. **Project Type**: evaluation configuration + baselines. **Performance Goals**: SC-001
floors, SC-002 fused means. **Constraints**: `xtriever-eval` only; v1 untouched; one dataset
in CI. **Scale/Scope**: ~120 lines of Rust and tests; 9 baselines; ~1 h of local eval runs.

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | | **PASS** | No component is built: an evaluation configuration (one field instead of two) and baselines; BM25 stays tantivy's. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | | **PASS** | Tests for the joined field and the v2 configurations' identity to v1 committed first (red); the baselines are verified by the 003 reference to 1e-6; deltas for lexical / hybrid / rerank on all three sets in the PR (the feature *is* a ranking change — of the evaluation's field layout). |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | | **N/A** | Nothing under the pure crates or the device changes; the eval crate stays `std`-only (one enum variant, three constructors). |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | | **PASS** | The motivating numbers are the spike's measured attribution (research D1); the feature's own numbers are the harness's baselines, verified and compared; no latency claim. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | | **PASS** | Core traits, format, errors untouched; `xtriever-eval` is above the pipeline and gains additive configurations; v1 kept. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | | **PASS** | Determinism unchanged (the same engine, one more schema shape); the baselines reproduce. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | | **PASS** | No new dependency; the eval crate's lints unchanged; every public item documented. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | | **PASS** | Every item the plan relies on is in the workspace and cited by path: `Source`/`FieldSpec`/`EvalConfig` (`xtriever-eval/src/run.rs:20–56`), `document_fields` (`:449–461`), `lexical_baseline_v1` (`:81–100`), `hybrid_baseline_v1` (`:436–445`), `hybrid_rerank_v1` (`:555`), the example's dispatch and `dense_fields` (`examples/beir.rs:110–113`, `:487–491`), the index-dir rebuild (`:474–481`), `smoke` (`src/report.rs:302–309`; FR-024, tolerance 0). |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | | **PASS** | Neither `xtriever-core` nor `deny.toml` is touched; the gate checks `git diff --stat main -- crates/ | grep -v xtriever-eval` empty. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | | **PASS** | ~120 lines of Rust + tests, nine baseline files, docs; one PR. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | | **PASS** | Phase 2 commits the v2 tests not compiling; the constructors and the variant follow. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | | **PASS** | Quickstart Steps 2–4: the nine baselines with verify-run and compare deltas, v1 reproduction, the full gate, CI smoke on push. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | | **PASS** | SC-001's floors and SC-002 are stop-and-report; the smoke's tolerance stays 0. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | | **PASS** | An enum variant and three constructors. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-16, agent.

**Post-design gate (post-Phase 1)**: PASS — 2026-09-16, agent (research D1–D7, data-model, contract; no row changed).

## Project Structure

### Documentation (this feature)

```text
specs/013-lexical-quality/
├── plan.md · research.md · data-model.md · quickstart.md · contracts/eval-configs.md
├── baselines/              # lexical-baseline-v2, hybrid-baseline-v2, hybrid-rerank-v2 × scifact/nfcorpus/fiqa
├── report.md · pr-description.md · tasks.md
```

### Source Code (repository root)

```text
crates/xtriever-eval/
├── src/run.rs              # Source::TitleAndText; document_fields; lexical_baseline_v2; hybrid_baseline_v2; hybrid_rerank_v2
├── src/lib.rs              # crate docs: the v2 configurations
├── examples/beir.rs        # name dispatch; dense_fields from the lexical configuration
└── tests/{run.rs, hybrid_run.rs}   # the v2 tests (field construction; identity to v1 but the fields)
.github/workflows/ci.yml    # smoke: --config lexical-baseline-v2 --baseline specs/013-lexical-quality/baselines/lexical-baseline-v2.scifact.json
python/README.md · crates/xtriever-cli/src/wiki/chunking.rs (comment) · specs/012-sparse-spike/report.md (F-002 resolved)
```

**Structure Decision**: `xtriever-eval` only, plus documentation and the CI smoke line.

## Complexity Tracking

No violations.
