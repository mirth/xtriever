# Feature Specification: Lexical Quality — One Field for BM25

**Feature Branch**: `013-lexical-quality`

**Created**: 2026-09-16

**Status**: Draft

**Input**: User description: "Lexical quality first (012 F-002)" — the spike found the engine's BM25 six points behind a one-field BM25 on SciFact; attribution experiments (2026-09-15/16, in the spike's BM25) showed the whole gap is the evaluation's field layout — `title` boosted 2.0 as a separate field beside `text` — and not the analyzer, the BM25 parameters or stop words. Fix the layout, re-baseline what fuses with it, and write down what was tried and lost.

## Why This Spec Reads Technically

The user is the engine's evaluation: every later stage (dense fusion, re-ranking, the sparse
stage 012 recommended) is measured against the lexical list, so a weak lexical baseline
understates the pipeline and mis-tunes fusion. The measurement that motivates the change is
precise — a BM25 built in the evaluation's exact shape (`title` × 2.0 + `text`) lands on the
engine's numbers (0.621 / 0.312 / 0.247 vs 0.627 / 0.312 / 0.250), and the same BM25 over one
joined field lands 6.0 and 1.1 points higher on SciFact and NFCorpus (FiQA has no titles) —
which is also how BEIR's reference BM25 indexes documents (one `contents` field). The three
alternatives were measured and lost: BM25 k1 = 0.9 / b = 0.4 (−0.4 to −1.3 points), Lucene's
stop-word list (−0.4 to −0.8), and a light extra title field beside the joined one (±0.3,
opposite signs on the two sets). So the feature is one configuration change with its
consequences: new baselines, and the re-baselining of every configuration that fuses the
lexical list. Naming fields and configurations is therefore the requirement.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The lexical baseline indexes one joined field (Priority: P1)

The evaluation's lexical configuration gains a version 2 that indexes `title + " " + text`
(title omitted when empty — the passage shape the dense stage already uses) as a single
`contents` field under the same analyzer, queried the same way; its baselines on the three
BEIR sets are recorded beside version 1's with the deltas.

**Why this priority**: This is the six points.

**Independent Test**: `lexical-baseline-v2` on SciFact / NFCorpus / FiQA, verified by the 003
reference scorer, with nDCG@10 within noise of the spike's one-field numbers (0.687 / 0.323 /
0.247) and above v1 on the two sets with titles.

**Acceptance Scenarios**:

1. **Given** the three datasets, **When** `lexical-baseline-v2` runs, **Then** its reports are
   committed as baselines and the deltas against v1 are stated (SciFact and NFCorpus up,
   FiQA within ±0.001).
2. **Given** v1 and v2, **When** compared, **Then** the only difference is the field layout —
   same analyzer, same query kind, same k, same BM25 parameters.
3. **Given** the 003 reference scorer, **When** each v2 report is verified, **Then** it matches
   to 1e-6.

---

### User Story 2 - Everything downstream is re-baselined on the better list (Priority: P1)

The hybrid (lexical + dense, RRF) and hybrid-re-rank configurations gain version 2s that
fuse the v2 lexical list; their baselines are recorded with deltas against version 1. The
CI smoke gate moves to the v2 lexical baseline.

**Why this priority**: A better lexical list should raise the fused numbers; if it does not,
that is a finding about fusion, and either way 014 (the sparse stage) must start from the
right baselines.

**Independent Test**: `hybrid-baseline-v2` and `hybrid-rerank-v2` on the three sets, verified
by the reference, with deltas against v1 stated; the SciFact smoke passes against the v2
lexical baseline.

**Acceptance Scenarios**:

1. **Given** the v2 lexical list, **When** fused with the dense list, **Then** the hybrid v2
   baseline is recorded and compared with v1 per dataset.
2. **Given** hybrid v2, **When** re-ranked at depth 20, **Then** the hybrid-re-rank v2 baseline
   is recorded and compared with v1 per dataset.
3. **Given** CI, **When** a ranking crate changes, **Then** the smoke compares against the v2
   lexical baseline; the v1 baselines stay committed as history.

---

### User Story 3 - The measurements that did not win are on record (Priority: P2)

The report states the attribution (the engine-shape replica reproducing the engine), the
lost alternatives with their numbers, and the recommendation for callers: index one joined
text field for BM25 (the Python and CLI guidance), with the note that the shipped Wikipedia
index keeps its two-field schema until it is rebuilt.

**Why this priority**: The next person to touch BM25 should not redo the experiments.

