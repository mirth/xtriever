# Feature Specification: The Re-rank Stage

**Feature Branch**: `006-rerank-stage`

**Created**: 2026-09-13

**Status**: Draft

**Input**: User description: "Feature 006" — the re-ranker proposed at the close of Feature 005: implement `xtriever-core`'s `Reranker` in `xtriever-rerank` with a pinned cross-encoder, make the hybrid pipeline re-score its fused candidates under a budget with partial results kept, extend `explain()` with the re-rank score, and measure the re-ranked pipeline on the three benchmark datasets as the first delta against the guarded fused baseline.

## Why This Spec Reads Technically

As in Features 001–005 the "user" is an Xtriever developer and the deliverable is a named core
trait made true against a pinned model, plus the pipeline's use of it. The trait name, the
feature name the core already declares for this stage (`rerank.score`), the metric names and the
harness are requirement content fixed by the constitution and earlier specs. No crate API items
are cited; they belong in `plan.md` under Rule 1.

Two things make this feature different from the dense stage. First, it is the first stage whose
**output is optional per item**: the core contract says a passage may come back unscored when
the budget runs out, and the pipeline must keep a coherent ranking anyway — Principle VI's
partial-degradation case, which Feature 005 only exercised as all-or-nothing. Second, it is the
first feature whose ranking number is measured **as a delta against a guarded baseline**
(`hybrid-baseline-v1`, Feature 005) rather than as a new absolute number.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A query and a passage get the reference model's relevance score (Priority: P1)

A developer loads the pinned cross-encoder from a set of files whose identity is verified, hands
it a query and a list of passages, and gets one relevance score per passage that agrees with the
reference implementation within a stated tolerance and orders the passages the same way.

**Why this priority**: Nothing else in this feature means anything without it, and it reuses the
discipline Feature 004 established (pins, oracle, per-item determinism).

**Independent Test**: Testable on a host with the model files present and no index: score the
fixture query/passage pairs, compare against committed reference scores.

**Acceptance Scenarios**:

1. **Given** the pinned model files, **When** the re-ranker is loaded, **Then** it reports a
   model identity that names the repository, revision and weight hash, and loading files whose
   size or hash differs fails naming the file and both values.
2. **Given** the committed fixture pairs, **When** they are scored, **Then** every score is
   within the stated tolerance of the reference and the order of the passages for each query
   is identical to the reference's order.
3. **Given** the same pairs scored in one call, in several calls, or in a different order,
   **When** the scores are compared, **Then** each pair's score is bit-identical every time.
4. **Given** a passage longer than the model's input limit, **When** scored, **Then** it is
   truncated the way the reference truncates it, and the fixture includes such a passage.
