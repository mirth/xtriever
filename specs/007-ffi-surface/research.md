# Phase 0 Research: The FFI Surface

**Feature**: `007-ffi-surface` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

Every registry item below was read from the pinned sources on 2026-09-13 and is cited as
`crate-version/path:line` (Agent Operating Rule 1); workspace items as `crate/path:line`. The
binding generator is the one Feature 001 pinned (`uniffi 0.32.1`); no new external dependency
is introduced (D12).

---

## D1. The surface: one uniffi object over the pipeline, synchronous Rust

**Decision**: `xtriever-ffi` exports one object, `XtrieverIndex`, holding
`Mutex<xtriever_pipeline::HybridIndex>`, with a constructor `open(index_dir, embedder_dir,
reranker_dir: Option<String>, load_path)` and two methods, `info()` and `search(query, k,
options)` — plain synchronous functions on the Rust side. The uniffi 0.32.1 proc-macro
attributes are used: `#[derive(uniffi::Object)]` (`uniffi_macros-0.32.1/src/lib.rs:131`),
`#[uniffi::constructor]` (`:320`), `#[uniffi::export]` on the `impl`, `#[derive(uniffi::Record)]`
/ `uniffi::Enum` / `uniffi::Error` for the wire types — the same macros the 001 spike used
(`crates/xtriever-ffi/src/ffi/mod.rs`, `types.rs`, `error.rs`), so ADR-0003's containment (a
crate-level `#![allow(unsafe_code)]` for generated code; hand-written modules re-deny) carries
over unchanged.

uniffi objects must be `Send + Sync` (`uniffi_core-0.32.1/src/ffi_converter_traits.rs:155`,
`:462`, blanket `HandleAlloc` at `:648`). `HybridIndex` holds a `TantivyIndex` (asserted
`Send + Sync` at `xtriever-lexical/src/index.rs:60-64`), a `FlatIndex`, the `PassageStore`
(plain data + a path), and boxed `Embedder` / `Reranker` (both traits require `Send + Sync`). The
`Mutex` is nonetheless the right container: it makes the object `Sync` by construction whatever
the fields do, and it **is** the per-handle serialisation FR-006 asks for — two searches on one
handle queue on the lock. Read-only: `HybridIndex::open` creates no tantivy writer
(`xtriever-lexical/src/index.rs:104-121` sets `writer: None`; the writer is created lazily at
`:171` by the mutation paths, which the FFI never calls), so nothing is written and no
pipeline API change is needed (FR-015).

**Rationale**: the pipeline's API already is the surface; the FFI adds a lock, a clock and
wire types. No trait of its own, no generics (Rule 7).

## D2. Async lives in the Swift layer, not in Rust

uniffi 0.32.1 can export `async fn`, but its future is polled **on the thread that calls
`rust_future_poll`** — `RustFuture::poll` (`uniffi_core-0.32.1/src/ffi/rustfuture/future.rs:205-
218`) locks the future and calls `poll` synchronously before returning; the Swift side calls it
from `uniffiRustCallAsync` (`uniffi_bindgen-0.32.1/src/bindings/swift/templates/Async.swift:6-
18`) on the awaiting task's thread. A CPU-bound body (a 2–3 s search) would therefore block a
thread of Swift's cooperative pool — exactly what FR-005 forbids. The only runtime uniffi 0.32.1
knows how to hand a future to is tokio (`AsyncRuntime::Tokio`,
`uniffi_macros-0.32.1/src/export/attributes.rs:271-273`), which Principle III excludes from the
pure crates and this feature does not want in the FFI crate either.

**Decision**: the Rust exports are synchronous. The Swift package wraps them: `XtrieverIndex`
(Swift) owns a private serial `DispatchQueue` (`.utility` QoS) and exposes
`func search(...) async throws -> SearchResponse` implemented with
`withCheckedThrowingContinuation` — the blocking call runs on the queue's thread, never on the
caller's or the cooperative pool. The queue plus the Rust `Mutex` give FR-006 (serialised per
handle); the caller's thread is never blocked (FR-005); a cancelled Swift task simply has no one
awaiting the continuation — the work runs to its budget and the result is dropped (FR-008,
"cooperative"). The constitution's "async wrappers only in `xtriever-ffi` or server crates"
is honoured: the wrapper is the FFI layer's Swift half, and no pure crate changes.

**Rejected**: Rust `async fn` + tokio (a runtime in the FFI crate for one blocking call);
a Rust worker thread per handle with a channel (re-implements what a serial queue already is).

