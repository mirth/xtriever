# Feature Specification: Sparse Expansion Spike — SPLADE-style Terms in the Inverted Index, Measured in Python

**Feature Branch**: `012-sparse-spike`

**Created**: 2026-09-15

**Status**: Draft

**Input**: User description: "but let's perform some cheap (python?) experiments before it please" — before committing the engine to learned sparse retrieval (the plan agreed in conversation: an inference-free document encoder whose expansion terms live in the existing inverted index, scored either by BM25 or by a dot product, fused with the lexical and dense stages), measure on the three BEIR sets, in Python, what each variant is worth — and decide go / no-go on numbers.

## Why This Spec Reads Technically

This is a de-risking spike, like 001: the "user" is an Xtriever developer who has to decide
whether to spend a Rust feature (an encoder in the dense crate, a weighted field and scorer in
the lexical crate, a format bump, a Wikipedia rebuild of hours) on learned sparse retrieval —
and, if so, on which model and which scoring. The spike answers that with the same oracle the
engine is judged by (nDCG@10 and Recall@100 on SciFact, NFCorpus, FiQA, scored by the 003
reference), against the engine's *own* exported runs, so the numbers predict what the pipeline
would show. Nothing it produces ships: no crate changes, no format, no model in the repository
— scripts, pinned environments, cached encodings under `target/`, and a report with a decision.
Naming models, scoring variants and datasets is therefore the requirement, not leaked detail.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Each scoring variant gets a number on every dataset (Priority: P1)

The developer runs one script per step and gets, for each of the three BEIR sets, nDCG@10 and
Recall@100 for: the engine's BM25 (exported), the engine's dense stage (exported), the engine's
hybrid (exported), and the new variants — expansions scored as a **dot product** (the model's
own score), expansions scored **as BM25 over an expansion field**, and their **fusions** with
the engine's lexical and dense runs (RRF, the engine's k). Every number is scored by the 003
reference scorer so it is comparable, digit for digit, with the committed baselines.

**Why this priority**: This is the decision. Everything else supports it.

**Independent Test**: The report's table has all cells filled for all three datasets, each
reproducible by the documented commands from the cached encodings.

**Acceptance Scenarios**:

1. **Given** the three corpora and queries on disk and the pinned models fetched, **When** the
   encoding step runs, **Then** every document and query has a cached sparse vector, the
   cache is keyed by model identity, and a second run encodes nothing.
2. **Given** the cached vectors, **When** the scoring step runs, **Then** each variant produces
   a run in the engine's export shape (query id → ranked doc ids, depth 100) and the 003
   scorer reports nDCG@10 and Recall@100 for it, per query and mean.
3. **Given** the engine's exported lexical, dense and hybrid runs for the same datasets,
   **When** the fusion step runs, **Then** RRF of (lexical, sparse), (dense, sparse) and
   (lexical, dense, sparse) are scored the same way, and the engine's exported hybrid run
   re-scores to the committed baseline (the harness and the spike agree).
4. **Given** the table, **When** the decision rule below is applied, **Then** the report
   states go or no-go, and which model and scoring it recommends.

---

### User Story 2 - The costs are measured, not guessed (Priority: P1)

The developer learns what the engine would pay: document encoding throughput on this host
(passages per second, CPU), expansion size (non-zero terms per document, per query), the
loss from **quantising weights into integer term frequencies** (the way the inverted index
would hold them) at several scales, the query side's cost (tokens, and — for the
inference-free family — no model), and the projected index growth for the 428k-passage
Wikipedia corpus.

**Why this priority**: 013's plan needs these numbers to state its build time, index size and
quantisation scheme; the memory ceiling and the phone are why the inference-free family was
chosen and the spike must confirm the query side really is model-free.

**Independent Test**: The report has a costs table with each figure and the command that
produced it.

**Acceptance Scenarios**:

1. **Given** a corpus, **When** encoded, **Then** the report states passages/second, wall time,
   thread count, and the mean / p95 non-zeros per document and per query.
2. **Given** the dot-product variant, **When** weights are quantised to integers at scales
   (e.g. ×10, ×100, ×1000) before scoring, **Then** the nDCG@10 loss against the unquantised
   score is reported per scale, and the smallest scale within 0.1 nDCG points is named.
