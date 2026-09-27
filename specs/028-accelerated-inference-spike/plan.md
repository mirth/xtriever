# Implementation Plan: Accelerated Inference Spike

**Branch**: `028-accelerated-inference-spike` | **Date**: 2026-09-27 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/028-accelerated-inference-spike/spec.md`

## Summary

Measure what Apple's matrix library (`accelerate`) and the Apple GPU (`metal`) — both opt-in
features of the pinned candle 0.9.2 — are worth for the embedder and the re-ranker, on the
iPhone 16e and the MacBook Pro, against today's CPU records, and decide go / no-go per path and
device by the rule fixed in the spec (FR-010). Research settled the technical questions from the
pinned sources and a throwaway probe: both features build for iOS (`cargo check`), Accelerate
reroutes the `f32` matmul both models already use, Metal needs only the device the two loaders
create (`Device::Cpu` → `Device::new_metal(0)`), batched re-ranking needs no model change, and
the dependency policy holds (`cargo deny` with the repository's config: ok). The spike adds
compile-time, non-default, mutually exclusive features (`spike-accelerate`, `spike-metal`,
plus `spike-batch` for the re-ranker), two labels and a hits digest to the three existing
harnesses, a packager flag and a host script — and changes no default build, format, trait or
identity.

## Technical Context

**Language/Version**: Rust 1.91.1 (edition 2024, `rust-toolchain.toml`); Swift 5.9 (harnesses);
Python ≥ 3.9 (the host demo); shell (packager, host script)

**Primary Dependencies**: candle 0.9.2 (pinned, ADR-0001) with its `accelerate` and `metal`
features (`candle-core`, `candle-nn`, `candle-transformers`), which bring `accelerate-src 0.3.2`,
`candle-metal-kernels 0.9.2`, `objc2-metal 0.3.2`, `objc2-foundation 0.3.2` — all already
resolvable, licence-checked (research D11). No new direct dependency.

**Storage**: none new — the shipped Wikipedia index and the SciFact / NFCorpus / FiQA eval caches,
per-path cache directories under `target/spike-028/`

**Testing**: `cargo nextest` (the existing golden tests re-run under each feature; new tests for
the label, the batched re-ranker and the digest), `pytest` (the demo's digest), XCTest on device
(the two existing measurement harnesses), `xtriever-eval` (quality, build throughput)

**Target Platform**: `aarch64-apple-darwin` (MacBook Pro, M1 Pro) and `aarch64-apple-ios`
(iPhone 16e) for the spike paths; every default target unchanged and checked

**Project Type**: library (engine crates) + measurement harnesses; a spike whose deliverable is
records and a verdict

**Performance Goals**: the verdict's thresholds (spec FR-010) — median re-ranked phase −30% on a
device, or host build throughput ×2; peak footprint < 600 MB on the phone; identical reruns;
scores within 1e-3 of the host goldens; nDCG@10 within ±0.005 per dataset for a recommended path

**Constraints**: no default build changes (FR-011); no core trait, format, identity or
`deny.toml` change; no model converted; no env var may change a number; device and team ids
never in a file

**Scale/Scope**: the Wikipedia index (427,947 passages), 20 queries, depths 0/5/10/20; 3 paths ×
2 batching modes × 2 devices; quality on 3 BEIR datasets for a recommended path only

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | PASS | Both paths are candle's own backends (research D1, D3, D4); no kernel, runtime or second engine is written. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | PASS | Red first: the label tests, the batched re-ranker's test and the digest tests are committed failing; the existing embedding and re-rank golden tests are the oracle for each path, at the tolerance they already state; a recommended path is evaluated on all three datasets (FR-008). No default ranking changes. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | PASS | The framework bindings live only in `xtriever-dense` / `-rerank` behind non-default features, forwarded by `-ffi`; pure crates untouched; `cargo deny` with the repository config passes them (D11). Default targets unchanged; the spike features are additionally checked for macOS and iOS. The phone's footprint is measured against 600 MB on every path (FR-005). |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | PASS | The spike ships no performance claim; its report states harness-measured medians and maxima against the spec's stated thresholds, as Features 008, 009 and 026 did on device, with pinned models, datasets and engine. A follow-up that ships a path adds its `criterion` bench (research D12). |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | PASS | No trait, format, error or binding-surface change: `Reranker::rerank` keeps its signature (the batch is internal, D7); the FFI gains Cargo features only. Dependencies still point downward. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | PASS | Default builds unchanged. In spike builds a device that cannot be opened fails the model load (the spec defers the fallback); per-search degradation is untouched. Determinism is measured, not assumed (FR-007, digest D8). The fingerprint is deliberately unchanged in spike builds so CPU-built indexes open (D6); the report hands the identity question to the follow-up. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | PASS | No new dependency (features of the pinned candle crates; `Cargo.lock` gains the optional crates and is committed); no `unsafe`; errors through the existing `Error::Model`; the gate runs with and without the spike features (clippy with `-D warnings` per feature). |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | PASS | `candle_core::Device::new_metal` (device.rs:258), `utils::metal_is_available` (utils.rs:27), `quantized::QTensor::dequantize` (quantized/mod.rs:624), the `accelerate` matmul (cpu_backend/mod.rs:1440–1496), `candle_metal_kernels` runtime compilation (kernel.rs:121), `maturin build --features`; cited in research D2–D10. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | PASS | Neither is touched (D11: `deny.toml` already passes). |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | PASS | Code about 500 lines (features and device selection ~80, batched re-rank ~120, labels and digests in three harnesses ~180, packager flag and host script ~120); the run records and report are data and prose. If the code passes 800 lines, split: PR A the features and tests, PR B the measurements. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | PASS | The label, batch and digest tests come first and fail (features and functions absent); the ⛔ commit checkpoint follows. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | PASS | The full local gate on the default build (proving SC-004), plus clippy/tests/iOS check per spike feature; the report and PR carry every record and, for a recommended path, the three-dataset deltas. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | PASS | A path that fails to build, lacks a kernel, breaks the ceiling or misses a tolerance is recorded as that path's result (spec Edge Cases, FR-010); no tolerance or threshold moves after the runs start. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | PASS | One function per crate returns the device for the active feature; the batch is one method; labels are strings in records. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-27, Claude (agent), on the spec as written.

**Post-design gate (post-Phase 1)**: PASS — 2026-09-27, Claude (agent), after research D1–D12 and the design below; no row changed.

## Project Structure

### Documentation (this feature)

```text
specs/028-accelerated-inference-spike/
├── plan.md              # This file
├── research.md          # Phase 0: D1–D12
├── data-model.md        # Phase 1: variants, run-record additions, verdict
├── quickstart.md        # Phase 1: the runs, in order
├── contracts/
│   └── spike-surface.md # Phase 1: features, packager flag, host script, record fields
├── runs/                # the records (added during implementation)
├── report.md            # the verdict (added during implementation)
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/
├── xtriever-dense/      # features spike-accelerate / spike-metal; spike::compute_device(),
│                        #   spike::COMPUTE_PATH; the two Device::Cpu sites in embedder.rs
├── xtriever-rerank/     # the same features + spike-batch; the two sites in scorer.rs;
│                        #   the batched rerank behind spike-batch
└── xtriever-ffi/        # forwards the three features (Cargo.toml only)

