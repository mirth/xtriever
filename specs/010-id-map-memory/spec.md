# Feature Specification: Shrink the Id Map's Resident Memory

**Feature Branch**: `010-id-map-memory`

**Created**: 2026-09-15

**Status**: Draft

**Input**: User description: "Feature 010: shrink the id map's resident memory (008 F-002). Today `HybridIndex::open` holds two full copies of the id map (`committed_ids` and `pending_ids`) — per passage an external id String, a ChunkInfo with its own parent String, and a reverse HashMap entry — roughly 100 MB each for the 427,947-passage Wikipedia index, plus the transient 32 MB `ids.json` parse; ~200 MB of the app's 533 MB footprint and the only term that grows with the corpus. Goal: the same index opens with substantially less resident memory and identical results, without touching ranking, the lexical/dense stages or the on-disk format unless decided otherwise."

## Why This Spec Reads Technically

The user is whoever opens an index — the demo app, the harness, the CLI, a future host — and
what they get is the same search for less memory. Nothing visible changes: no screen, no
result, no number on the About page but one. The feature exists because 008 found that the
corpus-sized part of the phone's footprint is not the vectors (mapped, not charged) nor the
models (fixed) but the *bookkeeping* that translates internal document numbers to external
ids and back: about 100 MB per copy for 428k passages, held twice, plus the parse that builds
it. It is the only term that grows with the corpus, and it is the difference between a demo
that passes the 600 MB ceiling with 64 MB to spare and an engine that could hold twice the
corpus. The requirements therefore speak of bytes per passage, copies, and identical results —
because those are the user-visible facts of this feature.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The same index opens with far less resident memory (Priority: P1)

A host opens the shipped Wikipedia index exactly as before — same directory, same files, same
call — and the process holds substantially less memory afterwards. On the reference device the
after-open footprint of the harness and of the demo app drops by the amount the success
criteria state; the 600 MB verdict gains the same headroom. The index on disk is untouched: no
rebuild, no migration, no new file.

**Why this priority**: This is the feature. The reduction must be real on the device, not on a
laptop, and it must come for free to every existing caller.

**Independent Test**: Open the 008 index through the existing harness on the reference device
before and after; compare the after-open footprint in the two run records.

**Acceptance Scenarios**:

1. **Given** the shipped Wikipedia index and the reference device, **When** the harness opens
   it in place with both models mapped, **Then** the recorded after-open footprint is lower
   than 008 run 2's 509 MB by at least the SC-001 amount, and the peak verdict is PASS with
   correspondingly more headroom.
2. **Given** the demo app, unchanged, **When** it prepares the index, **Then** its recorded
   after-open footprint drops by the same amount as the harness's (within measurement noise).
3. **Given** any index built before this feature (the 007 fixture, the SciFact indexes, the
   008 index), **When** opened, **Then** it opens without rebuild or migration, at the same
   format version, and reports the same `IndexInfo`.
4. **Given** the open itself, **When** its memory is watched, **Then** the transient cost of
   reading the id map (the parse) does not exceed the bounds SC-004 states — the peak during
   open is not a hidden second copy.

---

### User Story 2 - Nothing else changes: results, ids, provenance, errors (Priority: P1)

Every search returns the same hits with the same scores, the same external ids, the same chunk
provenance, in the same order. Every refusal the pipeline makes today (format version,
duplicate id, empty id, interrupted commit, count mismatch) is made with the same error class
and a message at least as informative. Every writer operation — assign, reuse, replace a chunk,
remove, never reuse a slot, commit, reopen — behaves exactly as today, including the rule that
a search sees the *committed* id map while changes are staged.

**Why this priority**: A memory optimisation that changes one bit of a result is a regression,
not a feature (constitution VI; Rule 6).

**Independent Test**: The existing golden and property suites pass without modification; the
BEIR baselines are re-run and identical per query; the 008 parity on device is bit-identical
where it was bit-identical.

**Acceptance Scenarios**:

1. **Given** the 007 fixture goldens and the 008 `expected.json`, **When** every query is run
   through the changed pipeline, **Then** lexical hits are bit-identical, fused order is
   identical, dense and re-rank scores are within the tolerances already recorded, and every
   hit's external id and chunk provenance are equal.
2. **Given** the three BEIR datasets and the committed baselines, **When** the evaluation is
   re-run locally, **Then** nDCG@10 and Recall@100 are identical to the baselines, per query.
3. **Given** an index with staged (uncommitted) additions, replacements and removals, **When**
   a search or a `contains` runs, **Then** it sees only the committed state; **When** the
   commit runs, **Then** the written id map equals what the pending state held and a reopen
   reads it back identically.
