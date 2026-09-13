# Feature Specification: The FFI Surface

**Feature Branch**: `007-ffi-surface`

**Created**: 2026-09-13

**Status**: Draft

**Input**: User description: "Feature 007: yes, read-only open of shipped index; let's leverage async here; let's put demo into `apps/ios-wiki-demo/`." — the first of three features toward the iOS Wikipedia demo: a Swift-callable surface over the whole pipeline (lexical → dense → fusion → re-rank) that opens a **shipped, read-only** hybrid index, loads both models from the app bundle, and searches **asynchronously** under a budget with explanation — packaged so the demo app (Feature 009, `apps/ios-wiki-demo/`) and the corpus feature (008) can build on it, and measured on a real device.

## Why This Spec Reads Technically

As in Features 001–006 the "user" is an Xtriever developer, here one writing Swift. The
deliverable is the boundary between the Rust engine and an iOS app: which operations cross it,
what they return, how errors and time budgets behave on the far side, and what it costs on a
phone. The constitution fixes much of the vocabulary — `xtriever-ffi` is the one crate allowed to
carry async wrappers, the on-device RSS ceiling, the determinism promise, the C/C++-free rule
— so the requirements name those things. No crate or binding-generator items are cited; they
belong in `plan.md` under Rule 1. Feature 001 already proved the three stacks build, link and
run on a device; this feature builds the real surface on that proof.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A Swift caller opens a shipped index and searches it (Priority: P1)

A developer bundles a hybrid index directory (built on a host by the pipeline) and the two model
directories with an app, opens the index **read-only** from Swift with the models, and runs a
search: they get hits with the document's external id, its passage text, its scores, and a
stage report saying what ran — the same hits, in the same order, the Rust pipeline returns for
the same directory and query.

**Why this priority**: Everything the demo does starts here; and "the same hits as Rust" is what
makes the boundary a boundary rather than a second implementation.

