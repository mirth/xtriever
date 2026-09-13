# Implementation Plan: The FFI Surface

**Branch**: `007-ffi-surface` | **Date**: 2026-09-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/007-ffi-surface/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Replace the Feature 001 spike in `xtriever-ffi` with the real surface: one uniffi object,
`XtrieverIndex`, wrapping `Mutex<HybridIndex>` with a read-only `open` (both pinned models,
buffered or mapped), `info`, and a synchronous `search` that converts the wire options, supplies
the pipeline's elapsed-time source from an `Instant` taken at entry, and lowers every core error
one-to-one (research D1, D3, D4). Async lives in the Swift package `swift/Xtriever/`: a serial
`DispatchQueue` plus `withCheckedThrowingContinuation` make `search` non-blocking and serialised
per handle without a Rust runtime — uniffi 0.32.1 polls a Rust future on the caller's thread and
only knows tokio, so Rust `async fn` is the wrong tool (D2). One script builds the two static
libraries, generates the bindings, assembles the XCFramework, stages models / the fixture index
/ SciFact, and generates the device harness app, encoding 001's traps as steps (D7). Parity is
proven with committed goldens minted by the same Rust code (D8); the device run measures
footprint against 300 MB and latency at depths 0 / 5 / 20 on SciFact with both models mapped
(D9). The spike, its harness and its dependencies are deleted; `Measure.swift` is ported
(D6, user decision A). CI loses two lines and gains none (D10).

## Technical Context

**Language/Version**: Rust, edition 2024, toolchain 1.91.1; Swift 5.9 tools, iOS 16 deployment
(the 001 harness settings); Xcode as pinned by `check-toolchain.sh`.