3. **Given** the per-document non-zero counts, **When** projected to 427,947 passages, **Then**
   the report gives the expected posting count and an on-disk size estimate with its method.

---

### User Story 3 - Models are compared on equal terms, with licences (Priority: P2)

The two candidate models — the inference-free document-only encoders, two generations — are
run through the same steps, each with its licence, size, and what it needs at query time
recorded; the ceiling a symmetric (query-time) encoder would add is quoted from published
results, not run. The recommendation names one model by its exact revision hash, as 004/006 pinned
theirs.

**Why this priority**: The choice is irreversible once a Wikipedia index is built on it (hours)
and once it ships on the phone (memory).

**Independent Test**: The report's model table has licence, parameter count, query-time
requirement, revision, and the per-dataset numbers for each candidate.

**Acceptance Scenarios**:

1. **Given** each candidate, **When** run, **Then** its numbers appear in the same table, on the
   same runs, with the same scorer.
2. **Given** a candidate whose licence forbids commercial use, **When** listed, **Then** it is
   marked ineligible for adoption regardless of its numbers (an MIT engine cannot pin it).

---

### Edge Cases

- A query that encodes to zero non-zero terms (all tokens unknown or pruned): the variant
  returns an empty ranking for it and the scorer counts it as such; the report states how many.
- A document longer than the encoder's window: truncated as the model card prescribes; the
  count of truncated documents is reported.
- Ties in the dot product: broken by ascending document id, as the engine does, so the run is
  deterministic.
- The engine's exported runs must exist for the same dataset snapshot (the pinned BEIR
  manifest): the script refuses a run whose query set differs from the dataset's.
- The host's GPU: not used unless stated; the throughput figure is CPU, thread count recorded.

## Requirements *(mandatory)*

### Functional Requirements

**Inputs and oracles**

- **FR-001**: The spike MUST use the repository's pinned BEIR datasets (SciFact, NFCorpus,
  FiQA) and score every run with the 003 reference scorer (`pytrec_eval`, the harness's
  conventions), so its numbers are comparable to the committed baselines.
- **FR-002**: The engine's own lexical, dense and hybrid runs MUST be exported from the
  existing harness (`--export-run`) for each dataset and re-scored by the spike; agreement with
  the committed baselines is a precondition for every other number.

**Encoding**

- **FR-003**: Candidate models MUST be fetched by pinned revision and verified (the
  repository's manifest pattern), never committed; encodings MUST be cached under `target/`
  keyed by model identity and corpus hash, resumable per shard.
- **FR-004**: Document encoding MUST follow each model card's recipe (masked-LM logits →
  `log(1 + ReLU)` → max over tokens, attention-masked; special tokens excluded as the card
  says); query encoding MUST follow the card's recipe — the inference-free family's
  tokenizer-plus-IDF weights with no model call, the symmetric reference's model call.

**Scoring variants**

- **FR-005**: The **dot-product** variant MUST rank documents by `Σ_t q_t · d_t` over the
  shared vocabulary, ties by ascending document id, depth 100.
- **FR-006**: The **BM25-over-expansions** variant MUST build a document field whose term
  frequencies are the quantised weights (`round(d_t · S)`, `S` stated), score it with BM25 at
  the engine's parameters (k1 1.2, b 0.75, the backend's defaults), with the query as its
  expansion terms (or its tokens, for the inference-free family) — and report it both alone
  and added as a boosted field to the text BM25 (boost stated).
- **FR-007**: **Fusion** MUST be reciprocal rank fusion with the engine's constant (k = 60),
  over the engine's exported runs plus the spike's, at depth 100, for the pairs and the triple
  named in US1.
- **FR-008**: **Quantisation** MUST be measured for the dot-product variant at scales ×10,
  ×100, ×1000 (integer term frequencies), reporting nDCG@10 loss per scale and per dataset.

**Costs**

- **FR-009**: The report MUST state, per model: document throughput (passages/s, CPU, thread
  count, wall time per corpus), mean and p95 non-zeros per document and per query, the count
  of truncated documents, model size on disk, parameter count, licence, revision hash, and
  what the query side needs at run time.
