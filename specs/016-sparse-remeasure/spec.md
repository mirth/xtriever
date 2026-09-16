# Feature Specification: Sparse Stage Re-measurement

**Feature Branch**: `016-sparse-remeasure`

**Created**: 2026-09-16

**Status**: Draft

**Input**: User description: "Sparse stage re-measurement (012's GO re-costed after 013–015): 012 measured the inference-free sparse expansions (opensearch doc-v3, dot product) fused three-way with the v1 lexical and dense lists at 0.484 mean nDCG@10 against a 0.468 baseline; the pipeline now sits at 0.4913 (hybrid-rerank-v3: joined lexical field + interpolated re-rank), and the interpolation alone lifted FiQA — the sparse stage's strongest case — to 0.391. Measure, offline and from artefacts already on disk, what the sparse list adds on top of the current pipeline: (1) three-way RRF of the v2 lexical run, the dense run and the sparse dot run, un-re-ranked, against hybrid-baseline-v2 (0.4790); (2) the same list re-ranked at depth 20 under the interpolating rule, using the cross-encoder scores from 014's depth-50 explain exports and the 006 torch reference for any pair those do not cover, against hybrid-rerank-v3 (0.4913); per dataset and mean, verified by the 003 scorer. Decision rule fixed in advance: the sparse stage is specified as a feature only if the re-ranked three-way mean is at least +0.005 above 0.4913 with no dataset below hybrid-rerank-v3 by more than 0.005; otherwise 012's GO is recorded as withdrawn with the numbers. No Rust changes; no new model; the result and decision on record."

## Why This Spec Reads Technically

The user is the roadmap. Feature 012 said GO to a sparse-expansion stage on a measured
+1.6 mean nDCG@10 points (0.484 against a 0.468 pipeline). Three features later the pipeline
is at 0.4913 without it — the joined lexical field (013) and the interpolating re-rank (015)
took most of what the three-way fusion had promised on the titled sets, and FiQA, the sparse
stage's strongest case, went from 0.369 to 0.391 on interpolation alone. A stage that costs
150–255 MB of postings on the phone and an out-of-Rust corpus encoder must be justified
against the pipeline it would join, not the one it was measured against. Everything needed to
re-measure is on disk: the sparse encodings and their dot-product runs (012), the v2 lexical
and dense runs (013), and the cross-encoder scores of every candidate in the v2 fused top-50
(014). The measurement is offline, deterministic, and verified by the same reference scorer
as every baseline; the decision rule is written down before the numbers are read.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The un-re-ranked fusion is re-measured on the current lists (Priority: P1)

Reciprocal-rank fusion of the v2 lexical list, the dense list and the sparse dot list (the
012 recipe with the v1 lexical list replaced by v2), plus the two pairings (dense + sparse;
lexical + sparse), scored on the three datasets and compared with `hybrid-baseline-v2`.

**Why this priority**: This is the first-stage question: does the sparse list still add
candidates the two current lists lack, and how much?

**Independent Test**: The three fused variants' nDCG@10 / Recall@100 per dataset and mean,
each verified by the 003 reference scorer, beside `hybrid-baseline-v2` (0.7144 / 0.3535 /
0.3692; mean 0.4790) and 012's numbers with the v1 lexical list.

**Acceptance Scenarios**:

1. **Given** the v2 lexical, dense and sparse runs, **When** fused three-way, **Then** the
   per-dataset numbers and the mean are on record with deltas against `hybrid-baseline-v2`.
2. **Given** the fusion of the v2 lexical and dense lists alone (no sparse), **When** scored
   by the same code, **Then** it reproduces `hybrid-baseline-v2` per query to 1e-6 (the
   fusion code is the engine's rule — the oracle for the whole measurement).

---

### User Story 2 - The re-ranked fusion is re-measured against the current default (Priority: P1)

Each fused list re-ranked at depth 20 under the interpolating rule (α 0.5), the cross-encoder
scores taken from 014's depth-50 explain exports where the candidate was in the v2 fused
top-50 and from the 006 torch reference otherwise, compared with `hybrid-rerank-v3` (0.7207 /
0.3622 / 0.3910; mean 0.4913).

**Why this priority**: The decision rule is stated on this number: it is what a caller would
get.

**Independent Test**: The re-ranked three-way variant per dataset and mean, verified, with the
count of pairs the reference had to score (and the agreement of the reference with the
engine's scores on the pairs both cover) on record; the same re-ranking applied to the
v2 fused list reproduces `hybrid-rerank-v3` per query to 1e-6.

**Acceptance Scenarios**:

1. **Given** the v2 fused list and the explain-export scores, **When** re-ranked offline,
   **Then** `hybrid-rerank-v3` is reproduced per query (the re-ranking code is the engine's
   rule).
2. **Given** a three-way fused list, **When** re-ranked, **Then** the pairs not covered by the
   explain exports are scored by the reference, their count and share per dataset are
   reported, and the reference's agreement with the engine on covered pairs is stated
   (maximum absolute difference; the 006 tolerance).
3. **Given** the re-ranked numbers, **When** the decision rule is applied, **Then** the
   outcome follows mechanically and is recorded.

---

### User Story 3 - The decision is recorded (Priority: P1)

The report states the rule, the numbers, and the outcome: either "the sparse stage is
specified as the next feature" with its expected gain restated from the measured cells, or
"012's GO is withdrawn" with the numbers that withdrew it and what would reopen it.

**Why this priority**: The roadmap needs a decision with a number, not a standing GO from a
superseded baseline.

**Independent Test**: The report's decision section names the rule, cites the cells, and
states the outcome; 012's report gains a pointer.

**Acceptance Scenarios**:

1. **Given** the rule "specify only if the re-ranked three-way mean ≥ 0.4913 + 0.005 and no
   dataset is more than 0.005 below `hybrid-rerank-v3`", **When** applied to the table,
   **Then** the outcome is stated with the qualifying (or failing) cells.
2. **Given** a withdrawal, **When** read, **Then** it names the conditions that would reopen
   the question (a corpus shape, a cheaper encoder, a measured need).

---

### Edge Cases

- A query whose three-way top-20 contains candidates outside the v2 fused top-50: those pairs
  are scored by the reference; counted and reported.
- A query absent from the sparse run (no expansion overlap): the fusion uses the two lists
  present, as the engine's RRF does for a candidate missing from one list.
- The reference cross-encoder disagrees with the engine's score on a covered pair beyond the
  006 tolerance: reported as a finding; the engine's score is used where available.
- Ties in the fused or combined scores: the engine's rules (fused ties by ascending internal
  id = corpus position; combined ties by fused rank) — the reproduction checks enforce them.
- Recall@100 of a re-ranked variant equals its un-re-ranked variant's (re-ordering within the
  first 100); a difference is a defect.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The measurement MUST use the artefacts on disk — the 012 sparse dot runs
  (doc-v3, unquantised), the 013 v2 lexical runs, the dense runs, the 014 depth-50 explain
  exports — and produce no new corpus encoding.
