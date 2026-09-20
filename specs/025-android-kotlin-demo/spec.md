# Feature Specification: The Android Kotlin Wikipedia Demo

**Feature Branch**: `025-android-kotlin-demo`

**Created**: 2026-09-20

**Status**: Draft

**Input**: User description: "Bindings for the Android platform and a Kotlin demo app: a reusable Android library module that carries the engine's Kotlin bindings and its native library, and an Android demo application at `apps/android-wiki-demo/` that searches Wikipedia on the phone, offline, through that module — the same demonstration the iOS app (009) and the Python command line (019) give: the fused (lexical + dense) list first, then the re-ranked order with what moved per hit, each hit's title, passage and article link, each hit's eight explained features under the engine's names, the engine's stage report, settings (re-rank depth 0/5/10/20, 10 as the app default per 018), and an About with the corpus identity, snapshot and counts, the engine's index information and the attribution verbatim. The app owns no retrieval logic and makes no network call for retrieval. **Owner's decisions (2026-09-20)**: the feature delivers *both* a reusable library module and the demo app (two pull requests, the module first); the app bundles the 2,000-article Wikipedia slice (24 MB, 8,529 passages) as its corpus; both pinned models (174 MB) are bundled in the application package and copied into application storage on first run. Established by a link attempt on 2026-09-20, before this spec: the workspace builds and links for `aarch64-linux-android` with the native toolkit release 28, but only with half-precision floating point enabled, because the inference engine's half-precision matrix kernel uses inline assembly the base 64-bit ARM profile rejects; the release profile's symbol stripping removes the binding metadata, exactly as it does for the Python wheel, so the module needs a profile that keeps symbols; and the workspace's existing binding generator already emits Kotlin (4,249 lines) from the Android library with no code change."

**Clarification (Q1, owner, 2026-09-20)**: the half-precision requirement is **accepted**.
Processors without it are refused with a named error at startup rather than supported through
a second build; the exclusion is recorded.

**Clarification (Q2, owner, 2026-09-20)**: the measured run is taken on an **emulator for
now**. The record says so, its latency and footprint figures describe a virtual device on the
host machine, and no comparison with the iPhone's record is claimed. A run on a physical
Android device is left to a later feature.

## Why This Spec Reads Technically

Feature 007 put the engine behind a foreign-function surface and Feature 009 proved it on an
iPhone; Feature 011 proved the same surface from Python and Feature 019 built the command
line around it. Android is the last platform in the constitution's portability rule
(`aarch64-linux-android` is checked on every push) that has never been *run* — the check
type-checks the code, it does not link it, load it or ask it a question. This feature closes
that gap twice over: a library module another Android project can depend on, and a demo that
shows the pipeline working on a phone exactly as the iOS demo does.

The claim is measured, not asserted. The engine's results on Android must equal the host's
bit for bit for the same index and query, checked against the same 40-document fixture
goldens the Swift package is checked against, and the demo's latency and footprint are
recorded in the same shape as 009's and 019's records rather than described in prose — on an
emulator for this feature, which the record states and which bounds what those figures mean.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The engine answers a query on Android through Kotlin (Priority: P1)

An Android project adds the library module, points the engine at an index directory and two
model directories in application storage, and asks a question. It gets back the same hits,
in the same order, with the same score bits the same index gives on the host.

**Why this priority**: This is the platform claim, and it is the whole of the first pull
request. Without it there is no demo to build; with it alone, an Android developer can
already embed the engine.

**Independent Test**: An instrumented test on an emulator opens the 40-document fixture
index staged by the packaging script, runs the eight golden queries at every re-rank depth and
compares the result with `swift/Xtriever/Tests/Fixtures/expected.json` — the goldens the Swift
package already uses — by the FR-004 rule: identifiers, order, lexical bits and fused bits
exact, model scores within 1e-3.

**Acceptance Scenarios**:

1. **Given** the module and the fixture index on a supported device, **When** the eight
   golden queries run at depths 0 / 5 / 10 / 20, **Then** the hit identifiers and their order
   equal the goldens, every lexical and fused score bit equals the goldens, and each dense and
   re-rank score is within 1e-3 of the goldens (FR-004).
2. **Given** the module, **When** a caller opens an index whose dense stage is format
   version 1, **Then** the engine's own error reaches Kotlin as a typed, catchable failure
   naming the rebuild, not a crash.