- **FR-010**: The report MUST project the index cost for the Wikipedia corpus (427,947
  passages): posting count from the mean non-zeros, and a bytes-on-disk estimate with the
  method stated (measured on FiQA's 57,638 documents and scaled).

**Decision**

- **FR-011**: The report MUST apply a decision rule stated **before** the runs: **go** if the
  recommended model's dot-product variant beats the engine's BM25 nDCG@10 on at least two of
  the three datasets **and** the three-way fusion beats the committed hybrid baseline's mean
  nDCG@10 across the three; otherwise **no-go** with the numbers. It MUST name the model
  (revision), the scoring (dot product, or BM25-over-expansions if within 0.5 nDCG points of
  it — cheaper to build), and the quantisation scale for 013.
- **FR-012**: Nothing under `crates/`, `swift/`, `apps/`, `python/` or `.github/` changes; the
  spike lives in `reference/` (scripts, pinned requirements, a model manifest) and
  `specs/012-sparse-spike/` (report, run summaries). CI is untouched (standing rule: no
  models, one BEIR set at most, and this runs nothing in CI).

### Key Entities

- **Candidate model**: identity (repository, revision hash, file hashes), licence, size,
  encoder recipe, query-side requirement.
- **Sparse vector**: a document's or query's `(term id, weight)` pairs over the model's
  vocabulary; cached per corpus and model.
- **Run**: query id → ranked document ids at depth 100, in the harness's export shape; scored
  by the 003 reference.
- **Variant**: dot product / BM25-over-expansions (alone, as boosted field) / fusions; each a
  run per dataset per model.
- **Decision record**: the table, the costs, the rule, the verdict, the pinned recommendation.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The engine's exported hybrid runs re-score in the spike to the committed
  baselines' nDCG@10 and Recall@100 exactly (all three datasets) — the two scorers agree.
- **SC-002**: For every candidate model and every dataset, the dot-product, the
  BM25-over-expansions and the fusion variants each have an nDCG@10 and a Recall@100 in the
  report, reproducible from the cache by one command each.
- **SC-003**: The quantisation table shows, per dataset, a scale at which the dot-product
  variant loses ≤ 0.1 nDCG@10 points against the unquantised score — or the report says none
  of the three does.
- **SC-004**: The costs table is complete (FR-009, FR-010) for every candidate, with encoding
  throughput measured on FiQA (the largest set) and stated per second.
- **SC-005**: The report states go / no-go by the rule of FR-011, with the model revision and
  scoring named, in one paragraph a reader of 013's spec can adopt verbatim.
- **SC-006**: The whole spike runs on this host in one unattended session per model — under
  3 hours wall time for the three corpora including encoding — and a second run reuses every
  encoding.

## Assumptions

- **Models** (owner decisions in conversation, 2026-09-15: inference-free family, expansions
  in the existing inverted index): the inference-free document encoders
  `opensearch-project/opensearch-neural-sparse-encoding-doc-v2-distill` and `…-doc-v3-distill`
  (Apache-2.0, DistilBERT-size) are the two candidates, and the only models run. A symmetric
  SPLADE is **not** run (Q1 = A): the ceiling a query-time model would add is quoted from the
  literature in the report, labelled as such, because it could not ship on the phone anyway.
  Non-commercial models (the naver `splade-*` family) are recorded as ineligible.
- **Datasets**: all three (Q2 = A) — SciFact 5,183 documents, NFCorpus 3,633, FiQA 57,638 —
  the oracle the engine is judged by; SciFact first as the smoke, FiQA last (the cost).
- **Compute**: CPU on this host (Apple silicon), PyTorch, thread count recorded; the GPU
  (MPS) may be used for encoding if it is faster, stated in the report — throughput is
  reported for the path used.
- **Environment**: `reference/.venv-012` with pinned `torch`, `transformers`, `numpy`,
  `pytrec_eval` (the 003/004 pins where they still resolve), and a BM25 implementation for
  FR-006 (a small pure-Python/NumPy one written for the spike, checked against the engine's
  exported BM25 run on the text field within the engine's tokenizer differences, or a pinned
  library if one matches).
- **RRF**: k = 60 (the engine's), rank-based, depth 100 in and out.
- **Not in scope**: any Rust change; the Wikipedia corpus (its cost is projected, not run);
  re-ranking on top of the sparse variants (the re-ranker's gain is known from 006 and
  independent of the candidate source).
