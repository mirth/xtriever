# PR text for Feature 007 — the FFI surface

Hand-written line counts: `crates/xtriever-ffi/src` ~470 (−650 spike) · `crates/xtriever-ffi/tests`
+ example ~700 · Swift package (`XtrieverIndex.swift`, ported `Measure.swift`, `Package.swift`,
README) ~480 · Swift tests ~620 · scripts ~230 · harness app ~90 · docs. **Deletions**: 2,137
lines (the 001 spike: `harness/ios/**`, `src/spike/**`, its tests, example and dependencies).
Suggested split (plan.md Rule 3):

| PR | contents | lines |
|---|---|---|
| 1 | spike deletion, dependency re-pointing, `.gitignore`, CI (−2), wire types + error enum + scaffold, red Rust suite, Swift package skeleton + tests, `fixture_index` example | ~1,900 (+) / 2,137 (−) |
| 2 | `index.rs` (open / info / search / conversions) — Rust suites green, `expected.json` goldens | ~350 |
| 3 | `build-ios-package.sh`, `XtrieverIndex.swift` (async layer, `writableCopy`), README — simulator suite green | ~450 |
| 4 | harness app schemes, `DeviceMeasurementTests`, `extract-device-run.py`, three device run records, report | ~500 |

---

## Title

`feat(ffi): Swift surface over the hybrid pipeline — read-only open, async searches with budgets, typed errors, one-script package, device measurement (Feature 007)`

## Body

Replaces the Feature 001 spike in `xtriever-ffi` with the real surface: one uniffi object,
`IndexHandle`, over `Mutex<HybridIndex>` — read-only `open` with both pinned models (buffered or
mapped), `info`, and `search` whose `Instant` feeds the pipeline's elapsed-time source so the
005/006 budget semantics cross the boundary unchanged. Every core error is lowered one-to-one
to `XtrieverError` with the engine's message. The Swift package `swift/Xtriever/` adds the
async layer — a serial dispatch queue plus continuations, because uniffi 0.32.1 polls Rust
futures on the caller's thread (research D2) — and `writableCopy(of:named:)` for bundled
indexes (report F-001). `scripts/build-ios-package.sh` builds both slices, the bindings, the
XCFramework, stages models / fixtures / SciFact and generates the device harness project.

Spec: `specs/007-ffi-surface/spec.md` · Plan: `plan.md` · Report: `report.md`

**Parity**: Rust `IndexHandle::search` equals `HybridIndex::search` bit-for-bit with and
without the re-ranker (`tests/parity.rs`); Swift hits equal committed goldens minted by the FFI
on the host (`Tests/Fixtures/expected.json`, pinned against drift). **Tests**: Rust 4 / 4
offline + 9 / 9 model-backed; Swift 16 / 16 on the simulator (parity, info, async
non-blocking / serialisation / cancellation, budgets, 6 error classes); workspace 224 / 224.
Committed red first (PR 1: Rust 2 / 4 offline, 0 / 8 model-backed on the scaffold; Swift
unlinkable).

**Device measurement (iPhone 16e, iOS 26.6.2, Release; SciFact + both models; 20 queries ×
depths 0 / 5 / 20)** — three records under `specs/007-ffi-surface/runs/`:

| run | load path | threads | ledger peak | verdict vs 300 MB (v1.3.0) | per pair | depth 5 | depth 20 |
|---|---|---|---:|---|---:|---:|---:|
| 1 | mmap | 1 | **376.4 MB** | **FAIL** | 464 ms | 2.67 s | 9.60 s |
| 2 | buffered | 1 | **383.0 MB** | **FAIL** | 462 ms | 2.64 s | 9.55 s |
| 3 | mmap | default | **372.7 MB** | **FAIL** | 299 ms | 1.39 s | 6.13 s |

**⛔ Rule 6 stop-and-report (F-002)**: two resident models put the full pipeline over the
ceiling — 262 MB after `open` with mapped weights (candle copies every tensor to the heap;
mapping saves ~100 MB on iOS, not enough), +80–115 MB transient during re-ranking. Nothing was
tuned; the levers (one model on device, `F16`/int8 weights, shorter re-rank input, loading the
re-ranker per query) are listed for the owner's decision. Parity on device: lexical
bit-identical 20 / 20, dense Δ 1.8e-7, re-rank Δ 5.7e-6.

**No eval delta is due**: the FFI adds no computation; parity proves the hits are the
pipeline's.

**Gate**: fmt · clippy `-D warnings` (workspace, `cli`) · nextest · deny · iOS / iOS-sim /
Android · no stubs · `check-containment` · toolchain · `index.rs` re-denies `unsafe`
(ADR-0003 layout kept) · eval graph pure · FFI graph C-free · `xtriever-core`, `-lexical`,
`-dense`, `-rerank`, `-pipeline`, `deny.toml` unchanged. No ADR, no new dependency class, no new
`unsafe`. CI: two `--features spike` lines removed; no job added (standing rule). After the
first push the Ubuntu test job ran out of disk (lld SIGBUS while linking a candle-bearing test
binary — report F-007): `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=line-tables-only`
in the workflow env, plus a `df -h` step on Linux; nothing compiled or tested changes.

**Constitution amended to v1.4.0 (ADR-0010)** on F-002, by owner decision: Principle III's
default on-device RSS ceiling becomes 600 MB for the full pipeline (100k-chunk index with
both models loaded), MINOR bump. The three run records keep their 300 MB FAIL verdict as
recorded; under 600 MB the same peaks pass. `DeviceMeasurementTests.ceilingBytes` → 600 MB;
CLAUDE.md and the plan template follow the wording. Nothing in the pipeline changed.

**Review round 1** (Copilot, nine comments, all taken — report table): retired
`gen_001_fixtures.py --emit-ranking`; exactly-20 query check and a clean resource tree in
the build script; atomic `writableCopy`; strict device parity (missing depth / hit /
one-sided score = failure); cancelled callers get `CancellationError`; cadence test skips
instead of failing when a ≥ 1 s search cannot be produced; read-only docs corrected to
"content never modified, directory must be writable". Simulator suite 16 / 16 after.

**Review round 2** (Copilot, six comments — report table): budget clock now starts before the
Rust lock (FR-007) with an overlapping-search test; the Swift queue wait is deliberately *not*
charged to the budget (documented, with the reason); `writableCopy` validates `name`, stages
under a unique sibling, swaps with `replaceItemAt` under a lock (two new tests); device parity
matches hits by id at the re-ranked depths; `Mmap` precondition names the dense index too; the
false cross-instance serialisation claim removed. Simulator 18 / 18, release budget tests 3 / 3.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
