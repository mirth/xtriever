# Quickstart: validating The Demo Re-ranks at Depth 10

**Feature**: `018-demo-rerank-depth-10` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## Step 1 — Red (Rule 4)

```bash
scripts/build-ios-package.sh --with-models --with-fixtures --demo
cd apps/ios-wiki-demo && xcodebuild test -project XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo \
  -destination 'platform=iOS Simulator,id=XXXXX' -configuration Debug ARCHS=arm64 -only-testing:XtrieverWikiDemoTests/SettingsTests 2>&1 | grep -E "Test Case.*(passed|failed)|TEST (SUCCEEDED|FAILED)"
```

Expected: `SettingsTests` fails on the default (20 ≠ 10) and the picker choices.

## Step 2 — Green on the simulator

```bash
cd apps/ios-wiki-demo && xcodebuild test -project XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo \
  -destination 'platform=iOS Simulator,id=XXXXX' -configuration Debug ARCHS=arm64 2>&1 | grep -E "Test Case.*(passed|failed)|TEST (SUCCEEDED|FAILED)"
```

Expected: `SettingsTests` green; every existing demo test green (they set depths explicitly
or use the default where the depth does not matter).

## Step 3 — The device run (owner; identifiers only on the command line)

```bash
scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki --demo
TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo-Measure \
  -configuration Release -destination 'platform=iOS,id=XXXXX' -skipMacroValidation -allowProvisioningUpdates \
  ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX -only-testing:XtrieverWikiDemoTests/DemoMeasurementTests 2>&1 | tee /tmp/018-demo-device.log
scripts/extract-device-run.py /tmp/018-demo-device.log specs/018-demo-rerank-depth-10/runs/
```

Expected: a record whose settings show depth 10; median fused ≈ 339 ms (within 20 %), median
re-ranked ≥ 30 % below 2,288 ms (≈ 1.4 s), median total ≤ 3 s, footprint PASS. **⛔** A
record failing 009's figures or SC-002 is stop-and-report.

## Step 4 — Gate (Rule 5)

```bash
git diff --stat main -- crates/ swift/ specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage specs/013-lexical-quality/baselines specs/014-rerank-depth-study/runs specs/015-rerank-interpolation/baselines specs/016-sparse-remeasure/runs specs/017-post-015-hygiene/runs   # empty
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo nextest run --workspace && cargo deny check   # unchanged
grep -rn "<device id>\|<team id>" --exclude-dir=target --exclude-dir=.git .   # nothing
```
