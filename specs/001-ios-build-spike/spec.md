# Feature Specification: iOS Build Spike — tantivy, tokenizers, candle on Device

**Feature Branch**: `001-ios-build-spike`

**Created**: 2026-09-10

**Status**: Draft

**Input**: User description: "Feature 001 — cross-platform build spike. Prove that tantivy (default features minus anything pulling C deps), tokenizers (default-features = false, fancy-regex path), and candle-core/candle-transformers compile for aarch64-apple-ios and aarch64-apple-ios-sim, are exposed through a uniffi Swift binding in sonar-ffi, and run on a physical iPhone: index 1,000 short documents, run one BM25 query, and embed one sentence with all-MiniLM-L6-v2. Record binary size, RSS and wall time. Success: all three work on device within the Constitution's default memory budget. Failure of any part is a documented finding, not something to work around silently."

## Why This Spec Reads Technically

This is a de-risking spike, not a user-facing feature. The "user" is an Xtriever developer, and the
subject matter *is* the crate/target matrix — Principle I's reuse mandate is only credible if the
reused crates actually build and run on the platforms Principle III requires. Naming `tantivy`,
`tokenizers`, `candle` and the iOS triples is therefore requirement content here, not leaked
implementation detail. Success criteria stay measurable and outcome-shaped; they do not prescribe
*how* the binding is written. Specific crate API items are deliberately **not** cited — under Agent
Operating Rule 1 those must be read from the pinned versions' docs and cited in `plan.md`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The three stacks cross-compile with no C/C++ dependencies (Priority: P1)

A developer runs the workspace build for `aarch64-apple-ios` and `aarch64-apple-ios-sim` and gets a
clean result, with a written record of the exact feature set each crate needed in order to avoid
pulling a C or C++ build dependency. If this fails, every later Xtriever milestone that assumes
on-device retrieval is built on sand, so nothing else in this spike matters until it passes.

**Why this priority**: It is the cheapest possible test of the project's central bet, it needs no
device or Apple developer account, and it gates Stories 2 and 3. A failure here is the single most
valuable finding this spike can produce, because it would land before any dependent code exists.

**Independent Test**: Fully testable on a macOS host with no iPhone and no provisioning profile, by
building the workspace for both iOS triples and inspecting the resolved dependency graph for
native-code build steps. Delivers value on its own: the recorded feature-set matrix is the input to
every future `Cargo.toml` in the project.

**Acceptance Scenarios**:

1. **Given** the pinned toolchain and both iOS targets installed, **When** the workspace is built
   for `aarch64-apple-ios`, **Then** it completes without error and without invoking a C or C++
   compiler for any dependency.
2. **Given** the same setup, **When** the workspace is built for `aarch64-apple-ios-sim`, **Then**
   it completes without error and without invoking a C or C++ compiler for any dependency.
3. **Given** the resolved dependency graph, **When** it is inspected for native-code build scripts
   and `-sys` crates, **Then** every one found is attributable to a leaf crate behind a
   non-default feature, and none is reachable from `xtriever-core`, `-analysis`, `-pipeline`,
   `-ltr` or `-eval`.
4. **Given** a candidate feature set that does pull a C/C++ dependency, **When** the minimal
   feature set that avoids it is found, **Then** both the working and the rejected feature sets are
   recorded with the reason.
5. **Given** a crate that cannot be made to build for an iOS triple under any C-free feature set,
   **When** that is established, **Then** Story 1 is reported as FAILED with an ADR, and the build
   is not made to pass by vendoring, patching, or forking the crate.

---

### User Story 2 - The three operations are callable from Swift (Priority: P2)

A developer builds a minimal Swift app against the generated binding and calls three operations —
index a document corpus, run one keyword query, embed one sentence — first in the simulator. The
binding surface is deliberately tiny: enough to prove the FFI boundary carries the data types these
operations need, not a design for the eventual public API.

