# Tasks: The FFI Surface

**Input**: Design documents from `/specs/007-ffi-surface/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/ffi-surface.md](./contracts/ffi-surface.md),
[quickstart.md](./quickstart.md), [ADR-0003](../../docs/adr/0003-uniffi-scaffolding-requires-unsafe-allow.md)

**Tests**: **Mandatory** (Principle II, spec FR-017). Phase 2 lands every test red: the Rust
offline suite fails at runtime on `NotImplemented`; the model-backed Rust suite is `#[ignore]`;
the Swift suite is written but cannot link until the package builds (PR 3) — that is its red
state, recorded. Story phases contain implementation only and end with the task that turns
their tests green.

**Organization**: Setup (spike deletion, deps) → Red suite → US1 (the Rust surface + parity)
→ US3 (errors — Rust side is part of US1's surface; the Swift half needs US4) → US4 (package +
script) → US2 (async + budgets, Swift) → US3 Swift half → US5 (device) → Polish. US4 precedes
US2 because the async layer *is* the package. The four-PR split from plan.md: **PR 1** =
Phases 1–2, **PR 2** = Phase 3, **PR 3** = Phases 4–6, **PR 4** = Phases 7–8.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US5 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers and data-model
  sections — read them before implementing. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

Crate `crates/xtriever-ffi/` (`src/`, `tests/`, `examples/`); Swift package `swift/Xtriever/`
(`Package.swift`, `Sources/Xtriever/`, `Tests/XtrieverTests/`, `Tests/Fixtures/`); harness app
`swift/XtrieverHarnessApp/`; script `scripts/build-ios-package.sh`; models
`reference/models/{all-MiniLM-L6-v2,ms-marco-MiniLM-L-6-v2}/`; SciFact index
`target/xt-rerank-index/scifact/`; run records `specs/007-ffi-surface/runs/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Delete the spike (FR-018, user decision A), re-point the crate's dependencies,
and prepare the ignore rules and CI lines.

- [ ] T001 Delete the Feature 001 spike from `crates/xtriever-ffi/`: `src/spike/` (mod, index, query, embed), `src/ffi/types.rs`, `src/ffi/error.rs`, `examples/gen_ranking.rs`, `tests/{bm25_parity,determinism,embed,fixtures_valid,index_query,load_paths,tokenize}.rs` and `tests/support/`; keep `src/lib.rs`, `src/ffi/mod.rs` (to be rewritten), `src/bin/uniffi-bindgen.rs`; delete `harness/ios/` entirely and `scripts/build-ios-harness.sh` — **after** copying `harness/ios/XtrieverSpike/Sources/XtrieverSpike/Measure.swift` to `swift/Xtriever/Sources/Xtriever/Measure.swift` and `harness/ios/XtrieverSpikeApp/{project.yml,App/XtrieverSpikeApp.swift}` to `swift/XtrieverHarnessApp/` (renamed `XtrieverHarnessApp`, package path `../Xtriever`, product `Xtriever`, bundle ids `dev.xtriever.harness[.tests]`, the arm64-only settings and the `RAYON_NUM_THREADS: "1"` scheme variable kept verbatim with their comments — research D6, D7)
- [ ] T002 Re-point `crates/xtriever-ffi/Cargo.toml` **with cargo**: `cargo remove -p xtriever-ffi tantivy tokenizers candle-core candle-nn candle-transformers serde_json sha2`; `cargo add -p xtriever-ffi --path crates/xtriever-pipeline --features mmap`, `cargo add -p xtriever-ffi --path crates/xtriever-dense --features mmap`, `cargo add -p xtriever-ffi --path crates/xtriever-rerank --features mmap`; make `uniffi` non-optional (`cargo add -p xtriever-ffi uniffi@0.32.1`); `cargo add -p xtriever-ffi --dev serde_json tempfile`; edit: `[features] default = []`, `cli = ["uniffi/cli"]`, remove `spike` and `serde_json` features and the `gen_ranking` example stanza; keep the `[lib] crate-type = ["lib", "cdylib", "staticlib"]` block and its comment; `description = "Xtriever: Swift/iOS FFI surface over the hybrid pipeline"` (research D12)
- [ ] T003 Verify after T002: `cargo tree -p xtriever-ffi -e normal --prefix none | grep -Ei '(-sys|^cc |onig|openssl)'` prints nothing (FR-011); `cargo deny check` passes with no `deny.toml` change; `cargo check -p xtriever-ffi --target aarch64-apple-ios`, `--target aarch64-apple-ios-sim`, `--target aarch64-linux-android` pass with the placeholder `lib.rs`
- [ ] T004 [P] Update `.gitignore`: replace the five `harness/ios/...` lines with `swift/Xtriever/Frameworks/`, `swift/Xtriever/Sources/Xtriever/Generated/`, `swift/Xtriever/Sources/Xtriever/XtrieverData/`, `swift/Xtriever/Tests/Fixtures/index/`, `swift/Xtriever/.build/`, `swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj/`, `build/`; update the comment to name `scripts/build-ios-package.sh`
- [ ] T005 [P] Remove the two `cargo check -p xtriever-ffi --features spike --target …` lines from `.github/workflows/ci.yml`; change nothing else — the workspace iOS checks cover the surface, and no simulator, device or model step is added (FR-012; standing rule)

---

## Phase 2: Red Suite (Blocking Prerequisite)

**Purpose**: The scaffolded surface, every Rust test red, the Swift package skeleton with its
tests written. **PR 1.**

**⚠️ CRITICAL**: No story implementation begins until T016 confirms the Rust suite fails for
want of an implementation, not for want of a fixture or a link error.

### Scaffold

- [ ] T006 Write the wire types in `crates/xtriever-ffi/src/ffi/types.rs` (`#![allow(unsafe_code)]` with the ADR-0003 comment) — **real** records and enums per [contracts/ffi-surface.md](./contracts/ffi-surface.md): `SearchOptions { k: u32, depth: Option<u32>, rerank_depth: Option<u32>, max_time_ms: Option<u64>, max_items: Option<u32>, strict: bool, explain: bool }`, `SearchResponse { hits, stages, elapsed_ms: u64 }`, `Hit { external_id, text, score: f64, rerank_score: Option<f32>, chunk: Option<ChunkInfo>, explain: Option<HitExplain> }`, `ChunkInfo { parent, ordinal: u32, byte_start: Option<u64>, byte_end: Option<u64> }`, `HitExplain` (seven fields, all `Option` except `fused: f64`), `StageReport`, `RerankReport`, `Degradation { stage: String, reason }`, `DegradeReason { StageError { message }, BudgetExceeded { elapsed_ms, limit_ms } }`, `IndexInfo { documents: u64, format_version: u32, embedder_fingerprint, reranker_model_id: Option<String>, candidate_depth: u32, rerank_depth: u32, rrf_k: u32, embedder_load_ms: u64, reranker_load_ms: Option<u64> }`, `LoadPath { Buffered, Mmap }`; every item documented
- [ ] T007 Write `crates/xtriever-ffi/src/ffi/error.rs` (`#![allow(unsafe_code)]`): `XtrieverError` (`thiserror::Error` + `uniffi::Error`, **not** `non_exhaustive`) with the eleven cases of data-model "XtrieverError" and their fields; `impl From<xtriever_core::Error>` matching every current variant explicitly and `_ => Backend { message: e.to_string() }` for the `#[non_exhaustive]` future (research D4); a `From<std::sync::PoisonError<_>>`-free helper `fn poisoned() -> XtrieverError` returning `Backend { message: "index lock poisoned" }`
- [ ] T008 Write the scaffold `crates/xtriever-ffi/src/ffi/mod.rs` (`#![allow(unsafe_code)]`; `pub mod error; pub mod types; pub use …`): `#[derive(uniffi::Object)] pub struct XtrieverIndex { inner: crate::index::Inner }` and `#[uniffi::export] impl XtrieverIndex { #[uniffi::constructor] pub fn open(index_dir: String, embedder_dir: String, reranker_dir: Option<String>, load_path: LoadPath) -> Result<Arc<Self>, XtrieverError>; pub fn info(&self) -> IndexInfo; pub fn search(&self, query: String, options: SearchOptions) -> Result<SearchResponse, XtrieverError> }` — each a one-line delegation to `crate::index`; `crates/xtriever-ffi/src/index.rs` (`#![deny(unsafe_code)]`): `pub(crate) struct Inner { index: Mutex<HybridIndex>, embedder_load: Duration, reranker_load: Option<Duration> }`, `pub(crate) fn open(...) -> Result<Inner, XtrieverError>`, `pub(crate) fn info(&Inner) -> IndexInfo`, `pub(crate) fn search(&Inner, &str, SearchOptions) -> Result<SearchResponse, XtrieverError>`, plus `pub(crate) fn to_pipeline_options(&SearchOptions, elapsed: Option<&dyn Fn() -> Duration>) -> xtriever_pipeline::SearchOptions<'_>` and `pub(crate) fn from_response(Response, elapsed_ms) -> SearchResponse` — the fallible ones returning `Err(XtrieverError::Backend { message: NotImplemented("…").to_string() })` where `pub(crate) struct NotImplemented(pub &'static str)` (Display `not implemented: {0}`) lives in `index.rs` so `scripts/check-no-stubs.sh` catches the scaffold, and the conversions as `todo`-free stubs that return defaults; expose `to_pipeline_options` and `from_response` `#[doc(hidden)] pub` for the offline tests; `src/lib.rs`: `#![allow(unsafe_code)]`, `uniffi::setup_scaffolding!()` unconditionally, `pub mod ffi; mod index;`, crate docs placeholder; `cargo check -p xtriever-ffi --all-targets` clean