**Primary Dependencies**: `xtriever-ffi` — `xtriever-pipeline`, `xtriever-dense` and
`xtriever-rerank` (all with `mmap`), `xtriever-core`, `uniffi 0.32.1` (already locked; becomes
non-optional), `thiserror`; the spike's direct `tantivy` / `tokenizers` / `candle-*` /
`serde_json` / `sha2` are removed (D12). Swift: no third-party packages. Tools: `uniffi-bindgen`
(the crate's `cli` binary), `xcodebuild`, `xcodegen` (device harness only).

**Storage**: none written. Reads a pipeline directory (format v2) and two model directories.
Generated artifacts (XCFramework ~260 MB, bindings, staged resources) are gitignored;
`expected.json` and the device run records are committed.

**Testing**: `cargo nextest` — offline (error and option mapping) and model-backed `#[ignore]`
(parity vs `HybridIndex`, budget, info, read-only); Swift `xcodebuild test` on the simulator
(parity, async, budget, errors, info) — local, not CI; device measurement hosted by the harness
app — manual, recorded (D13).

**Target Platform**: `aarch64-apple-ios`, `aarch64-apple-ios-sim` for the product; host for
the Rust tests; Android still `cargo check`s (FR-011).

**Project Type**: FFI library crate + Swift package + build script + measurement harness.

**Performance Goals**: none claimed; Story 5 records footprint and latency per depth on a
physical device (D9). Non-blocking is a correctness requirement, tested (SC-002).

**Constraints**: pure crates untouched (no async/threads/clock); `Instant` and the queue live
in the FFI crate and the Swift layer; C/C++-free in every feature set; no simulator/device/model
step in CI; `xtriever-core`, the stage crates, `xtriever-pipeline`, `deny.toml` unchanged
(FR-015).

**Scale/Scope**: FFI crate ~500 lines (exports, conversions, index module) − ~650 spike lines;
Rust tests ~350; Swift package ~450 (wrapper, `Measure.swift` ported, `Package.swift`) + tests
~500; script ~150; example ~150; harness app project ~80; docs. **Over Rule 3's ~800 lines
— four PRs** (see Rule 3 row).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.3.0 at planning; amended to v1.4.0 by ADR-0010 on this feature's F-002 — Principle III's default ceiling 300 MB → 600 MB, full pipeline).
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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | Nothing is built: the surface wraps the pipeline; bindings are generated by uniffi (the crate 001 chose); the async layer is a serial dispatch queue and a continuation — platform primitives. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | PR 1 is the red suite (Rust and Swift). The reference for the boundary is the Rust pipeline itself: `expected.json` goldens minted by the same code, compared bit-for-bit on the Swift side (D8); every core error variant enumerated in a mapping test (D4). **Not ranking-affecting**: the FFI adds no computation, so no eval delta is due — the parity test proves the hits are the pipeline's; the device run adds the cross-architecture check at the 004/006 tolerances (D9). |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 300 MB for a 100k-chunk index including loaded models). | **PASS** | The pure crates are untouched (FR-015; `check-containment.sh` unchanged). `Instant` and the dispatch queue are in `xtriever-ffi` and its Swift half — the crates the principle names for exactly this. No C/C++ anywhere (uniffi is pure Rust; the spike's optional deps go away); `deny.toml` unchanged; the three mobile targets check. **The RSS clause is measured on a device** (Story 5) — the first time with the full pipeline and both models resident. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | No performance claim; the device numbers are observations with run records committed verbatim (D9, 001's discipline). Reproducibility: pinned models, a pinned SciFact index identity in the record, `RAYON_NUM_THREADS` recorded. `explain` crosses the boundary intact (`HitExplain` + the seven names via `features()`). |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | Core traits, formats and error variants untouched; the FFI **lowers** each error variant one-to-one (a mirror, not a change — D4). Core APIs stay synchronous; the async wrapper is in `xtriever-ffi`'s Swift half, the one place allowed. Swift sees external ids and text only. Direction: `ffi → pipeline → stages → core`. Read-only by construction (D1). |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | Degradation and strict mode cross the boundary unchanged (`StageReport`, `Degradation`, `RerankReport`, `BudgetExhausted`); the FFI's clock feeds the pipeline's check points so the semantics are 005/006's exactly (D3). Determinism: the FFI adds no computation — bit-identity vs `HybridIndex` is the parity test. Open refusals (version, fingerprint, marker, counts) arrive as typed errors (Story 3). |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | Dependencies via `cargo add` / `cargo remove` at locked versions; every export returns `Result` (uniffi lowers to `throws`; panics caught by uniffi's `catch_unwind` — D4); the crate keeps ADR-0003's shape: `lib.rs` + `ffi/` carry the generated-code allow, `index.rs` re-denies; `missing_docs` on; `deny.toml` unchanged. The `Mutex` lock is `.lock().map_err(...)` — a poisoned lock is `Backend`, never `unwrap`. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | uniffi 0.32.1: `Object`/`constructor`/`export` macros (`uniffi_macros/src/lib.rs:131,320`), the `Send + Sync` object bound (`uniffi_core/src/ffi_converter_traits.rs:155,462,648`), `RustFuture::poll` on the caller's thread (`ffi/rustfuture/future.rs:205-218`), `AsyncRuntime::Tokio` only (`export/attributes.rs:271-273`), `catch_unwind` in `ffi/rustcalls.rs:207,228`, the Swift `uniffiRustCallAsync` template — all read; workspace items (`HybridIndex::open`/`search`, `SearchOptions.elapsed`, lazy tantivy writer at `xtriever-lexical/src/index.rs:104-121,171`, `Send + Sync` assertion at `:60-64`) cited by line in research D1–D4. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither is touched; nor is the pipeline's API — read-only needed no `open_read_only` because the lexical writer is lazy (D1). |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Branch `007-ffi-surface`. **PR 1** spike deletion, deps, scaffold (`NotImplemented`), red Rust suite, Swift package skeleton + red Swift tests, `Package.swift` (~1,300 incl. −650 deleted); **PR 2** the Rust surface + `fixture_index` example + `expected.json` — Rust suites green (~500); **PR 3** the Swift async layer, ported `Measure.swift`, the build script, CI lines — Swift suites green on the simulator (~600); **PR 4** harness app, device runs, run records, report (~300). |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | PR 1 lands the Rust suite red on `NotImplemented` and the Swift suite red (it cannot link until the package builds — recorded as the red state); `check-no-stubs.sh` covers `xtriever-ffi` already. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 6 plus the FR-015 diff, the FFI dependency grep, the CI diff (two lines removed), the containment script; the PR carries the parity result, the Swift suite summary and the device run table (no eval delta is due — nothing ranking-affecting; stated in the PR). |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | Parity is bit-exact and stays so; device dense/re-rank tolerance is the 004/006 1e-3, not a new number; a footprint over 300 MB is ⛔ stop-and-report (a finding with the buffered/mapped breakdown), never a raised ceiling; a non-blocking test that fails is a design defect, not a looser cadence bound. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | One object, a `Mutex`, an `Instant`, one `From` impl, one dispatch queue, one continuation; no Rust async, no runtime, no callback interfaces, no cancellation machinery beyond the budget. |