**Why this priority**: Cross-compiling proves the code *builds*; it does not prove it *links* into
an app bundle or that the generated Swift surface can carry the arguments and results across. This
is where toolchain problems that a `cargo build` never sees (static library packaging, framework
layout, bridging of arrays and strings) actually surface — and it can still be done without a
physical device.

**Independent Test**: Testable entirely in the iOS simulator. Delivers value on its own by proving
the binding-generation and packaging path works, which is reusable regardless of what the on-device
numbers turn out to be.

**Acceptance Scenarios**:

1. **Given** the binding is generated and packaged for `aarch64-apple-ios-sim`, **When** a minimal
   Swift app links against it, **Then** the app compiles and launches in the simulator.
2. **Given** the running simulator app, **When** it calls the index operation with the fixture
   corpus, **Then** it returns success and reports the number of documents indexed.
3. **Given** an index built in the simulator, **When** the app calls the query operation with the
   fixture query, **Then** it returns a ranked list of document identifiers and scores.
4. **Given** the running simulator app, **When** it calls the embed operation with the fixture
   sentence, **Then** it returns a fixed-length numeric vector.
5. **Given** a Rust-side error in any of the three operations, **When** it crosses the binding,
   **Then** Swift receives a typed error rather than a process abort.

---

### User Story 3 - The numbers are measured on a physical iPhone (Priority: P3)

A developer installs the app on a real iPhone, runs all three operations against the committed
fixtures, and records installed binary size, peak memory footprint and wall time for each
operation — together with the device model, iOS version and build configuration that produced them.
The result is a committed report that a reviewer can read without owning the device.

**Why this priority**: It is the only story that answers the question the spike was commissioned to
answer, but it is last because it depends on both earlier stories and on hardware and provisioning
that the earlier stories do not need. The simulator does not share the device's memory limits,
thermal behaviour or CPU, so simulator numbers cannot substitute.

**Independent Test**: Testable by installing the Story 2 app on one physical iPhone and running the
three operations, given Stories 1 and 2 pass. Delivers the spike's actual deliverable: a baseline
measurement set and a verdict against the memory ceiling.

**Acceptance Scenarios**:

1. **Given** the app installed on a physical iPhone, **When** the 1,000-document corpus is indexed,
   **Then** indexing completes without termination and the wall time and peak memory footprint are
   recorded.
2. **Given** the on-device index, **When** the fixture query runs, **Then** the returned ranking is
   byte-identical to the host-produced golden ranking for the same corpus, query and configuration.
3. **Given** the app on device, **When** the fixture sentence is embedded, **Then** the returned
   vector matches the reference vector within the tolerance stated in this spec.
4. **Given** a completed device run, **When** peak memory footprint across the whole run is
   compared to the 300 MB ceiling, **Then** the comparison is recorded as a pass or a fail with the
   measured number, not a rounded or best-case number.
5. **Given** a completed device run, **When** the report is written, **Then** it states installed
   binary size broken down far enough to attribute it (code vs. model weights vs. resources).
6. **Given** the app is terminated by the operating system for memory pressure, **When** that
   happens, **Then** it is recorded as the measured outcome and reported as a FAILED memory verdict
   with an ADR — not retried at a smaller corpus size and reported as a pass.
7. **Given** the same commit and the same device, **When** the run is repeated, **Then** the
   recorded wall times agree within the stated reproducibility band.

---

### User Story 4 - Every failure survives as a finding (Priority: P1)

Whoever picks this work up next can read what did not work and why, without re-running the spike.
Each failed item has a written finding with the evidence that produced it and an ADR recording the
decision it forces.

**Why this priority**: P1 alongside Story 1 because it is what converts a negative result from a
wasted week into the spike's most useful output, and because it is the requirement most easily lost
under time pressure. It applies to whatever subset of Stories 1–3 completes.

