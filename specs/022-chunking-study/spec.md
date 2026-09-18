# Feature Specification: The Chunking Study

**Feature Branch**: `022-chunking-study`

**Created**: 2026-09-17

**Status**: Draft

**Input**: User description: "Chunking study: measure how chunking long documents affects retrieval quality — nDCG@10 and Recall@100 on the BEIR sets — because the engine indexes each BEIR document as one passage whose embedding is truncated at 256 positions, and 71 % of SciFact, 79 % of NFCorpus and 19 % of FiQA documents exceed that window (measured with the embedder's tokenizer on 2026-09-17); the 021 chunker comparison on Wikipedia showed what chonky changes but not whether it helps. A reference study script (`reference/chunking_study.py`, the 014/016 pattern: subcommands to build | search | score | table | decide, with `reference/tests_022/`) runs through the Python package: for each variant and dataset it splits every document into passages, indexes them (`contents` = title + " " + passage, the baseline's join, `standard_en`, the engine's defaults), searches every test query at k = 300 passages with candidate depth 300 at depths 0 and 20 (α 0.5), aggregates passages to documents by MaxP (a document's score is its best passage's; order by first occurrence), truncates to 100 documents, writes TREC run files and scores them with pytrec_eval against the test qrels. Variants: `whole` (one passage per document — today's behaviour and the anchor), `contract` (the Feature 008 contract chunker, budget 256 − title positions, the reference implementation in `reference/gen_008_fixtures.py`), `chonky` (the pinned `mirth/chonky_distilbert_base_uncased_1` splitter as in 021, unbounded), `chonky-bounded` (chonky, then any chunk over the window split at blank lines then sentences up to the window, and fragments under 16 positions merged into the preceding passage — the two fixes 021's comparison proposed). Before any variant runs, the harness check: `whole` at the baseline configuration (k = 100, candidate depth 100) MUST reproduce `hybrid-rerank-v3` (0.720711 / 0.362246 / 0.390964 nDCG@10 re-ranked; fused 0.714369 / 0.353510 / 0.369210) exactly on all three datasets — otherwise stop. Scope: SciFact and NFCorpus for all four variants; FiQA for `whole` and, if one exists, the variant that wins on the first two (its embedding is ~2 h per variant); every cell is a committed run file under the feature's `runs/` with the score table in the report. Decision rule fixed now: a chunking variant is recommended for the demos' recipe and the documentation if its re-ranked three-way mean nDCG@10 (or two-way where FiQA was not run, stated as such) is at least 0.005 above `whole` with no dataset more than 0.005 below it and mean Recall@100 not more than 0.005 below; between `contract`, `chonky` and `chonky-bounded` the best by the same rule, ties to the variant without a neural splitter; the shipped Wikipedia index (built by the Rust CLI with the contract chunker) is not changed by this study — if a chonky variant beats `contract` by the rule, a separate feature would add a pre-split passages input to the Rust build. No engine, FFI, format or baseline change; CI runs nothing (all model-backed); tests first for the study's pure parts (chunk bounding and merging, MaxP aggregation, run-file writing, the decision rule) against small fixtures; the chonky splitting and embedding of FiQA are the expensive steps and are run unattended."

**Owner's framing (2026-09-17)**: "I want to pursue chonky further. We actually didn't compute
how Chonky chunking affects the metrics."

**Owner's amendment (2026-09-18)**, after the four variants had run: a fifth variant,
`chonky-if-long` — chonky only for documents whose contents exceed the window, the rest
indexed whole — because on FiQA (81 % of posts inside the window) chonky fragmented short
documents and lost 7 nDCG@10 points. The same rule applies to it; it runs on all three sets.

## Why This Spec Reads Technically

Every retrieval number this repository has published was measured on documents indexed
whole — one passage each, its embedding cut at the embedder's 256-position window. That
was never a decision; it is how BEIR arrives. Yet most of two of the three test corpora are
longer than the window: 71 % of SciFact's abstracts and 79 % of NFCorpus's, so the dense
stage has been reading the first half of most documents. The 021 comparison showed that a
semantic splitter changes half of what a person sees on Wikipedia, and also that unbounded
chunks and fragments cost as much as they give — but Wikipedia has no relevance judgments,
so it could not say which side wins. The BEIR sets can. This study chunks the same
documents four ways, retrieves passages, folds them back to documents by their best
passage, and scores exactly as the baselines were scored, with the decision rule written
down before a single cell runs. Its anchor is the study harness reproducing the published
baselines to the last digit; its outcome is a recommendation for the demos' ingestion
recipe, and evidence for or against moving the shipped Wikipedia index later.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The study harness reproduces the baselines (Priority: P1)

