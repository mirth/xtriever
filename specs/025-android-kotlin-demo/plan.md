# Implementation Plan: The Android Kotlin Wikipedia Demo

**Branch**: `025-android-kotlin-demo` | **Date**: 2026-09-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/025-android-kotlin-demo/spec.md`

## Summary

Two deliverables, two pull requests. **PR A** is a reusable Android library module at
`android/xtriever/` carrying the engine's Kotlin bindings and one native library for 64-bit
ARM, built from source by `scripts/build-android-package.sh` exactly as the Swift package is
built by `scripts/build-ios-package.sh`, and proved by an instrumented test that reproduces the
committed 40-document fixture goldens bit for bit on an emulator. **PR B** is the demo
application at `apps/android-wiki-demo/`, a Compose user interface mirroring the iOS demo's
four screens over the bundled 2,000-article Wikipedia slice, with the models and the index
shipped inside the package and extracted to application storage on first launch.

No crate logic changes. The engine already compiles for `aarch64-linux-android` on every push;
what this feature adds is the two things a compile check cannot give — a linked library and a
device that runs it. Both were proved by hand before this plan (2026-09-20): the workspace
links with the native toolkit release 28 once half-precision floating point is enabled, the
release profile's symbol stripping must be relaxed for the binding metadata to survive, the
existing generator emits 4,249 lines of Kotlin from the Android library with no code change,
and the prepared emulator reports the half-precision processor features the build requires.

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: Rust (workspace, edition 2024, toolchain pinned) for the engine; Kotlin on
the Java Virtual Machine for the module and the application. Java Development Kit 21
(`/opt/homebrew/opt/openjdk@21`, installed 2026-09-20); Android Studio's bundled runtime is 25
and is not used for the build. Gradle, the Android Gradle Plugin, Kotlin and Compose versions
come from a generated project's version catalogue at implementation time — never typed from
memory (Principle VII).

**Primary Dependencies**: `uniffi 0.32.1` (already in `crates/xtriever-ffi`; its
`uniffi::uniffi_bindgen_main()` entry point behind the crate's `cli` feature emits Kotlin via
`generate --library <path> --language kotlin --out-dir <dir>`, verified on the Android library
2026-09-20); Java Native Access, which the generated Kotlin imports (`com.sun.jna.Library`,
`IntegerType`, `Native`) and which must be on the module's runtime path as the Android archive
variant; `cargo-ndk 4.1.2` (installed 2026-09-20) driving the native toolkit
`28.2.13676358`. No new Rust dependency.

**Storage**: the application's private files directory holds the extracted index directory and
the two model directories. The index is dense format version 2 (`dense/manifest.bin`,
`dense/vectors.<generation>.bin`) — a version-1 directory is refused at open, as everywhere
else.

**Testing**: instrumented tests on the prepared emulator (`xtriever-arm64`, Android 16 / API 36,
`google_apis;arm64-v8a`) for everything that needs the engine; the existing workspace gate is
unchanged and still runs `cargo check --workspace --target aarch64-linux-android`. The oracle
is the committed `swift/Xtriever/Tests/Fixtures/expected.json` — the same goldens the Swift
package is checked against.

**Target Platform**: `aarch64-linux-android` only (`arm64-v8a`), minimum operating system
Android 8.0 (API 26). The library requires the ARMv8.2 half-precision features `fphp` and
`asimdhp`; the emulator prepared for this feature reports both.

**Project Type**: an Android library module plus an Android application, alongside a build
script. No crate is added, and no crate's logic changes.

**Performance Goals**: none are budgeted, because the record is an emulator record (spec
FR-017). Latency and footprint are measured and written down; the project's 600 MB phone
ceiling is reported beside the figure for comparison only. Correctness is the hard claim:
every hit and score bit identical to the host.

**Constraints**: no change to any stage crate, to the on-disk formats, or to a committed
baseline (FR-014); no continuous-integration job that downloads the native toolkit, boots an
emulator or loads a model (FR-015); nothing generated is committed (FR-003); no device
identifier in any tracked file.

**Scale/Scope**: a 8,529-passage corpus (24 MB) and 174 MB of model weights inside the
application package; two new top-level directories; roughly 700 lines for PR A and 700 for
PR B, split per Rule 3.

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | PASS | Nothing is built that a crate already provides: the bindings come from `uniffi`'s own Kotlin generator, the native build from `cargo-ndk`, the user interface from the platform's toolkit. No retrieval code is written at all. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | PASS | The acceptance test is the committed `swift/Xtriever/Tests/Fixtures/expected.json`, replayed on the emulator at all four re-rank depths and committed failing first (Rule 4). Ranking is untouched, so there are no nDCG/Recall deltas to report — FR-014 forbids a ranking change and the gate proves it by the absence of a diff in the stage crates. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | PASS | This feature *extends* portability: the third target in the principle's own list stops being a compile check and starts being a running library. The pure crates are untouched, no C or C++ dependency is added, and the half-precision requirement narrows which Android processors are supported — recorded in a new decision record (FR-016) rather than assumed. On-device memory is measured and reported; no phone claim is made. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | PASS | Every figure is a record under `runs/` naming the emulated device, its image, the host and the thread count, in the shape Features 009, 018 and 019 use. No `criterion` benchmark applies — nothing in the engine's hot path changes — and the plan says so rather than inventing one. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | PASS | No `xtriever-core` trait, on-disk format or error type changes. The Kotlin surface is the existing foreign-function surface as the generator emits it; the hand-written wrapper adds no capability the Swift wrapper lacks. Dependencies still point downward: the module sits outside the crates, above `xtriever-ffi`. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | PASS | Determinism is the feature's central claim and its test: the same index and query give the same bits on Android as on the host. Degradation behaviour is the engine's, unchanged. The format version and embedder fingerprint are still checked at open, and a version-1 index pushed by hand is refused with the engine's own error. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | PASS | No Rust source changes beyond a build profile; the full gate still runs. Versions for the Android toolchain are taken from generated project files and the package manager, never written from memory. The module's Kotlin wrapper is small and boring, and the generated binding file is not committed. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | PASS | Every item was exercised on this machine before the plan: `uniffi 0.32.1`'s `generate --library --language kotlin` (produced the Kotlin), `cargo-ndk 4.1.2`, native toolkit `28.2.13676358`, and the generated file's own Java Native Access imports. Gradle-side versions are resolved from a generated project, not recalled. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | PASS | `xtriever-core` and `deny.toml` are untouched. The one build-file change is a new Cargo profile that keeps symbols, alongside the existing `wheel` profile. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | PASS | Two pull requests, stated in the Summary and enforced by the task phases: PR A the module and its proof, PR B the application. Each is planned under ~800 changed lines. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | PASS | The first implementation task in each pull request is the failing test: the fixture-goldens replay for PR A, the model and preparation tests for PR B. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | PASS | The workspace gate still runs unchanged, plus the module build, the instrumented suite on the emulator, and the record. The pull request carries them. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | PASS | The goldens are the Swift package's existing file, used as-is. A parity failure on Android is a stop-and-report, never a tolerance change. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | PASS | A thin wrapper, a first-run file copy, four screens. No abstraction over the generated bindings and no shared user-interface framework invented across platforms. |

**Initial gate (pre-Phase 0)**: **PASS** — 2026-09-20, evaluated while writing this plan. No row is FAIL; Complexity Tracking is empty.

**Post-design gate (post-Phase 1)**: **PASS** — 2026-09-20, re-evaluated after `research.md`, `data-model.md`, `contracts/` and `quickstart.md` were written. The design added no crate, no dependency inside a pure crate and no format change; the only new build-file line is the Cargo profile, and the hardware floor's decision record is a planned task rather than an unrecorded narrowing.

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

```text
android/
└── xtriever/                    # NEW (PR A) — the reusable library module
    ├── build.gradle.kts         # minimum API 26, Java Native Access, the generated source set
    └── src/
        ├── main/
        │   ├── kotlin/…/XtrieverIndex.kt   # the thin wrapper, mirroring the Swift one
        │   ├── kotlin/…/DeviceSupport.kt   # the half-precision check, before any engine call
        │   └── jniLibs/arm64-v8a/          # staged by the script; never committed
        └── androidTest/…/FixtureParityTest.kt   # the goldens replay (the oracle)