**Independent Test**: Testable by inspecting the committed artifacts against the set of attempted
items: every attempted item has a recorded verdict, and every non-pass verdict has a finding and an
ADR. Testable even if all three earlier stories fail outright.

**Acceptance Scenarios**:

1. **Given** any acceptance scenario in Stories 1–3 that does not pass, **When** the spike is
   closed out, **Then** a finding exists naming the item, the observed behaviour, the evidence, and
   the smallest reproduction found.
2. **Given** a finding, **When** it constrains a future design decision, **Then** an ADR exists in
   `docs/adr/` recording that decision and referencing the finding.
3. **Given** a workaround was applied to make something pass, **When** the report is written,
   **Then** the workaround is disclosed as a deviation with its cost, and is not presented as an
   unqualified pass.
4. **Given** the spike is abandoned partway, **When** it is closed out, **Then** the items never
   attempted are listed as untested rather than left to look like passes.

---

### Edge Cases

- A crate builds for `aarch64-apple-ios-sim` but not `aarch64-apple-ios`, or vice versa. Both
  triples are separate verdicts; a pass on one is never reported as a pass on both.
- A feature set that is C-free on the host silently re-enables a native dependency on iOS through
  target-specific or transitive default features. The dependency-graph inspection is per-target, not
  host-only.
- Disabling `tokenizers` default features changes tokenization behaviour versus the regex path used
  to produce reference outputs, so embeddings diverge for reasons unrelated to iOS. Tokenization is
  compared against the reference before the embedding comparison, so the two failures cannot be
  confused.
- `candle` builds but the model cannot be loaded on device because the weight format or dtype path
  differs from host. Loading is a distinct recorded step from embedding.
- Memory-mapped index files behave differently inside the iOS application sandbox than on a
  desktop filesystem, or the index cannot be opened from the chosen sandbox directory at all.
- Peak memory occurs during model load rather than during indexing or embedding, so a steady-state
  reading would pass while the peak fails. Peak across the whole run is what is compared.
- The device is thermally throttled, low on battery, or under memory pressure from other apps, so
  wall times are not comparable between runs. Device state is recorded with each measurement.
- Wall-time measurement needs a clock that is unavailable in `wasm32` library code under Principle
  III, so timing must not be introduced into the pure crates to satisfy this spike.
- Binary size is ambiguous between the static library, the packaged framework, the app bundle, and
  the installed-on-device size. All are reported so the number cannot be quoted selectively.
- The corpus indexes but the query returns an empty ranking, e.g. because analysis dropped the query
  terms. An empty result is a failure of the query scenario, not a trivially passing one.
- Stories 1 and 2 pass and Story 3 cannot run because no device or provisioning is available. The
  memory verdict is then reported as untested, never inferred from simulator numbers.

## Requirements *(mandatory)*

### Functional Requirements

**Build matrix (Story 1)**

- **FR-001**: The spike MUST establish, for `tantivy`, `tokenizers`, `candle-core` and
  `candle-transformers`, whether each builds for `aarch64-apple-ios` and for
  `aarch64-apple-ios-sim`, recording a separate verdict per crate per target.
- **FR-002**: The spike MUST record, for each of those crates, the exact feature set used, and MUST
  record any feature that had to be disabled to avoid a C or C++ build dependency together with the
  dependency it pulled.
- **FR-003**: The spike MUST verify per-target that no C or C++ build dependency is reachable from
  `xtriever-core`, `-analysis`, `-pipeline`, `-ltr` or `-eval`, and that any such dependency it does
  introduce is confined to `xtriever-dense`, `-rerank` or `-ffi` behind a non-default feature.
- **FR-004**: All new dependencies MUST be added at their current published versions using the
  project's dependency tooling, with versions never written from memory, and the resolved versions
  MUST be recorded in the report.
- **FR-005**: The spike MUST NOT vendor, patch, or fork any of the four crates in order to make a
  build succeed. If a build cannot succeed without one, that is a FAILED verdict plus an ADR.

