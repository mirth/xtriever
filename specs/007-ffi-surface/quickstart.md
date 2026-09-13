# Quickstart: validating the FFI Surface

**Feature**: `007-ffi-surface` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

> **Status**: **executed 2026-09-13** — every step ran; results in [report.md](./report.md).
> Parity bit-identical (Rust and Swift); simulator suite 16 / 16; device: **footprint FAIL 372–383 MB
> vs 300 MB on all three runs (⛔ F-002, reported, not tuned — then resolved by amendment: constitution
> v1.4.0 / ADR-0010 sets the default at 600 MB, under which the same peaks PASS)**; per pair 299–464 ms on the phone.

## Step 0 — Toolchain, models, indexes

```bash
./scripts/check-toolchain.sh                                   # rustup toolchain + both iOS targets + Xcode SDK
scripts/fetch-model.sh && scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json
ls target/xt-rerank-index/scifact/xtriever-pipeline.json        # from 006 (Step 6 needs it; rebuild with `beir run --config hybrid-rerank-v1 --dataset scifact --index-dir target/xt-rerank-index/scifact` if absent)
xcodegen --version                                              # brew install xcodegen (device runs only)
```

## Step 1 — Red checkpoint (Rule 4)

```bash
cargo nextest run -p xtriever-ffi                               # offline: errors/options mapping red on NotImplemented
cargo nextest run -p xtriever-ffi --release --run-ignored only  # model-backed: parity, budget, info, read-only — red (release: each test embeds the 40-document fixture; debug takes ~8 min per test)
./scripts/check-no-stubs.sh                                     # FAILs here, PASSes after PR 2
```

The Swift tests are written in PR 1 too but cannot run until the package builds (Step 3).

## Step 2 — The Rust surface (after PR 2)

```bash
cargo nextest run -p xtriever-ffi
cargo nextest run -p xtriever-ffi --release --run-ignored only
cargo run --release -p xtriever-ffi --example fixture_index -- swift/Xtriever/Tests/Fixtures
git diff --stat swift/Xtriever/Tests/Fixtures/expected.json     # regenerated goldens are byte-identical
```

Expected: every core error variant maps to its mirrored kind with the message (SC-003 Rust
half); the fixture index searched through `XtrieverIndex` equals `HybridIndex::search`
bit-for-bit on every fixture query, with and without the re-ranker (SC-001 Rust half); a
200 ms budget gives a partial re-rank; the index directory's file list and mtimes are unchanged
after open + search (read-only, FR-002).

## Step 3 — The package on the simulator (after PR 3)

```bash
scripts/build-ios-package.sh --with-models --with-fixtures
cd swift/Xtriever && xcodebuild test -scheme Xtriever \
  -destination 'platform=iOS Simulator,name=iPhone 16' -configuration Release 2>&1 | tail -20
```

Expected: `ParityTests` 100 % ids/order and bit-identical scores vs `expected.json` (SC-001);
`AsyncTests` — a main-thread timer keeps ≥ 90 % of its ticks during a ≥ 1 s search, two
back-to-back searches serialise, a cancelled task leaves the handle usable (SC-002);
`BudgetTests` — 200 ms budget returns < 1 s with `0 < scored < depth` on ≥ 1 query;
`ErrorTests` 6 / 6 classes with the mapped case and message (SC-003); `InfoTests`.

## Step 4 — Rust gate parts that touch this feature

```bash
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo tree -p xtriever-ffi -e normal --prefix none | grep -Ei '(-sys|^cc |onig|openssl)' ; echo "(expect nothing — FR-011)"
git diff --stat main -- .github/workflows/ci.yml                # only the two spike lines removed (FR-012)
```

## Step 5 — Device measurement (after PR 4; manual, recorded)

```bash
scripts/build-ios-package.sh --with-models --with-scifact --app
open swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj      # select the device, Release, run XtrieverTests/DeviceMeasurementTests
# twice with RAYON_NUM_THREADS=1 (the scheme default), once unset, once with load path Buffered
# copy each run record from the test attachment into specs/007-ffi-surface/runs/
```

Expected: a run record per run with `footprint.verdict` against the constitution's ceiling, 20 queries × 3 depths
with `elapsed_ms`, lexical parity bit-identical 20 / 20, dense and re-rank max abs diff ≤ 1e-3
(SC-005, SC-006). The per-pair cost on the device is derived as (depth-20 mean − depth-0 mean)
/ 20 and recorded beside 006's laptop number.

## Step 6 — Full gate (Rule 5)

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
RUSTFLAGS="-D warnings" cargo clippy -p xtriever-ffi --features cli --all-targets
cargo nextest run --workspace
cargo nextest run -p xtriever-ffi --release --run-ignored only
cargo deny check
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo check --workspace --target wasm32-unknown-unknown   # best-effort; fails at getrandom via candle — tracked
./scripts/check-no-stubs.sh
./scripts/check-containment.sh                            # the five pure crates: no unsafe, no clock; the FFI crate is not in scope (ADR-0003 allows generated unsafe; Instant is permitted)
grep -rn 'unsafe' crates/xtriever-ffi/src/index.rs ; echo "(expect nothing — hand-written module re-denies)"
grep -n 'deny(unsafe_code)' crates/xtriever-ffi/src/index.rs   # expect the module-level deny
cargo tree -p xtriever-eval -e normal | grep -E 'candle|tantivy|memmap2|xtriever-pipeline|xtriever-rerank|uniffi' ; echo "(expect nothing)"
git diff --stat main -- crates/xtriever-core crates/xtriever-lexical crates/xtriever-dense crates/xtriever-rerank crates/xtriever-pipeline deny.toml   # expect empty (FR-015)
ls harness/ios crates/xtriever-ffi/src/spike 2>&1 | head -2   # expect "No such file" (FR-018)
```

## Step 7 — Report

`specs/007-ffi-surface/report.md`: nextest summaries (Rust offline / model-backed, Swift
simulator), the parity goldens, the device run table (footprint verdict, latency per depth,
per-pair cost, parity), the FR-015 diff, what the 001 spike's deletion removed, findings.
