# Feature Specification: Re-rank Depth Study

**Feature Branch**: `014-rerank-depth-study`

**Created**: 2026-09-16

**Status**: Draft (clarified 2026-09-16: Q1 = C both interpolation forms; Q2 = B the +0.005 / −0.005 rule)

**Input**: User description: "Re-rank stage study (013 F-001 / ADR-0011): with the v2 fused lists the three-set mean of hybrid-rerank-v2 (0.4768) is below hybrid-baseline-v2 (0.4790) — the cross-encoder is net-negative on the benchmark (+0.7 NFCorpus, +0.5 FiQA, −1.9 SciFact). Measure, runs only, no model change: a re-rank depth sweep (5 / 10 / 20 / 50) over the v2 hybrid list on SciFact / NFCorpus / FiQA, and a score-interpolation variant (fused rank signal combined with the cross-encoder score instead of replacing the order), each verified by the 003 reference scorer; decide whether depth 20 stays the pipeline default before 014 builds on it, and record the numbers and the decision."

## Why This Spec Reads Technically

The user is the pipeline's default configuration. The re-rank stage is the most expensive
thing the engine does per query on a phone (a cross-encoder forward pass per candidate, the
larger of the two models in memory), and Feature 013 showed that on the three BEIR sets it
now costs quality on average: re-ranking the improved fused list at depth 20 gives a
three-set mean nDCG@10 of 0.4768 against 0.4790 without it (SciFact 0.7144 → 0.6954,
NFCorpus 0.3535 → 0.3609, FiQA 0.3692 → 0.3742). Two explanations are cheap to separate
without touching a model: the re-ranker re-orders too deep (a shallower head might keep the
gains and lose the damage), and the re-ranker is allowed to *replace* the fused order rather
than *inform* it (the fused rank carries evidence from two retrievers that the cross-encoder
throws away). This study measures both and produces a decision with numbers. It is a study:
the pipeline's behaviour and defaults do not change here; if the decision is to change them,
that is the next feature's one-line change with its own tests.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The depth sweep is measured (Priority: P1)

The re-ranked configuration is evaluated at depths 5, 10, 20 and 50 over the v2 fused list
on SciFact, NFCorpus and FiQA, each run verified by the 003 reference scorer, with nDCG@10
and Recall@100 per dataset and the three-set mean beside `hybrid-baseline-v2` (depth 0) and
`hybrid-rerank-v2` (depth 20, which must reproduce its committed baseline exactly).

**Why this priority**: The depth is the one knob already in the pipeline; if a shallower
depth is net-positive the decision needs nothing new.

**Independent Test**: A table of five depths × three datasets, every cell verified, with
the depth-20 column equal to the 013 baseline to 1e-6.

**Acceptance Scenarios**:

1. **Given** the v2 fused list and the pinned re-ranker, **When** depth 20 is evaluated,
   **Then** it reproduces `hybrid-rerank-v2` on every dataset to 1e-6 (the oracle for the
   whole sweep).
2. **Given** the sweep, **When** read, **Then** every depth has nDCG@10 and Recall@100 on
   every dataset and the three-set mean, and the per-dataset delta against depth 0.
3. **Given** a depth, **When** Recall@100 is compared with depth 0, **Then** it is equal
   unless the depth exceeds the retrieval depth (re-ranking within the top-k cannot change
   which documents are in the top 100 — a difference is a defect, not a finding).

---

### User Story 2 - The interpolation variant is measured (Priority: P1)

Within the re-ranked head, the fused rank signal and the cross-encoder score are combined
instead of the cross-encoder replacing the order, in two forms: **rank fusion** of the fused
order and the cross-encoder order within the head (parameter-free, the engine's own fusion
idiom), and **linear interpolation** of the two scores after per-query normalisation at three
mixing weights (0.25 / 0.5 / 0.75 on the cross-encoder side) — evaluated at the same depths
and datasets, verified by the reference scorer, in the same table.

**Why this priority**: If a shallower depth does not recover SciFact, this is the second
cheapest explanation; it costs no extra model calls (the cross-encoder scores from the
deepest run are reused).

**Independent Test**: The variant's rows in the table, verified, with the mean and the
per-dataset deltas against both depth 0 and the replace-order rows.

**Acceptance Scenarios**:

1. **Given** the cross-encoder scores of the deepest run, **When** a variant is derived from
   them offline, **Then** its depth-20 replace-order form reproduces `hybrid-rerank-v2` to
   1e-6 (the derivation is exact, not an approximation).
2. **Given** the variant rows, **When** compared with the replace-order rows at the same
   depth, **Then** every difference is stated per dataset.

---

### User Story 3 - The decision is recorded (Priority: P1)

The report states whether the pipeline's default re-rank behaviour (replace the order, depth
20) should stay, and if not, which measured configuration replaces it — by a decision rule
fixed before the numbers are read: **a configuration replaces the default only if its
three-set mean nDCG@10 is at least +0.005 above the current default's (0.4768) and no dataset
is more than 0.005 nDCG@10 below depth 0** (`hybrid-baseline-v2`); among qualifying
configurations the highest mean wins, ties by fewer cross-encoder calls. If none qualifies,
the default stays and the loss is on record. The report also records the cost side of every
candidate: cross-encoder calls per query (the depth), so the decision names what it saves or
spends on the phone.

**Why this priority**: The study exists to make this call before the sparse stage builds on
the pipeline; a table without a decision would leave 013 F-001 open.

**Independent Test**: The report's decision section names the rule, the winning
configuration, its numbers per dataset, the cost per query, and what changes (or does not) in
the follow-up.

