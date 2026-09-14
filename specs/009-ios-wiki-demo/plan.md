# Implementation Plan: The iOS Wikipedia Demo App

**Branch**: `009-ios-wiki-demo` | **Date**: 2026-09-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/009-ios-wiki-demo/spec.md`

## Summary

A thin SwiftUI app at `apps/ios-wiki-demo/` over the 007 package and the 008 index: it opens
the bundled index in place (both models mapped), warms it once, and for each submitted query
runs a fused search (shown at once, ~0.35 s) followed by the re-ranked search (replacing the
list with change marks, ~2.3 s), with per-hit explanations, the engine's stage report, a
settings screen (depth / budget / strict) and an About screen with the corpus identity and the
CC BY-SA attribution. The app owns no retrieval logic; its own logic (a state machine, the
two-phase search with cooperative cancellation, change marks, display fidelity) is tested on
the simulator against the 007 fixture; one device run of the full app is recorded under the
600 MB ceiling. No engine crate changes.

## Technical Context

**Language/Version**: Swift 5.9, SwiftUI, iOS 16 floor (the package's); xcodegen for the project; Xcode 26.x toolchain already in use.

**Primary Dependencies**: the `Xtriever` package (`XtrieverIndex`, `SearchOptions`, `SearchResponse`, `Hit.titleAndPassage/wikipediaURL`, `HitExplain.features()`, `HarnessResources`, `Measure`) — nothing else.

**Storage**: the package's staged resources in the app bundle; `@AppStorage` for three settings.

**Testing**: app-hosted XCTest on the simulator (Debug) against the 007 fixture index + goldens; a device-only measurement test in the 007/008 record shape.

**Target Platform**: iOS 16+, arm64 device and simulator; sideload.

**Project Type**: app (SwiftUI), no library changes.

**Performance Goals**: after warm-up, median fused ≤ 1 s and median total (fused + re-ranked) ≤ 3 s over the 20 measurement queries at default settings on the reference device; main thread never stalls > 100 ms (timer-cadence method); peak footprint < 600 MB.

**Constraints**: no retrieval logic in the app; no network but the article link; submit-only search; FFI and package sources unchanged except what this plan names (nothing); CI builds nothing for the app.

**Scale/Scope**: ~1,200 lines of Swift (model ~300, views ~500, tests ~400), `project.yml`, a `--demo` flag in the build script, `.gitignore` line.

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | The app builds nothing that retrieves; every stage is the engine's through the 007 package. The one algorithm the app adds is change marks over two ranked lists (D3). |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | App tests red first (PR 1); display fidelity pinned to the 007 fixture goldens (`withoutReranker` / `withReranker`, bit-exact score bits — SC-003); change marks tested by hand cases and the goldens. Not ranking-affecting: no eval delta due, stated in the PR. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | **PASS** | No crate is touched. The full app is measured on the device against 600 MB (SC-005) with the 008 corpus — larger than the reference configuration, stated. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | The spec's latency and footprint targets come from 008's device records, are re-measured by `DemoMeasurementTests` in the same record shape, and the record is committed; `explain()` is what the hit detail shows — first-class on screen. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | Nothing under `crates/` or `swift/Xtriever/Sources/` changes; the app sits above the FFI. The two-searches-per-query design is chosen precisely to avoid a "re-rank this list" entry point in the pipeline (D2). |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | The app shows degradation as the report says it (reason on screen), errors only in strict mode; the engine's open refusals are displayed with the engine's message (edge case 2). |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | No Rust changes; the Rust gate is re-run unchanged. Swift: no force-unwraps outside tests except `Link(destination:)` guarded by `if let`. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | Package API read from `swift/Xtriever/Sources/Xtriever/{XtrieverIndex,Measure}.swift` and the generated bindings (`SearchOptions` fields, `StageReport { lexicalCandidates, denseCandidates, degraded, rerank, timeLimitIgnored }`, `RerankReport { candidates, scored, skipped }`, `Hit`, `HitExplain.features()`); SwiftUI iOS 16 items: `NavigationStack`, `.searchable(text:placement:prompt:)`, `.onSubmit(of: .search)`, `Link(destination:)`, `Picker`, `Toggle`, `@AppStorage`, `ObservableObject`/`@Published`/`@MainActor` — all iOS 16 (D7); xcodegen `project.yml` keys copied from the harness (D1). |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither touched; `git diff main -- crates/` expected empty. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Four PRs: **PR 1** project, scaffold, build flag, red tests (~450); **PR 2** the model — preparation, two-phase search, cancellation, change marks, settings (~400); **PR 3** views + About + hit detail (~500); **PR 4** device measurement test, run record, report (~250 + JSON). |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | PR 1 is the red test target; each later PR's tests exist before its code. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 4; the device record is the bench evidence; "no eval delta due" stated. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | A footprint over 600 MB, a median over SC-001, a cadence miss, a display mismatch against the goldens: ⛔ report. The 4,000 ms default budget is set *before* measurement from 008's records (D5), not tuned to pass. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | One `ObservableObject`, plain enums for state, a pure function for marks, stock SwiftUI views; no architecture framework. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-14, Claude (Opus 5), before research.

**Post-design gate (post-Phase 1)**: PASS — 2026-09-14, Claude (Opus 5), after D1–D11 and the contract; no row changed.

## Project Structure

### Documentation (this feature)

```text
specs/009-ios-wiki-demo/
├── plan.md              # This file
├── research.md          # D1–D11
├── data-model.md        # Preparation, Settings, SearchState, ChangeMark, DisplayedHit, sidecar, run record
├── quickstart.md        # Steps 0–5
├── contracts/app.md     # the model's API and the screens
├── runs/                # the device record (PR 4)
├── report.md            # PR 4
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
apps/ios-wiki-demo/
├── project.yml                      # xcodegen; package path ../../swift/Xtriever; arm64-only settings as the harness; schemes XtrieverWikiDemo, XtrieverWikiDemo-Measure
├── App/
│   ├── WikiDemoApp.swift            # @main, DemoModel as StateObject
│   ├── Model/
│   │   ├── DemoModel.swift          # @MainActor ObservableObject: start(), submit(), cancel()
│   │   ├── Preparation.swift        # the state enum, ReadyInfo, PreparationFailure
│   │   ├── SearchState.swift        # phase, fused/reranked, marks, timings, footprint
│   │   ├── ChangeMark.swift         # compute(fused:reranked:)
│   │   ├── Settings.swift           # @AppStorage-backed
│   │   ├── DisplayedHit.swift       # derived view model
│   │   └── CorpusSidecar.swift      # Codable for corpus.json
│   └── Views/
│       ├── RootView.swift           # preparation gate → SearchView
│       ├── PreparationView.swift
│       ├── SearchView.swift         # searchable, list, stage label, footer
│       ├── HitRow.swift, HitDetailView.swift
│       ├── StageReportView.swift
│       ├── SettingsView.swift
│       └── AboutView.swift
├── Tests/
│   ├── ChangeMarkTests.swift
│   ├── DemoModelTests.swift
│   ├── AboutTests.swift
│   └── DemoMeasurementTests.swift   # device only
└── README.md
scripts/build-ios-package.sh         # --demo
.gitignore                           # apps/ios-wiki-demo/*.xcodeproj
```

**Structure Decision**: the app is a leaf above the package; nothing below it changes. The
model is the only place with logic; views render state; tests drive the model.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

None.