**Initial gate (pre-Phase 0)**: **PASS** on all 14 rows — 2026-09-13, Claude (agent). No ADR
required: no core or format change, no new `unsafe` (ADR-0003 already covers uniffi's generated
code), no new dependency class, nothing commodity built.

**Post-design gate (post-Phase 1)**: **PASS** on all 14 rows — 2026-09-13, Claude. Design fixed
the Swift-side async (D2), the one-to-one error lowering (D4), the fixture-index goldens (D8)
and the device protocol (D9); none touches a row. `/speckit-tasks` may run.

## Project Structure

### Documentation (this feature)

```text
specs/007-ffi-surface/
├── plan.md              # This file
├── research.md          # Phase 0 — D1–D13 + risks
├── data-model.md        # Phase 1 — handle, options, response, errors, goldens, run record
├── quickstart.md        # Phase 1 — Steps 0–7
├── contracts/
│   └── ffi-surface.md
├── runs/                # PR 4 — device run records (committed verbatim)
├── checklists/requirements.md
├── tasks.md             # /speckit-tasks
└── report.md            # closing report
```

### Source Code (repository root)

```text
crates/xtriever-ffi/
├── Cargo.toml                 # deps: xtriever-{core,pipeline,dense,rerank} (mmap), uniffi 0.32.1, thiserror; features: default = [], cli; spike removed
├── src/
│   ├── lib.rs                 # #![allow(unsafe_code)] (ADR-0003), uniffi::setup_scaffolding!(), pub mod ffi, mod index
│   ├── ffi/mod.rs             # #![allow(unsafe_code)]: the XtrieverIndex object + #[uniffi::export] impl — thin shims into index.rs
│   ├── ffi/types.rs           # #![allow(unsafe_code)]: Record/Enum derives (SearchOptions, SearchResponse, Hit, ChunkInfo, HitExplain, StageReport, RerankReport, Degradation, DegradeReason, IndexInfo, LoadPath)
│   ├── ffi/error.rs           # #![allow(unsafe_code)]: XtrieverError (uniffi::Error) + From<xtriever_core::Error>
│   ├── index.rs               # #![deny(unsafe_code)]: open (models + HybridIndex), info, search (lock, Instant, options/response conversion) — all hand-written logic
│   └── bin/uniffi-bindgen.rs  # unchanged (cli feature)
├── examples/fixture_index.rs  # builds Tests/Fixtures/index from reference/fixtures/005/hybrid.json with the real models; writes expected.json (D8)
└── tests/
    ├── support/mod.rs         # model dirs, fixture paths, bit helpers
    ├── errors.rs              # every core variant → mirrored XtrieverError kind + message (D4)
    ├── options.rs             # wire SearchOptions → pipeline options; defaults; k = 0
    ├── parity.rs              # #[ignore]: XtrieverIndex::search == HybridIndex::search bit-for-bit, with/without re-ranker; expected.json regenerates identically
    ├── budget.rs              # #[ignore]: max_time_ms 200 ⇒ partial re-rank; 0 ⇒ degraded / BudgetExhausted (strict); time_limit_ignored never set (the FFI always supplies the clock when a limit exists)
    ├── info.rs                # #[ignore]: IndexInfo fields, load times > 0
    └── readonly.rs            # #[ignore]: directory listing + mtimes identical after open + searches; commit.pending refused; v1 refused naming both

swift/Xtriever/
├── Package.swift              # iOS 16, tools 5.9; binaryTarget XtrieverFFI; target Xtriever (resources XtrieverData); testTarget XtrieverTests
├── Sources/Xtriever/
│   ├── Generated/             # uniffi output (gitignored)
│   ├── XtrieverIndex.swift    # the async layer: serial queue + continuations; features(); SearchOptions convenience init
│   ├── Measure.swift          # ported from harness/ios (task_vm_info footprint, verdict gate, run record)
│   └── XtrieverData/          # staged resources (gitignored)
└── Tests/
    ├── Fixtures/expected.json # committed goldens (D8); Fixtures/index/ gitignored
    └── XtrieverTests/{ParityTests,AsyncTests,BudgetTests,ErrorTests,InfoTests,DeviceMeasurementTests}.swift

swift/XtrieverHarnessApp/      # xcodegen project.yml + App/ (ported from harness/ios/XtrieverSpikeApp); .xcodeproj gitignored
scripts/build-ios-package.sh   # replaces build-ios-harness.sh (D7)
.github/workflows/ci.yml       # − the two `--features spike` check lines
.gitignore                     # + swift/Xtriever/Frameworks, Sources/Xtriever/Generated, Sources/Xtriever/XtrieverData, Tests/Fixtures/index, swift/XtrieverHarnessApp/*.xcodeproj

deleted: harness/ios/**, scripts/build-ios-harness.sh, crates/xtriever-ffi/src/spike/**, crates/xtriever-ffi/examples/gen_ranking.rs, the spike's tests (bm25_parity, determinism, embed, fixtures_valid, index_query, load_paths, tokenize)
```