**Acceptance Scenarios**:

1. **Given** the rule and the table, **When** the decision is read, **Then** it follows from
   the rule mechanically and cites the cells.
2. **Given** the decision, **When** it changes the default, **Then** the report names the
   follow-up change (the default constant, its tests, the boundary goldens to re-check) and
   nothing in the pipeline changes in this feature.

---

### Edge Cases

- Depth 50 exceeds the re-rank depths seen so far: the candidate list is built to
  max(k, depth) so a candidate below k can be promoted; Recall@100 is unchanged at depth 50
  because k = 100 ≥ 50.
- A query with fewer fused candidates than the depth: the head is whatever exists; no
  padding, no error.
- The cross-encoder score ties: the order within a tie is by ascending internal id (the
  engine's tie rule); the derived variants use the same rule so the depth-20 derivation is
  exact.
- Interpolation normalisation with a single candidate or all-equal scores: the head keeps its
  fused order.
- The derived variants must match a real pipeline run wherever both exist (depth 20 replace,
  and one further depth run end-to-end as a second check) — a mismatch is a defect in the
  derivation, stop-and-report.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The study MUST evaluate replace-order re-ranking at depths 5, 10, 20 and 50
  over `hybrid-baseline-v2` on SciFact, NFCorpus and FiQA, with depth 0 =
  `hybrid-baseline-v2` and depth 20 = `hybrid-rerank-v2` as anchors reproduced to 1e-6.
- **FR-002**: The study MUST evaluate both interpolation forms of User Story 2 — rank fusion
  (one row per depth) and linear interpolation at weights 0.25 / 0.5 / 0.75 (three rows per
  depth) — at depths 5, 10, 20 and 50 on the same datasets.
- **FR-003**: Every reported run MUST be verified by the 003 reference scorer to 1e-6.
- **FR-004**: Variants derived offline from the cross-encoder scores of a deeper run MUST
  reproduce the end-to-end pipeline run at depth 20 exactly and at one further depth
  end-to-end; the report states both checks.
- **FR-005**: The report MUST carry the full table (variant × depth × dataset, nDCG@10 and
  Recall@100, three-set mean, deltas against depth 0 and against the current default) and the
  cross-encoder calls per query for each row.
- **FR-006**: The decision rule is fixed here, before any run: a configuration replaces the
  default only if its three-set mean nDCG@10 ≥ 0.4768 + 0.005 and no dataset is > 0.005 below
  depth 0; highest qualifying mean wins, ties by fewer cross-encoder calls; none qualifying →
  the default stays. The decision MUST follow from it; the report states the rule, the
  qualifying set and the winner (or that none qualified).
- **FR-007**: No model, engine, format, FFI, application or default changes: the pipeline
  crates, `xtriever-rerank`, `xtriever-core`, `deny.toml` and every shipped default stay as
  they are; only the evaluation harness (a configurable depth and the offline derivation) and
  documents change. A default change, if decided, is a follow-up feature.
- **FR-008**: The 013 baselines and every earlier baseline MUST remain byte-identical.
- **FR-009**: The runs' per-query, per-candidate evidence (fused rank, fused score,
  cross-encoder score) MUST be kept under the build directory, never committed; the committed
  artefacts are the reports and the table.
- **FR-010**: Tests for the offline derivation (depth truncation, tie rule, interpolation
  normalisation edge cases) MUST be committed first, failing.

### Key Entities

- **Re-rank variant**: how the head is ordered — replace (cross-encoder order) or
  interpolate (fused signal combined with the cross-encoder score) — and its depth.
- **Sweep cell**: one variant at one depth on one dataset: a verified report with nDCG@10,
  Recall@100 and cross-encoder calls per query.
- **Decision**: the rule, the winning configuration, its cells, the cost, the follow-up.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Depth 20 (replace) reproduces `hybrid-rerank-v2` on all three datasets to 1e-6;
  depth 0 reproduces `hybrid-baseline-v2`.
- **SC-002**: Every cell of the table is verified by the reference scorer to 1e-6; Recall@100
  equals depth 0's in every cell.
- **SC-003**: The offline derivation matches the end-to-end pipeline on two depths exactly.
- **SC-004**: The report contains the complete table (4 depths × 5 variants — replace, rank
  fusion, three interpolation weights — × 3 datasets, plus depth 0) and a decision that
  follows the fixed rule and cites its cells.
- **SC-005**: No file under `crates/` outside `crates/xtriever-eval/` changes; every
  committed baseline is byte-identical to before.
- **SC-006**: Wall time for the study's runs stays within one working day on the reference
  laptop (the deepest run per dataset is the cost; shallower depths and variants are derived).

## Assumptions

- **The cross-encoder score of a (query, document) pair is independent of the batch it is
  scored in**, so depth-5/10/20 replace-order results and the interpolation variants can be
  derived exactly from the depth-50 run's scores; FR-004 / SC-003 check this rather than
  assume it, and if it fails the shallower depths are run end-to-end (more wall time, same
  conclusion).
- **Depth 0 is `hybrid-baseline-v2`** (no re-ranking), the reference every variant is measured
  against; the current default is replace-order at depth 20.
- **Cost is counted in cross-encoder calls per query** (= depth), not measured latency: the
  006 record already gives per-call cost on the device.
- **The default change, if any, is a follow-up**: the pipeline's default depth is a shipped
  constant with boundary goldens (007 Swift, 011 Python) built at depth 20; changing it is a
  small feature of its own, not a line in a study.
- **No new model, no conditional re-ranking policy** (e.g. skipping by score margin): both
  are candidates for later features and out of scope here.