**Binding and callability (Story 2)**

- **FR-006**: `xtriever-ffi` MUST expose exactly three operations to Swift for this spike — index a
  corpus, run one keyword query, embed one sentence — and MUST NOT grow additional surface to make
  the spike more convenient.
- **FR-007**: The binding MUST carry, across the language boundary, a document corpus in, and a
  ranked list of identifiers with scores plus a fixed-length numeric vector out.
- **FR-008**: Errors originating in Rust MUST reach Swift as typed errors; no failure path in the
  three operations may abort the host process.
- **FR-009**: The spike MUST NOT modify any `xtriever-core` trait. If the binding cannot be built
  without such a change, the spike stops and reports rather than making the change.
- **FR-010**: All spike-only scaffolding — timing instrumentation, measurement hooks, the Swift
  harness app — MUST live in `xtriever-ffi`, the harness app, or the eval/CLI crates, and MUST NOT
  introduce timing, async, threading, or C/C++ dependencies into the pure crates.

**Fixtures and oracles (Stories 2 and 3, per Principle II)**

- **FR-011**: The spike MUST use a committed, deterministic fixture set: 1,000 short documents with
  stable identifiers, one query, and one sentence to embed.
- **FR-012**: The expected outputs MUST be generated by a committed script in `reference/` and
  committed as golden fixtures: the host-produced ranking for the query, the reference embedding
  vector for the sentence, and the reference token sequence for that sentence.
- **FR-013**: The acceptance tests asserting FR-011 and FR-012 MUST be written and committed in a
  failing state before the implementation that satisfies them.
- **FR-014**: The on-device ranking MUST be compared for exact equality against the host-produced
  golden ranking, with ties broken by ascending internal document identifier.
- **FR-015**: The on-device embedding MUST be compared against the reference vector within the
  tolerance stated in Assumptions, and the on-device token sequence compared for exact equality
  against the reference token sequence.
- **FR-016**: The embedding model MUST be pinned by content hash, and that hash recorded in the
  report, so a later run can prove it used the same weights.

**Measurement (Story 3)**

- **FR-017**: The spike MUST record, from a physical iPhone: peak memory footprint across the whole
  run; wall time for each of indexing, model load, and embedding, separately; and binary size.
- **FR-018**: Binary size MUST be reported broken down into at least code and model weights, and at
  the granularity of both the packaged artifact and the installed application.
- **FR-019**: Every measurement MUST be recorded with the device model, iOS version, build
  configuration, and observed device state (thermal and battery) at the time of the run.
- **FR-020**: Peak memory footprint MUST be compared against the 300 MB ceiling from Principle III,
  and the verdict recorded with the measured value.
- **FR-021**: The report MUST state explicitly that 1,000 documents is 1% of the 100,000-chunk
  reference configuration the 300 MB ceiling is written against, and MUST NOT claim the ceiling is
  satisfied at 100,000 chunks on the basis of this measurement.
- **FR-022**: The device run MUST be repeated at least twice on the same commit and device, with all
  runs recorded, so the reproducibility band is measured rather than assumed.
- **FR-023**: The spike MUST NOT set performance budgets. It establishes the baseline that later
  specs set budgets against, and the report MUST say so.

**Findings (Story 4)**

- **FR-024**: Every attempted item in FR-001 through FR-022 MUST end with a recorded verdict of
  pass, fail, or untested; no item may be left without one.
- **FR-025**: Every fail verdict MUST have a written finding stating the observed behaviour, the
  evidence, and the smallest reproduction found.
- **FR-026**: Every finding that constrains a future design decision MUST have a corresponding ADR
  in `docs/adr/`.
- **FR-027**: Any workaround applied to obtain a pass MUST be disclosed in the report as a
  deviation with its cost, and MUST NOT be reported as an unqualified pass.
