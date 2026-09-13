# Feature 007 Report: The FFI Surface

**Branch**: `007-ffi-surface` | **Closed**: 2026-09-13 | **Spec**: [spec.md](./spec.md) |
**Plan**: [plan.md](./plan.md)

## Verdict

The surface works and is proven at the boundary: one uniffi object (`IndexHandle`) over
`Mutex<HybridIndex>` opens a shipped index read-only with both pinned models, and its hits equal
the Rust pipeline's **bit for bit** — in Rust (9 / 9 model-backed tests) and in Swift against
goldens the FFI minted on the host (16 / 16 simulator tests). Async lives in the Swift package:
a serial dispatch queue plus continuations keep the caller's thread free (a 50 ms main-thread
timer keeps ≥ 90 % of its ticks through a second of searches), serialise calls per handle, and
make cancellation cooperative. Every core error crosses as its mirrored case with the engine's
message. One script builds both static libraries, the bindings, the XCFramework, the resources
and the device harness; CI lost two lines and gained none. The 001 spike is gone (2,137 lines);
`Measure.swift` and the run-record discipline survived.

**The device measurement is a ⛔ Rule 6 stop-point.** On the iPhone 16e, with SciFact and both
models memory-mapped, the process footprint peaks at **372–376 MB against the 300 MB ceiling —
FAIL on all three runs** (buffered: 383 MB). Nothing was tuned around it: the runs are committed
verbatim under [`runs/`](./runs/), the breakdown is in F-002, and the decision is the human's.
Latency on the phone: **299–464 ms per re-ranked pair** (default threads / one thread), 6.1–9.6 s
per query at depth 20, 1.4–2.7 s at depth 5, 0.16–0.32 s at depth 0. Parity on device: lexical
bit-identical 20 / 20, dense Δ 1.8e-7, re-rank Δ 5.7e-6 (tolerance 1e-3).

