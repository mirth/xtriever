# Feature Specification: Interpolated Re-ranking as the Default

**Feature Branch**: `015-rerank-interpolation`

**Created**: 2026-09-16

**Status**: Draft (clarified 2026-09-16: Q1 = A, descriptors without a mode read as the new default)

**Input**: User description: "Interpolated re-ranking as the pipeline default (014's decision): the re-rank stage combines the fused (RRF) score and the cross-encoder score inside the re-ranked head — (1−α)·minmax(fused) + α·minmax(cross-encoder), α = 0.5, ties by fused rank — instead of replacing the order by cross-encoder score. Measured in 014: 0.7207 / 0.3622 / 0.3910 nDCG@10 on SciFact / NFCorpus / FiQA, mean 0.4913 vs 0.4768 for today's replace-order default, positive on every dataset, same 20 calls per query. Scope: the pipeline's re-rank order rule and descriptor (a recorded mode with α; format/default change → ADR), explain reporting both terms, the FFI/Python defaults following, the 007 Swift and 011 Python goldens regenerated for the new order, hybrid-rerank-v3 baselines on the three sets reproducing 014's lin-0.5-d20 cells exactly, replace-order still selectable for reproducibility."

## Why This Spec Reads Technically

The user is every caller of the pipeline — the iOS demo, the Python package, the CLI — and
what they get is the top of the result list. Feature 014 measured, under a rule fixed before
the runs, that the pipeline's most expensive stage was throwing away evidence: when the
cross-encoder's order *replaces* the fused order in the re-ranked head, SciFact loses at
every depth and the three-set mean lands below not re-ranking at all; when the cross-encoder
score is *combined* with the fused score (half and half, each min-max normalised over the
head; ties by fused rank), every dataset gains — +0.6 / +0.9 / +2.2 nDCG@10 points over the
un-re-ranked list, a mean of 0.4913 against today's 0.4768 — at the same twenty cross-encoder
calls per query. This feature makes that measured rule the engine's behaviour and its
default, records it where the index records its other ranking parameters, keeps the old rule
selectable so earlier results can be reproduced, and re-baselines everything that depends on
the order: the harness, the boundary goldens the Swift and Python suites check bit for bit,
and the documentation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The engine orders the re-ranked head by the combined score (Priority: P1)

A search with a re-ranker loaded orders the first `depth` fused candidates by
`(1 − α)·minmax(fused score) + α·minmax(cross-encoder score)` with α = 0.5, ties by fused
rank, followed by the remaining candidates in fused order — exactly the rule 014 derived —
and the harness's `hybrid-rerank-v3` configuration reproduces 014's `lin-0.5` depth-20 cells
on the three datasets to 1e-6 per query.

**Why this priority**: This is the +1.45 mean points.

**Independent Test**: `hybrid-rerank-v3` on SciFact / NFCorpus / FiQA, verified by the 003
reference scorer, equal per query to `specs/014-rerank-depth-study/runs/lin-0.5-d20.<d>.json`
and list for list to the derived runs where they still exist on disk.

**Acceptance Scenarios**:

1. **Given** a re-ranked head with distinct scores, **When** searched, **Then** the head's
   order is the combined-score order and the tail is the fused order (the 006 reference
   generator, extended with the interpolation rule, is the oracle: golden cases including a
   single candidate, a constant cross-encoder column, ties, a head shorter than the depth).
2. **Given** the three datasets, **When** `hybrid-rerank-v3` runs, **Then** its baselines
   match 014's cells (nDCG@10 0.7207 / 0.3622 / 0.3910; Recall@100 unchanged).
3. **Given** the same index, query and configuration, **When** searched twice, **Then** the
   results are identical (determinism; ties by fused rank, which is itself tie-broken by
   ascending internal id).

---

### User Story 2 - The rule is recorded, selectable and reported (Priority: P1)