4. **Given** every existing refusal case (wrong format version, duplicate external id, empty
   external id, id space exhausted, interrupted commit, partial commit), **When** triggered,
   **Then** the same error class results and the message names the same cause.
5. **Given** the read-only open (008), **When** used on the changed pipeline, **Then** it
   behaves as before: a search works, a write is refused.

---

### User Story 3 - The reduction is measured, not asserted (Priority: P2)

The feature ships with its evidence: a host-side accounting of the id map's resident bytes for
the full Wikipedia index before and after, by one method; a device run record of the harness
on the same index after the change, in the format 008 used, beside 008's records; and the
per-passage cost stated as a number a future corpus can be multiplied by.

**Why this priority**: 008 stated the cost as an estimate ("roughly 100 MB"). This feature
turns the estimate into two measured numbers and makes the term's growth rate part of the
record, so the next corpus decision can be made on paper.

**Independent Test**: The report shows the before/after host accounting, the device record,
and the bytes-per-passage figure, all reproducible by the commands in the quickstart.

**Acceptance Scenarios**:

1. **Given** the full Wikipedia index on the host, **When** the accounting is run before and
   after the change by the same method, **Then** the report states both numbers, their
   difference, and the resulting bytes per passage.
2. **Given** the reference device, **When** the harness's device measurement is re-run on the
   Wikipedia corpus after the change, **Then** a run record is committed verbatim under the
   feature's `runs/`, and the report compares it line by line with 008 run 2.
3. **Given** a host-side test on a synthetic index of at least 100,000 passages, **When** run
   before the change, **Then** it fails on the per-passage bound; **When** run after, **Then**
   it passes — the bound is the number the plan derives from the compact shape (Q1 = B).

---

### Edge Cases

- An index whose id map has deleted slots (`null` entries): the slot stays reserved, is never
  reused, and costs no more than a live entry.
- Documents without chunk provenance (plain documents, no `ChunkInfo`): supported as today;
  their cost is lower, not higher.
- An empty index (zero passages): opens, searches return nothing, costs nothing to speak of.
- An external id that is a prefix of another ("12", "123"), ids containing `#`, `/`, spaces
  or non-ASCII, and very long ids: looked up exactly, never confused.
- Many chunks sharing one parent (the Wikipedia shape: 428k chunks over 239k articles): the
  parent is not the reason the map is large.
- A corpus twice the size of 008's: the id map's cost scales linearly at the stated bytes per
  passage, and the plan says where the next ceiling would be hit.
- Reopen after a commit: the just-written map reads back identically; a reopen while
  read-only still works.
- The writer path (ingest → commit) on the host may hold more memory than a reader while
  changes are staged; that is allowed and stated, not hidden.

## Requirements *(mandatory)*

### Functional Requirements

**Memory**

- **FR-001**: Opening an index MUST NOT hold two full copies of the id map. While no change is
  staged, the committed and pending views MUST share one representation; the first staged
  change MAY create a private pending copy, and the committed view MUST remain unchanged and
  observable until the commit.
- **FR-002**: The resident cost of the id map after open MUST be bounded per passage by a
  figure the plan states and a test enforces (SC-003): the bytes of the external ids and the
  parent ids themselves plus a fixed overhead per passage — not a per-passage allocation
  for every field.
- **FR-003**: Reading the id map at open MUST NOT hold, at any moment, more than the final
  representation plus the on-disk file's own size plus a stated constant — the transient is
  bounded (SC-004).
- **FR-004**: The on-disk id map format MUST NOT change: every existing index opens unchanged
  at format version 2 — no rebuild, no migration, no new file. The reduction comes from one
  shared copy held in a compact shape (ids and parents stored once, provenance as fixed-width
  entries, lookups by position), not from a new file. *(Owner decision 2026-09-15, Q1 = B; a
  memory-mapped id map with a format bump — option C — was declined as not paying for its
  ADR and rebuild today.)*

**Identity**

- **FR-005**: Search results MUST be identical to today's for every index: hits, scores, order,
  external ids, chunk provenance, explanations and stage reports — verified against the 007
  fixture goldens, the 008 `expected.json`, and the BEIR baselines on all three datasets
  (locally; CI keeps its SciFact smoke only).
- **FR-006**: All writer semantics MUST be preserved: assignment in ingestion order, reuse of
  an existing id on re-ingest, chunk replacement, removal that reserves the slot forever,
  refusal of empty and duplicate ids, the u32 space limit, and the commit that writes the
  pending map and makes it the committed one.
- **FR-007**: All open-time refusals MUST be preserved with the same error class (`Corrupt`,
  `Schema`, `Io`, `Backend` as today) and a message naming the same cause.