5. **Given** an empty passage or an empty query, **When** scored, **Then** a score comes back
   (the reference's) rather than an error.

---

### User Story 2 - Scoring stops at the budget and says which passages it did not reach (Priority: P1)

A developer passes a budget with an item limit and/or a time limit. The re-ranker scores
passages in input order until the budget is spent and returns "not scored" for the rest —
never an error, never a guess.

**Why this priority**: This is the behaviour the core trait was designed around and the one that
makes the stage usable on a phone.

**Independent Test**: Testable with the model, or with the fixture scores replayed, using item
limits and a tiny time limit.

**Acceptance Scenarios**:

1. **Given** an item limit of *n* and more than *n* passages, **When** scored, **Then** exactly
   the first *n* passages carry scores and the rest are reported as not scored, in input order.
2. **Given** a time limit the scoring cannot finish within, **When** scored, **Then** the
   passages reached before the limit carry scores, the rest are not scored, and the count of
   scored passages is at least one whenever the first passage fits the limit.
3. **Given** no budget, **When** scored, **Then** every passage carries a score.
4. **Given** a budget of zero items, **When** scored, **Then** no passage is scored and no error
   is raised.

---

### User Story 3 - The pipeline re-ranks its fused candidates and keeps a coherent order under any budget (Priority: P1)

A developer opens a hybrid index with a re-ranker attached and searches; the top of the fused
list is re-scored by the cross-encoder and the response is ordered by the re-rank score, with
whatever the budget did not reach placed after the re-ranked part in its fused order. If the
re-ranker fails, the response is the fused ranking, marked as degraded; in strict mode the error
is returned.

**Why this priority**: This is the product change. P1 because Stories 1 and 2 are only useful
through it.

**Independent Test**: Testable with a stub re-ranker (fixed scores, a failing one, one that
scores only the first *n*) over the Feature 005 fixture index — no model needed.

**Acceptance Scenarios**:

1. **Given** a re-rank depth *d* and a fused list longer than *d*, **When** searched, **Then**
   the first *d* fused candidates are re-scored and returned ordered by re-rank score (ties by
   the deterministic tie-break), followed by the remaining fused candidates in fused order, and
   the total is at most `k`.
2. **Given** a budget that lets the re-ranker score only the first *m* < *d* candidates,
   **When** searched, **Then** the *m* scored candidates come first ordered by re-rank score,
   followed by every unscored candidate — the *d − m* the budget did not reach and the ones
   beyond *d* — in fused order; the response says that *m* were re-scored. One rule covers
   "not reached" and "not selected": whatever the re-ranker vouched for comes first, and no
   candidate disappears (a fused rank-1 candidate the budget did not reach lands at position
   *m + 1*).
3. **Given** a re-ranker that fails, **When** searched in the default mode, **Then** the fused
   ranking is returned, the response names the skipped stage and the reason, and no error is
   raised; in strict mode the error is returned unchanged.
4. **Given** a search with no re-ranker attached, **When** searched, **Then** the response is
   exactly Feature 005's.
5. **Given** the same index, query and budget, **When** searched twice, **Then** the responses
   are identical.
6. **Given** the dense stage already degraded (Feature 005), **When** a re-ranker is attached,
   **Then** the re-ranker still runs over the lexical list — degradation is per stage, not a
   cascade — and the response says which stages ran.

---

### User Story 4 - Every hit explains its re-rank score (Priority: P2)

A developer asks for an explanation and gets, on every hit, the re-rank score and 1-based
re-rank position where the stage scored it, marked absent where it did not, under the feature
name the core declares — in addition to Feature 005's stage explanation.

**Why this priority**: The LTR stage will consume exactly this value; the vocabulary must be
right now.

**Independent Test**: The Story 3 fixtures with a stub re-ranker.

**Acceptance Scenarios**:

1. **Given** an explained search with re-ranking, **When** a hit was re-scored, **Then** its
   explanation carries the re-rank score and position alongside the Feature 005 fields.
2. **Given** a hit the budget did not reach, **When** explained, **Then** its re-rank fields are
   absent, not zero, and its fused fields are present.
3. **Given** the same search without explanation, **When** compared, **Then** the hits and order
   are identical — explanation never changes ranking.

---

### User Story 5 - The re-ranked pipeline is measured as a delta against the guarded baseline (Priority: P2)

A developer runs the Feature 003 harness with a re-ranking configuration on the three benchmark
datasets, reusing the cached embeddings, and records nDCG@10 / Recall@100 with a delta table
against `hybrid-baseline-v1` — the first delta against a guarded number in this repository.

**Why this priority**: The whole point of a re-ranker is a better top-10; the constitution
demands the claim be measured, and the regression rule now has a number to defend.

**Independent Test**: Run the harness on SciFact with the re-rank configuration; the report
exists in the 003 format with the delta.

**Acceptance Scenarios**:

1. **Given** the harness and the re-rank configuration, **When** each dataset is evaluated,
   **Then** a report in the 003 format is produced with both model identities recorded and a
   delta table against `hybrid-baseline-v1`.
2. **Given** the cached dense embeddings, **When** the evaluation runs, **Then** no document is
   embedded; only queries and query–passage pairs are scored.
3. **Given** the three re-ranked results, **When** compared with the fused baseline on nDCG@10,
   **Then** they are not below it on at least two of the three datasets. A miss is a
   stop-and-report finding (Rule 6): investigated (re-rank depth, truncation, a defect),
   reported with the numbers, decided by the human — never a bar moved until it passes.
4. **Given** the re-ranked baseline, **When** re-run, **Then** the report is byte-identical.
5. **Given** the CI smoke, **When** considered, **Then** it stays the SciFact lexical smoke —
   no model runs in CI (standing rule).

---

### User Story 6 - Cost is observed, not assumed (Priority: P3)

A developer records how long re-ranking takes per query at the chosen depth on the largest
dataset, and how much memory the loaded re-ranker adds — as observations with method.

**Why this priority**: The re-ranker is the most expensive stage per query; its number decides
the on-device budget of the next feature.

**Independent Test**: The observations are present in the report with method.

**Acceptance Scenarios**:

1. **Given** the largest dataset, **When** evaluated, **Then** the wall time per query (total
   and re-rank part), the per-pair scoring time, and the peak memory of the process are recorded.
2. **Given** the loaded re-ranker, **When** measured in a fresh process, **Then** its memory is
   recorded per load path as Feature 004 did for the embedder.

---

### Edge Cases

- A passage with no text (the document's configured text fields were all empty).
- The re-ranker returns a list of the wrong length, or a non-finite score — a defect, reported
  as an error in every mode, never silently ordered.
- Re-rank depth larger than the fused list; depth of zero (re-ranking disabled for that call).
- `k` smaller than the re-rank depth (re-rank *d*, return *k*).
- A time budget spent before the first pair is scored.
- The re-ranker attached to an index whose text store is absent (built by Feature 005) —
  re-ranking has nothing to read.
- Two re-scored hits with identical scores.
- Explanation requested on a response whose re-ranker was skipped.

## Requirements *(mandatory)*

### Functional Requirements

**The re-ranker**

- **FR-001**: `xtriever-rerank` MUST provide a type implementing `xtriever-core`'s `Reranker` in
  full: `model_id`, `rerank`.
- **FR-002**: The pinned model MUST be `cross-encoder/ms-marco-MiniLM-L-6-v2` at a revision and
  file hashes recorded by this feature, with 32-bit weights and a maximum input of 512 model
  tokens for the query–passage pair; every one of those MUST be asserted at load from the
  files, never assumed.
- **FR-003**: Loading MUST verify each model file against a recorded size and content hash
  before use, failing with an error that names the file and both values (the 004 discipline),
  and MUST offer the same two load paths as the dense stage (safe by default, mapped opt-in)
  under the same conditions. *(Plan-time note, 2026-09-13: constitution v1.2.0 confined
  hand-written `unsafe` to `xtriever-dense`; the owner chose to amend it to v1.3.0 admitting the
  same block in `xtriever-rerank` — ADR-0009 — rather than ship buffered-only; research D3.)*
- **FR-004**: `model_id()` MUST be a stable string composed of the repository, revision, weight
  hash, the maximum input length, the weight precision and the inference engine version.
- **FR-005**: Scoring MUST be deterministic: the same query–passage pair MUST yield a
  bit-identical score regardless of the other passages in the call, their order, the call
  boundaries, or thread count (the Feature 004 rule, applied per pair).
- **FR-006**: Scores MUST agree with the reference implementation on the committed golden set
  within the stated tolerance, and the reference's order of passages per query MUST be
  reproduced exactly; the golden set MUST include an over-length passage, an empty passage, an
  empty query, and a set with a near-tie.
- **FR-007**: `rerank` MUST score passages in input order and stop when the budget's item
  limit is reached or its time limit is exceeded, returning "not scored" for every passage not
  reached; a zero item limit scores nothing; no budget scores everything. The time limit is
  measured by the re-ranker itself (this crate is a leaf crate and may read a clock).
- **FR-008**: A non-finite score from the model MUST be reported as an error, never returned.

**The pipeline**

- **FR-009**: A hybrid index MUST be able to carry an optional re-ranker; without one, every
  Feature 005 behaviour is unchanged (same responses, same explanations).
- **FR-010**: The pipeline MUST retain the passage text of every document (the same text the
  dense stage embedded) in its own versioned store, written at ingest for **every** hybrid
  index — whether or not a re-ranker is attached — and returned with hits. Hits are therefore
  self-describing for RAG callers, and any index can later gain a re-ranker without a rebuild;
  the cost is the corpus text stored once more (FiQA: ~48 MB beside a 103 MB index) on disk;
  a hit's text is read on demand, and the store is never held in memory whole.
- **FR-011**: `search` MUST accept a re-rank depth *d* (default 20; 0 disables) and re-score
  the first *d* fused candidates. The response is the scored candidates first, ordered by
  re-rank score (FR-012), then every unscored candidate in fused order — those the budget did
  not reach and those beyond *d* alike; the response states how many were scored; at most `k`
  are returned.
- **FR-012**: Re-scored hits MUST be ordered by re-rank score descending with ties broken by
  ascending internal id; the same index + query + options MUST yield identical responses.
- **FR-013**: The search budget MUST be passed through to the re-ranker (item limit capped at
  *d*; time limit shared with the whole call via the Feature 005 elapsed-time source, so the
  re-ranker receives the *remaining* time).
- **FR-014**: A re-ranker error MUST degrade the response to the fused ranking in the default
  mode, recorded on the response with the reason, and MUST be returned unchanged in strict
  mode; a wrong-length or non-finite result is a defect and an error in every mode.
- **FR-015**: Degradation MUST be per stage: a degraded dense stage does not skip the re-ranker,
  and the response MUST say which stages ran and which were skipped.
- **FR-016**: When explanation is requested, every hit MUST carry the re-rank score and 1-based
  re-rank position where scored, absent otherwise, under the core's `rerank.score` name (and a
  rank name this feature adds to the pipeline's explanation), alongside the Feature 005 fields;
  explanation MUST NOT change ranking.

**Evaluation**

- **FR-017**: The harness MUST gain a `hybrid-rerank-v1` configuration: `hybrid-baseline-v1`
  plus re-ranking at a stated depth, reusing the 004 embedding cache, scoring queries and
  query–passage pairs only.
- **FR-018**: The report MUST carry a delta table against `hybrid-baseline-v1` for each dataset
  and both model identities; the numbers MUST be verified through the 003 `--verify-run` path.
- **FR-019**: The re-ranked nDCG@10 MUST NOT be below `hybrid-baseline-v1` on the majority of
  the datasets; a miss stops the feature for a report and a human decision (Rule 6).
- **FR-020**: Observations (Story 6) MUST be recorded with method; the CI smoke stays
  SciFact-only and lexical.

**Constraints and scope**

- **FR-021**: `xtriever-rerank` is a leaf crate: it MAY carry native dependencies only behind
  non-default features enforced by `deny.toml`; the default feature set MUST stay free of C/C++.
  It MAY read a monotonic clock (Principle III names it among the leaf crates).
- **FR-022**: This feature MUST NOT modify `xtriever-core` traits, `xtriever-lexical`,
  `xtriever-dense`, the 003 metric/dataset layers, or `deny.toml`. Pipeline and harness changes
  are additive; the pipeline's on-disk format gains one file and a descriptor field under a
  bumped pipeline format version only if the change is not backward-compatible.
- **FR-023**: This feature MUST NOT implement LTR, an FFI surface, on-device packaging, query
  or passage batching in the model, or grouping of chunk hits.
- **FR-024**: Acceptance tests MUST be committed failing before implementation; model-backed
  tests MUST be separated from stub-driven tests so the offline suite runs with no model.
- **FR-025**: Cross-compilation for the iOS and Android targets MUST still be verified.

### Key Entities

- **Pinned Re-rank Model**: Repository, revision, file sizes and hashes, maximum input length,
  precision; a committed manifest with a fetch script (the 004 arrangement).
- **Model Identity**: The string derived from the pinned model and preprocessing; recorded in
  reports.
- **Re-rank Goldens**: Committed reference scores for a fixed set of query–passage pairs,
  including the named edge cases.
- **Passage Store**: The pipeline's per-document text, versioned on disk, keyed by internal id.
- **Re-rank Options**: Depth, and the Feature 005 options (budget, elapsed-time source, strict,
  explain).
- **Re-ranked Response**: Feature 005's response with per-hit re-rank fields and a stage report
  extended with the re-ranker's outcome (scored count, skipped reason).
- **Re-rank Evaluation Configuration**: `hybrid-baseline-v1` plus depth.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every golden pair scores within the tolerance (max absolute difference ≤ 1e-3 on
  the raw score) and every golden query's passage order is reproduced exactly — 100 % of cases
  including the over-length, empty-passage, empty-query and near-tie cases.
- **SC-002**: Scores are bit-identical across call composition, order and thread count — 0
  differing bits over the golden set in at least three arrangements and two thread counts.
- **SC-003**: Under an item limit *n*, exactly the first *n* passages are scored — asserted for
  *n* ∈ {0, 1, half, all}; under a time limit that cannot be met, at least one and fewer than all
  are scored.
- **SC-004**: With a stub re-ranker, the pipeline's re-ranked order equals the stub's order for
  the first *d* candidates followed by the fused order — 0 differences on every fixture query;
  with depth 0 or no re-ranker, responses equal Feature 005's byte for byte.
- **SC-005**: A failing re-ranker yields the fused ranking with the degraded marker in the
  default mode and the error in strict mode; a wrong-length result is an error in both.
- **SC-006**: Explanations carry re-rank fields exactly where the stub scored and never change
  ranking — 0 differences.
- **SC-007**: The re-ranked baseline exists for all three datasets, verified by `--verify-run`
  within 1e-6, with delta tables against `hybrid-baseline-v1`, and re-runs byte-identically.
- **SC-008**: Re-ranked nDCG@10 is not below `hybrid-baseline-v1` on at least 2 of 3 datasets (a
  miss is a reported finding, never a tuned bar).
- **SC-009**: The offline suite passes with no model; `xtriever-core`, `xtriever-lexical`,
  `xtriever-dense`, the 003 metric/dataset layers and `deny.toml` are unchanged; the re-rank
  crate checks on the three mobile targets with no C/C++ dependency in the default features.
- **SC-010**: Per-query and per-pair re-rank times, and the re-ranker's memory per load path
  in a fresh process, are recorded with method for the largest dataset.

## Assumptions

- **The model is `cross-encoder/ms-marco-MiniLM-L-6-v2`**: the same MiniLM family as the
  embedder (so the 004 findings about determinism and cost carry over), ~90 MB of 32-bit
  weights, trained for exactly this task. Its revision and hashes are measured by this feature.
- **The reference implementation is the model's own Hugging Face sequence-classification
  pipeline** under the 004 Python pins (`torch`, `transformers`, `tokenizers`), scoring the raw
  logit of the pair; tolerance 1e-3 absolute on the raw score (the 004 tolerance was 1e-3 on
  unit-vector components; raw logits have magnitude ~10, so this is proportionally stricter, and
  the 004 measurement of 2e-7 says it is comfortably achievable) plus exact order per query.
- **Each pair is scored alone at its own length** (query and passage tokenised together,
  truncated to 512, no padding to a fixed length): with one pair per forward pass, a pair's
  shape depends only on itself, so bit-identity holds without the fixed padding the batched
  embedder needed — and cost is proportional to length rather than always the maximum.
- **The re-rank input is the same passage text the dense stage embedded** (`title + " " +
  text` in the baseline), read from the pipeline's own store, never from the stages.
- **Re-rank depth defaults to 20**: the top-10 is what nDCG@10 measures and what a RAG caller
  consumes; re-scoring 20 gives the cross-encoder room to promote from just below the fold at a
  fifth of the cost of depth 100. The baseline records the depth; a deeper run is a
  configuration change measured on its own.
- **Recall@100 cannot change** when *d* ≤ 100 candidates are re-ordered among themselves; only
  nDCG@10 is expected to move.
- **Cost, for planning**: ~50–100 ms per pair on the development host at typical passage
  lengths ⇒ 1–2 s per query at depth 20 ⇒ the three-dataset baseline is on the order of 30–45
  minutes; the re-ranker's memory is another ~90 MB of tensors (the 004 finding: both load paths
  copy to the heap).
- **The 004 embedding cache is reused** as in 005; nothing is re-embedded.
- **Degrade-by-default, strict opt-in**, per stage, exactly as 005.
- **No public API stability is promised yet**; the FFI feature will shape it.
