# Contract: the FFI surface, the Swift package, the build script

**Feature**: `007-ffi-surface` | **Date**: 2026-09-13 | **Plan**: [../plan.md](../plan.md)

The whole surface after Feature 007: the Rust exports (what uniffi lowers), the Swift layer over
them, the build script, and the measurement harness. The spike's three operations and their
types are gone (FR-018). No API stability is promised yet; Feature 009 will shape it.

## Rust exports (`crates/xtriever-ffi/src/ffi/`)

```rust
#[derive(uniffi::Object)]
pub struct XtrieverIndex { /* Mutex<HybridIndex>, load timings */ }

#[uniffi::export]
impl XtrieverIndex {
    /// Open a hybrid index read-only with the pinned embedder and, optionally, the pinned re-ranker.
    /// Locked open when the directory permits; lock-free only when it refuses the lock file (an app bundle) — 008 D11.
    #[uniffi::constructor]
    pub fn open(index_dir: String, embedder_dir: String, reranker_dir: Option<String>, load_path: LoadPath) -> Result<Arc<Self>, XtrieverError>;
    /// Identity and configuration of the open index.
    pub fn info(&self) -> IndexInfo;
    /// One search; serialised per handle; the time budget is measured from entry.
    pub fn search(&self, query: String, options: SearchOptions) -> Result<SearchResponse, XtrieverError>;
}

#[derive(uniffi::Record)] pub struct SearchOptions { pub k: u32, pub depth: Option<u32>, pub rerank_depth: Option<u32>, pub max_time_ms: Option<u64>, pub max_items: Option<u32>, pub strict: bool, pub explain: bool }
#[derive(uniffi::Record)] pub struct SearchResponse { pub hits: Vec<Hit>, pub stages: StageReport, pub elapsed_ms: u64 }
#[derive(uniffi::Record)] pub struct Hit { pub external_id: String, pub text: String, pub score: f64, pub rerank_score: Option<f32>, pub chunk: Option<ChunkInfo>, pub explain: Option<HitExplain> }
#[derive(uniffi::Record)] pub struct ChunkInfo { pub parent: String, pub ordinal: u32, pub byte_start: Option<u64>, pub byte_end: Option<u64> }
#[derive(uniffi::Record)] pub struct HitExplain { pub bm25_score: Option<f32>, pub bm25_rank: Option<u32>, pub dense_score: Option<f32>, pub dense_rank: Option<u32>, pub fused: f64, pub rerank_score: Option<f32>, pub rerank_rank: Option<u32> }
#[derive(uniffi::Record)] pub struct StageReport { pub lexical_candidates: u32, pub dense_candidates: Option<u32>, pub degraded: Option<Degradation>, pub rerank: Option<RerankReport>, pub time_limit_ignored: bool }
#[derive(uniffi::Record)] pub struct RerankReport { pub candidates: u32, pub scored: u32, pub skipped: Option<DegradeReason> }
#[derive(uniffi::Record)] pub struct Degradation { pub stage: String, pub reason: DegradeReason }
#[derive(uniffi::Enum)]   pub enum DegradeReason { StageError { message: String }, BudgetExceeded { elapsed_ms: u64, limit_ms: u64 } }
#[derive(uniffi::Record)] pub struct IndexInfo { pub documents: u64, pub format_version: u32, pub embedder_fingerprint: String, pub reranker_model_id: Option<String>, pub candidate_depth: u32, pub rerank_depth: u32, pub rrf_k: u32, pub embedder_load_ms: u64, pub reranker_load_ms: Option<u64> }
#[derive(uniffi::Enum)]   pub enum LoadPath { Buffered, Mmap }
#[derive(uniffi::Error)]  pub enum XtrieverError { Schema { message }, InvalidQuery { message }, UnknownField { field }, DimensionMismatch { expected: u64, actual: u64 }, NotFound { id: u32 }, Model { model, message }, Corrupt { message }, FingerprintMismatch { index, current }, BudgetExhausted { message }, Io { message }, Backend { message } }
impl From<xtriever_core::Error> for XtrieverError { /* one-to-one; wildcard → Backend (research D4) */ }
```

Module layout keeps ADR-0003: `src/lib.rs` (`#![allow(unsafe_code)]`, `uniffi::setup_scaffolding!()`),
`src/ffi/{mod,types,error}.rs` (the exports and derives — generated `unsafe` only, logic-free
shims), `src/index.rs` (`#![deny(unsafe_code)]` — the hand-written logic: open, option
conversion, the clock, the response conversion). `src/bin/uniffi-bindgen.rs` unchanged
(`--features cli`).

### Semantics

| item | behaviour | errors |
|---|---|---|
| `open` | `MiniLmEmbedder::load(embedder_dir, load_path)` → `HybridIndex::open` / `open_mapped` (by `load_path`) → if `reranker_dir`: `MiniLmCrossEncoder::load(reranker_dir, load_path)` + `set_reranker`; load times recorded. Nothing is written; a `commit.pending`, wrong version, mismatched fingerprint or torn store is refused by the pipeline | the pipeline's / models' errors, lowered one-to-one |
| `info` | descriptor-derived fields + `embedder.fingerprint()` + `reranker().map(model_id)` + load times | — |
| `search` | lock → `start = Instant::now()` → pipeline `SearchOptions { depth, rerank_depth, strict, budget: Budget { max_time: max_time_ms.map(Duration::from_millis), max_items }, elapsed: max_time_ms.map(\|_\| &\|\| start.elapsed()), explain }` → `HybridIndex::search(query, None, k, &opts)` → convert; `elapsed_ms` = wall time of the call after the lock | the pipeline's, lowered; strict mode surfaces `Model` (stage failure) vs `BudgetExhausted` (spent budget) |

