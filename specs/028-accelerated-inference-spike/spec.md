# Feature Specification: Accelerated Inference Spike — Can the Apple GPU (or Apple's Matrix Library) Make the Embedder and the Re-ranker Faster?

**Feature Branch**: `028-accelerated-inference-spike`

**Created**: 2026-09-27

**Status**: Draft

**Input**: User description: "is it possible to make dense/re-ranking steps to be GPU accelerated?"
— answered in conversation (2026-09-27): candle, the pinned inference engine, offers a GPU
backend for Apple hardware (`metal`), Apple's CPU matrix library (`accelerate`) and NVIDIA GPUs
(`cuda`), and nothing for Android. The owner then asked for a spike (`/speckit-specify` on the
proposal "a measuring spike first"): measure what each Apple option is worth for the embedder
and the re-ranker, on the laptop and on the iPhone, and decide go / no-go on numbers.

## Why This Spec Reads Technically

This is a de-risking spike, like 001 and 012: the "user" is an Xtriever developer deciding
whether to spend a feature on a second compute path for the two models — a path every later
change to either model would have to keep working, measured and in parity. The spike answers
with the numbers the engine is already judged by: the phone's measured search (the Feature 009
harness, its 20 queries, its 600 MB ceiling), the host's goldens, and nDCG@10 on the three BEIR
datasets. Nothing it produces ships on by default: no format change, no core change, no change
to any default build. Naming the engine's backends, the devices and the models is therefore the
requirement, not leaked detail.

Today's reference numbers, which every variant is measured against:

| where | what | today (CPU) |
|---|---|---|
| iPhone 16e, Wikipedia (428k passages), re-rank depth 10 | fused list, median | 200.5 ms |
| same | re-ranked phase, median | 1,213 ms |
| same | peak `phys_footprint` | 335.4 MB (ceiling 600 MB) |
| MacBook Pro (M1 Pro), same queries | fused / re-ranked, median | 141.5 / 988.5 ms |
| same host | full Wikipedia build | 12.4 h |

(Records: `specs/026-eight-bit-precision/runs/`.)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Each compute path gets a latency on the phone and the laptop (Priority: P1)

The developer runs the same measured search three ways — today's CPU path, the CPU path through
Apple's matrix library, and the Apple GPU — on the iPhone and on the laptop, and gets for each:
the query embedding time, the re-ranked phase time (10 candidates), the whole search, and the
models' load time, as medians and maxima over the 20 measurement queries. The re-ranker is also
measured with its 10 pairs scored together as one batch, on each path where that is possible.

**Why this priority**: The re-ranker is 86% of a phone search today. Whether any path makes it
meaningfully faster is the decision.

**Independent Test**: The report's latency table has a cell for every path × device × stage,
each backed by a run record in `runs/`, or a stated reason why that path could not run there.

**Acceptance Scenarios**:

1. **Given** the Wikipedia index staged on the iPhone, **When** the measured search runs once
   per path, **Then** each run produces a record in the Feature 009 shape naming its compute path,
   and the report tabulates medians and maxima per stage against today's record.
2. **Given** a path that fails to build or to run on a device (for example, the GPU backend on
   iOS), **When** it is attempted, **Then** the report records the failure with its exact error
   and the path is marked "not available" for that device — the spike does not work around it by
   changing the engine's defaults.
3. **Given** the re-ranker in batched mode, **When** it scores the same 10 pairs, **Then** its
   time and its scores are recorded beside the one-pair-at-a-time run of the same path.

---

### User Story 2 - Every path's cost in memory, numbers and repeatability is known (Priority: P1)

For each path that runs, the developer learns: the peak memory on the phone against the 600 MB
ceiling; how far its embedding and re-rank scores move from today's host goldens; whether a
second run on the same device gives exactly the same results; and — for any path that would be
recommended — whether ranking quality holds on the three BEIR datasets.

**Why this priority**: A faster path that breaks the memory ceiling, changes the ranking or
gives different answers on a rerun cannot ship, however fast it is. Feature 026 (F-001) showed
that a precision choice is a parity choice.

**Independent Test**: The report's cost table has, per path and device: peak memory, the largest
score difference from the host goldens (embedding and re-rank separately), the count of hits
whose order differs from the host's, and a repeatability verdict.

**Acceptance Scenarios**:

1. **Given** a path on the iPhone, **When** its measured run completes, **Then** its peak
   memory is recorded by the harness's own ledger and judged against 600 MB.