3. **Given** an unsupported processor (no half-precision floating point), **When** the module
   loads, **Then** the failure is named and catchable before any engine call is made.
4. **Given** a clean checkout, **When** the packaging script runs once, **Then** it produces
   the bindings, the native library and the module without any generated file being read
   from version control.

---

### User Story 2 - A person searches Wikipedia on the phone and watches the pipeline (Priority: P2)

Someone installs the demo, waits once while it prepares itself, types a question and sees
the fused list appear first, then the re-ranked order with a mark per hit saying what moved.
Each hit shows the article title, the passage and a link to the article; opening a hit shows
the eight explained features under the engine's names. The device is offline throughout.

**Why this priority**: This is the demonstration the feature is named for, and it is the
second pull request. It depends on the first and delivers nothing new about the engine, but
it is what a person can be shown.

**Independent Test**: With the application installed on a device in airplane mode, a question
returns passages with titles and links within the recorded latency, and the two lists appear
in that order with marks between them.

**Acceptance Scenarios**:

1. **Given** the prepared application offline, **When** a question is submitted, **Then** the
   fused list appears first, then the re-ranked order, each hit carrying a mark of moved up,
   moved down, new or unchanged, with the fused hits that left the head named.
2. **Given** a result list, **When** a hit is opened, **Then** its eight explained features
   appear under the engine's names, with "not seen by this stage" where a stage did not
   retrieve it.
3. **Given** any search, **When** it completes, **Then** the stage report shows candidate
   counts, degradation and its reason, the re-rank counts, the time-limit flag and the
   elapsed milliseconds the engine reported.
4. **Given** an empty query or one of only stop-words, **When** it is submitted, **Then** the
   application says there are no results and stays usable; this is never an error.
5. **Given** a second question submitted while the first is still running, **When** it
   arrives, **Then** the first is cancelled and only the second's results are shown.

---

### User Story 3 - The application prepares itself once and explains what it is (Priority: P3)

On first launch the application copies the bundled models and index into its own storage,
showing progress, and is searchable when that finishes. An About screen names the corpus,
its identity and counts, what the engine reports about the index, this session's timings,
and the attribution text with its licence link. Settings offers the re-rank depth and
remembers the choice.

**Why this priority**: Without preparation nothing runs at all, but it is one screen and a
file copy; without About the demo still demonstrates retrieval, though it stops being an
honest one — the corpus licence requires the attribution.

**Independent Test**: A fresh install reaches a searchable state with progress shown; About's
fields equal what the engine and the corpus sidecar report; a depth chosen in Settings
survives a restart.

**Acceptance Scenarios**:

1. **Given** a fresh install, **When** the application first starts, **Then** it extracts the
   bundled resources with visible progress and becomes searchable without further action.
2. **Given** an extraction interrupted by a kill or a reboot, **When** the application starts
   again, **Then** it completes the preparation rather than opening a partial index.
3. **Given** a device without room for the resources, **When** preparation starts, **Then**
   the application names the space it needs and stops, rather than failing midway.
4. **Given** the prepared application, **When** About is opened, **Then** it shows the corpus
   identity, snapshot and counts, the engine's document count, format version, model
   fingerprints, candidate depth, fusion constant, recorded re-rank mode and recorded dense
   compaction share, the session's open and load times, and the attribution verbatim with the
   licence link.
5. **Given** a re-rank depth chosen in Settings, **When** the application is restarted,
   **Then** the choice is still in force.

---

### User Story 4 - The platform claim is recorded, not asserted (Priority: P3)

A run over the measurement queries on the demo's own corpus is recorded in the shape the
iPhone's and the laptop's records use: per-depth latencies, the footprint, what it ran on and
the thread count, with the parity verdict. For this feature that is an emulator, and the
record says so.

**Why this priority**: The project's rule is that a performance or parity claim is a record
under `runs/`, not a sentence in a report. It comes last because it needs the finished app.

**Independent Test**: The recorded run exists, names what it ran on and the thread count, and
its parity verdict is a pass computed by the same rule the iOS and Python records use.

**Acceptance Scenarios**:

1. **Given** the finished application, **When** the measurement queries run, **Then** a record
   is written with per-depth medians, peak memory, the emulated device and its image, the
   operating-system version, the host machine and the thread count.
2. **Given** that record, **When** it is compared with the host's answers for the same index
   and queries, **Then** every hit and score bit is identical.
3. **Given** the record, **When** it is read, **Then** it states that the figures are from an
   emulator and claims no physical-device latency or footprint.

