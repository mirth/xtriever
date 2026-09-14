# Quickstart: validating the iOS Wikipedia Demo App

**Feature**: `009-ios-wiki-demo` | **Date**: 2026-09-14 | **Plan**: [plan.md](./plan.md)

## Step 0 — Resources

```bash
scripts/fetch-model.sh && scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json
ls target/xt-wiki/index/xtriever-pipeline.json target/xt-wiki/expected.json   # the 008 artefact (rebuild: 008 quickstart Step 4 — minutes from the cache)
xcodegen --version
```

## Step 1 — Red checkpoint (Rule 4)

```bash
scripts/build-ios-package.sh --with-models --with-fixtures --demo
cd apps/ios-wiki-demo && xcodebuild test -project XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo \
  -destination 'platform=iOS Simulator,id=822F3C90-5124-432B-B84A-75426A04722D' -configuration Debug ARCHS=arm64 2>&1 | tail -5
```

Expected: the test target does not compile (no `DemoModel`, no `ChangeMark`) — the red state,
recorded. The app target compiles as a scaffold that shows "not implemented".

## Step 2 — The model and views on the simulator (after PR 2–3)

```bash
cd apps/ios-wiki-demo && xcodebuild test -project XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo \
  -destination 'platform=iOS Simulator,id=822F3C90-5124-432B-B84A-75426A04722D' -configuration Debug ARCHS=arm64 \
  2>&1 | grep -E 'Test Case.*(passed|failed|skipped)|TEST (SUCCEEDED|FAILED)'
```

Expected: `ChangeMarkTests` (pure + fixture goldens), `DemoModelTests` (ready with timings;
fused == `withoutReranker`, re-ranked == `withReranker` goldens; empty query; cancellation
20 / 20; depth 0 → no re-rank report; strict 1 ms → displayed engine error; main-thread
cadence ≥ 90 %); `AboutTests` skipped (fixture only — "which index is open" shown instead);
`DemoMeasurementTests` skipped.

Then run the app on the simulator (`xcodebuild build` + Simulator, or Xcode) and search the
fixture: the fused list, the re-ranked list with marks, a hit's explanation, the stage report,
Settings, About saying "fixture index".

## Step 3 — The full app on the device (manual, recorded)

```bash
scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki --demo
# run the app on the phone from Xcode (Release), airplane mode on: search, open a hit, open the article, About
TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo-Measure \
  -configuration Release -destination 'platform=iOS,id=A3C0F8DE-11F1-5D1E-AA6F-C728A4F91BB4' -skipMacroValidation -allowProvisioningUpdates \
  ARCHS=arm64 DEVELOPMENT_TEAM=J483F464F3 -only-testing:XtrieverWikiDemoTests/DemoMeasurementTests 2>&1 | tee /tmp/demo-device.log
scripts/extract-device-run.py /tmp/demo-device.log specs/009-ios-wiki-demo/runs/
```

Expected: `AboutTests` green on the device build (sidecar / attribution equality — SC-004); a
run record with `feature: "009-ios-wiki-demo"`, footprint verdict against 600 MB (SC-005),
median fused ≤ 1 s and median total ≤ 3 s after warm-up (SC-001), maxima beside them.

## Step 4 — Gate (Rule 5)

```bash
cargo fmt --all --check && RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets && cargo nextest run --workspace && cargo deny check
git diff --stat main -- crates/ swift/Xtriever/Sources/                      # empty, or only what the plan names
git diff --stat main -- .github/                                             # empty (standing rule)
cd swift/Xtriever && xcodebuild test -scheme Xtriever -destination 'platform=iOS Simulator,id=822F3C90-5124-432B-B84A-75426A04722D' -configuration Release ARCHS=arm64 -skip-testing:XtrieverTests/DeviceMeasurementTests   # still 18 / 18
```

No eval delta is due: the app adds no computation to the pipeline.

## Step 5 — Report

`specs/009-ios-wiki-demo/report.md`: the simulator suite, the device record (footprint,
medians and maxima per stage, thread count observed), the cost of the fused-first design
(one extra fused pass per query), screenshots' worth of description, findings.