- **FR-028**: The spike MUST NOT respond to a failure by weakening a fixture, a tolerance, the
  corpus size, or the memory ceiling.

**Scope boundaries**

- **FR-029**: The spike MUST NOT implement any Xtriever retrieval stage, pipeline, fusion,
  re-ranking or LTR behaviour beyond the minimum needed to execute the three operations.
- **FR-030**: Android and `wasm32` targets are out of scope for this spike and MUST be recorded as
  untested rather than inferred from the iOS result.
- **FR-031**: The three-operation binding surface is explicitly provisional. It MUST NOT be treated
  as the production `xtriever-ffi` contract, MUST NOT be depended upon by later specs, and its
  shape MUST NOT be defended in review as an API decision. The Swift harness app, by contrast, is a
  durable deliverable and MUST be committed in a state where a later spec can reuse it to take
  on-device measurements without rebuilding it.
- **FR-032**: The model weights MUST be bundled into the application, not fetched at run time, so
  that the FR-017 binary size is the end-to-end installed cost and no network path enters the
  wall-time measurements. The FR-018 code-versus-weights breakdown supplies the code-only figure.
- **FR-033**: The weights MUST be the as-published 32-bit float variant. The spike MUST NOT
  substitute a quantised or reduced-precision variant to fit the memory ceiling; if 32-bit floats
  do not fit, that is the finding, and evaluating a quantised variant is a separate spec.

### Key Entities

- **Fixture Corpus**: 1,000 short documents with stable identifiers, plus one query string and one
  sentence to embed. Committed, deterministic, and small enough to review.
- **Golden Fixtures**: The host-produced expected outputs — ranking for the query, reference
  embedding vector, reference token sequence — generated by a `reference/` script and committed
  alongside the tolerance that applies to each.
- **Feature-Set Matrix**: For each of the four crates and each of the two iOS triples, the feature
  set used, the features disabled, the native dependency each disabled feature pulled, and the
  verdict.
- **Measurement Record**: One device run — device model, iOS version, build configuration, device
  state, peak memory footprint, per-operation wall times, and binary size breakdown.
- **Pinned Model**: The `all-MiniLM-L6-v2` weights identified by content hash, with the dtype and
  the format used.
- **Finding**: One failed or degraded item — the item, observed behaviour, evidence, smallest
  reproduction, and the ADR it produced if any.
- **Spike Report**: The committed close-out artifact holding the feature-set matrix, all
  measurement records, every verdict, and every finding.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All four crates build for both iOS triples with zero C or C++ build dependencies
  reachable from the five pure crates — 8 of 8 crate/target verdicts pass.
- **SC-002**: A developer with a physical iPhone, this branch, and no prior context can index the
  corpus, run the query, and embed the sentence on device by following the committed instructions,
  without needing to ask the spike's author anything.
- **SC-003**: All three operations complete on a physical iPhone without the operating system
  terminating the app.
- **SC-004**: Peak memory footprint across the whole device run is at or under 300 MB, reported as
  the measured value.
- **SC-005**: The on-device ranking is identical to the host golden ranking, and the on-device
  embedding is within the stated tolerance of the reference vector — 0 unexplained mismatches.
- **SC-006**: Every one of binary size, peak memory footprint, and per-operation wall time is
  recorded with the device model, iOS version, build configuration and device state — 3 of 3
  metrics, no metric left as an estimate.
- **SC-007**: Repeated runs on the same commit and device reproduce wall times within the stated
  reproducibility band, and the embedding and ranking results exactly.
- **SC-008**: Every attempted item carries a pass, fail, or untested verdict — 0 items without one.
- **SC-009**: Every fail verdict has a finding with a reproduction, and every design-constraining
  finding has an ADR — 0 undocumented failures, 0 undisclosed workarounds.
- **SC-010**: A reviewer who does not own an iPhone can read the report and reach the same
  conclusions as the spike's author, with no measurement having to be taken on trust.
