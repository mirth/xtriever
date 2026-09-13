# Xtriever — the Swift package

The iOS surface over the hybrid pipeline (Feature 007): a binary framework built from
`crates/xtriever-ffi`, the uniffi-generated bindings, and `XtrieverIndex`, a thin async layer.
The demo app (`apps/ios-wiki-demo/`, Feature 009) consumes it by path; the device harness
(`swift/XtrieverHarnessApp/`) hosts its tests on a phone.

## What is generated, and why it is not committed

| path | produced by | why not committed |
|---|---|---|
| `Frameworks/XtrieverFFI.xcframework` | `scripts/build-ios-package.sh` | ~280 MB of static libraries; a binary in git history is effectively permanent |
| `Sources/Xtriever/Generated/xtriever_ffi.swift` | uniffi-bindgen, from the Rust | a second source of truth that can drift from the crate |
| `Sources/Xtriever/XtrieverData/` | the script's `--with-*` flags | models (175 MB) and indexes pinned elsewhere by size and hash |
| `Tests/Fixtures/index/` | `--with-fixtures` | derived from `reference/fixtures/005/hybrid.json` and the models |

`Tests/Fixtures/expected.json` **is** committed: the parity goldens the FFI minted on the host
(`cargo run --release -p xtriever-ffi --example fixture_index -- swift/Xtriever/Tests/Fixtures`),
pinned against drift by `crates/xtriever-ffi/tests/parity.rs`.

## Build

```sh
scripts/check-toolchain.sh                                  # rustup toolchain, both iOS targets, Xcode SDK
scripts/build-ios-package.sh --with-models --with-fixtures  # staticlibs, bindings, XCFramework, staged resources
```

Both models must be fetched first (`scripts/fetch-model.sh`, and
`scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json`); the script verifies
them before staging.

## Run the tests on the simulator

```sh
cd swift/Xtriever
xcodebuild test -scheme Xtriever -configuration Release ARCHS=arm64 \
  -destination 'platform=iOS Simulator,id=<a simulator id from: xcrun simctl list devices available>' \
  -skip-testing:XtrieverTests/DeviceMeasurementTests
```

Release, because a Debug measurement is not a measurement and because `@testable` does not
survive Release — the tests use only the public API. `ARCHS=arm64`: the XCFramework carries no
x86_64 slice (001 F-008). Tests that need a resource that is not staged skip with the flag that
would stage it, never pass silently.

Passing a variable to the test process needs the **`TEST_RUNNER_`** prefix (without it you set a
build setting and the test sees nothing):

```sh
TEST_RUNNER_XTRIEVER_LOAD_PATH=buffered xcodebuild test …
```

## Run on a device

See `specs/007-ffi-surface/quickstart.md` Step 5: `scripts/build-ios-package.sh --with-models
--with-scifact --app`, open `swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj`, select the
phone, run `DeviceMeasurementTests` under the `XtrieverHarnessApp` scheme (`RAYON_NUM_THREADS=1`)
and the `-DefaultThreads` scheme. A device rejects hostless test bundles, which is why the app
exists (001 F-007). Each run writes its record to the console and as an `XCTAttachment`
(`device-run.json`); commit it under `specs/007-ffi-surface/runs/`.

## Using the API

```swift
import Xtriever

let index = try await XtrieverIndex.open(indexDir: indexURL, embedderDir: embedderURL,
                                         rerankerDir: rerankerURL, loadPath: .mmap)
let response = try await index.search("how many people live in berlin",
                                      options: SearchOptions(k: 10, rerankDepth: 20, maxTimeMs: 3000, explain: true))
for hit in response.hits { print(hit.externalId, hit.score, hit.rerankScore ?? .nan, hit.text) }
print(response.stages)      // what ran, what was skipped and why, how many were re-scored
```

`search` never blocks the caller; calls on one instance run one at a time; cancelling the task
drops the result when it arrives (the time budget is the bound). Every engine error is a
`XtrieverError` case with the engine's message (`error.message`).
