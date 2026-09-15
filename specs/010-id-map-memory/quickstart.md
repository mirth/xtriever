# Quickstart: validating the Id Map's Resident Memory

**Feature**: `010-id-map-memory` | **Date**: 2026-09-15 | **Plan**: [plan.md](./plan.md)

## Step 0 — Prerequisites

The 008 index at `target/xt-wiki/index/` (`ids.json` 32,037,349 B, sha256 `20028054…`), the
two models under `reference/models/`, the SciFact / NFCorpus / FiQA caches under
`target/xt-dense-cache` and the rerank indexes under `target/xt-rerank-index/` (all present
from 006–008). Xcode's licence accepted (`sudo xcodebuild -license accept` — Xcode 27.0 was
installed on 2026-09-15 and blocks every link until then).

## Step 1 — Red checkpoint (Rule 4): the baseline by the final method

```bash
cargo nextest run -p xtriever-pipeline --lib                     # id_map_cost, the no-slot refusal: red (run before the Arc sharing test is added — that one breaks the build, the recorded red state)
cargo nextest run -p xtriever-pipeline --test ids_golden          # byte identity from the pre-change golden: green today (it was written by this code)
XTRIEVER_IDS_JSON=$PWD/target/xt-wiki/index/ids.json cargo test -p xtriever-pipeline --release --lib id_map_cost -- --nocapture
```

Expected at the red commit: the synthetic 100k test fails on the bound (~207 B/passage held
against ≤ ~64); the transient test fails (peak ≈ file + 2.7× the bound); the sharing test,
added last, does not compile (`Arc::ptr_eq` on plain fields); the full-index run prints **held ≈ 88.5 MB, peak ≈
138.6 MB, 207 B/passage** — the "before" line of the report — and fails the same bounds.

## Step 2 — Green (after the implementation)

```bash
cargo nextest run -p xtriever-pipeline                            # everything, including the unchanged 005–008 suites
XTRIEVER_IDS_JSON=$PWD/target/xt-wiki/index/ids.json cargo test -p xtriever-pipeline --release --lib id_map_cost -- --nocapture
XTRIEVER_IDS_JSON=$PWD/target/xt-wiki/index/ids.json cargo test -p xtriever-pipeline --release --lib ids_golden_full -- --nocapture   # write-back sha256 == 20028054…
```

Expected: held ≈ 27 MB (≈ 63 B/passage) and peak ≈ 67 MB for the full file, both under their
bounds; the synthetic bound predicts the full-index figure within 20 % (SC-003); read time
within +10 % of the red-commit figure printed by the same test (FR-009); the 008 `ids.json`
reproduces byte-for-byte from read → write.

## Step 3 — Results unchanged on the host

```bash
cargo nextest run --workspace                                     # 251 + the new tests
cargo nextest run -p xtriever-pipeline -p xtriever-ffi --release --run-ignored only -j 1   # model-backed suites, serially (008 F-007)
cargo run --release -p xtriever-cli -- wiki verify --index target/xt-wiki/index \
  --embedder-dir reference/models/all-MiniLM-L6-v2 --snapshot-dir reference/datasets/wiki   # every passage re-read through the new map: PASS
cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki/index --embedder-dir reference/models/all-MiniLM-L6-v2 \
  --reranker-dir reference/models/ms-marco-MiniLM-L-6-v2 --queries reference/fixtures/008/queries.json --out /tmp/expected-010.json
diff <(jq 'del(.generated_by)' /tmp/expected-010.json) <(jq 'del(.generated_by)' target/xt-wiki/expected.json)    # empty: 20 queries × 3 depths identical (generated_by carries the build's commit)
```

## Step 4 — BEIR unchanged (Rule 5; local only — CI keeps its SciFact smoke)

```bash
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-rerank-v1 --cache-dir target/xt-dense-cache \
    --index-dir target/xt-rerank-index/$d --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 --out /tmp/$d-010.json
  diff <(jq .metrics /tmp/$d-010.json) <(jq .metrics specs/006-rerank-stage/baselines/hybrid-rerank-v1.$d.json) && echo "$d identical"
done
```

Expected: nDCG@10 / Recall@100 identical on all three, per query.

## Step 5 — Device record (manual; the reference iPhone 16e, plugged and unlocked)

```bash
scripts/build-ios-package.sh --with-models --with-wiki --app                   # rebuilds the XCFramework from this branch; staging unchanged
TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj -scheme XtrieverHarnessApp -configuration Release \
  -destination 'platform=iOS,id=XXXXXX' -skipMacroValidation -allowProvisioningUpdates ARCHS=arm64 DEVELOPMENT_TEAM=XXXXXX \
  -only-testing:XtrieverHarnessAppTests/DeviceMeasurementTests 2>&1 | tee /tmp/wiki-device-010.log
scripts/extract-device-run.py /tmp/wiki-device-010.log specs/010-id-map-memory/runs/
```

Expected: `afterOpen` ≤ 359 MB (008 run 2: 509.0), peak PASS vs 600 MB, open within +10 % of
1,009 ms, depth 0 / 5 / 20 medians within ±10 % of 339 / 864 / 2,285 ms, parity lexical
bit-identical 20 / 20, fused order 20 / 20, dense / re-rank within tolerance. Below 150 MB of
saving is ⛔ stop-and-report (Rule 6), not a threshold to move. Optionally the demo app's
`DemoMeasurementTests` (009 quickstart) for the "within 10 MB" clause of SC-001.

## Step 6 — Full gate (Rule 5)

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
cargo nextest run --workspace && cargo deny check
for t in aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android; do cargo check --workspace --target $t; done
cargo check --workspace --target wasm32-unknown-unknown   # best-effort, tracked failure at getrandom
scripts/check-no-stubs.sh
git diff --stat main -- crates/xtriever-core deny.toml crates/xtriever-lexical crates/xtriever-dense crates/xtriever-rerank crates/xtriever-ffi swift/ apps/ .github/   # empty
```