| | |
|---|---|
| Rust suite | **4 / 4** offline (errors, options) · **9 / 9** model-backed in release (parity with/without re-ranker, goldens pinned, budget, info, read-only, open failures) |
| Swift suite (simulator, iPhone 15 Pro / iOS 17.5, Release) | **16 / 16** — Parity 3, Info 2, Async 3, Budget 2, Error 6; `DeviceMeasurementTests` runs there too with verdict `untested (simulator)` |
| workspace | **224 / 224** (the 226 of 006 minus the spike's six ungated tests, plus the four new ones) |
| parity goldens | `swift/Xtriever/Tests/Fixtures/expected.json`: 8 queries × {with, without re-ranker} × 10 hits, score bits as hex; regenerates byte-identically; pinned by `tests/parity.rs` |
| device runs | 3 committed records (`runs/iPhone17,5-*.json`): mapped/1 thread, buffered/1 thread, mapped/default threads — footprint **FAIL** ×3, parity **PASS** ×3 |
| gate | fmt ✓ · clippy `-D warnings` (workspace, `cli`) ✓ · nextest ✓ · deny ✓ · iOS / iOS-sim / Android ✓ · wasm32 best-effort fails at `getrandom` via candle (unchanged) · no stubs ✓ · `check-containment` ✓ · toolchain ✓ · `index.rs` re-denies `unsafe` ✓ · eval graph pure ✓ · FFI graph C-free ✓ |
| `xtriever-core`, `-lexical`, `-dense`, `-rerank`, `-pipeline`, `deny.toml` | **unchanged** (`git diff --stat main` empty; FR-015, SC-007) |
| CI | `ci.yml`: the two `--features spike` lines removed, a comment updated; no simulator, device or model step (FR-012) |

## The surface, as built

Rust (`crates/xtriever-ffi`): `IndexHandle::{open, info, search}`, the wire records and
`XtrieverError` in `ffi/` (generated `unsafe` only, ADR-0003), the logic in `index.rs`
(`#![deny(unsafe_code)]`): the `Instant` taken at entry becomes the pipeline's elapsed-time
source exactly when `max_time_ms` is set, so check points A/B/C and the re-ranker's remaining
time behave as 005/006 define. `SearchOptions` carries uniffi defaults, so Swift writes
`SearchOptions(k: 10, rerankDepth: 5, maxTimeMs: 200, explain: true)`.

Swift (`swift/Xtriever`): `XtrieverIndex.open(...) async throws`, `.search(_:options:) async
throws`, `.info`, `HitExplain.features()` (the seven names), `XtrieverError.message`, and —
after F-001 — `XtrieverIndex.writableCopy(of:named:)`. `swift/XtrieverHarnessApp` (xcodegen)
hosts the tests on a device with two schemes (`RAYON_NUM_THREADS=1` and default).
`scripts/build-ios-package.sh` and `scripts/extract-device-run.py` are the two tools.

## Device measurement (Story 5) — iPhone 16e (`iPhone17,5`), iOS 26.6.2, Release

SciFact `hybrid-rerank-v1` index (5,183 documents; 19 MB with `passages.bin`), both models,
20 judged queries × depths 0 / 5 / 20, `k = 10`, explain. Records committed verbatim.

| run | load path | threads | thermal | after `open` | sampled max | **ledger peak** | verdict | depth 0 | depth 5 | depth 20 | per pair |
|---|---|---|---|---:|---:|---:|---|---:|---:|---:|---:|
| 1 | mmap | 1 | nominal | 262.0 MB | 340.2 MB | **376.4 MB** | **FAIL** | 318 ms | 2,673 ms | 9,599 ms | 464 ms |
| 2 | buffered | 1 | nominal | 361.0 MB | 383.0 MB | **383.0 MB** | **FAIL** | 306 ms | 2,639 ms | 9,551 ms | 462 ms |
| 3 | mmap | default (6 cores) | fair | 259.2 MB | 362.5 MB | **372.7 MB** | **FAIL** | 157 ms | 1,391 ms | 6,128 ms | 299 ms |

Process baseline 13.9 MB in every run; `open` 347–386 ms (embedder ~180 ms, re-ranker ~180 ms
mapped); device per-app limit observed at 3.54 GB (a different threshold, kept apart as in 001).
Parity vs the host's `expected-scifact.json` on every run: lexical hits and BM25 scores
bit-identical 20 / 20, fused order at depth 0 identical 20 / 20, dense max |Δ| 1.79e-7, re-rank
max |Δ| 5.72e-6. A second mapped/1-thread run for wall-time reproducibility was not taken: the
device had already reached `fair` thermal state after run 3, and a fourth run would not have
been comparable (001 F-009); the three runs agree on footprint within 3 %.

## Findings

### F-001 — The lexical backend needs a writable directory to open (device bundles are read-only)

The first device run failed at `open` with `Failed to acquire Lockfile: … PermissionDenied`:
tantivy's `MmapDirectory` opens `lexical/.tantivy-meta.lock` for writing at every `Index::open`,
and an iOS app bundle is read-only. Reproduced on the host with `chmod -R a-w` on a pristine
index copy. The index *content* is never modified — the lock is zero bytes and already exists in
a built index — but spec FR-002's "the directory is never written" is not literally true at the
filesystem level, and a read-only mount refuses the open. `xtriever-lexical` is out of scope
(FR-015), so the fix is in the Swift layer: `XtrieverIndex.writableCopy(of:named:)` copies a
bundled index into Application Support once (keyed by the descriptor bytes) and the harness and
tests open the copy. Documented on `open`. The cost is one copy of the index on first launch
(19 MB here; the Wikipedia index of Feature 008 will be far larger — that feature should weigh a
lock-free read-only open in `xtriever-lexical` against the copy).

### F-002 — ⛔ Two resident models put the full pipeline over the 300 MB ceiling on device

The 001 headline ("mapped weights cut footprint ~40×, 101 MB → 2.56 MB") measured one
*embedding call* after the buffered path had freed its buffer (001 report's own ordering
caveat; 004 research D1 explained it: candle 0.9.2 copies every tensor onto the heap whichever
loader is used, so mapping removes the transient file buffer, not the tensors). With **two**
models resident and a real index, the phone shows what that means:

| component (mapped, run 1) | footprint |
|---|---|
| process before `open` | 13.9 MB |
| after `open`: two models' tensors (2 × ~87 MB `F32`), two tokenizers, SciFact (dense 8 MB buffered, lexical mapped, passage offsets), candle/tantivy working sets | **262.0 MB** (+248 MB) |
| peak during depth-5/20 re-ranking (attention over 512-token pairs, transient tensors) | **340–376 MB** (+80–115 MB) |

Mapping the weights is worth **~100 MB at open** on iOS (361 → 262 MB — far more than the 13 %
/ 2.7 % the laptop showed, because `phys_footprint` does not count clean file-backed pages),
and it is still not enough. The re-ranker's transient tensors alone are ~90 MB at depth 20.
**Nothing was changed to make this pass** (Rule 6): the ceiling, the depths and the models are
as specified. The levers, all outside this feature's scope and all needing a spec of their own:
(a) one model instead of two — drop the re-ranker on device (the fused pipeline alone would
sit at ~175 MB + transients); (b) smaller weights — `F16` or int8 tensors halve or quarter the
174 MB, but candle's CPU matmul paths and the determinism story change with them (a Principle I
measurement plus an ADR); (c) a smaller re-rank input — truncating pairs to 256 tokens cuts the
transient attention memory ~4× and the per-pair cost, at a measured quality cost; (d) not
keeping both models resident — load the re-ranker per query (180 ms mapped) and drop it.
The decision is the owner's; this report only records the numbers.

### F-003 — The phone re-ranks a pair in 299–464 ms, 2.5–3× the laptop