2. **Given** a path, **When** its hits for the 20 queries at depths 0, 5, 10 and 20 are compared
   with the host's goldens, **Then** the report states the largest embedding-score and
   re-rank-score differences and the number of order differences.
3. **Given** a path, **When** the same queries run twice on the same device, **Then** the two runs
   are compared hit by hit and score by score, and the report states "identical" or the first
   difference.
4. **Given** a path the report recommends, **When** the three BEIR datasets are evaluated with it
   (the full pipeline, `hybrid-rerank-v3`), **Then** nDCG@10 and Recall@100 are reported against
   the committed CPU records.

---

### User Story 3 - Index building throughput on the laptop is measured (Priority: P2)

The developer learns whether an accelerated path makes building an index faster on the laptop:
passages embedded per second for SciFact's corpus (5,183 documents) on each path, and the
projected time of the full Wikipedia build (427,947 passages) against today's 12.4 hours.

**Why this priority**: The build is the other place the embedder's speed costs real time, and the
host has no memory ceiling — but a build path that embeds differently from the query path must
not leave the index and its queries in different spaces.

**Independent Test**: The report's build table has passages per second per path on the host, the
projection, and the measured quality of an index built on the accelerated path and searched on
the CPU path (the mixed case).

**Acceptance Scenarios**:

1. **Given** SciFact, **When** its corpus is embedded on each host path, **Then** the throughput
   and wall time are recorded, with the thread count.
2. **Given** an index built on an accelerated path, **When** it is searched on today's CPU path,
   **Then** its nDCG@10 on SciFact is reported against the all-CPU record.

---

### Edge Cases

- **The GPU backend does not build for iOS**, or builds but has no kernel for an operation one of
  the two models uses: recorded as the result for that device, with the error; not patched in
  the engine or in the backend.
- **The GPU is unavailable at run time** (another app holds it, the device is under thermal
  pressure, the simulator): the attempt's error is recorded; the spike states what a shipping
  path would have to do (fall back to the CPU, Principle VI) but does not build that fallback.
- **Thermal throttling on the phone**: each record carries the device's thermal state; runs taken
  at anything but "nominal" are repeated or reported as such.
- **Batching changes the arithmetic**: padding pairs to a common length can change scores even
  where the maths says it should not; the batched scores are compared with the unbatched ones,
  not assumed equal.
- **The first GPU call is slow** (shader compilation, buffer set-up): the warm-up is timed
  separately from the measured queries, as the harness already does.
- **Memory the GPU allocates is on the same chip as the CPU's**: on the phone it counts toward
  the footprint; the ledger reading is the measure, not an estimate.

## Requirements *(mandatory)*

### Functional Requirements

**Paths and devices**

- **FR-001**: The spike MUST measure three compute paths for the embedder and the re-ranker:
  today's CPU path (the reference), the CPU path through Apple's matrix library, and the Apple
  GPU — each through the pinned inference engine's own opt-in features, with the pinned
  engine version, the pinned eight-bit models and their current arithmetic (`compute=f32`).
- **FR-002**: Devices MUST be the iPhone 16e used for every phone record since Feature 008, and the
  2021 MacBook Pro (M1 Pro) used for the host records. Android, NVIDIA GPUs and the Apple Neural
  Engine are out of scope (see Assumptions).
- **FR-003**: The re-ranker MUST additionally be measured with the 10 pairs of each query scored
  as one batch, on every path that runs, beside the one-pair-at-a-time mode.

**Measurements**

- **FR-004**: Phone and host latency MUST be measured by the existing harnesses (the demo's
  measured run on the phone, the Python demo's `measure` on the host) over their 20 queries,
  each record naming its compute path and batching mode.
- **FR-005**: Each record MUST carry peak memory by the harness's ledger, the models' load times,
  the warm-up time and the device's thermal state.
- **FR-006**: For every path, score differences from the host's goldens (embedding and re-rank
  scores separately, as maximum absolute difference) and order differences at depths 0, 5, 10
  and 20 MUST be reported.
- **FR-007**: For every path, repeatability MUST be tested by running the same queries twice on
  the same device and comparing every hit and score.
- **FR-008**: For any path the report recommends, nDCG@10 and Recall@100 of `hybrid-rerank-v3`
  MUST be measured on SciFact, NFCorpus and FiQA with that path on the host, against the
  committed CPU records.