## D3. The time budget: `Instant` in the FFI shim feeds the pipeline's elapsed-time source

`SearchOptions.elapsed: Option<&dyn Fn() -> Duration>` (`xtriever-pipeline/src/types.rs`) is the
pipeline's clock; the pipeline reads none itself. **Decision**: the FFI `search` takes
`let start = Instant::now()` on entry and passes `elapsed: Some(&|| start.elapsed())` whenever
the caller set a time budget, so check points A/B/C and the re-ranker's *remaining* time behave
exactly as 005 D7 / 006 D8 define (FR-007). `xtriever-ffi` is not a pure crate (Principle III
names it with the leaf crates); the containment script's clock check covers the five pure
crates and is unchanged. The budget's `max_time` is a `u64` of milliseconds on the wire.

## D4. Errors: one wire enum mirroring the core's variants, lowered with the engine's message

**Decision**: `XtrieverError` (`uniffi::Error`, `#[non_exhaustive]` is *not* used on the wire
type — Swift needs the full case list) with one variant per `xtriever_core::Error` variant
(`xtriever-core/src/error.rs:9-57`): `Schema`, `InvalidQuery`, `UnknownField`,
`DimensionMismatch { expected, actual }`, `NotFound { id }`, `Model { model, message }`,
`Corrupt`, `FingerprintMismatch { index, current }`, `BudgetExhausted`, `Io`, `Backend` — each
carrying the engine's message. The `From<xtriever_core::Error>` mapping matches every current
variant explicitly; because the core enum is `#[non_exhaustive]` the match needs a wildcard,
which lowers to `Backend { message: e.to_string() }` — a variant added later is not silently
lost, it arrives with its text. The 001 spike deliberately declined this mapping ("changes
error semantics, needs an ADR"): it does not — no core variant, message or semantics changes;
the FFI *lowers* each variant one-to-one, which Principle V's "error semantics unchanged"
permits. A unit test enumerates every variant and asserts its mirrored kind and message, so a
future variant that falls into the wildcard shows up as a test to extend.

`search` errors in strict mode keep their kinds, so Story 3 scenario 5 (stage failure vs spent
budget) is `Model` vs `BudgetExhausted`. Panics: every export returns `Result`, which uniffi
lowers to a Swift `throws`; uniffi additionally catches a panic inside an export and reports it as an
internal error rather than aborting (`uniffi_core-0.32.1/src/ffi/rustcalls.rs:207`
`panic::catch_unwind(callback)`, with a second `catch_unwind` at `:228` around the message), so
no panic crosses the boundary (FR-009); the crate has none by lint anyway.

## D5. Wire types

Records (`uniffi::Record`): `SearchOptions { k, depth: Option<u32>, rerank_depth:
Option<u32>, max_time_ms: Option<u64>, max_items: Option<u32>, strict, explain }`; `Hit {
external_id, text, score: f64, rerank_score: Option<f32>, chunk: Option<ChunkInfo>, explain:
Option<HitExplain> }`; `ChunkInfo { parent, ordinal, byte_start: Option<u64>, byte_end: Option<u64> }` (mirrors the
core's `byte_range` tuple flattened); `HitExplain { bm25_score, bm25_rank, dense_score, dense_rank, fused, rerank_score,
rerank_rank }` (all `Option` except `fused`); `StageReport { lexical_candidates,
dense_candidates: Option<u32>, degraded: Option<Degradation>, rerank: Option<RerankReport>,
time_limit_ignored }`; `RerankReport { candidates, scored, skipped: Option<DegradeReason> }`;
`Degradation { stage, reason }`; `SearchResponse { hits, stages, elapsed_ms }` (the FFI's own
wall time, for the UI); `IndexInfo { documents: u64, format_version, embedder_fingerprint,
reranker_model_id: Option<String>, candidate_depth, rerank_depth, rrf_k }`. Enums:
`LoadPath { Buffered, Mmap }`, `DegradeReason { StageError { message }, BudgetExceeded {
elapsed_ms, limit_ms } }`. Feature names are not re-declared: `HitExplain` is positional and
the Swift layer offers `features()` returning `[(String, Float)]` under the same seven names.

`f32` scores cross as `Float`, `f64` as `Double` — uniffi lowers both bit-exactly, so SC-001's
"bit-for-bit" is checkable in Swift via `bitPattern`.

## D6. Package layout and the spike's retirement (FR-018, user decision A)

```
swift/Xtriever/                         the Swift package (SwiftPM), consumed by path
├── Package.swift                       binaryTarget XtrieverFFI (xcframework, gitignored) + target Xtriever + tests
├── Sources/Xtriever/Generated/         uniffi output (gitignored, produced by the script)
├── Sources/Xtriever/XtrieverIndex.swift  the async layer (D2), features(), convenience
├── Sources/Xtriever/Measure.swift      ported from harness/ios (task_vm_info footprint, verdict gate)
├── Sources/Xtriever/XtrieverData/      staged resources (fixture index, models, SciFact — gitignored)
└── Tests/XtrieverTests/                parity, async, budget, errors (simulator); DeviceMeasurementTests (device)
swift/XtrieverHarnessApp/               xcodegen project hosting the tests on a device (ported from harness/ios/XtrieverSpikeApp)
scripts/build-ios-package.sh            replaces build-ios-harness.sh
```

`harness/ios/` is deleted, as are `crates/xtriever-ffi/src/spike/`, `src/ffi/{types,error}.rs`
(replaced), the spike's seven Rust test files, `examples/gen_ranking.rs`, and the `spike`
feature with its optional dependencies (`tantivy`, `tokenizers`, `candle-*`, `serde_json`,
`sha2` become unnecessary: the pipeline and stage crates bring their own). `Measure.swift` and
the run-record format come across verbatim with their attributions; `DeviceMeasurementTests`
is rewritten for Story 5. The 001 report and `specs/001-ios-build-spike/runs/` stay.

## D7. The build script: 001's traps become steps

`scripts/build-ios-package.sh [--debug] [--with-models] [--with-fixtures] [--with-scifact]
[--app]` does, in order: `check-toolchain.sh`; `cargo build -p xtriever-ffi --release --target
aarch64-apple-ios` and `-sim` (default features — the surface is no longer feature-gated,
D11); `uniffi-bindgen` `--swift-sources`, `--headers`, `--modulemap --module-name
xtriever_ffiFFI --modulemap-filename module.modulemap` (001 F-006: **not** `--xcframework`);
`xcodebuild -create-xcframework` with both slices and a headers directory holding header +
modulemap; stage resources on request (models 87 + 87 MB; the fixture index + `expected.json`;
the SciFact index); `xcodegen generate` for the harness app (001 F-007: a device rejects a
hostless test bundle; `ARCHS: arm64`, 001 F-008). Exit 2 with the reason when `xcodegen` is
missing, as today. The XCFramework is ~260 MB and gitignored (001 FR-031's "reproducible from
source" rule).

## D8. The fixture index and the Swift parity goldens

The Swift parity test (SC-001) needs an index the FFI can open with the **real** models (the
FFI names `MiniLmEmbedder` / `MiniLmCrossEncoder`; a stub cannot cross the boundary). **Decision**:
`crates/xtriever-ffi/examples/fixture_index.rs` builds a hybrid index from the 005
`hybrid.json` documents (40 documents, the same schema and `dense_fields`) with the real
embedder into `swift/Xtriever/Tests/Fixtures/index/` (gitignored, ~1 MB) and writes
`swift/Xtriever/Tests/Fixtures/expected.json` (**committed**): for each of the 8 fixture
queries, the FFI's own `search` at `k = 10`, `rerank_depth` 5, `explain` — hit ids, `score`
bits, `rerank_score` bits, `rerank_rank` — with and without the re-ranker, plus `IndexInfo`.
Swift compares by `bitPattern`. The goldens are minted by the same Rust code the Swift side
calls, on the same architecture, so equality is the boundary's correctness, not the model's;
model correctness is 004/006's business.

## D9. Device measurement (Story 5)

Index: the SciFact `hybrid-rerank-v1` directory (`beir run --config hybrid-rerank-v1 --dataset
scifact --index-dir …` — 5,183 documents; ~10 MB lexical, 8.0 MB dense, ~8 MB `passages.bin`),
staged by the script. Queries: the first 20 judged SciFact queries by id. Per run: open time,
each model's load time (the FFI records them into `IndexInfo`? — no: the harness times `open`
with the re-ranker and without, the difference is the re-ranker's load), then 20 queries at
`rerank_depth` 0, 5, 20 (with `k = 10`, `explain`), each timed via `SearchResponse.elapsed_ms`;
footprint sampled by `Measure` around every call; peak = max(ledger peak, sampled max) as 001
did. `RAYON_NUM_THREADS`: 1 (001's setting, for comparability) and unset (candle's default —
the phone's core count) as two separate runs. Both models `Mmap`, plus one `Buffered` run for
the memory comparison. Run records committed under `specs/007-ffi-surface/runs/` in 001's JSON
shape. Host-side truth for SC-006: the same 20 queries through the same FFI on the Mac,
`expected-scifact.json`, compared on device: lexical ids/scores bit-identical; dense and
re-rank scores within 1e-3 (the 004/006 tolerances) — the 001 cross-architecture rule.

## D10. CI

The two `cargo check -p xtriever-ffi --features spike --target …` lines in `ci.yml` are
replaced by nothing: with the surface as the default feature set, the existing `cargo check
--workspace --target aarch64-apple-ios` / `-sim` lines already cover it (FR-012). No
simulator, device, or model step is added (standing rule). `cargo nextest run --workspace` now
compiles the FFI crate's offline tests (error/options mapping) on every runner.

## D11. Feature gating

**Decision**: the surface is the crate's default. The 001 spike sat behind `spike` so the
default build was empty; the real surface *is* the crate. `cli` (bindgen binary) stays
non-default. Every dependency of the surface is pure Rust (candle/tantivy through the stage
crates; uniffi is pure Rust), so Principle III's "C/C++ behind non-default features" has
nothing to gate.

## D12. Dependencies

`cargo add -p xtriever-ffi --path crates/xtriever-pipeline --features mmap`,
`--path crates/xtriever-dense --features mmap`, `--path crates/xtriever-rerank --features mmap`
(the concrete model types for `open`); `uniffi 0.32.1` becomes non-optional (already locked);
`cargo remove` the spike's `tantivy`, `tokenizers`, `candle-core`, `candle-nn`,
`candle-transformers`, `serde_json`, `sha2`. Dev: `tempfile`, `serde_json`. `deny.toml`
unchanged. `cargo tree -p xtriever-ffi -e normal | grep -E '(-sys|^cc |onig|openssl)'` empty
(FR-011).

## D13. Test strategy

- **Rust offline** (`crates/xtriever-ffi/tests/`): `errors.rs` — every core variant maps to its
  mirrored kind with the message (D4); `options.rs` — wire `SearchOptions` → pipeline options
  (depth/rerank_depth `None`→`None`, `max_time_ms`→`Duration`, `k` passed through), defaults;
  `surface.rs` — the exported `impl` has no mutation method (a compile-time reflection is not
  available; the test opens a fixture directory read-only and asserts the directory's mtimes and
  file list are unchanged after `open` + `search` — model-backed, `#[ignore]`).
- **Rust model-backed** (`#[ignore]`): `parity.rs` — `XtrieverIndex::open` on the fixture index
  and `search` equal `HybridIndex::search` bit-for-bit for every fixture query, with and
  without the re-ranker; `budget.rs` — `max_time_ms: 200` with the re-ranker gives
  `0 < scored < depth` on at least one query, `max_time_ms: 0` degrades (default) / errors
  (strict); `info.rs` — `IndexInfo` fields.
- **Swift, simulator** (`swift/Xtriever/Tests/XtrieverTests/`): `ParityTests` (SC-001 via
  `expected.json`), `AsyncTests` (main-thread timer keeps its cadence during a ≥ 1 s search;
  two back-to-back searches serialise; a cancelled task leaves the handle usable),
  `BudgetTests` (SC-002), `ErrorTests` (SC-003: 6 classes), `InfoTests`.
- **Device** (`DeviceMeasurementTests`, hosted by the harness app): Story 5.

## Risks

| Risk | Mitigation |
|---|---|
| A 2–3 s (laptop) search is 5–10 s on a phone at depth 20 | That is the measurement's purpose; depths 0/5 are in the run for the demo's defaults |
| Two models mapped + SciFact exceed 300 MB on device | 001's mapped-weights headline (2.56 MB per model resident) says no; if yes it is a finding and the buffered/mapped comparison says where the memory is |
| uniffi-generated Swift trips Swift 6 strict concurrency in the demo later | The package declares Swift 5 language mode; the async layer is `Sendable`-clean by construction (a final class holding a queue and an `Arc` handle) |
| Xcode/toolchain drift since 001 (F-004/F-005 class failures) | The script re-checks the toolchain first and fails with the 001 remedy text |
| `Instant` in the FFI shim | Permitted (not a pure crate); `check-containment.sh` covers only the five pure crates and is unchanged |
