# Feature Specification: The Demo Re-ranks at Depth 10

**Feature Branch**: `018-demo-rerank-depth-10`

**Created**: 2026-09-16

**Status**: Draft

**Input**: User description: "The iOS Wikipedia demo re-ranks at depth 10 by default (014's F-003, 017's device number): the pipeline's default stays 20 — the quality-maximising setting the baselines are measured at — and the demo app, whose users wait for the answer on a phone, chooses depth 10: 014 measured 0.4881 vs 0.4913 mean nDCG@10 (−0.3 points) for half the cross-encoder calls, and 017 measured the phone at 1.41 s vs 2.31 s median per re-ranked query (default threads). Scope: the demo's Settings default (20 → 10) and its depth picker (0 / 5 / 10 / 20), the About/Settings copy that names the default and the trade-off with the numbers, the demo README, one device run of the demo's own measurement (009's DemoMeasurementTests, mmap, default threads, airplane mode) recorded beside 009's so the app-level latency at the new default is on record (009 SC-001: fused within 1 s, re-ranked within 3 s), and the 009 spec/report cross-referenced. No engine, pipeline, FFI, format or baseline change; the harness package tests unchanged."

## Why This Spec Reads Technically

The user is the person holding the phone. Two measurements make the decision: Feature 014
found that re-ranking ten candidates instead of twenty costs 0.3 mean nDCG@10 points
(0.4881 against 0.4913 across the three BEIR sets) for half the cross-encoder calls, and
Feature 017 found what half the calls is worth on the reference phone — a re-ranked answer
in 1.41 s instead of 2.31 s (median, default threads). The engine keeps depth 20 as its
default because that is the quality-maximising setting every baseline is measured at; an
app that shows a person a list decides its own trade-off, and for the demo the 0.9 s is
worth more than the 0.3 points. The change is an app setting and its explanation; the
evidence that it does what it says is one more device run of the demo's own measurement,
recorded beside Feature 009's.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A search answers faster at the default setting (Priority: P1)

The demo's default re-rank depth becomes 10; the depth picker offers 0 / 5 / 10 / 20; a
person who never opens Settings gets a re-ranked list in roughly 60 % of the previous time.

**Why this priority**: This is the feature.

**Independent Test**: A fresh install (no persisted settings) searches with depth 10; the
Settings picker shows 10 selected and offers 0, 5, 10, 20; picking 20 restores the previous
behaviour.

**Acceptance Scenarios**:

1. **Given** a fresh install, **When** a search runs, **Then** the stage report shows ten
   re-ranked candidates and the re-ranked list arrives correspondingly sooner.
2. **Given** Settings, **When** opened, **Then** the depth picker offers 0 / 5 / 10 / 20 with
   10 selected by default, and each choice is applied to the next search.
3. **Given** a person who prefers depth 20, **When** they select it, **Then** searches re-rank
   twenty candidates as before.

---

### User Story 2 - The trade-off is explained where the setting is (Priority: P1)

The Settings and About screens say what the default is and why, with the numbers: depth 10
halves the cross-encoder work for a 0.3-point loss in the benchmark mean and answers in
1.4 s instead of 2.3 s on the reference phone; the engine's own default is 20.

**Why this priority**: A demo whose purpose is to show the pipeline working must not hide a
setting that trades quality for time.

**Independent Test**: The Settings screen's depth control carries the explanation; About
distinguishes "engine default 20" from "app default 10".

**Acceptance Scenarios**:

1. **Given** Settings, **When** read, **Then** the depth control names the default, the
   benchmark cost (−0.3 mean nDCG@10) and the phone-side gain (1.4 vs 2.3 s), and the
   engine's default of 20.
2. **Given** About, **When** read, **Then** the engine's re-rank depth (20, from the index's
   information) and the app's default (10) are both shown, labelled.

---

### User Story 3 - The app-level latency at the new default is on record (Priority: P1)

