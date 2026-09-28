# Quickstart: Running the Accelerated Inference Spike (Feature 028)

**Plan**: [plan.md](./plan.md) · **Surface**: [contracts/spike-surface.md](./contracts/spike-surface.md)

Device and team identifiers appear below as `XXXXX`; the real values go on the command line only,
never into a file.

## 0 — Prerequisites

The pinned eight-bit models fetched, the Wikipedia artefact at `target/xt-wiki/` with its
`expected.json`, the three BEIR datasets fetched, the Python demo's environment
(`apps/python-wiki-demo/.venv`), the iPhone 16e connected and unlocked, Xcode with XcodeGen.

## 1 — Red, then green

```bash
cargo nextest run -p xtriever-dense -p xtriever-rerank --features spike-metal        # label test: red until the features exist
cargo nextest run -p xtriever-rerank --features spike-batch --run-ignored all       # batched = unbatched within 1e-3: red
(cd apps/python-wiki-demo && .venv/bin/pytest -q -k digest)                          # the digest: red
```

After implementation all three pass, and so do the existing golden tests under each feature:

```bash
for f in spike-accelerate spike-metal; do
  cargo nextest run -p xtriever-dense -p xtriever-rerank --release --features "$f mmap" --run-ignored all -j 1
done
```

A golden test failing under a feature is a **result** (that path's scores leave the tolerance):
record it, do not loosen the test.

## 2 — The default build is untouched

The whole local gate (CLAUDE.md), on the default build — this is SC-004.

## 3 — Host runs (the laptop)

```bash
for p in cpu accelerate metal; do
  scripts/spike-028-host.sh $p measure
  scripts/spike-028-host.sh $p measure            # the second run: digests must match
  scripts/spike-028-host.sh $p --batch measure
done
for p in cpu accelerate metal; do scripts/spike-028-host.sh $p build; done
scripts/spike-028-host.sh cpu mixed
```

## 4 — Phone runs (the iPhone 16e)

For each path (`cpu`, `accelerate`, `metal`) and batching mode (off, on):

```bash
scripts/build-ios-package.sh --with-models --with-wiki --demo --app --spike-compute metal   # prints OTHER_LDFLAGS
# the app's measured run (latency, footprint):
TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj \
  -scheme XtrieverWikiDemo-Measure -configuration Release -destination 'platform=iOS,id=XXXXX' \
  -allowProvisioningUpdates ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX OTHER_LDFLAGS="<as printed>" \
  -only-testing:XtrieverWikiDemoTests/DemoMeasurementTests
# the package harness (depths, parity, digest), twice; the DefaultThreads scheme is the phone's
# own thread count, as the demo runs (the plain scheme pins one thread, 001 D15):
TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj \
  -scheme XtrieverHarnessApp-DefaultThreads -configuration Release -destination 'platform=iOS,id=XXXXX' \
  -allowProvisioningUpdates ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX OTHER_LDFLAGS="<as printed>" \
  -only-testing:XtrieverHarnessAppTests/DeviceMeasurementTests
python3 scripts/extract-device-run.py <log> specs/028-accelerated-inference-spike/runs/
```

Let the phone cool between runs; a record whose thermal state is not `nominal` is repeated or
reported as such. Grep `runs/` for the device and team identifiers before committing.

## 5 — Quality, for a recommended path only

```bash
scripts/spike-028-host.sh <path> quality        # SciFact, NFCorpus, FiQA; deltas against the CPU records
```

## 6 — The verdict

`report.md`: the latency, cost and build tables (data-model), the verdict per path and device by
FR-010, and the follow-up it proposes.