- **FR-002**: The fusion MUST be the engine's RRF (k 60, ties by ascending corpus position)
  and MUST reproduce `hybrid-baseline-v2` per query when given the v2 lexical and dense lists.
- **FR-003**: The re-ranking MUST be the engine's interpolating rule (α 0.5, depth 20) and MUST
  reproduce `hybrid-rerank-v3` per query when given the v2 fused list and the explain-export
  scores.
- **FR-004**: Pairs without an explain-export score MUST be scored by the 006 reference
  cross-encoder (the pinned model); their count per dataset and the reference-vs-engine
  agreement on covered pairs MUST be reported.
- **FR-005**: Variants: `rrf(lex2, dense, dot)`, `rrf(dense, dot)`, `rrf(lex2, dot)`, each
  un-re-ranked and re-ranked; anchors `hybrid-baseline-v2`, `hybrid-rerank-v3`, and 012's
  three-way number with the v1 lexical list.
- **FR-006**: Every run MUST be scored by the 003 reference scorer; Recall@100 of each
  re-ranked variant MUST equal its un-re-ranked variant's.
- **FR-007**: The decision rule is fixed here: the sparse stage is specified as a feature only
  if the re-ranked `rrf(lex2, dense, dot)` mean nDCG@10 ≥ 0.4913 + 0.005 = 0.4963 **and** no
  dataset is more than 0.005 below `hybrid-rerank-v3`; otherwise 012's GO is withdrawn. Other
  variants inform the findings, not the decision.
- **FR-008**: Tests for the fusion and re-ranking code MUST be committed first, failing; the
  two reproduction checks (FR-002, FR-003) are the executable oracles.
- **FR-009**: No change under `crates/`; no new model; no new Python environment; the runs
  stay under the build directory; the committed artefacts are the cells, the table and the
  report.

### Key Entities

- **Fused variant**: which lists are fused; un-re-ranked or re-ranked.
- **Cell**: a variant on a dataset — nDCG@10, Recall@100, per-query metrics, and for re-ranked
  variants the count of reference-scored pairs.
- **Decision**: the rule, the cells it reads, the outcome, the reopening conditions.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `rrf(lex2, dense)` reproduces `hybrid-baseline-v2` per query to 1e-6 on the
  three sets; re-ranking it reproduces `hybrid-rerank-v3` per query to 1e-6.
- **SC-002**: Every cell verified by the 003 scorer; Recall@100 identical between each
  re-ranked variant and its un-re-ranked one.
- **SC-003**: The table has 3 variants × 2 (un-re-ranked, re-ranked) × 3 datasets plus the
  anchors; the reference-scored pair count and the agreement figure are stated per dataset.
- **SC-004**: The decision follows FR-007 and is recorded in this feature's report and as a
  pointer in 012's.
- **SC-005**: No file under `crates/` changes; the wall time stays within a working day (the
  reference scoring of uncovered pairs is the only model work).

## Assumptions

- **The doc-v3 model, dot product, unquantised** is the sparse variant re-measured (012's best
  and its GO basis); quantisation (×100, ≤ 0.03 points) would not change the decision.
- **The explain-export scores are the engine's** (f32 cross-encoder logits from the 014 runs);
  the reference scores fill only the gaps, and the report says how many and how well they
  agree where both exist — an estimate, stated as one, of what the engine would produce.
- **The decision's headline is the three-way re-ranked mean**; a strong pairing (e.g. dense +
  sparse on FiQA) is a finding for a corpus-specific configuration, not a reason to build the
  stage by default.
- **Withdrawal is not a permanent no**: the report names what would reopen it.