- **FR-009**: Host build throughput MUST be measured on SciFact per path (passages per second,
  wall time, threads), projected to the Wikipedia corpus, and the mixed case (built on the
  accelerated path, searched on the CPU path) scored on SciFact.

**Decision**

- **FR-010**: The report MUST apply this rule, fixed before any run. A path is **go** for a
  device only if all of these hold:
  1. it cuts the median re-ranked phase by at least 30% against today's record on that device,
     or, on the host only, raises build throughput at least 2×;
  2. its peak memory on the phone stays under 600 MB;
  3. two runs on the same device are identical;
  4. its scores stay within the device tolerance already in use (1e-3 of the host's goldens)
     and, if recommended, nDCG@10 moves by no more than 0.005 on any of the three datasets.

  Otherwise it is **no-go** with the numbers. The verdict names the path, the device, the
  batching mode, and what a follow-up feature would build.

**Containment**

- **FR-011**: No default build may change: every accelerated path is reachable only through
  non-default features or spike-only build flags. No on-disk format, no `xtriever-core` trait and
  no model artefact changes. The CPU path's records, goldens and tests are untouched.
- **FR-012**: The spike's code MUST be labelled as spike code, confined to the dense and re-rank
  crates, the FFI crate's manifest (forwarding the spike features, nothing else), the three
  existing measurement harnesses (the Swift package's device measurement, the iOS demo's
  measured run, the Python demo's `measure`), the iOS packager's spike flag and one host script,
  and either removed or promoted by the follow-up feature the verdict names. CI is untouched
  (standing rule: no models in CI). *(Amended at planning, 2026-09-27: the plan found the FFI
  manifest, the Swift package harness and the host script necessary.)*

### Key Entities

- **Compute path**: CPU (reference), CPU with Apple's matrix library, Apple GPU; with the
  engine's feature that selects it.
- **Batching mode**: one pair at a time (today) or all 10 pairs of a query at once, for the
  re-ranker.
- **Measured run**: a record of 20 queries on one device, one path and one batching mode —
  latency per stage, peak memory, load and warm-up times, thermal state.
- **Parity comparison**: a path's hits against the host's goldens — maximum score differences
  per model, order differences per depth.
- **Verdict**: go / no-go per path and device by FR-010, with the follow-up it proposes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every path × device cell of the latency table is filled from a committed run
  record, or carries the exact error that stopped it.
- **SC-002**: Every path that runs has its peak memory, score differences, order differences and
  repeatability verdict in the report.
- **SC-003**: The report states go / no-go per path and device by the rule of FR-010, in one
  paragraph a follow-up feature's spec can adopt verbatim.
- **SC-004**: After the spike, today's default builds, records, goldens and the whole local gate
  are unchanged and pass — the spike leaves no default behaviour different.
- **SC-005**: The host build throughput of each path on SciFact is reported with its projection
  to the Wikipedia corpus, and the mixed build-accelerated / search-CPU case has an nDCG@10.

## Assumptions

- **Scope of platforms** (from the conversation, 2026-09-27): Apple only. The pinned inference
  engine has no Android GPU backend; accelerating Android would need a second inference engine
  and different model files, which the owner would have to supply — a separate decision. NVIDIA
  GPUs are out of scope because neither measurement device has one; the engine's `cuda` feature
  is noted for a future build-server question. The Apple Neural Engine is excluded: ADR-0015
  reserved it for "its own decision, later", as a second model implementation outside the
  pinned engine.
- **The decision thresholds** in FR-010 (30% on the phone's re-ranked phase, 2× host build
  throughput) are this spec's defaults, chosen as the smallest gains that would plausibly pay for
  maintaining a second path; the owner may change them in `/speckit-clarify`, before any run.
- **Precision**: every path keeps today's float arithmetic. Half precision is not measured:
  Feature 026 (F-001) showed it breaks cross-platform parity.
- **Models and engine**: the pinned eight-bit artefacts and candle 0.9.2 as pinned; no model is
  converted or produced locally (standing rule), and no engine version bump is part of the spike.
- **Harnesses**: the Feature 009 demo measurement on the phone and the Feature 019 `measure` on
  the host, both of which already produce comparable records; the spike adds a compute-path
  label, not a new harness.
- **The owner's device**: the iPhone 16e is available and connected for the phone runs, as in
  Features 026–027; device and team identifiers stay out of every file (standing rule).
- **Not in scope**: building the fallback from GPU to CPU, any change to defaults, the Wikipedia
  corpus rebuild (its time is projected from SciFact), and any Android or Windows measurement.