006 measured 116–163 ms per pair on the M1 Pro at 4 threads. The iPhone 16e: 464 ms at one
thread, 299 ms with candle's default pool (6 cores; the device warmed to `fair` during that
run). Depth 20 is 6–10 s per query — unusable interactively; depth 5 is 1.4–2.7 s; depth 0
(fused only) 0.16–0.32 s. For the demo app (Feature 009) the numbers say: default depth 0–5,
a time budget of 1–2 s, and the partial-re-rank path as the normal case, exactly what the
budget contract was designed for.

### F-004 — uniffi 0.32.1 polls Rust futures on the caller's thread (design consequence, not a defect)

Confirmed in the pinned source before writing a line: `RustFuture::poll` runs the future's
`poll` synchronously on the thread calling `rust_future_poll`, and the only runtime uniffi can
hand a future to is tokio. A Rust `async fn` around a multi-second search would have blocked
Swift's cooperative pool. The Swift-side serial queue is the boring answer and is what the
non-blocking test proves.

### F-005 — `@testable import` does not survive a Release test build

The first simulator build failed with "unable to resolve Swift module dependency to a
compatible module": Release builds disable testability. The tests use the public API only, so
the import became a plain `import Xtriever`. Recorded because the message points nowhere near
the cause.

### F-006 — The signing team is not the certificate's parenthetical

`DEVELOPMENT_TEAM=<certificate suffix>` failed with "No profiles for 'dev.xtriever.harness'";
the valid wildcard iOS profile on this Mac belongs to team `J483F464F3`, which is what the
`Developer ID` identity's suffix shows but the `Apple Development` identity's does not.
`security cms -D -i <profile>` on the installed `.mobileprovision` files gives the team that
works. Recorded for the next person running the harness.

### F-007 — The Ubuntu CI runner ran out of disk on the workspace test build

First push of this branch: `ld terminated with signal 7 [Bus error]` while linking
`xtriever-dense`'s `model_pins` test. Rust 1.91 links x86_64 Linux with lld, which writes its
output through a memory-mapped file, so a full disk surfaces as SIGBUS rather than as `ENOSPC`.
The cause is volume, not code: the 001 spike kept uniffi/candle/tantivy behind the non-default
`spike` feature, so this branch is the first to link the FFI crate in full on Linux, and the
workspace now has 69 test executables, ~35 of them carrying candle + tantivy — on Linux each
embeds the full DWARF of that tree, against ~14 GB of runner disk. Fix in `ci.yml`, environment
only: `CARGO_INCREMENTAL=0` (a fresh runner never reuses the cache; 800 MB of a 3.7 GB fresh
build locally) and `CARGO_PROFILE_DEV_DEBUG=line-tables-only` (backtraces keep file:line; the
type/variable DWARF goes). Nothing compiled, tested or linted changes; the macOS and Windows
jobs are unaffected. A `df -h` / `du -sh target` step after nextest on Linux records the
headroom for the next time.

## Success criteria → evidence

| SC | evidence |
|---|---|
| SC-001 | Rust `parity.rs` (3 tests) bit-identical with/without re-ranker; Swift `ParityTests` 3 / 3 vs `expected.json`; goldens regenerate byte-identically |
| SC-002 | `AsyncTests`: ≥ 90 % of 50 ms ticks kept through ≥ 1 s of searches; back-to-back searches return their own hits; a cancelled task leaves the handle usable; `BudgetTests`: 200 ms budget < 1 s with `0 < scored < candidates` |
| SC-003 | `ErrorTests` 6 / 6 (`Corrupt` ×3 naming path / versions / marker, `Model` ×2 naming file and sizes, `BudgetExhausted`); Rust `errors.rs` enumerates every core variant |
| SC-004 | `build-ios-package.sh` from this checkout → package, XCFramework (279 MB), simulator suite green; `ci.yml` diff −2 lines |
| SC-005 | three device records with footprint verdicts (**FAIL**) and per-depth latencies; per-pair cost derived (299 / 462 / 464 ms) |
| SC-006 | lexical bit-identical 20 / 20 on every run; dense 1.8e-7 and re-rank 5.7e-6 within 1e-3 |
| SC-007 | FR-015 diff empty; pure crates untouched (`check-containment` PASS); FFI graph C-free; three mobile targets check |

## Known costs (stated, not claimed small)

- **The 300 MB ceiling is not met with two resident models** (F-002) — the feature's headline,
  and the input to whatever comes next.
- A bundled index must be copied out before it can be opened (F-001).
- The XCFramework is 279 MB (both slices, release) and the staged models 174 MB — none committed.
- The model-backed Rust suite must run in release (`--release`): each test embeds the 40-document
  fixture, ~8 min per test in debug.

## Review round 1

_(GitHub Copilot comments, when they arrive, are recorded here with the action taken.)_