**Independent Test**: A Swift test opens a small fixture index (built on the host by the
pipeline's own test fixtures) with the real models and compares every hit's id, order and
scores with the Rust side's for the same queries.

**Acceptance Scenarios**:

1. **Given** an index directory and both model directories, **When** the caller opens them,
   **Then** a handle is returned that reports the index's document count, format version,
   embedder identity and re-ranker identity.
2. **Given** an open handle and a query, **When** the caller searches for `k` hits, **Then**
   the hits' external ids, order, fused scores and re-rank scores equal the Rust pipeline's for
   the same directory, query and options (ids and order exactly, scores bit-for-bit — both sides
   are the same code on the same architecture).
3. **Given** an open handle, **When** the caller searches without a re-ranker configured (no
   re-rank model directory given at open), **Then** the response is the fused ranking and the
   stage report says the re-rank stage did not run.
4. **Given** the handle, **When** the caller looks for a way to add, delete or commit
   documents, **Then** there is none: the surface is read-only, and a directory with an
   interrupted commit is refused at open like any other partial state.
5. **Given** a search with explanation requested, **When** hits come back, **Then** each carries
   its per-stage scores and ranks (lexical, dense, fused, re-rank — absent where a stage did not
   see it) under the names the core declares, so a UI can show why a hit is where it is.

---

### User Story 2 - Searches never block the caller and respect a time budget (Priority: P1)

A developer calls `search` from the app's main flow with `async`/`await`; the call returns
control immediately, the work runs off the calling thread, and the result arrives when done. A
time budget bounds the call: past it the re-ranker stops early (partial results kept, the
response says how many were re-scored) or the dense stage is skipped, exactly as the pipeline
defines; strict mode turns those into errors instead.

**Why this priority**: A re-ranked query costs seconds on a laptop (006 F-001) and more on a
phone; an app cannot freeze for that. The budget is what makes the stage usable on a device.

**Independent Test**: A Swift test starts a search that takes at least a second and proves a
main-thread ticker keeps firing while it runs; a second test sets a 200 ms budget with the
re-ranker attached and observes a partial re-rank (scored count between 0 and the depth) and a
response well under a second.

**Acceptance Scenarios**:

1. **Given** a search in flight, **When** the caller's thread is observed, **Then** it is not
   blocked (a timer on it keeps firing at its normal cadence).
2. **Given** a time budget shorter than a full re-rank, **When** searched, **Then** the response
   arrives with the scored candidates first and the rest in fused order, and the stage report
   states the scored count; no error.
3. **Given** a time budget already spent before the dense stage, **When** searched in the
   default mode, **Then** the lexical ranking comes back marked as degraded; in strict mode a
   budget error is returned.
4. **Given** two searches issued back to back on one handle, **When** both complete, **Then**
   each result is the one its query would have produced alone (searches are serialised per
   handle; the second waits for the first).
5. **Given** a search whose Swift task is cancelled, **When** the cancellation lands, **Then**
   the in-flight work still runs to its budget and its result is discarded; nothing leaks and
   the handle stays usable.

---

### User Story 3 - Every failure arrives as a typed error on the Swift side (Priority: P2)

A developer opens a missing directory, an index of another format version, an index built with
a different embedder, or a model directory with a tampered file, and gets a Swift error whose
kind and message say which — never a crash, never a silent empty result.

**Why this priority**: The demo will be run by people who did not build the index; the failure
messages are the only diagnostics they get.

**Independent Test**: Swift tests for each failure class assert the error kind and that the
message names the offending path or both values.

**Acceptance Scenarios**:

1. **Given** a path that is not a hybrid index, **When** opened, **Then** a "corrupt or
   incompatible index" error naming the path is thrown.
2. **Given** a format-version-1 directory, **When** opened, **Then** the error names both
   versions and says to rebuild.
3. **Given** an embedder that does not match the index, **When** opened, **Then** a
   fingerprint-mismatch error names both fingerprints.
4. **Given** a model file whose size or hash is wrong, **When** opened, **Then** a model error
   names the file and both values.
5. **Given** a search error in strict mode (dense or re-rank stage failed, budget spent), **When**
   it is thrown, **Then** its kind distinguishes a stage failure from a spent budget.

---

### User Story 4 - The surface ships as a Swift package built by one script (Priority: P2)

A developer runs one script on a Mac and gets a Swift package: a binary framework for device
and simulator, the generated bindings, and a small Swift layer providing the async API. The demo
app (Feature 009) adds it by path.

**Why this priority**: Feature 001 did this by hand for the spike and recorded every trap
(F-004–F-008); this makes it repeatable.

**Independent Test**: The script runs from a clean checkout on a Mac with the toolchain
targets installed and produces the package; the Swift tests build and run against it on the
simulator.

**Acceptance Scenarios**:

1. **Given** a Mac with the pinned toolchain and both iOS targets, **When** the script runs,
   **Then** the package exists with device and simulator slices and the tests pass on the
   simulator.
2. **Given** the continuous-integration workflow, **When** the feature lands, **Then** it still
   only builds the Rust library for the two iOS targets (as the 001 matrix does) — no simulator
   runs, no device runs, no model in CI (standing rule; limited runner resources).

---

### User Story 5 - The pipeline's on-device cost is measured, not assumed (Priority: P3)

A developer runs the measurement harness on a physical iPhone against a small real index
(SciFact: 5,183 documents, shipped with the harness) with both models memory-mapped and records:
open time, model load times, peak footprint, and per-query latency at re-rank depths 0, 5 and 20
— with the verdict against the constitution's ceiling (300 MB when this spec was written and
the runs were taken; 600 MB since v1.4.0 / ADR-0010, decided on this feature's F-002).

**Why this priority**: 006 F-001 measured 116–163 ms per pair on a laptop; the phone number
decides the demo's default depth and budget, and the 001 memory headline (mapped weights) has
never been measured with two models and a real index resident.

**Independent Test**: The run records are committed verbatim, as 001 did, so a reader without an
iPhone reaches the same verdict.

**Acceptance Scenarios**:

1. **Given** the harness on a device, **When** it opens the index with both models mapped and
   runs the measurement queries, **Then** peak footprint is recorded and compared with the
   constitution's ceiling.
2. **Given** the same run, **When** latency per query is recorded at depths 0, 5 and 20, **Then**
   the per-pair re-rank cost on the device is derived and recorded beside 006's laptop number.
3. **Given** the lexical hits on device, **When** compared with the host's for the same
   queries, **Then** ids, order and scores are bit-identical; dense and re-rank scores are within
   the goldens' tolerances (the 001 cross-architecture rule).

---

### Edge Cases

- Opening the same directory twice (two handles): allowed; each is independent and read-only.
- A query that is empty or whitespace: a valid search (the pipeline's behaviour), not an error.
- `k` of zero: an empty response, no stages run.
- The re-rank model directory given but the embedder's not: an error at open, not a partial handle.
- A budget of zero: nothing beyond the lexical stage runs (default mode) or a budget error (strict).
- The app is backgrounded mid-search: the work completes or is reclaimed by the system; no
  corruption is possible because nothing is ever written.
- Device and host disagree on a dense or re-rank score beyond tolerance: a finding (Rule 6),
  reported with the pair, never a widened tolerance.
- A handle dropped while a search is in flight: the search completes or is discarded; no crash.

## Requirements *(mandatory)*

### Functional Requirements

**The surface**

- **FR-001**: `xtriever-ffi` MUST expose, for Swift: open a hybrid index **read-only** from a
  directory path with an embedder model directory and an optional re-rank model directory and a
  load-path choice (buffered / mapped); search with a query, `k`, and options (candidate depth,
  re-rank depth, time budget, item budget, strict, explain); and read the handle's identity
  (document count, format version, embedder fingerprint, re-ranker identity or none).
- **FR-002**: The surface MUST NOT expose add, delete or commit; the directory is never written.
  Open MUST apply the pipeline's own refusals (format version, fingerprint, partial commit,
  torn store) unchanged.
- **FR-003**: A search's hits MUST equal the Rust pipeline's for the same directory, query and
  options — external ids and order exactly, fused and re-rank scores bit-for-bit on the same
  architecture — and MUST carry the passage text, chunk provenance where present, and, when
  requested, the per-stage explanation under the core's feature names.
- **FR-004**: The stage report MUST cross the boundary: which stages ran, what was skipped and
  why, the re-rank candidate and scored counts, and whether a time limit was ignored.

**Async and budgets**

- **FR-005**: `search` MUST be callable with `async`/`await` from Swift and MUST NOT block the
  calling thread; the work runs off it.
- **FR-006**: Searches on one handle MUST be serialised (one at a time, in call order); the
  handle MAY be used from any thread.
- **FR-007**: The time budget MUST be measured by the FFI layer from the moment the call starts
  and supplied to the pipeline as its elapsed-time source, so the pipeline's check points and
  the re-ranker's remaining-time budget behave exactly as Feature 005/006 define them.
- **FR-008**: Cancelling the Swift task MUST NOT abort the Rust work mid-pipeline; the work runs
  to its budget and its result is discarded; the handle remains usable.

**Errors**

- **FR-009**: Every engine error MUST arrive as a typed Swift error mirroring the core's error
  kinds (schema, invalid query, unknown field, dimension mismatch, not found, model, corrupt,
  fingerprint mismatch, budget exhausted, I/O, backend) with the engine's message; no panic
  crosses the boundary.

**Packaging**

- **FR-010**: One script MUST build the binary framework for device and simulator, generate the
  bindings, and assemble a Swift package with the async layer; the package MUST be usable by
  path from an app (Feature 009 at `apps/ios-wiki-demo/`).
- **FR-011**: The Rust side of the surface MUST stay C/C++-free in every feature set and MUST
  compile on the two iOS targets and Android as today.
- **FR-012**: Continuous integration MUST NOT gain simulator or device runs or model downloads;
  the existing build of the Rust library for the iOS targets is the CI coverage.

**Measurement**

- **FR-013**: The measurement harness MUST record, on a physical device, with a SciFact index
  built on the host and both models mapped: open time, each model's load time, peak footprint
  against the constitution's ceiling, and per-query latency at re-rank depths 0, 5 and 20 over a fixed
  query set, with run records committed verbatim.
- **FR-014**: The device's lexical hits MUST be bit-identical to the host's; dense and re-rank
  scores MUST be within the goldens' tolerances; a miss is a finding.

**Constraints and scope**

- **FR-015**: This feature MUST NOT modify `xtriever-core` traits, the stage crates, the
  pipeline's public API or on-disk format, or `deny.toml`. The pure crates stay free of async,
  threads and clocks; async and the clock live in `xtriever-ffi` and the Swift layer only.
- **FR-016**: This feature MUST NOT build the demo app, the Wikipedia corpus, an index built on
  device, or any write path — those are Features 008 and 009.
- **FR-017**: Acceptance tests MUST be committed failing before implementation; Rust-side tests
  MUST run offline where they need no model; Swift tests run on the simulator; device
  measurement is a manual, recorded step.
- **FR-018**: The Feature 001 spike code in `xtriever-ffi` (the `spike` feature, its three
  operations, its Rust tests and the Swift harness under `harness/ios/`) MUST be deleted once
  the real surface exists — one FFI crate, one surface — and its device-measurement harness
  (footprint via the task-info probe, the verdict gate, the run-record format) MUST be ported
  into this feature's measurement harness rather than rewritten. The 001 report and its
  committed run records remain the spike's evidence. *(User decision 2026-09-13, option A.)*

### Key Entities

- **Index Handle**: A read-only open hybrid index plus its models; identity (documents, format
  version, embedder fingerprint, re-ranker identity); owner of the serialised search queue.
- **Search Options**: `k`, candidate depth, re-rank depth, time budget, item budget, strict,
  explain — the pipeline's options, with the clock supplied by the FFI layer.
- **Hit**: external id, passage text, fused score, re-rank score (optional), chunk provenance
  (optional), explanation (optional: per-stage scores and ranks).
- **Stage Report**: lexical count, dense count or skipped-with-reason, re-rank candidates /
  scored / skipped-with-reason, time-limit-ignored.
- **Engine Error**: kind + message, one kind per core error variant.
- **Swift Package**: the binary framework (device + simulator), generated bindings, async layer,
  tests; built by one script.
- **Device Run Record**: device, OS, build settings, index and model identities, footprint,
  timings per depth, verdict — committed verbatim.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Over the fixture index and every fixture query, Swift hits equal Rust hits — ids
  and order 100 %, fused and re-rank scores bit-identical — with and without the re-ranker.
- **SC-002**: A search of ≥ 1 s leaves a main-thread timer firing within 10 % of its cadence;
  a 200 ms budget with the re-ranker attached returns in under 1 s with a scored count strictly
  between 0 and the depth on at least one query.
- **SC-003**: Every error class in Story 3 arrives as the mapped kind with the engine's message —
  6 / 6 cases; no test crashes the process.
- **SC-004**: One script produces the package on a clean Mac checkout; the Swift tests pass on
  the simulator; the CI workflow diff adds no job.
- **SC-005**: On a physical device with SciFact and both models mapped, peak footprint is
  recorded with the ceiling verdict; latency at depths 0 / 5 / 20 is recorded per query with the
  derived per-pair cost; run records are committed.
- **SC-006**: Device lexical hits are bit-identical to the host's on 100 % of the measurement
  queries; dense and re-rank scores within tolerance on 100 %.
- **SC-007**: `xtriever-core`, the stage crates, `xtriever-pipeline`, `deny.toml` are unchanged;
  the pure crates carry no async, thread or clock; the FFI crate's Rust is C/C++-free; the three
  mobile targets check.

## Assumptions

- **The async layer is Swift-side over synchronous Rust calls**, or Rust-side async exposed by
  the binding generator — the plan decides by reading the generator's pinned version. Either way
  the pure crates stay synchronous and the clock is read in `xtriever-ffi` or Swift.
- **Serialised searches per handle** (FR-006): the demo issues one search at a time; concurrent
  searches would run two model forward passes at once and double the transient footprint on a
  phone — a later feature may lift this with a measurement.
- **Cancellation is cooperative** (FR-008): the pipeline is synchronous and has no cancellation
  hook; the budget is the bound. Adding a cancellation token to the core is a core change and
  out of scope.
- **The Swift package lives at `swift/Xtriever/`** beside `crates/`; the demo app at
  `apps/ios-wiki-demo/` (Feature 009) consumes it by path. The 001 spike harness under
  `harness/ios/` is the model for the device-measurement harness.
- **Both models are loaded through the mapped path on device** (the 001 headline; ADR-0007/0009
  make it available for both); buffered is the fallback and is measured once for comparison.
- **The measurement index is SciFact** built on the host by the 005/006 harness (5,183
  documents; ~10 MB lexical + 8 MB dense + 8 MB text) — small enough to ship in the harness app,
  real enough for the latency number. The Wikipedia index is Feature 008's.
- **Depth 20 is the reference point** (006's baseline depth); depths 0 and 5 bracket what a
  phone can afford.
- **Minimum platform**: whatever Swift concurrency requires on the toolchain the 001 spike used;
  the plan records the exact versions.
- **No public API stability is promised yet**; Feature 009 will shape the surface further.
