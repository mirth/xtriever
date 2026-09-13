# Implementation Plan: [FEATURE]

**Branch**: `[###-feature-name]` | **Date**: [DATE] | **Spec**: [link]

**Input**: Feature specification from `/specs/[###-feature-name]/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

[Extract from feature spec: primary requirement + technical approach from research]

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: [e.g., Rust 1.8x, edition 2024 or NEEDS CLARIFICATION]

**Primary Dependencies**: [e.g., tantivy, tokenizers, candle, roaring or NEEDS CLARIFICATION]

**Storage**: [if applicable, e.g., on-disk index segments, files or N/A]

**Testing**: [e.g., cargo nextest, proptest, criterion, xtriever-eval or NEEDS CLARIFICATION]

**Target Platform**: [host + aarch64-apple-ios, aarch64-apple-ios-sim, aarch64-linux-android; wasm32 best-effort]

**Project Type**: [e.g., library/cli/ffi or NEEDS CLARIFICATION]

**Performance Goals**: [p50/p99 latency, index size, RSS — state budgets or NEEDS CLARIFICATION]

**Constraints**: [e.g., RSS ceiling, no async, no C/C++ deps in pure crates or NEEDS CLARIFICATION]

**Scale/Scope**: [e.g., 100k chunks, N crates touched or NEEDS CLARIFICATION]

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | | |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | | |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | | |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | | |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | | |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | | |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | | |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | | |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | | |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | | |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | | |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | | |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | | |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | | |

**Initial gate (pre-Phase 0)**: [PASS / FAIL — date + who evaluated]

**Post-design gate (post-Phase 1)**: [PASS / FAIL — date + who evaluated]

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused crates and expand the chosen structure with
  real paths. Dependencies point downward only (Principle V).
-->

```text
crates/
├── xtriever-core/       # traits, DocId, errors — pure Rust, std-only
├── xtriever-analysis/   # analyzers/tokenization — pure Rust, std-only
├── xtriever-lexical/    # tantivy-backed BM25
├── xtriever-dense/      # embedders, vector index (C/C++ deps behind features)
├── xtriever-rerank/     # cross-encoder re-ranking (C/C++ deps behind features)
├── xtriever-ltr/        # learning-to-rank — pure Rust, std-only
├── xtriever-pipeline/   # orchestration, fusion, id mapping — pure Rust, std-only
├── xtriever-eval/       # BEIR harness, nDCG/Recall — pure Rust, std-only
├── xtriever-ffi/        # FFI surface, async wrappers
└── xtriever-cli/        # binaries (anyhow allowed here)

reference/               # Python scripts producing golden fixtures
docs/adr/                # ADRs required by Principles I, II, V
```

**Structure Decision**: [Document which crates this feature touches and why, and confirm the
dependency direction stays `core` ← stage crates ← `pipeline` ← `ffi`/`cli`]

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| [e.g., custom ANN graph] | I | [measured requirement the reused crate fails] | [why `tantivy`/`candle` insufficient] | [docs/adr/NNNN-*.md] |
| [e.g., trait signature change] | V | [specific problem] | [why an additive API insufficient] | [docs/adr/NNNN-*.md] |