One device run of the demo's own measurement (the 009 harness: 20 questions, airplane mode,
memory-mapped models, default threads) at the new default, committed beside the 009 record,
with the 009 acceptance figures re-checked: fused within 1 s, re-ranked within 3 s after
warm-up, footprint under the 600 MB ceiling.

**Why this priority**: The decision was made on the harness's per-depth number; the app's
end-to-end number is what the person sees.

**Independent Test**: A record under this feature's `runs/` with median re-ranked time
materially below 009's 2,288 ms, median total below 009's 2,631 ms, and the footprint verdict
PASS.

**Acceptance Scenarios**:

1. **Given** the reference phone, **When** the demo measurement runs at the new default,
   **Then** the record is committed and its medians are stated beside 009's.
2. **Given** the record, **When** compared with 009's, **Then** the fused median is unchanged
   within noise (the first stage does not change) and the re-ranked median falls by roughly
   the harness's 0.9 s.

---

### User Story 4 - The documents follow (Priority: P2)

The demo README, the 009 spec's default-settings assumption and the 009 report carry the
change with a pointer to the evidence.

**Independent Test**: The README names the default and the trade-off; the 009 documents
point here.

---

### Edge Cases

- Persisted settings from a previous install carry a depth of 20: the person keeps what they
  chose — only a fresh default changes.
- The index information still reports the engine's depth (20): About must not present it as
  the app's setting; the two are labelled.
- The 009 record's latency assumption ("re-rank depth 20 at the pipeline defaults") stays
  true of that record; the new record carries its own settings.
- The harness package's tests and the engine's baselines are untouched; a device parity run
  is not part of this feature (017 covered depth 10's parity).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The demo's default re-rank depth MUST be 10; the depth picker MUST offer 0 / 5 /
  10 / 20; persisted settings MUST be honoured as before.
- **FR-002**: The Settings depth control MUST explain the default with the numbers (−0.3 mean
  nDCG@10 on the BEIR sets; 1.4 vs 2.3 s median on the reference phone) and name the engine's
  default of 20; About MUST show the engine's depth and the app's default, labelled.
- **FR-003**: One demo-measurement device run at the new default MUST be committed under this
  feature's `runs/` in the 009 record shape, with its medians stated beside 009's in the
  report; the 009 acceptance figures MUST hold (fused ≤ 1 s, re-ranked ≤ 3 s after warm-up,
  footprint < 600 MB).
- **FR-004**: The demo README, the 009 spec (its default-settings assumption) and the 009
  report MUST reference this feature.
- **FR-005**: No engine, pipeline, FFI, format, harness-test or baseline change; the demo's
  existing tests that set a depth explicitly are unchanged, and any test asserting the
  default is updated to 10.
- **FR-006**: Tests first: the demo test asserting `Settings()`'s default depth and the
  picker's choices is committed failing.

### Key Entities

- **Demo settings**: re-rank depth (default 10; choices 0 / 5 / 10 / 20), time budget, strict.
- **Demo run record**: the 009 shape — device, build, settings, per-query fused / re-ranked /
  total wall times, medians and maxima, footprint, verdicts.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a fresh install the stage report shows 10 re-ranked candidates.
- **SC-002**: The committed device record's median re-ranked time is at least 30 % below 009's
  2,288 ms and its median fused time is within 20 % of 009's 339 ms; footprint PASS.
- **SC-003**: The 009 acceptance figures hold at the new default (fused ≤ 1 s, re-ranked ≤ 3 s).
- **SC-004**: Settings and About carry the explanation with the numbers; the README and the
  009 documents point here.
- **SC-005**: `git diff --stat main -- crates/ swift/ specs/*/baselines` is empty.

## Assumptions

- **The engine's default stays 20** (ADR-0012's configuration); the app's choice is an app
  constant, so no goldens, baselines or harness tests move.
- **Depth 10's parity on the device is already on record** (017); this feature records the
  app-level latency only.
- **The numbers quoted in the UI are the measured ones** (014 table; 017 records) and cite
  their features; they are not re-measured here beyond the demo run.
- **The device identifiers stay on the command line**.