swift/Xtriever/Tests/XtrieverTests/DeviceMeasurementTests.swift   # labels + hitsDigest
apps/ios-wiki-demo/Tests/DemoMeasurementTests.swift               # labels
apps/python-wiki-demo/wikidemo/measure.py (+ tests)               # labels + hitsDigest
scripts/build-ios-package.sh                                      # --spike-compute / --spike-batch
scripts/spike-028-host.sh                                         # host runs, one command per path
```

**Structure Decision**: the spike touches the two model crates (the only place a device is
chosen), the FFI crate's manifest (to forward features to every binding), the three existing
measurement harnesses, and two scripts. `core`, `analysis`, `lexical`, `pipeline`, `ltr`, `eval`
and `cli` are untouched; dependencies still point `core` ← stage crates ← `pipeline` ←
`ffi`/`cli`. Every spike item is named `spike` / `spike-028` so the follow-up can find and remove
or promote it (FR-012).

**Spec note**: FR-012 lists "the dense and re-rank crates, their examples, the iOS packager's
spike flag and the demo's measurement". The design above also touches the FFI crate's
`Cargo.toml` (feature forwarding only), the Swift package's device harness and the Python demo's
`measure` (labels and digest), and adds `scripts/spike-028-host.sh`. The spec's FR-012 is amended
to list them (same change set as this plan).

## Complexity Tracking

No constitution violations; nothing to justify.