**Determinism**: for the same directory, models, query and options, `search` through the FFI
returns exactly `HybridIndex::search`'s hits (ids, order, `score` and `rerank_score` bits) —
the FFI adds no computation.

## The Swift package (`swift/Xtriever/`)

```swift
public final class XtrieverIndex: @unchecked Sendable {
    public static func open(indexDir: URL, embedderDir: URL, rerankerDir: URL?, loadPath: LoadPath = .mmap) async throws -> XtrieverIndex
    public var info: IndexInfo { get }
    public func search(_ query: String, options: SearchOptions = .init(k: 10, explain: true)) async throws -> SearchResponse
}
public extension HitExplain { func features() -> [(name: String, value: Float)] }   // the seven names, .nan for absent
public extension SearchOptions { init(k: UInt32, depth: UInt32? = nil, rerankDepth: UInt32? = nil, maxTimeMs: UInt64? = nil, maxItems: UInt32? = nil, strict: Bool = false, explain: Bool = false) }
```

- `open` and `search` run on a private serial `DispatchQueue` (`.utility`) and resume the
  caller through `withCheckedThrowingContinuation`; the caller's thread is never blocked
  (FR-005); calls on one instance are serialised in call order (FR-006); a cancelled task drops
  the result when it arrives (FR-008).
- The generated types (`SearchOptions`, `Hit`, `XtrieverError`, …) are the package's public
  types as uniffi emits them; the wrapper adds only the async surface and `features()`.
- `Package.swift`: `platforms: [.iOS(.v16)]`, Swift tools 5.9, `binaryTarget XtrieverFFI`
  (`Frameworks/XtrieverFFI.xcframework`, gitignored), target `Xtriever` with
  `resources: [.copy("XtrieverData")]` (staged, gitignored), `testTarget XtrieverTests`.
  `Measure.swift` (ported from 001) lives in the library target so the device harness can use it.

## The build script (`scripts/build-ios-package.sh`)

`scripts/build-ios-package.sh [--debug] [--with-models] [--with-fixtures] [--with-scifact] [--app]`

| step | does | 001 trap it encodes |
|---|---|---|
| 0 | `scripts/check-toolchain.sh` | F-001 |
| 1 | `cargo build -p xtriever-ffi --release --target aarch64-apple-ios` and `aarch64-apple-ios-sim` (default features) | — |
| 2 | `uniffi-bindgen` `--swift-sources` → `Sources/Xtriever/Generated/`; `--headers`; `--modulemap --module-name xtriever_ffiFFI --modulemap-filename module.modulemap` | F-006 (`--module-name`; never `--xcframework`) |
| 3 | `xcodebuild -create-xcframework` with both `.a` slices and a headers dir holding header + modulemap → `Frameworks/XtrieverFFI.xcframework` | — |
| 4 | `--with-fixtures`: `cargo run --release -p xtriever-ffi --example fixture_index -- <out>` → `Tests/Fixtures/index/` + `expected.json`; stage into `XtrieverData/fixtures/` | — |
| 5 | `--with-models`: stage both model directories into `XtrieverData/models/{embedder,reranker}/` (verified by `fetch-model.sh` first) | — |
| 6 | `--with-scifact`: stage `target/xt-rerank-index/scifact` (built by `beir run --config hybrid-rerank-v1`) + the 20 measurement queries + `expected-scifact.json` into `XtrieverData/scifact/` | — |
| 7 | `--app`: `xcodegen generate` in `swift/XtrieverHarnessApp/` (ARCHS arm64) — exit 2 with the remedy if `xcodegen` is missing | F-007, F-008 |

Exit codes: 0 PASS, 1 failure, 2 incomplete (harness app not generated).

## The measurement harness (`swift/XtrieverHarnessApp/` + `DeviceMeasurementTests`)

An xcodegen-generated iOS app hosting `XtrieverTests` on a device (a device rejects hostless
test bundles — 001 F-007). `DeviceMeasurementTests` (skipped unless `XtrieverData/scifact`
and both models are present) opens SciFact with both models, runs the 20 queries at depths
0 / 5 / 20, samples `Measure` around every call, checks parity against `expected-scifact.json`,
and writes the run record JSON to the test attachments / console for committing under
`specs/007-ffi-surface/runs/`.

## CI (`.github/workflows/ci.yml`)

The two `cargo check -p xtriever-ffi --features spike --target …` lines are removed; the
existing `cargo check --workspace --target aarch64-apple-ios` / `-sim` lines cover the surface.
No simulator, device or model step (FR-012; standing rule).

## Scripts and files removed

`harness/ios/**` (spike package, harness app, README, DEVICE-RUN.md — the content that survives
is `Measure.swift` and the run-record shape, ported), `scripts/build-ios-harness.sh` (replaced),
`crates/xtriever-ffi/src/spike/**`, `crates/xtriever-ffi/examples/gen_ranking.rs`, the spike's
tests and the `spike` feature with its optional dependencies. `reference/fixtures/001/` and
`reference/models/001/` stay (001's report cites them); the model directories are gitignored
anyway.