Before any chunking variant runs, the harness indexes every BEIR document whole at the
baseline configuration and scores it: the fused and re-ranked nDCG@10 and Recall@100 must
equal the published `hybrid-baseline-v2` / `hybrid-rerank-v3` figures on all three
datasets, digit for digit. Anything else means the study is measuring the harness, not
the chunking, and it stops.

**Why this priority**: Rule 6 — a study whose anchor drifts proves nothing.

**Independent Test**: the three `whole@baseline` score files equal the committed
baselines' metrics.

**Acceptance Scenarios**:

1. **Given** SciFact, NFCorpus and FiQA indexed whole at k = 100 / depth 100, **When**
   scored, **Then** fused nDCG@10 is 0.714369 / 0.353510 / 0.369210 and re-ranked
   0.720711 / 0.362246 / 0.390964, and Recall@100 equals the baselines' too.
2. **Given** a mismatch, **When** found, **Then** the study stops and reports which dataset
   and which stage differ; no variant cell is run.

---

### User Story 2 - Four ways of chunking, scored the same way (Priority: P1)

For SciFact and NFCorpus, each document is split four ways — whole, the contract chunker,
chonky, chonky bounded — and the passages are indexed, retrieved at the study's depth,
folded back to documents by their best passage, truncated to 100, and scored fused and
re-ranked. Every cell is a committed run file; the report's table shows all of them beside
the anchor with the deltas.

**Why this priority**: This is the measurement.

**Independent Test**: sixteen run files (4 variants × 2 datasets × 2 stages) under `runs/`,
each scored, in one table.

**Acceptance Scenarios**:

1. **Given** a variant and a dataset, **When** built and searched, **Then** the run file
   holds at most 100 documents per query, each document once, ordered by its best
   passage's score with ties by first occurrence.
2. **Given** the `whole` variant at the study's depth, **When** compared with the anchor,
   **Then** the report states the difference the depth alone makes (so that chunking's
   effect is read against the right baseline).
3. **Given** `chonky-bounded`, **When** its passages are inspected, **Then** none exceeds
   the window and none is shorter than 16 positions unless it is a whole document.

---

### User Story 3 - FiQA for the anchor and the winner (Priority: P2)

FiQA — the largest set, where only 19 % of documents exceed the window — is run for `whole`
and for the variant that leads on SciFact and NFCorpus by the rule, if any does. The table
marks which cells are two-way and which three-way.

**Why this priority**: FiQA's embedding is hours per variant; the rule allows a two-way
verdict stated as such.

---

### User Story 4 - The decision is made by the rule written down first (Priority: P1)

A chunking variant is *recommended* if its re-ranked mean nDCG@10 across the datasets it
ran on is at least 0.005 above `whole`'s (at the same depth), no dataset is more than 0.005
below `whole`, and its mean Recall@100 is not more than 0.005 below. Among the chunking
variants, the best by the same rule; a tie goes to the one without a neural splitter. The
outcome is recorded (`owner-decision.json` shape of 016) and states what follows: the
demos' recipe and the documentation adopt the recommended variant in a later feature; the
shipped Wikipedia index is untouched by this study, and only a chonky variant beating
`contract` by the rule would open a separate feature to feed pre-split passages to the
Rust build.

**Independent Test**: `decide` reads the score files and prints the verdict with the
numbers; a unit test checks the rule on synthetic rows at the boundaries.

---

### Edge Cases

- A chunked query returns fewer than 100 distinct documents even at the study's depth:
  the run file holds what there is; Recall@100 counts it — the report states how often.
