# Implementation Plan: The Demo Re-ranks at Depth 10

**Branch**: `018-demo-rerank-depth-10` | **Date**: 2026-09-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/018-demo-rerank-depth-10/spec.md`

## Summary

An app-level default: the iOS Wikipedia demo's `Settings.rerankDepth` becomes 10 with a
0 / 5 / 10 / 20 picker, the Settings footer and About explain the trade-off with 014's and
017's numbers (−0.3 mean nDCG@10; 1.4 s instead of 2.3 s on the reference phone; engine
default 20), a red `SettingsTests` pins the default and persistence, one demo-measurement
device run at the new default is committed beside 009's, and the README and 009 documents
point here. Nothing under `crates/`, `swift/` or any baseline changes.

## Technical Context

**Language/Version**: Swift 6 (the demo app and its tests), Markdown

**Primary Dependencies**: unchanged — the Xtriever Swift package, the two models, the 008 Wikipedia index; xcodegen for the demo project; the iPhone

**Storage**: the demo's `UserDefaults` settings blob (unchanged shape); the device record under `specs/018-demo-rerank-depth-10/runs/`

**Testing**: the demo's XCTest target on the simulator (`SettingsTests` red → green; existing suites); `DemoMeasurementTests` on the device (owner)

**Target Platform**: iOS (the demo); nothing else touched

**Project Type**: application setting + record

**Performance Goals**: the record — median re-ranked ≥ 30 % below 009's 2,288 ms, fused within 20 % of 339 ms, 009's ≤ 1 s / ≤ 3 s figures hold, footprint < 600 MB

**Constraints**: engine default stays 20; no goldens, baselines or harness tests move; identifiers only on the command line

**Scale/Scope**: ~15 lines of app Swift, ~30 of test Swift, ~30 of Markdown, one JSON record

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | Nothing built. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | `SettingsTests` committed red first; the device record is the executable evidence; no ranking change (the engine is untouched) — the quality cost is 014's measured number, cited, not re-measured. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | **PASS** | No crate changes; the device record re-checks the footprint under the ceiling. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | The default is chosen on 014's and 017's measurements and the app-level effect is recorded on the device. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | No interface change; an app constant. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | Unchanged. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | No Rust; the gate runs unchanged. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | `Settings` / `SettingsStore` (`apps/ios-wiki-demo/App/Model/Settings.swift`), `SettingsView`, `AboutView`, the 009 device command and extractor — research D1–D4. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Nothing under `crates/`. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | ~75 lines plus one record. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | `SettingsTests` red before the constant changes. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 4: the demo suite on the simulator, the device run, the Rust gate unchanged, `git diff --stat main -- crates/ swift/ specs/*/baselines` empty. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | The 009 acceptance figures are re-checked, not moved; a record failing them is stop-and-report. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | Two constants, a footer, a row, a test. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-16, Claude (plan author).

**Post-design gate (post-Phase 1)**: PASS — 2026-09-16, Claude, after research D1–D6.

## Project Structure

### Documentation (this feature)

```text
specs/018-demo-rerank-depth-10/
├── plan.md · research.md · data-model.md · quickstart.md
├── runs/<device>-<timestamp>-mmap-threadsdefault.json   # the demo measurement (owner's run)
├── report.md · pr-description.md
└── tasks.md
```

### Source Code (repository root)

```text
apps/ios-wiki-demo/App/Model/Settings.swift        # rerankDepth = 10; depths [0, 5, 10, 20]; doc comment
apps/ios-wiki-demo/App/Views/SettingsView.swift    # the trade-off footer with the numbers
apps/ios-wiki-demo/App/Views/AboutView.swift       # "re-rank depth (engine default)" + "app default" rows
apps/ios-wiki-demo/Tests/SettingsTests.swift       # new (red first)
apps/ios-wiki-demo/README.md · specs/009-ios-wiki-demo/{spec,report}.md
```

**Structure Decision**: app code and app tests only; the engine, the Swift package and its
goldens are untouched.

## Complexity Tracking

None.