---

### Edge Cases

- **A processor without half-precision floating point.** The native library cannot run there
  at all. The application must say so plainly at startup instead of dying on an illegal
  instruction, and the module's documentation must state the requirement.
- **An emulator image for Intel processors.** Unsupported for the same reason; the failure
  must be named rather than mysterious.
- **Too little free storage** for the bundled resources, or storage exhausted midway.
- **The operating system kills the application** during a search or during preparation.
- **An index directory left half-extracted** by an earlier interrupted run.
- **A format-version-1 index** pushed to the device by hand from an old build.
- **A device with fewer cores** than the laptop: the thread count must be recorded, never
  assumed.
- **Airplane mode throughout**: nothing in the retrieval path may need the network.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The library module MUST expose the engine's existing foreign-function surface
  to Kotlin — index open and close, search with its options, hits with their explanations,
  the stage report and the index information — with no capability that the Swift surface
  does not already have.
- **FR-002**: The module MUST carry a native library for 64-bit ARM. On a device whose
  processor cannot run it, loading MUST fail with a named, catchable error before any engine
  call, and the requirement MUST be stated in the module's documentation.
- **FR-003**: Bindings, native library and staged resources MUST be produced from source by a
  single documented command; nothing generated may be committed, as the Swift package's
  bindings are not.
- **FR-004**: The engine's answers on Android MUST match the host's for the same index, query
  and configuration by the project's cross-device parity rule (Features 009 and 019): hit
  identifiers and their order identical, lexical score bits exact, fused score bits identical,
  and the model-computed scores — dense and re-rank — within 1e-3 per document. **Owner's
  decision (2026-09-20)**, on measured evidence from the emulator: over 80 hits per mode the
  identifiers, the order, every BM25 bit and every fused bit were identical, while 67 of 80
  dense scores differed by at most 1.3e-7 and 32 of 80 re-rank scores by at most 3.3e-6. Those
  two stages run through the inference engine's matrix kernels, which are compiled for this
  target with half precision enabled and round their reductions differently from the host, so
  bit-identity is not available there. The ranking — what a person sees — is bit-identical, and
  that is what this requirement pins.
- **FR-005**: The demo MUST search its bundled corpus with no network call in the retrieval
  path, and MUST work with the device offline.
- **FR-006**: The demo MUST show the fused list first and the re-ranked order second, with a
  per-hit mark of moved up, moved down, new or unchanged, and MUST name the fused hits that
  left the head.
- **FR-007**: Each hit MUST show the article title, the passage and the article link, derived
  from the hit text by the Feature 008 convention, and on demand the eight explained features
  under the engine's names with "not seen by this stage" where a stage did not retrieve it.
- **FR-008**: Every search MUST show the engine's stage report: candidate counts per stage,
  degradation and its reason, re-rank candidates, scored and skipped, the time-limit flag and
  the elapsed milliseconds.
- **FR-009**: About MUST show the corpus identity, snapshot and counts from the index's own
  sidecar; the engine's document count, format version, model fingerprints, candidate depth,
  fusion constant, recorded re-rank mode and recorded dense compaction share; this session's
  open and model-load times; and the attribution text verbatim with its licence link.
- **FR-010**: Settings MUST offer re-rank depths 0, 5, 10 and 20 with 10 as the application's
  default, and the choice MUST survive a restart.
- **FR-011**: First launch MUST prepare the bundled resources into application storage with
  visible progress; an interrupted preparation MUST be completed on the next launch rather
  than leaving a partial index openable; insufficient storage MUST be reported with the
  amount required, before anything is written.
- **FR-012**: The application MUST own no retrieval logic. Every number it displays MUST be
  the engine's own or a wall clock around a single engine call.
- **FR-013**: An automated test running on an Android device or emulator MUST open the
  40-document fixture index and reproduce its committed goldens bit for bit at every re-rank
  depth.
- **FR-014**: The feature MUST NOT change the engine's ranking behaviour: no change to any
  stage crate's logic, to the on-disk formats, or to any committed baseline. Build
  configuration needed to produce an Android library is not a ranking change.
- **FR-015**: Continuous integration MUST NOT gain a job that downloads the native toolkit,
  runs an emulator, or loads a model; the existing Android compile check stays as it is, and
  everything else is local, as the iPhone's tests already are.