**Independent Test**: The report contains the attribution table and the alternatives table
with numbers; the READMEs carry the recommendation.

**Acceptance Scenarios**:

1. **Given** the report, **When** read, **Then** the engine-shape replica, the joined field,
   the parameter change, the stop words and the extra title field each have a number per
   dataset.
2. **Given** the Python and CLI documentation, **When** a caller designs a schema, **Then**
   they are told to index one joined text field for BM25 and why.

---

### Edge Cases

- A document with an empty title: the joined field is the text alone, no leading separator
  (as the dense passage).
- A document with an empty text and a title: the joined field is the title.
- Version 1 configurations remain runnable and their baselines unchanged, so any earlier
  report can be reproduced.
- The 007 fixture index and the Swift/Python goldens use the 005 fixture's own schema
  (title 2.0 / text 1.0) and are **not** touched: they are parity oracles for the boundary,
  not quality baselines.
- The 008 Wikipedia index (title 2.0 / text 1.0) is not rebuilt here: its retrieval quality
  is unmeasured (no oracle), the rebuild is hours plus a device record; the report says so
  and 014 decides.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The evaluation harness MUST offer `lexical-baseline-v2`: one indexed text field
  `contents` = `title + " " + text` (title omitted when empty), analyzer `standard_en`, boost
  1.0, the same query kind, BM25 parameters and depth as v1.
- **FR-002**: The harness MUST offer `hybrid-baseline-v2` and `hybrid-rerank-v2`, identical to
  their v1s except for the lexical configuration (v2).
- **FR-003**: Baselines for the three v2 configurations on SciFact, NFCorpus and FiQA MUST be
  committed under this feature's `baselines/`, each verified by the 003 reference scorer to
  1e-6, with `compare` deltas against v1 in the PR description.
- **FR-004**: The v1 configurations and baselines MUST remain unchanged and runnable.
- **FR-005**: The CI smoke gate MUST compare against `lexical-baseline-v2` on SciFact (the
  standing rule: one dataset, no model).
- **FR-006**: No change to any crate other than `xtriever-eval` (the configurations) and
  documentation; no engine, format, FFI or app change; `deny.toml` and `xtriever-core`
  untouched.
- **FR-007**: The tests for the v2 field construction (empty title, empty text, both present)
  and for the v2 configurations' identity to v1 except the fields MUST be committed first.
- **FR-008**: The report MUST record the attribution and the lost alternatives with numbers,
  and the Python and CLI documentation MUST recommend one joined text field for BM25.

### Key Entities

- **Evaluation configuration**: named, versioned; fields (name, source, analyzer, boost),
  query kind, depth; v1 and v2 side by side.
- **Baseline**: a scored report per configuration per dataset, verified by the reference,
  committed.
- **Delta**: v2 vs v1 per configuration per dataset, nDCG@10 and Recall@100.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `lexical-baseline-v2` nDCG@10 ≥ v1 + 0.05 on SciFact, ≥ v1 + 0.008 on NFCorpus,
  within ±0.001 of v1 on FiQA (the spike's one-field BM25 measured +0.060, +0.011, −0.003;
  the engine's tokenizer differs slightly, so the floors are set below the measured gains).
- **SC-002**: `hybrid-baseline-v2` and `hybrid-rerank-v2` mean nDCG@10 across the three sets
  ≥ their v1 means; any per-dataset drop is reported as a finding, not hidden.
- **SC-003**: Every v2 baseline verifies against the 003 reference to 1e-6.
- **SC-004**: The v1 baselines are byte-identical to before; the v1 configurations still run
  and reproduce them.
- **SC-005**: The smoke gate passes on CI against the v2 lexical baseline.
- **SC-006**: No executable change under `crates/` outside `crates/xtriever-eval/`; the one
  permitted exception is the documentation comment in `crates/xtriever-cli/src/wiki/chunking.rs`
  that FR-008 requires (its diff consists of `///` lines only).

## Assumptions

- **Field name** `contents` (BEIR/Anserini's name for the joined field).
- **Analyzer** unchanged (`standard_en`): the attribution shows tokenization is not the gap;
  a stop-word list and k1/b changes were measured and lost.
- **Version numbering**: `-v2` configurations beside `-v1`, the 003–006 convention.
- **Wikipedia**: not rebuilt; 014's plan decides whether the sparse build also joins the
  fields (it will re-encode the corpus anyway).
- **Fixture goldens** (007/009/011): untouched — boundary parity, not quality.