- **SC-011**: The spike leaves the pure crates free of timing, async, threading and native
  dependencies, and leaves `xtriever-core` traits and `deny.toml` unchanged unless an ADR says
  otherwise — verifiable from the diff.

**Interpretation of overall success**: SC-001, SC-003, SC-004 and SC-005 together constitute the
"all three work on device within the memory budget" outcome. If any of them fails, the spike is
still *complete* — and still valuable — when SC-008, SC-009 and SC-010 hold. The spike fails only
by producing an undocumented or silently worked-around result.

## Assumptions

- **`sonar-ffi` means `xtriever-ffi`.** No crate or directory named `sonar-ffi` exists, and the
  string "sonar" appears nowhere in the repository; Principles III and V name `xtriever-ffi` as the
  FFI crate. If `sonar-ffi` referred to a different project, this spec is misfiled and should be
  corrected before planning.
- **Scope is iOS only.** The description says "cross-platform" but names only the two iOS triples
  and a physical iPhone. Android and `wasm32` are recorded as untested (FR-030).
- **The memory ceiling is the Principle III default: 300 MB RSS**, and the metric compared against
  it is peak memory footprint across the whole run — the most conservative reading, and the one the
  operating system uses when deciding to terminate an app. Steady-state readings are recorded too
  but do not replace the peak.
- **Embedding tolerance**: cosine similarity ≥ 0.9999 against the reference vector, and maximum
  absolute element-wise difference ≤ 1e-3, for 32-bit floats. Chosen as a plausible starting band
  for a spike; the measured agreement should inform a tighter tolerance in later specs. This band
  is only defensible because FR-033 pins 32-bit weights — a quantised variant would need a much
  looser tolerance and would therefore be a weaker oracle.
- **The binding is provisional, the harness is not** (FR-031). Review effort goes to the harness and
  the report, not to the shape of the three operations.
- **Weights are bundled and 32-bit** (FR-032, FR-033). At roughly 90 MB, the weights are expected to
  account for a large share of the 300 MB ceiling, leaving on the order of 200 MB for the index,
  runtime and application. This is the unoptimised worst case on purpose: a pass here generalises
  down to any quantised variant, whereas a pass at reduced precision would not generalise up. The
  actual weight footprint is measured, not assumed.
- **Reproducibility band for wall time**: ±20% across runs on the same device and commit. Wide on
  purpose, because this spike is measuring an unknown; the observed spread should replace it.
- **No performance budget applies.** The description asks for numbers to be recorded, not met, so
  the spike establishes the baseline (FR-023).
- **"Short documents"** means roughly one to three sentences each; the exact corpus is whatever the
  committed fixture contains, and its size distribution is recorded in the report.
- **BM25 is checked by two separate oracles**, because one fixture cannot serve both. (a) *Parity*
  against an independent Python transcription of tantivy's documented BM25 — `k1 = 1.2`,
  `b = 0.75`, the Lucene IDF, and the `FIELD_NORMS_TABLE` u8 fieldnorm quantization — compared
  ids-and-order exact with scores within **`score_rel_tol = 1e-5`** (float64 in Python versus f32
  in tantivy makes bit equality unachievable and therefore a fake oracle). (b) *Determinism*,
  host-versus-device, compared bit-exact — the property Principle VI actually promises, and the one
  that matters on device.
- **One physical iPhone is sufficient** for this spike. The device model is recorded, and results
  are not generalised across device classes with different memory limits.
- **An Apple developer account and provisioning for the device are available.** Without them
  Story 3 cannot run and its verdict is untested.
- **`uniffi` is pure Rust** and lives in `xtriever-ffi`, a leaf crate, so it does not implicate
  Principle III's constraints on the pure crates. The spike verifies this rather than assuming it.
- **Timing lives outside the pure crates** (FR-010), so nothing in this spike introduces a clock
  into code that must compile for `wasm32`.