apps/
└── android-wiki-demo/           # NEW (PR B) — the demonstration
    ├── src/main/kotlin/…/       # model, preparation, four screens
    ├── src/main/assets/         # staged by the script; never committed
    ├── src/androidTest/…/       # model, marks, settings, preparation
    └── README.md

scripts/
└── build-android-package.sh     # NEW (PR A) — the one command, mirroring the iOS script

crates/xtriever-ffi/             # unchanged; its `cli` feature already emits Kotlin
docs/adr/0014-android-half-precision-floor.md   # NEW (PR A) — FR-016
specs/025-android-kotlin-demo/   # this feature's documents
Cargo.toml                       # one new profile: release without symbol stripping
```

**Structure Decision**: no crate is added and no crate's logic changes, so the dependency
direction (`core` ← stage crates ← `pipeline` ← `ffi`) is untouched. The two new top-level
trees sit outside the workspace, above `xtriever-ffi`, exactly as `swift/Xtriever` and
`apps/ios-wiki-demo` do — the module consumes the foreign-function surface and the application
consumes the module. The only file inside the workspace that changes is `Cargo.toml`, which
gains a profile that keeps the symbols the binding generator reads, beside the `wheel` profile
that exists for the same reason.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| *(none)* | — | — | — | — |

No principle is violated. One decision still needs a record — the half-precision hardware
floor, required by FR-016 and planned as `docs/adr/0014-android-half-precision-floor.md` in
PR A. It narrows which Android processors the engine supports rather than breaching a stated
rule, which is why it is a record and not an entry above.