- **FR-016**: The hardware floor MUST be recorded in an architecture decision record: which
  processors are excluded, why the dependency requires it, and what was rejected. **Owner's
  decision (2026-09-20)**: requiring half-precision floating point is accepted. Devices
  without it are refused with the named error of FR-002; no second build and no runtime
  fallback is provided, and the exclusion — 64-bit processors older than roughly 2018, such
  as the Cortex-A53 generation — MUST appear in the module's documentation as well as the
  record.
- **FR-017**: The measured record MUST name what it ran on. **Owner's decision (2026-09-20)**:
  this feature records an emulator run. The record MUST state that, MUST name the emulated
  device, its system image and the host machine, and MUST NOT present its latency or memory
  figures as a physical-device result or compare them with the iPhone's record. A
  physical-device run is explicitly out of scope here.

### Key Entities

- **The library module**: the reusable Android artefact — generated Kotlin bindings, the
  native library for 64-bit ARM, and a thin wrapper mirroring the Swift package's.
- **The demo application**: screens for search, hit detail, stage report, About and Settings;
  no retrieval logic of its own.
- **The bundled corpus**: the 2,000-article Wikipedia slice — 8,529 passages, 24 MB — with
  its corpus sidecar and attribution file, the same shape as the shipped artefact.
- **The bundled models**: the pinned embedder and cross-encoder, 87 MB each, identified by
  the fingerprints the engine reports.
- **The fixture index and its goldens**: the 40-document index and the eight golden queries
  the Swift package is already checked against.
- **The device record**: per-depth latencies, footprint, device, operating-system version,
  thread count and parity verdict, stored beside the iPhone's and the laptop's.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a supported device, every one of the eight fixture queries returns the
  goldens' hits in the goldens' order at all four re-rank depths, with every lexical and fused
  score bit identical and every model-computed score within 1e-3 (FR-004). Measured maxima are
  recorded, so a drift larger than the one established on 2026-09-20 is visible rather than
  absorbed.
- **SC-002**: A person with the application installed and the network disabled receives
  passages for a typed question, with the fused list visible before the re-ranked one.
- **SC-003**: The measured run over the twenty measurement queries reports a parity pass
  against the host's answers for the same corpus, by the same rule as SC-001 — the rule the
  iPhone's own records are judged by.
- **SC-004**: Peak memory during a search over the bundled corpus is recorded. The project's
  600 MB phone ceiling is reported beside it for comparison only; this feature measures an
  emulator and claims nothing about a physical device.
- **SC-005**: A first launch reaches a searchable state within 60 seconds on the emulator used
  for the record, and later launches are searchable within 5 seconds.
- **SC-006**: From a clean checkout, one documented command produces the module and one more
  installs a working application; both are reproducible on another machine with the same
  prerequisites.
- **SC-007**: No committed evaluation baseline changes, and the ranking crates show no
  difference against the previous release.

## Assumptions

- **Only 64-bit ARM is supported.** It covers physical devices and the emulator images that
  run on the owner's machine. A 32-bit or Intel variant is out of scope.
- **Half-precision floating point is required** by the inference engine's matrix kernel, as
  established by the link attempt that preceded this spec, and that requirement is accepted
  (FR-016). Devices without it are refused, not worked around.
- **The emulator can run the build.** The virtual device prepared for this feature reports
  both half-precision processor features the library requires, verified on 2026-09-20, so the
  hardware floor of FR-016 does not stand in the way of the emulator record of FR-017.
- **The recorded run is an emulator run** (FR-017). Correctness — the ranking, bit for bit, and
  the model scores within 1e-3 — is a real claim; latency and footprint describe a virtual
  device on the host machine.
- **The application is installed over a cable, not from a store.** Bundling 174 MB of model
  weights exceeds what a store listing would allow, and the demo has no store audience.
- **The bundled corpus is the 2,000-article slice**, rebuilt on the current dense format. The
  full 1 GB artefact can be pushed by hand for a comparison run; it is not bundled.
- **The minimum supported operating-system version is Android 8.0**, which every device with
  the required processor exceeds.
- **The demo does not build an index on the phone.** Building is what the Python demo
  demonstrates; this one searches, as the iOS demo does.
- **The module is consumed by path within this repository**, not published to a package
  registry.
- **No device identifier belongs in the repository.** Commands that need one take it on the
  command line, as the iOS demo's already do.
- **Tests that need a device or an emulator run locally**, under the project's standing rule
  about continuous-integration resources.
