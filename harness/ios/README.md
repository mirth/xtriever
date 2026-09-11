# iOS harness — Feature 001

Drives the three provisional FFI operations on a simulator or a physical iPhone, and (in PR 3) takes
the measurements the spike exists to produce.

## Build

```sh
scripts/build-ios-harness.sh          # or --debug
```

Produces, all gitignored and reproducible from source:

| path | what |
|---|---|
| `XtrieverSpike/Frameworks/XtrieverFFI.xcframework` | device + simulator staticlib slices, ~262 MB |
| `XtrieverSpike/Sources/XtrieverSpike/Generated/xtriever_ffi.swift` | uniffi-generated bindings |

FR-031 makes the harness a durable deliverable. That means *reproducible*, not *committed*: a 262 MB
binary in git history is effectively permanent, and the bindings are generated from the Rust, so
committing them would create a second source of truth that can drift.

## Run

```sh
# Simulator (SwiftPM package is fine here — hostless tests run on simulators)
cd harness/ios/XtrieverSpike
xcodebuild test -scheme XtrieverSpike -configuration Release \
  -destination 'platform=iOS Simulator,name=<a simulator you have>' ARCHS=arm64

# Device — see DEVICE-RUN.md. Must use the app-hosted project.
```

with the fixture and model paths exported first — note the **`TEST_RUNNER_` prefix**, which is the
only way `xcodebuild test` forwards a variable to the test process (without it you are setting a
build setting and the test sees nothing):

```sh
export TEST_RUNNER_XTRIEVER_FIXTURES_DIR=$PWD/../../../reference/fixtures/001
export TEST_RUNNER_XTRIEVER_MODEL_DIR=$PWD/../../../reference/models/001
```

The fixtures and weights are passed by **environment variable rather than bundled**. In the Simulator
the host filesystem is reachable, so the tests read exactly the same bytes the Rust tests do instead
of a copy that could silently drift. `testEmbeddingCrossesAsFloatArray` skips when
`XTRIEVER_MODEL_DIR` is unset, because the 87.1 MiB weights are never committed.

A **device** run has no such luxury and must bundle both — that is PR 3's problem, and it is the
reason installed binary size (FR-018) is measured there rather than here.

## Why a test bundle and not an app with three buttons

Task T036 specifies "a Swift harness app with three buttons". This is a SwiftPM package with an
XCTest target instead, and the deviation is deliberate:

- **FR-022 requires at least two device runs**, and SC-002 requires someone else to reproduce them
  from committed instructions. `xcodebuild test` does that in one command; a button-driven app makes
  every run a manual ritual whose fidelity depends on tapping things in the right order.
- **XCTest runs on a physical device too** — but only with an **app host**. A hostless ("tool-hosted") bundle is simulator- and macOS-only, which is why `XtrieverSpikeApp/` exists: a minimal generated app whose sole job is to host these same tests on device. One copy of the tests, two destinations.
- Apple's own performance-testing guidance (research D11) is built around XCTest — Release
  configuration, "Debug executable" off, coverage and sanitizers disabled.

What is lost: nothing the spike needs. Nobody has to *watch* these operations run; the spike needs
their numbers recorded reproducibly. If a demo UI is wanted later, it can link the same
`XtrieverSpike` target.

## Status

**Simulator: 4/4 green** on iOS 26.5 (`xcodebuild test`). Indexing, querying and embedding all cross
the boundary, and Rust errors arrive as caught Swift errors rather than aborting the process.

**Device: measured.** `Measure.swift`, model staging, the app-hosted project
(`../XtrieverSpikeApp/`) and the run instructions (`../DEVICE-RUN.md`) all exist, and two runs on an
iPhone 16e are committed verbatim in `specs/001-ios-build-spike/runs/`. Peak footprint 238.1 MB
against a 300 MB ceiling — PASS.

To take a new measurement, follow `../DEVICE-RUN.md`. The harness refuses to report PASS unless every
prerequisite holds — Release build, not the Simulator, `RAYON_NUM_THREADS=1`, model bundled, and
valid `task_info` readings — and serializes `UNTESTED` otherwise.

## Gotchas already paid for

Both cost real time; both are recorded in `specs/001-ios-build-spike/report.md`.

- **The modulemap needs `--module-name xtriever_ffiFFI` and must NOT use `--xcframework`.** The
  generated Swift opens with `#if canImport(xtriever_ffiFFI)`, and `--xcframework` emits
  `framework module`, which a static-library slice is not. Get either wrong and you get dozens of
  "cannot find type 'RustBuffer' in scope" errors that point nowhere near the cause (F-006).
- **`xcodebuild test` does not pass arbitrary environment variables to the test process.** Only
  variables prefixed `TEST_RUNNER_` are forwarded, with the prefix stripped. Passing
  `XTRIEVER_MODEL_DIR=...` on the command line sets a *build setting* and the test sees nothing.