**Structure Decision**: `xtriever-ffi` is implemented in place and depends on the pipeline and
the two model crates (`ffi → pipeline → stages → core`); nothing depends on it. The Swift
package is a sibling of `crates/` at `swift/Xtriever/`, consumed by path by the Feature 009 app
at `apps/ios-wiki-demo/`; the device harness app is its test host.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| *(none)* | — | — | — | — |

## Decisions this plan takes that the spec left open (recorded for the human)

1. **Async is Swift-side over synchronous Rust** (D2) — uniffi 0.32.1 polls Rust futures on the
   caller's thread and supports only tokio as a runtime; a serial `DispatchQueue` plus a
   continuation is the boring, runtime-free way to honour FR-005/FR-006.
2. **The surface is the crate's default feature set** (D11); `cli` stays non-default; `spike`
   is gone.
3. **`Mutex<HybridIndex>` is the handle** (D1) — the lock is the serialisation FR-006 asks for.
4. **Error lowering is one-to-one with a `Backend` wildcard** (D4) — no core change, no ADR;
   the enumerating test makes a future variant visible.
5. **Both models share one `LoadPath` at open** (D5) — the demo has no reason to mix them; the
   measurement compares mapped vs buffered as whole configurations.
6. **Parity goldens are minted by the FFI itself and committed** (D8) — the boundary's
   correctness is "equals Rust", and Rust's correctness is 004/006's.
7. **The Swift package lives at `swift/Xtriever/`**, the harness app at
   `swift/XtrieverHarnessApp/`, the demo later at `apps/ios-wiki-demo/` (spec assumption).
8. **Device runs at `RAYON_NUM_THREADS=1` and unset, mapped; one buffered run** (D9) —
   comparability with 001 plus the phone's real default.
9. **`SearchResponse.elapsed_ms` is the FFI's own wall time** (D5) — the UI's latency readout
   without a clock in the pipeline.