The index's descriptor records the re-rank mode and α beside the re-rank depth; a search
option overrides it per query (as the depth can be overridden), so replace-order re-ranking
remains available for reproducing 006–014 results; the explanation of a re-ranked hit
reports the fused term, the cross-encoder term and the combined score it was ordered by; the
FFI and Python surfaces expose the mode in the build configuration, the search options and
the index information.

**Why this priority**: Reproducibility (Principle VI) and the boundary contract: a caller
must be able to see and choose the rule.

**Independent Test**: Build an index with the default → its info reports the interpolating
mode with α 0.5; search with the replace override → the 006-order result; explain shows the
three numbers; the Swift and Python suites' regenerated goldens pass.

**Acceptance Scenarios**:

1. **Given** an index built by this version, **When** its information is read, **Then** the
   mode is `interpolate` with α 0.5 and the depth 20.
2. **Given** an index built before this feature (no recorded mode), **When** opened by this
   version, **Then** it reads as the new default — interpolate, α 0.5 — so the shipped
   Wikipedia index and the 007 fixture index get the gain without a rebuild; the format
   version stays; the ADR records that an upgrade changes the head order of existing indexes
   and that the earlier order is reproducible through the per-search `replace` override.
3. **Given** a search with the replace override on an interpolating index, **When** compared
   with the 013 `hybrid-rerank-v2` baseline, **Then** it reproduces it exactly.
4. **Given** a re-ranked hit with `explain`, **When** read, **Then** it carries the fused
   score, the cross-encoder score, the combined score and the re-rank rank; an un-re-ranked
   hit carries none of the re-rank fields.
5. **Given** the Swift and Python suites, **When** run against regenerated goldens, **Then**
   they pass, and the goldens' with-re-ranker responses differ from the previous ones only in
   the re-ranked heads.

---

### User Story 3 - Documentation and the record (Priority: P2)

The ADR records the default change and the descriptor field; the harness's crate docs, the
Python README and the CLI's schema notes describe the mode; 014's report gains a "landed in
015" line; the report carries the baselines and the golden regeneration diff summary.

**Why this priority**: The next person must find why the head is ordered this way and how
to get the old order.

**Independent Test**: The ADR exists and is accepted; each document names the mode, α and the
override; the report has the three baselines and the goldens' change summary.

**Acceptance Scenarios**:

1. **Given** the ADR, **When** read, **Then** it states the measured numbers, the rule, the
   default change, the descriptor field and the reproducibility path.
2. **Given** the Python README, **When** a caller reads the search section, **Then** they see
   the default mode and how to select replace-order.

---

### Edge Cases

- A head of one scored candidate: min-max of a single value is 0 for both terms; the
  candidate stays first (it is the whole head).
- A constant cross-encoder column: its term is 0 for every candidate; the head keeps its
  fused order.
- A constant fused column (candidates with identical RRF sums): the fused term is 0; the head
  is ordered by the cross-encoder term, ties by fused rank.
- Fewer scored candidates than the depth (the re-ranker skipped or a short list): the head is
  the scored ones; unscored candidates keep their fused positions after the head, as today.
- Re-ranker degraded (budget, failure): the fused list is returned unchanged, as today; the
  mode has no effect.
- Depth 0 or no re-ranker loaded: the fused list; the mode has no effect.
- α outside [0, 1] in a configuration or option: rejected as a schema/option error at build or
  search time, never clamped.
- The 006 `pipeline_order` goldens (replace order) remain valid and are still checked — the
  replace mode is the same code path as before.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The pipeline MUST implement the interpolating order rule exactly as 014's
  derivation defines it (`(1 − α)·minmax(fused) + α·minmax(cross-encoder)` over the scored
  head; ties by fused rank; tail in fused order; cut at k) and make it the default with
  α = 0.5 at depth 20.
- **FR-002**: The replace-order rule MUST remain available under a named mode and be
  bit-identical to the 006–014 behaviour (the 006 goldens still pass; `hybrid-rerank-v2`
  reproduces its baselines).