- A document whose title alone fills the window (the contract chunker's error case): the
  document is indexed whole for that variant and counted.
- chonky returns a non-partition: the build stops (as in 021).
- The harness check at k = 100 but the study at depth 300: the two configurations are
  kept apart in the file names and the table.
- FiQA's `whole` run is the anchor's third dataset and must reproduce too; the winner's
  FiQA run is optional by the rule and its absence is stated.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The study MUST run through the Python package with the pinned models, in a
  reference script with subcommands (build | search | score | table | decide) and a test
  suite for its pure parts, on the 014/016 pattern.
- **FR-002**: Documents MUST be shaped as the baselines': `contents` = title + " " + text
  (or passage), the title omitted when empty, one `contents` field under `standard_en` as
  the lexical and the dense field; the engine's defaults otherwise.
- **FR-003**: The harness check MUST index every document whole at k = 100 / candidate
  depth 100 and reproduce the committed `hybrid-baseline-v2` and `hybrid-rerank-v3`
  metrics exactly on SciFact, NFCorpus and FiQA before any variant runs.
- **FR-004**: The variants MUST be: `whole`; `contract` (the 008 contract chunker with
  budget 256 − title positions, priced by the embedder's tokenizer); `chonky` (the pinned
  splitter, unbounded); `chonky-bounded` (chonky, then chunks over the window split at
  blank lines and then at sentence ends until each part fits, and passages under 16
  positions merged into the preceding passage of the same document); and, by the owner's
  amendment, `chonky-if-long` (chonky for a document whose `title + " " + text` exceeds the
  window, the document whole otherwise).
- **FR-005**: Every passage MUST carry its document id; retrieval MUST be at k = 300
  passages with candidate depth 300, at re-rank depth 0 and 20 (α 0.5); aggregation MUST
  be MaxP (a document's score is its best passage's; order by first occurrence in the
  passage list), truncated to 100 documents; scoring MUST be pytrec_eval nDCG@10 and
  Recall@100 against the test qrels, as the baselines.
- **FR-006**: Every cell MUST be a committed TREC run file under the feature's `runs/` and
  its scores a committed JSON; the report MUST hold one table of all cells with deltas
  against `whole` at the same depth, and the `whole@100` vs `whole@300` difference.
- **FR-007**: Scope MUST be SciFact and NFCorpus for all four variants and FiQA for `whole`
  and the leading variant (if any); a two-way mean MUST be labelled as such.
- **FR-008**: The decision MUST follow the rule in User Story 4 exactly and be recorded
  with the numbers; the study MUST NOT change the engine, the FFI, any format, any
  baseline file or the shipped Wikipedia artefact.
- **FR-009**: Tests first, committed failing: the bounding and merging of chunks (windows,
  blank lines, sentence ends, the 16-position merge), MaxP aggregation (ties, truncation,
  first occurrence), run-file writing, the anchor comparison, the decision rule at its
  boundaries — on small fixtures; no CI job.
- **FR-010**: The expensive steps (chonky splitting and embedding, especially FiQA) MUST be
  resumable per (variant, dataset) — a finished index is reused, a finished run file is not
  recomputed.

### Key Entities

- **Passage**: document id, ordinal, text; from one of the four splitters.
- **Cell**: (variant, dataset, depth) → a run file (TREC) and a score file (nDCG@10,
  Recall@100, per-query values).
- **Anchor**: `whole@100` scores against the committed baselines.
- **Decision**: the rule's verdict per variant with the numbers; the owner's decision file.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `whole@100` reproduces the six published nDCG@10 figures and the Recall@100
  figures exactly on all three datasets.
- **SC-002**: Sixteen SciFact/NFCorpus cells plus the FiQA `whole` cells (and the winner's,
  if any) are committed with scores; one table in the report.
- **SC-003**: `chonky-bounded` has zero passages over the window and zero under 16 positions
  (except whole documents) — counted in the build record.
- **SC-004**: The decision file states the verdict by the rule with the means and per-dataset
  deltas; the rule's constants (0.005, 0.005, 0.005, 16) are literals in the script and the
  tests, unchanged from this spec.
- **SC-005**: `git diff --stat main -- crates/ swift/ python/src apps/ specs/*/baselines` is
  empty.

## Assumptions

- **Where the study runs**: the Wikipedia demo's environment (the wheel, chonky, torch,
  tokenizers) plus pytrec_eval — installed there for the study.
- **Cost**: per variant, SciFact ~15 min and NFCorpus ~12 min of embedding plus ~20 min of
  re-ranking each; FiQA ~2 h embedding plus ~45 min re-ranking per variant. The full
  SciFact/NFCorpus grid is ~5 h unattended; FiQA adds ~3 h per variant run.
- **The contract chunker's reference implementation** is `reference/gen_008_fixtures.py`'s
  (the goldens' source); the study imports its functions rather than copying them.
- **Recall@100 with fewer than 100 documents** is scored as it comes; the report states the
  share of queries affected per cell.
- **No branch or commit by the agent**; the owner creates `022-chunking-study` and commits
  at the checkpoints.