- **FR-008**: The public surface of `xtriever-pipeline` (`HybridIndex` methods, `IndexInfo`,
  `OpenOptions`, the passage store, `external_id`, `passage_text`) and the FFI wire MUST NOT
  change; the Swift package and the demo app MUST need no change to benefit.

**Latency**

- **FR-009**: Open time MUST NOT regress by more than 10 % on the host for the full Wikipedia
  index and on the reference device (008 run 2: 1,009 ms); id lookups in either direction
  MUST stay constant-time on average, so search latency medians stay within noise of 008's.

**Discipline**

- **FR-010**: No change to `xtriever-core` traits, to `deny.toml`, to `xtriever-lexical`,
  `xtriever-dense` or `xtriever-rerank`; the change is confined to `xtriever-pipeline` (and
  its tests) plus the measurement tooling and records. `ChunkInfo` keeps its public shape.
- **FR-011**: The tests that prove the bound (SC-003), the transient (SC-004) and the
  committed-vs-pending rule (US2 scenario 3) MUST be written first and committed failing
  where today's code fails them; no existing test is weakened.
- **FR-012**: The evidence (FR-013) MUST be committed with the feature: host accounting before
  and after by one method, the device run record in 008's format, the bytes-per-passage
  figure, and the BEIR deltas in the PR description (Rule 5).
- **FR-013**: The measurement method for host accounting MUST be stated in the plan and
  reproducible by one command; it MUST count what the process actually holds (heap bytes
  attributable to the open index), not an estimate from type sizes.

### Key Entities

- **Id map**: the two-way translation between an internal document number and an external
  id, plus each chunk's provenance (parent id, ordinal, byte range). Invariants: internal
  numbers are dense, assigned in ingestion order, never reused; a removed entry keeps its
  number; the committed map is what searches see, the pending map is what the next commit
  writes.
- **Committed view / pending view**: two logical states of the same map, physically one until
  a change is staged.
- **Per-passage cost**: bytes held per passage after open — the number this feature reduces
  and records.
- **Run record**: the device measurement (baseline, after open, peak, verdict, latencies), in
  the shape 008 committed.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the reference device, the harness's after-open footprint for the full
  Wikipedia index is lower than 008 run 2 (509.0 MB) by at least **150 MB** (i.e. at most 359 MB
  after open; owner decision Q1 = B) and the demo app's after-open footprint (009:
  531.9 MB) drops by the same amount within 10 MB. The 600 MB verdict is PASS.
- **SC-002**: 100 % of golden queries are identical after the change: 007 fixture goldens (all
  queries, score bits), 008 `expected.json` on device (lexical bit-identical 20 / 20, fused
  order 20 / 20, dense and re-rank within the recorded tolerances, ids and provenance equal),
  BEIR nDCG@10 and Recall@100 identical to the committed baselines per query on SciFact,
  NFCorpus and FiQA.
- **SC-003**: A host test on a synthetic index of ≥ 100,000 passages proves the per-passage
  bound the plan states for the compact shape, and fails on today's code; the same bound,
  multiplied by 427,947, predicts the Wikipedia measurement within 20 %.
- **SC-004**: The transient during open, measured on the host for the full index, is at most
  the after-open cost of the id map plus the file size (32,037,349 B) plus 16 MB.
- **SC-005**: Open time on the host for the full index and on the reference device is within
  +10 % of the pre-change figure; the 20-query latency medians on device (depth 0 / 5 / 20)
  are within ±10 % of 008 run 2.
- **SC-006**: The existing suites pass unmodified: workspace `nextest` (251 today, plus the
  new tests), the model-backed release suites, the package's simulator suite 18 / 18, the
  app's suite 15 / 15 — with no change to any test outside `xtriever-pipeline` and none to
  the Swift package or the app.

## Assumptions

- **Scope of the reduction**: the reader path is what matters (a phone opens, never writes);
  the writer path may keep a private pending copy once a change is staged, and the CLI build
  on the host is not memory-constrained.
- **Which records are re-taken**: a new device run of the harness on the Wikipedia corpus
  (and, if cheap, of the demo app's measurement) is committed under this feature's `runs/`;
  007, 008 and 009's records stay as recorded, as precedent (the 007 300 MB FAIL was kept).
- **The reference device** is the iPhone 16e used by 007–009, Release, models mapped, index
  opened in place, default threads.
- **Chunk parents**: in the Wikipedia shape the parent id is shared by all chunks of an
  article; a representation may exploit that, but correctness must not depend on it.
- **Measurement method**: a heap accounting around the open on the host (the plan names the
  method); the device number is the harness's existing footprint ledger.
- **Growth headroom** after this feature is stated in the report as "passages per 100 MB" so
  the next corpus decision needs no re-measurement.