- **FR-003**: The descriptor MUST record the mode and α; the search options MUST allow a
  per-query override of the mode (and α); the index information MUST report both. The
  descriptor without the field reads as the default (`interpolate`, α 0.5), the format
  version unchanged; recorded in the ADR.
- **FR-004**: `explain` MUST report, for a re-ranked hit, the fused score, the cross-encoder
  score, the combined score used for ordering, and the re-rank rank.
- **FR-005**: The FFI (Swift) and Python surfaces MUST expose the mode in the build
  configuration, the search options and the index information, defaulting to the new mode;
  their goldens MUST be regenerated by the existing generator and the suites pass.
- **FR-006**: The harness MUST offer `hybrid-rerank-v3` (`hybrid-baseline-v2` re-ranked at
  depth 20 in the interpolating mode) with baselines on the three datasets committed, each
  verified by the 003 reference and equal per query to 014's `lin-0.5-d20` cells; `hybrid-
  rerank-v2` stays and reproduces.
- **FR-007**: Tests MUST be committed first, failing: the 006 reference generator gains the
  interpolation rule and golden cases; the pipeline's order-rule tests; the descriptor
  round-trip with the new field; the FFI/Python surface tests for the mode.
- **FR-008**: An ADR MUST record the default change and the descriptor change (Principle V,
  II); the constitution is not amended.
- **FR-009**: No `xtriever-core` trait changes; `deny.toml` untouched; no new dependency.
- **FR-010**: Every earlier baseline stays byte-identical; the 014 cells are the oracle, not
  re-derived.

### Key Entities

- **Re-rank mode**: `replace` (cross-encoder order) or `interpolate { alpha }`; recorded in
  the descriptor, overridable per search, reported in info and explain.
- **Combined score**: the number a re-ranked hit is ordered by under `interpolate`.
- **Golden**: the 006 order cases (both modes), the 007 parity goldens (regenerated), the
  `hybrid-rerank-v3` baselines (equal to 014's cells).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `hybrid-rerank-v3` nDCG@10 equals 014's `lin-0.5-d20` cells on all three sets
  to 1e-6 per query: 0.7207 / 0.3622 / 0.3910; Recall@100 0.955000 / 0.321648 / 0.707111.
- **SC-002**: `hybrid-rerank-v2` still reproduces its 013 baselines; the 006 order goldens
  pass; every baseline of 003–014 byte-identical.
- **SC-003**: The Swift and Python suites pass against regenerated goldens; the regeneration
  changes only the with-re-ranker responses' heads (a diff summary in the report).
- **SC-004**: Info reports the mode; the replace override on a new index reproduces the v2
  baseline on SciFact exactly.
- **SC-005**: The cross-encoder call count per query is unchanged (depth 20); no latency
  claim beyond that — the normalisation is arithmetic over ≤ 20 numbers.
- **SC-006**: The PR stays under ~800 changed lines excluding regenerated goldens and
  baselines, or is split as the plan states.

## Assumptions

- **α is recorded, not a constant**: the descriptor stores `interpolate { alpha: 0.5 }`; a
  later feature can tune it without a format change. 014 showed 0.25–0.75 all qualify, so no
  per-dataset α.
- **The hit's public `score` stays the fused score** (its documented meaning); the combined
  score is reported in `explain`, not as a new public hit field — the boundary types do not
  grow for a number only the explanation needs.
- **Goldens are regenerated by the existing 007 generator** (the engine is the reference for
  boundary parity; the 006 reference generator is the oracle for the order rule itself).
- **The Wikipedia index is not rebuilt**: under Q1 = A it reads as `interpolate` on upgrade,
  so the demo app gains the new order with no rebuild and no device record in this feature
  (its memory and latency are unchanged: same calls, arithmetic over ≤ 20 numbers).
- **The 014 cells are the oracle** for `hybrid-rerank-v3`, so the engine's rule must match
  the derivation bit for bit — including min-max in f64 over the head with the fused score as
  f64 and the cross-encoder score as f32 widened to f64.