### Rust tests (red except where noted)

- [ ] T009 [P] Write `crates/xtriever-ffi/tests/support/mod.rs` (model dirs from `XTRIEVER_MODEL_DIR` / `XTRIEVER_RERANK_MODEL_DIR` with the `reference/models/…` defaults; `fixture_docs()` loading `reference/fixtures/005/hybrid.json`'s documents and queries; `build_fixture_index(dir) -> HybridIndex` with the real `MiniLmEmbedder` (buffered) over the fixture schema/`dense_fields`, one commit; `bits(f64) -> u64`, `bits32(f32) -> u32`) and `crates/xtriever-ffi/tests/errors.rs` (offline) covering **US3 scenario 5** and FR-009: for every current `xtriever_core::Error` variant constructed by hand (`Schema`, `InvalidQuery`, `UnknownField`, `DimensionMismatch`, `NotFound`, `Model`, `Corrupt`, `FingerprintMismatch`, `BudgetExhausted`, `Io`, `Backend`), `XtrieverError::from(e)` is the mirrored case with the fields/message equal to the core's `Display` pieces; the `Display` of each `XtrieverError` contains the message — **green** once T007 is in (the mapping is data, not scaffold; record it as green at the checkpoint)
- [ ] T010 [P] Write `crates/xtriever-ffi/tests/options.rs` (offline) covering FR-001/FR-007 at the conversion level: `to_pipeline_options` maps `depth`/`rerank_depth` `None`→`None` and `Some(n)`→`Some(n as usize)`, `max_time_ms: Some(200)`→`budget.max_time == Some(200 ms)`, `max_items`, `strict`, `explain`, and sets `elapsed` to the supplied closure exactly when `max_time_ms` is `Some`; `from_response` maps a hand-built pipeline `Response` (two hits, one re-ranked, one with chunk and explain; a `StageReport` with `rerank: Some(..)` and `degraded: Some(..)`) field by field including `DegradeReason::BudgetExceeded { elapsed_ms, limit_ms }` and `rerank.skipped`
- [ ] T011 [P] Write `crates/xtriever-ffi/tests/parity.rs` (`#[ignore]`, model-backed) covering **US1 scenarios 1–3, 5** and FR-003/FR-004 (SC-001 Rust half): build the fixture index in a temp dir; `XtrieverIndex::open(dir, embedder, Some(reranker), Buffered)` → `info()` has `documents == 40`, `format_version == 2`, the two identities, depths 100/20, `rrf_k 60`, `embedder_load_ms > 0`, `reranker_load_ms.is_some()`; for every fixture query, `search(text, SearchOptions { k: 10, rerank_depth: Some(5), explain: true, .. })` vs `HybridIndex::search(text, None, 10, &SearchOptions { rerank_depth: Some(5), explain: true, .. })` on a second handle: same `external_id` sequence, `bits(score)`, `bits32(rerank_score)`, `chunk`, and `explain` fields equal; `stages` equal field by field; the same with `reranker_dir: None` (`stages.rerank == None`, every `rerank_score` `None`); `elapsed_ms > 0`
- [ ] T012 [P] Write `crates/xtriever-ffi/tests/budget.rs` (`#[ignore]`) covering **US2 scenarios 2–3** at the Rust level (SC-002 partial): with the re-ranker, `max_time_ms: Some(200)`, `rerank_depth: Some(20)`, `k: 20` over the fixture queries ⇒ at least one query with `0 < stages.rerank.scored < stages.rerank.candidates` and every response `elapsed_ms < 1000`; `max_time_ms: Some(0)` ⇒ `stages.degraded.is_some()` and `stages.rerank.skipped.is_some()` in default mode, `Err(XtrieverError::BudgetExhausted { .. })` in strict; `time_limit_ignored` is never `true` when `max_time_ms` is set
- [ ] T013 [P] Write `crates/xtriever-ffi/tests/readonly.rs` (`#[ignore]`) covering **US1 scenario 4** and FR-002 (plus **US3 scenarios 1–4** on the Rust side): snapshot the fixture directory's recursive file list + sizes + mtimes; open + three searches (with re-ranker) + drop; snapshot again ⇒ identical; `open` on a temp dir with no descriptor ⇒ `Err(Corrupt { message })` naming the path; on a copy whose descriptor says `"format_version": 1` ⇒ `Corrupt` whose message contains `1`, `2` and `rebuild`; with `commit.pending` written ⇒ `Corrupt` containing `interrupted commit`; with the embedder directory pointed at the re-rank model directory ⇒ `Err(Model { .. })` (its pins fail); with one byte appended to `model.safetensors` in a temp copy of the embedder ⇒ `Model` naming the file and both sizes
- [ ] T014 [P] Write `crates/xtriever-ffi/examples/fixture_index.rs` (`anyhow` allowed): args `<out_dir>`; builds `<out_dir>/index/` from `reference/fixtures/005/hybrid.json` with the real embedder (buffered) exactly as `support::build_fixture_index`, then opens it through `XtrieverIndex::open` twice (with and without the re-ranker) and writes `<out_dir>/expected.json` per data-model "Parity goldens": `generated_by` (git head), `info`, and per fixture query `{ id, text, k: 10, rerank_depth: 5, with_reranker: { hits: [{ external_id, score_bits (u64 hex), rerank_score_bits (u32 hex or null), rerank_rank }], stages: { lexical_candidates, dense_candidates, rerank: { candidates, scored } } }, without_reranker: { … } }`; deterministic key order (serde_json with `preserve_order` off is fine: use `BTreeMap`s); **red** until PR 2 (the example fails on the scaffold's `not implemented`)

### Swift package skeleton and tests (written now, linkable after PR 3)

- [ ] T015 [P] Create `swift/Xtriever/Package.swift` per contract "The Swift package" (tools 5.9, iOS 16, `binaryTarget XtrieverFFI` at `Frameworks/XtrieverFFI.xcframework`, target `Xtriever` with `resources: [.copy("XtrieverData")]`, `testTarget XtrieverTests`), `Sources/Xtriever/XtrieverIndex.swift` as a stub (`public final class XtrieverIndex` with the contract's signatures throwing `XtrieverError.backend(message: "not implemented")`), `Sources/Xtriever/Measure.swift` (ported in T001, `import` names updated), and the tests: `Tests/XtrieverTests/Support.swift` (resource URLs from `Bundle.module`: `XtrieverData/models/{embedder,reranker}`, `XtrieverData/fixtures/index`, `expected.json`; `XCTSkip` when absent; `Float.bitPattern`/`Double.bitPattern` helpers), `ParityTests.swift` (**US1 sc. 2, 3, 5**, SC-001: for every query in `expected.json`, `search` with and without the re-ranker equals the goldens by id sequence, `score.bitPattern`, `rerank_score.bitPattern`, `rerank_rank`, and `stages` counts; `features()` yields the seven names), `AsyncTests.swift` (**US2 sc. 1, 4, 5**: a `Timer` on the main run loop at 50 ms keeps ≥ 90 % of its expected ticks during a search that takes ≥ 1 s at depth 20 — assert the search took ≥ 1 s or skip; two `async let` searches on one handle both return their own query's hits; a `Task` cancelled 10 ms after starting a search leaves a following `search` on the same handle succeeding), `BudgetTests.swift` (**US2 sc. 2–3**, SC-002: `maxTimeMs: 200` with the re-ranker returns in < 1 s with `0 < scored < candidates` on ≥ 1 query; `maxTimeMs: 0` ⇒ degraded / `XtrieverError.budgetExhausted` when strict), `ErrorTests.swift` (**US3 sc. 1–5**, SC-003: six cases — missing directory ⇒ `.corrupt`; a copied index with `format_version` 1 ⇒ `.corrupt` mentioning `rebuild`; `commit.pending` ⇒ `.corrupt`; the re-rank model dir as embedder ⇒ `.model`; a tampered weights copy ⇒ `.model` naming the file; strict `maxTimeMs: 0` ⇒ `.budgetExhausted` — each asserting the case and a message fragment; none crash), `InfoTests.swift` (**US1 sc. 1**: `info` fields), `DeviceMeasurementTests.swift` (**US5**; skipped unless `XtrieverData/scifact` and both models are present — body in T032)

### Red checkpoint

- [ ] T016 Run `cargo nextest run -p xtriever-ffi --no-fail-fast` and `cargo nextest run -p xtriever-ffi --run-ignored only --no-fail-fast`: `errors` **passes** (data), `options` fails on the stub conversions, every model-backed test fails on `not implemented` — no failure names a fixture, model or link problem; `cargo run --release -p xtriever-ffi --example fixture_index -- /tmp/x` fails on `not implemented`; `./scripts/check-no-stubs.sh` **FAILs** on `xtriever-ffi` (the `NotImplemented` scaffold type); the Swift tests are present but unlinkable (no XCFramework yet) — record that as their red state. Commit red (Rule 4). **Checkpoint — PR 1.**

---

## Phase 3: User Story 1 — A Swift caller opens a shipped index and searches it (Priority: P1) 🎯 MVP

**Goal**: The Rust surface — `open`, `info`, `search` with bit-identical parity, read-only —
plus the committed parity goldens. (The Swift half of US1 lands with the package in Phase 4.)

**Independent Test**: `cargo nextest run -p xtriever-ffi --run-ignored only` green
(`parity`, `readonly`, `info`); `expected.json` regenerates byte-identically. Completes
**PR 2**.

- [ ] T017 [US1] Implement `open` in `crates/xtriever-ffi/src/index.rs` (research D1; contract "Semantics"): time `MiniLmEmbedder::load(embedder_dir, load_path.into())`; `HybridIndex::open` (Buffered) or `open_mapped` (Mmap) with the boxed embedder; if `reranker_dir` is `Some`, time `MiniLmCrossEncoder::load(dir, load_path.into())` and `set_reranker`; every error through `XtrieverError::from`; store the `Duration`s; `impl From<LoadPath> for xtriever_dense::LoadPath` and `for xtriever_rerank::LoadPath` (both crates' `Mmap` variants are behind the `mmap` feature the FFI enables)
- [ ] T018 [US1] Implement `info` and the two conversions in `crates/xtriever-ffi/src/index.rs`: `info` from `HybridIndex::config()`, `len()`, `embedder().fingerprint()`, `reranker().map(|r| r.model_id())`, `FORMAT_VERSION`, the load times in ms; `to_pipeline_options` and `from_response` per T010's mapping (`u32`/`usize` via `usize::try_from`/`u32::try_from` with saturation on overflow, never `as` truncation of a count — hits and candidates are far below `u32::MAX`, but say so in a comment)
- [ ] T019 [US1] Implement `search` in `crates/xtriever-ffi/src/index.rs` (research D3): `let guard = inner.index.lock().map_err(|_| poisoned())?; let start = Instant::now(); let elapsed = || start.elapsed(); let opts = to_pipeline_options(&options, options.max_time_ms.map(|_| &elapsed as &dyn Fn() -> Duration)); let r = guard.search(&query, None, options.k as usize, &opts)?; Ok(from_response(r, start.elapsed().as_millis()))` — `elapsed_ms` measured after the lock; wire the three `ffi/mod.rs` shims to these
- [ ] T020 [US1] Run `cargo nextest run -p xtriever-ffi` (offline green) and `cargo nextest run -p xtriever-ffi --run-ignored only -E 'binary(parity) | binary(readonly)'` — **green**. A parity difference is ⛔ — the FFI adds no computation, so a differing bit is a conversion defect; report it, never a tolerance
- [ ] T021 [US1] Run `cargo run --release -p xtriever-ffi --example fixture_index -- swift/Xtriever/Tests/Fixtures` (models present); commit `swift/Xtriever/Tests/Fixtures/expected.json`; run it again to a temp dir and `diff` ⇒ identical (SC-001's goldens are reproducible); add the `parity.rs` assertion that `expected.json` equals a fresh in-test regeneration (so the committed goldens cannot drift from the code)
- [ ] T022 [US1] Delete every `NotImplemented` in `crates/xtriever-ffi/src/`; `./scripts/check-no-stubs.sh` **passes**; `RUSTFLAGS="-D warnings" cargo clippy -p xtriever-ffi --all-targets` and `--features cli` clean; `grep -rn 'unsafe' crates/xtriever-ffi/src/index.rs` prints nothing and the module carries `#![deny(unsafe_code)]`. **Checkpoint — PR 2.**

---

## Phase 4: User Story 4 — The surface ships as a Swift package built by one script (Priority: P2)

**Goal**: `scripts/build-ios-package.sh` produces the XCFramework, bindings and package; the
Swift tests link and run on the simulator.

**Independent Test**: quickstart Step 3 builds on a clean checkout; `InfoTests` and
`ParityTests` green on the simulator (the async/budget/error suites turn green in Phases 5–6).

- [ ] T023 [US4] Write `scripts/build-ios-package.sh` per contract "The build script" (steps 0–7; options `--debug --with-models --with-fixtures --with-scifact --app`): reuse `build-ios-harness.sh`'s structure and every explanatory comment (the `--module-name xtriever_ffiFFI` / `--modulemap-filename module.modulemap` / **no** `--xcframework` block; the arm64-only note; the `xcodegen` exit-2 rule) with the new paths — staticlibs for both targets with default features, bindings into `swift/Xtriever/Sources/Xtriever/Generated/`, headers + modulemap into `build/xcf-headers/`, `xcodebuild -create-xcframework` into `swift/Xtriever/Frameworks/XtrieverFFI.xcframework`; `--with-models` verifies both models with `scripts/fetch-model.sh` (default and `--manifest reference/models/manifest-rerank.json`) then stages them into `Sources/Xtriever/XtrieverData/models/{embedder,reranker}/`; `--with-fixtures` runs the `fixture_index` example into `Tests/Fixtures/` and stages `index/` + `expected.json` into `XtrieverData/fixtures/`; `--with-scifact` stages `target/xt-rerank-index/scifact/` (fail with the `beir run` command if absent) plus `XtrieverData/scifact/queries.json` (the first 20 judged SciFact queries by id, extracted from `reference/datasets/beir/scifact/queries.jsonl` + `qrels/test.tsv` with `jq`/python) and `expected-scifact.json` (T031); `--app` runs `xcodegen generate` in `swift/XtrieverHarnessApp/`; prints sizes and `build-ios-package: PASS`
- [ ] T024 [US4] Run `scripts/build-ios-package.sh --with-models --with-fixtures` on the host; then `cd swift/Xtriever && xcodebuild test -scheme Xtriever -destination 'platform=iOS Simulator,name=<available iPhone>' -configuration Release ARCHS=arm64 -only-testing:XtrieverTests/InfoTests` — the package links and `InfoTests` runs (it will still fail on the stub `XtrieverIndex.swift` until Phase 5; the goal here is the link). Record any toolchain trap hit (001 F-004/F-005 class) in the report
- [ ] T025 [US4] Write `swift/Xtriever/README.md` (from `harness/ios/README.md`'s surviving content): what the script produces, why the XCFramework and bindings are not committed, how to run the simulator tests, the `TEST_RUNNER_` environment prefix note, and the device run pointer (Phase 7)

---

## Phase 5: User Story 2 — Searches never block the caller and respect a time budget (Priority: P1)

**Goal**: The Swift async layer — serial queue + continuations — over the Rust surface.

**Independent Test**: `ParityTests`, `InfoTests`, `AsyncTests`, `BudgetTests` green on the
simulator.

- [ ] T026 [US2] Implement `swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift` (research D2; contract "The Swift package"): `public final class XtrieverIndex: @unchecked Sendable` holding the generated object and `private let queue = DispatchQueue(label: "dev.xtriever.index", qos: .utility)`; `public static func open(indexDir: URL, embedderDir: URL, rerankerDir: URL?, loadPath: LoadPath = .mmap) async throws -> XtrieverIndex` (runs the generated constructor on a one-off `DispatchQueue.global(qos: .utility)` via `withCheckedThrowingContinuation`); `public var info: IndexInfo` (cached at open); `public func search(_ query: String, options: SearchOptions = SearchOptions(k: 10, explain: true)) async throws -> SearchResponse` — `withCheckedThrowingContinuation { c in queue.async { c.resume(with: Result { try inner.search(query: query, options: options) }) } }`; `public extension SearchOptions { init(k:depth:rerankDepth:maxTimeMs:maxItems:strict:explain:) }` with the documented defaults; `public extension HitExplain { func features() -> [(name: String, value: Float)] }` under the seven names with `.nan` for absent; doc comments state serialisation, cooperative cancellation and the `mmap` external-writer precondition
- [ ] T027 [US2] Run the simulator suites: `xcodebuild test … -only-testing:XtrieverTests/ParityTests -only-testing:XtrieverTests/InfoTests -only-testing:XtrieverTests/AsyncTests -only-testing:XtrieverTests/BudgetTests` — **green** (SC-001 Swift half, SC-002). A main-thread cadence miss is ⛔ — a design defect in the queue/continuation, never a looser bound

---

## Phase 6: User Story 3 — Every failure arrives as a typed error on the Swift side (Priority: P2)

**Goal**: The six failure classes assert their mapped case and message in Swift.

**Independent Test**: `ErrorTests` 6 / 6 green on the simulator. Completes **PR 3**.

- [ ] T028 [US3] Make the error cases ergonomic in Swift if the generated enum needs it (in `swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift`): a `public extension XtrieverError { var message: String }` returning each case's message/field text so tests and the demo can show it; no re-mapping — the generated cases are the API
- [ ] T029 [US3] Run `xcodebuild test … -only-testing:XtrieverTests/ErrorTests` — **6 / 6 green**; then the whole simulator suite (`-skip-testing:XtrieverTests/DeviceMeasurementTests`) green. **Checkpoint — PR 3.**

---

## Phase 7: User Story 5 — The pipeline's on-device cost is measured, not assumed (Priority: P3)

**Goal**: The harness app hosts the tests on a physical iPhone; run records for SciFact with
both models mapped at depths 0 / 5 / 20 are committed with the 300 MB verdict.

**Independent Test**: `specs/007-ffi-surface/runs/*.json` exist with `footprint.verdict`,
per-depth latencies and parity results; a reader without a device can reproduce the verdict
from them.

- [ ] T030 [US5] Finish `swift/XtrieverHarnessApp/project.yml` + `App/XtrieverHarnessApp.swift` (ported in T001): package `../Xtriever`, product `Xtriever`, test target sources `../Xtriever/Tests/XtrieverTests`, scheme `XtrieverHarnessApp` testing `XtrieverHarnessAppTests` in Release with `RAYON_NUM_THREADS: "1"`; a second scheme `XtrieverHarnessApp-DefaultThreads` without the variable; `xcodegen generate` succeeds
- [ ] T031 [US5] Write the host-side truth for the device parity check: extend `crates/xtriever-ffi/examples/fixture_index.rs` with a `--scifact <index_dir> <queries.json> <out>` mode that opens the SciFact index through `XtrieverIndex` (buffered, both models) and writes `expected-scifact.json`: per query at depths 0, 5, 20 (`k: 10`, `explain`) the hit ids, `score_bits`, `rerank_score_bits`, and every hit's `bm25_score` bits (lexical parity is bit-exact on device, dense/re-rank scores within 1e-3 — research D9); wire it into `build-ios-package.sh --with-scifact`
- [ ] T032 [US5] Write `swift/Xtriever/Tests/XtrieverTests/DeviceMeasurementTests.swift` (**US5 scenarios 1–3**; ported from the 001 `DeviceMeasurementTests` shape): skip unless `XtrieverData/scifact` and both models exist; `Measure.snapshot()` before, after `open` (mapped, both models; time it; record `info.embedderLoadMs`/`rerankerLoadMs`), and around every query; for each of the 20 queries and each depth in [0, 5, 20]: `search(text, options: SearchOptions(k: 10, rerankDepth: depth, explain: true))`, record `elapsedMs`, `stages.rerank?.candidates/scored`; parity vs `expected-scifact.json`: lexical `bm25_score` bits and hit ids at depth 0 identical (assert), dense/re-rank max abs diff ≤ 1e-3 (assert), fused ids/order at depth 0 identical; footprint peak = max(sampled max, ledger peak) with `peakMethod`, verdict `PASS` iff every prerequisite held (Release, device not simulator, valid `task_info`) and peak ≤ 300 MB; a `--load-path buffered` variant selected by the `XTRIEVER_LOAD_PATH` test environment variable; emit the run record as JSON to the console and as an `XCTAttachment` per data-model "Device run record"
- [ ] T033 [US5] Device runs (manual; quickstart Step 5): `scripts/build-ios-package.sh --with-models --with-scifact --app`; open `swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj`, select the iPhone, Release; run `DeviceMeasurementTests` twice under the `RAYON_NUM_THREADS=1` scheme (mapped), once under the default-threads scheme (mapped), once mapped→buffered via `XTRIEVER_LOAD_PATH=buffered`; copy each run record verbatim to `specs/007-ffi-surface/runs/<device>-<date>-<n>.json`. A footprint over 300 MB is ⛔ — stop and report with the mapped/buffered breakdown; a parity miss beyond tolerance is ⛔ — report the pair. Derive the per-pair device cost as (mean depth-20 − mean depth-0) / 20 for the report

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Docs, report, PR description, the full gate. Completes **PR 4**.

- [ ] T034 [P] Write `crates/xtriever-ffi/src/lib.rs` crate docs: the surface (one object, three operations, read-only), the ADR-0003 layout (generated `unsafe` in `ffi/`, hand-written logic in `index.rs` re-denying), the clock and why it lives here, error lowering, the Swift package and where async lives, the build script; `cargo doc -p xtriever-ffi --no-deps` clean
- [ ] T035 [P] Write `specs/007-ffi-surface/report.md`: verdict; Rust suites (offline / model-backed), Swift simulator suite summary; the parity goldens; the device run table (device, OS, per-run peak footprint + verdict, load times, latency per depth mean/p50/max over 20 queries, derived per-pair cost beside 006's 116–163 ms, parity results, mapped vs buffered); the FR-015 diff (empty); the FR-012 CI diff (two lines removed); what FR-018 deleted (line counts) and what was ported; findings
- [ ] T036 [P] Write `specs/007-ffi-surface/pr-description.md`: summary, four-commit split with hand-written line counts (deletions shown separately), the parity and simulator results, the device table, "no eval delta due — the FFI adds no computation; parity proves it", the "no ADR, no new `unsafe`, no new dependency class" line, CI unchanged beyond two removed lines (standing rule)
- [ ] T037 Run the full gate from quickstart Step 6: `cargo fmt --all --check`; `RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets`; `RUSTFLAGS="-D warnings" cargo clippy -p xtriever-ffi --features cli --all-targets`; `cargo nextest run --workspace`; `cargo nextest run -p xtriever-ffi --run-ignored only`; `cargo deny check`; `cargo check --workspace --target aarch64-apple-ios` / `aarch64-apple-ios-sim` / `aarch64-linux-android`; `cargo check --workspace --target wasm32-unknown-unknown` (best-effort, record the failure point); `./scripts/check-no-stubs.sh`; `./scripts/check-containment.sh`; `./scripts/check-toolchain.sh`; `grep -rn 'unsafe' crates/xtriever-ffi/src/index.rs` (nothing) and the module's `deny(unsafe_code)` present; the eval purity `cargo tree` (now also `uniffi` absent); the FR-015 `git diff --stat main -- crates/xtriever-core crates/xtriever-lexical crates/xtriever-dense crates/xtriever-rerank crates/xtriever-pipeline deny.toml` (empty); `ls harness/ios crates/xtriever-ffi/src/spike` (absent); the simulator suite once more after the final build. Paste the results into `report.md` and `pr-description.md`. Any failure ⛔ — stop and report

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T002 → T003; T004 ‖ T005
- **Red suite (Phase 2)**: T006 → T007 → T008 (after T002); T009–T014 after T008; T015 after T001 (Measure.swift copied) — parallel with the Rust tests; T016 last → **PR 1**
- **US1 (Phase 3)**: T017 → T018 → T019 → T020 → T021 → T022 → **PR 2**
- **US4 (Phase 4)**: T023 → T024 → T025 (needs PR 2 for the fixture goldens)
- **US2 (Phase 5)**: T026 → T027 (needs T024's linking package)
- **US3 (Phase 6)**: T028 → T029 (needs US2's layer) → **PR 3**
- **US5 (Phase 7)**: T030 ‖ T031 → T032 → T033 (needs PR 3)
- **Polish (Phase 8)**: T034 ‖ T035 ‖ T036 → T037 → **PR 4**

### Rule 6 stop-points (⛔)

T020 (a parity bit difference); T027 (main-thread cadence miss); T033 (footprint over 300 MB, or
a device parity miss beyond tolerance); T037 (any gate failure). The response is a report,
never a looser bound, a wider tolerance or a raised ceiling.

### Parallel Opportunities

- Phase 1: T004 ‖ T005
- Phase 2: T009 ‖ T010 ‖ T011 ‖ T012 ‖ T013 ‖ T014 ‖ T015 (seven files/trees)
- Phase 7: T030 ‖ T031
- Phase 8: T034 ‖ T035 ‖ T036

---

## Parallel Example: Phase 2 red suite

```text
after T008 (scaffold) and T001 (Measure.swift + harness app ported):
  T009 support + errors     T010 options        T011 parity        T012 budget
  T013 readonly             T014 fixture_index  T015 Swift package skeleton + six test files
then T016 (red checkpoint, PR 1)
```

## Implementation Strategy

1. **PR 1** (Phases 1–2): the spike goes, the scaffold and red suites come in, the Swift
   package skeleton with its tests is written.
2. **PR 2** (Phase 3) — **MVP**: the Rust surface with bit-identical parity, read-only,
   and the committed goldens. Usable from any uniffi consumer already.
3. **PR 3** (Phases 4–6): the build script, the package, the async layer, typed errors —
   green on the simulator.
4. **PR 4** (Phases 7–8): the device harness, four run records with the 300 MB verdict and
   the per-depth latencies, the report, the gate.
