# Feature Specification: The Hybrid Pipeline

**Feature Branch**: `005-hybrid-pipeline`

**Created**: 2026-09-13

**Status**: Draft

**Input**: User description: "Feature 005" — the pipeline proposed at the close of Feature 004: `xtriever-pipeline` orchestrates the lexical stage (002) and the dense stage (004) behind one synchronous API — ingest documents under external string ids, search with a query text and optional filter, fuse the two candidate lists into one ranking, degrade gracefully when the dense stage fails or exceeds its budget, explain every hit's per-stage scores — and records the fused BEIR baseline through the 003 harness as the first number the constitution's regression rule guards.

## Why This Spec Reads Technically

As in Features 001–004 the "user" is an Xtriever developer, and the deliverable is the first
component that is *not* a single core trait but the thing the traits exist for: the orchestration
the constitution names as the innovation budget ("pipeline orchestration, fusion"). The stage
names, the fused-score feature name the core already declares, the metric names and the harness
are requirement content fixed by the constitution and earlier specs. No crate API items are
cited; they belong in `plan.md` under Rule 1.

Three things make this feature different from the stage features. It is the first place external
string ids and internal `DocId`s meet (Principle V). It is the first place Principle VI's
"degrade, don't error" and Principle IV's `explain()` stop being N/A rows in a plan. And its
baseline is the first one the regression rule applies to: from this feature on, a change that
lowers the fused nDCG@10 on the majority of datasets needs an ADR.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Documents go in under their own ids and come back out under them (Priority: P1)

A developer creates a hybrid index in a directory, adds documents identified by external string
ids with their fields, commits, and later opens the same directory and finds the same documents.
The developer never sees an internal id, and the two stages underneath never see a string id.

**Why this priority**: Nothing else can be tested without it, and the id boundary is the
Principle V rule most easily broken by accident.

**Independent Test**: Testable with a handful of synthetic documents and no model — ingest,
commit, reopen, look up by external id, delete by external id.

**Acceptance Scenarios**:

1. **Given** an empty directory and a schema, **When** documents with external ids are added and
   committed, **Then** each is retrievable by its external id and the count of live documents
   equals the number of distinct ids added.
2. **Given** a committed index, **When** a document is added again under an existing external id,
   **Then** after commit it replaces the earlier one and the id appears once.
3. **Given** a committed index, **When** external ids are deleted and committed, **Then** they are
   gone from results and from the count; deleting an unknown id is not an error.
4. **Given** a committed index, **When** the directory is reopened, **Then** every external id
   resolves to the same document, and the stages report the same fingerprints and schema.
5. **Given** an index built with one embedder, **When** it is opened with a different embedder,
   **Then** opening fails naming both fingerprints — the pipeline surfaces the stage's refusal,
   it does not paper over it.
6. **Given** documents that are chunks of a larger source (chunk provenance present), **When**
   they are added, **Then** each chunk is its own indexed unit and its provenance is preserved
   for results.

---

### User Story 2 - One query, one fused ranking (Priority: P1)

A developer searches with a query text and gets back one ranked list of hits carrying external
ids, produced by running the lexical stage and the dense stage and fusing their candidate lists.
Hits are ordered by the fused score with ties broken deterministically. An optional metadata
filter applies to both stages.

**Why this priority**: This is the product. Fusion is the reason two stages exist.

**Independent Test**: Testable with synthetic documents whose lexical and dense results are
known (a stub embedder with hand-made vectors), against committed fusion goldens computed by an
independent reference implementation of the fusion rule.

**Acceptance Scenarios**:

1. **Given** a committed hybrid index, **When** a query is searched with `k`, **Then** at most `k`
   hits come back, each with an external id and a fused score, ordered by fused score descending
   and then by a deterministic tie-break, and the same query on the same index gives the same
   list every time.
2. **Given** the two stage candidate lists for a query, **When** they are fused, **Then** the fused
   order equals the reference implementation's order for every committed golden, including
   documents present in only one list.
3. **Given** a filter, **When** the query is searched with it, **Then** every hit satisfies the
   filter, both stages were restricted by the same resolved set before ranking, and the result
   is the fusion of the two restricted candidate lists; a stage's score for a document is the
   same with or without the filter. *(Corrected at implementation: the original wording "equals
   the unrestricted result filtered to that set" is false under rank fusion — removing
   candidates shifts ranks — and was never the intended contract; see report F-001.)*
4. **Given** a document that only the lexical stage retrieves (or only the dense stage), **When**
   fused, **Then** it still appears with the contribution of the one stage that found it.
5. **Given** chunked documents, **When** several chunks of one source are retrieved, **Then**
   each chunk is its own hit with its provenance (parent id, ordinal, byte range) attached;
   `k` counts chunks and the pipeline does no grouping — grouping by source is the caller's (or
   a later feature's) policy. The three BEIR datasets are unchunked, so the baseline is
   unaffected.
6. **Given** a query that neither stage matches, **When** searched, **Then** the result is empty,
   not an error.

---

### User Story 3 - A failing or slow dense stage degrades to lexical results (Priority: P2)

A developer whose embedder fails at query time (model error) or exceeds the budget still gets a
result: the lexical ranking, marked as degraded, with the reason available. A developer who opted
into strict mode gets the error instead.

**Why this priority**: The constitution's Principle VI, made real for the first time. P2 only
because Story 2 must exist to degrade from.

**Independent Test**: Testable with a stub embedder that fails, or that reports elapsed time
beyond the budget, against a stub clock; no model needed.

**Acceptance Scenarios**:

1. **Given** an embedder that returns an error for the query, **When** searched in the default
   (degrading) mode, **Then** the lexical results are returned, the response says the dense stage
   was skipped and why, and no error is raised.
2. **Given** the same failure in strict mode, **When** searched, **Then** the stage's error is
   returned unchanged.
3. **Given** a budget with a time limit and a caller-supplied monotonic time source, **When**
   the elapsed time read from that source exceeds the limit at a check point between stages
   (before the dense stage starts, and after it returns), **Then** the pipeline degrades exactly
   as for a failure, naming the budget. **Given** no time source, **When** a time limit is set,
   **Then** it is ignored and the response says so — only failures and item budgets degrade. The
   pipeline itself never reads a clock (the constitution forbids `Instant` in the pure crates
   because it does not exist on wasm32); hosts, the CLI and the FFI supply a real source, tests
   supply a stub.
4. **Given** a lexical stage failure, **When** searched in any mode, **Then** the error is
   returned — there is no earlier stage to degrade to, and the constitution's rule names ML
   stages only.
5. **Given** a degraded response, **When** explained, **Then** every hit shows its lexical score
   and rank and an absent dense contribution, and the response-level explanation names the
   skipped stage.

---

### User Story 4 - Every hit can explain itself (Priority: P2)

A developer asks for an explanation and gets, for every hit, the score and rank it had in each
stage that retrieved it (absent where a stage did not), the fused score, and the well-known
feature names the core declares for exactly these values.

**Why this priority**: Principle IV names `explain()` a first-class feature; the LTR stage later
consumes the same values as features, so getting the vocabulary right now is cheap and getting it
wrong later is not.

**Independent Test**: Testable with the Story 2 fixtures: the explanation of each hit must
reproduce the stage lists and the fused score exactly.

**Acceptance Scenarios**:

1. **Given** a search with explanation requested, **When** a hit was retrieved by both stages,
   **Then** its explanation carries both scores, both 1-based ranks and the fused score, under the
   core's well-known feature names.
2. **Given** a hit retrieved by one stage only, **When** explained, **Then** the other stage's
   score and rank are marked absent, not zero.
3. **Given** the same query without explanation requested, **When** searched, **Then** the hits
   and their order are identical to the explained search — explanation never changes ranking.

---

### User Story 5 - The fused baseline is measured and becomes the guarded number (Priority: P2)

A developer runs the Feature 003 harness with a hybrid configuration on the three benchmark
datasets, reusing the cached dense embeddings from Feature 004, and records the fused nDCG@10 and
Recall@100 alongside deltas against **both** stage baselines. From this feature on, the fused
number is what the constitution's regression rule guards.

**Why this priority**: The whole point of fusion is a better ranking than either stage; the
constitution demands the claim be measured.

**Independent Test**: Run the harness on SciFact with the hybrid configuration; the report exists
in the 003 format with the fused deltas and reproduces on re-run.

**Acceptance Scenarios**:

1. **Given** the harness and the hybrid configuration, **When** each of the three datasets is
   evaluated, **Then** a report in the Feature 003 format is produced with both stage
   fingerprints/identities recorded, and a delta table against the lexical and the dense
   baselines is produced for each.
2. **Given** the cached dense embeddings from Feature 004, **When** the hybrid evaluation runs,
   **Then** no document is re-embedded.
3. **Given** the three fused results, **When** compared with the better of the two stage
   baselines per dataset on nDCG@10, **Then** the fused number is not below it on at least two
   of the three datasets. A miss is a **stop-and-report finding** (Rule 6): investigated (fusion
   constant, candidate depth, a defect), reported with the numbers, and decided by the human —
   never a threshold tuned until it passes.
4. **Given** the fused baseline, **When** it is re-run, **Then** the report is byte-identical.
5. **Given** the fused baseline, **When** the SciFact CI smoke is considered, **Then** it stays
   the lexical smoke on SciFact only — the constitution's fixed set is a local Rule 5 obligation,
   and CI runs one dataset (standing rule since Feature 004).

---

### Edge Cases

- A query text that is empty or whitespace only (the lexical stage matches nothing; the dense
  stage embeds it anyway).
- `k` of zero; `k` larger than both candidate lists; candidate depth smaller than `k`.
- A filter that resolves to the empty set — both stages are skipped and the result is empty.
- A document present in the lexical index but missing from the dense index (or vice versa)
  after a partial failure between the two stages' commits.
- An external id that is the empty string, or that repeats within one `add` batch.
- Two hits with identical fused scores — the tie-break must be by external id or by the
  internal id, and stated.
- The dense stage returns an id the id map does not know (a stale or foreign index).
- Opening a directory written by a newer pipeline format version.
- Strict mode combined with a time budget of zero.
- Explanation requested on a degraded response.

## Requirements *(mandatory)*

### Functional Requirements

**Ingest and identity**

- **FR-001**: `xtriever-pipeline` MUST provide a hybrid index type that owns a lexical index, a
  dense (vector) index, an embedder and an id map, all living under one directory with a
  pipeline descriptor carrying a format version and the identities (schema, embedder
  fingerprint) it was created with.
- **FR-002**: Documents MUST be added under external string ids; the pipeline assigns internal
  ids and neither stage ever receives a string id. A repeated external id replaces the earlier
  document on commit; deleting an unknown id is a no-op.
- **FR-003**: The id map MUST be persisted with the index, versioned, and reloaded on open;
  after reopening, every external id resolves to the same internal id and back.
- **FR-004**: On ingest the pipeline MUST embed each document's configured text fields (joined in
  a stated way) with the embedder as a passage and add the vector to the dense index, and add the
  document to the lexical index; `commit` MUST commit both stages and the id map so that a
  successful commit leaves all three consistent.
- **FR-005**: A commit that fails part-way MUST be detectable on the next open (the descriptor
  records what was committed) and MUST NOT silently serve a half-committed generation.
- **FR-006**: Opening MUST verify that the stages' identities match the descriptor (schema,
  embedder fingerprint, dense format) and fail naming both sides on any mismatch.
- **FR-007**: Chunk provenance on a document MUST be preserved and returned with hits.

**Search and fusion**

- **FR-008**: `search(query, filter, k, options)` MUST run the lexical stage with the query
  text and the dense stage with the query embedding, each to a configurable candidate depth
  (default 100), and fuse the two lists into one ranking of at most `k` hits carrying external
  ids.
- **FR-009**: The fusion rule MUST be reciprocal rank fusion with a stated constant (default 60),
  the rule the core's well-known `fused.score` feature already names; a document in one list only
  contributes its one reciprocal rank. Any other fusion method is a later, delta-gated change.
- **FR-010**: Fusion MUST be verified against an independent reference implementation through
  committed goldens covering: both lists, one list only, disjoint lists, identical lists, ties in
  fused score, and candidate depth smaller than `k`.
- **FR-011**: Hits MUST be ordered by fused score descending, ties broken by ascending internal
  id (the constitution's rule), and the same index + query + configuration MUST yield identical
  results across calls and reopening.
- **FR-012**: A filter MUST be resolved once by the lexical stage and applied to both stages as
  the same allowed set; an empty resolved set MUST short-circuit to an empty result.
- **FR-013**: `k == 0` MUST return an empty result; an empty or whitespace query MUST NOT error.

**Degradation and budgets**

- **FR-014**: By default, a failure of the embedder at query time or of the dense search MUST
  degrade the response to the lexical ranking (lexical scores, lexical order) and record the
  skipped stage and the reason on the response; no error is raised.
- **FR-015**: In strict mode the same failure MUST be returned as the stage's error, unchanged.
- **FR-016**: A budget with an item limit MUST cap the dense candidate depth. A budget with a
  time limit MUST be honoured through an optional caller-supplied monotonic time source
  ("elapsed since the call started"), checked between stages — never inside one — and
  degrading exactly as a failure does when exceeded; with no time source the time limit is
  ignored and the response records that it was. The pipeline MUST NOT read a clock itself.
- **FR-017**: A lexical stage failure MUST be an error in every mode.
- **FR-018**: A degraded response MUST be distinguishable from a normal one by the caller
  without parsing text.

**Explanation**

- **FR-019**: When explanation is requested, every hit MUST carry: lexical score and 1-based
  rank (absent if not retrieved lexically), dense score and rank (absent likewise), and the fused
  score — under the core's well-known feature names (`bm25.score`, `bm25.rank`, `dense.score`,
  `dense.rank`, `fused.score`).
- **FR-020**: Requesting an explanation MUST NOT change hits or their order.
- **FR-021**: The response MUST also carry a stage-level explanation: which stages ran, their
  candidate counts, and which were skipped and why.

**Evaluation**

- **FR-022**: The Feature 003 harness MUST gain a hybrid configuration that builds a hybrid index
  from a dataset, reusing Feature 004's cached corpus embeddings when their key matches, and
  scores the fused ranking in the 003 report format with both stage identities recorded.
- **FR-023**: The report MUST carry delta tables against the lexical and the dense baselines for
  each dataset, and the fused numbers MUST be verified through the 003 `--verify-run` path.
- **FR-024**: From this feature on, the fused baseline is the number the regression rule guards.
  The spec's expectation of it: fused nDCG@10 MUST NOT be below the better single-stage baseline
  on the majority (at least two of three) of the datasets; a miss stops the feature for a report
  and a human decision (Rule 6) and is never resolved by adjusting the bar.
- **FR-025**: The CI smoke stays SciFact-only and lexical; no model runs in CI (standing rule).

**Constraints and scope**

- **FR-026**: `xtriever-pipeline` MUST remain pure and `std`-only: no C/C++, no async, no threads
  of its own, no wall-clock reads in library code. The lexical and dense crates are its
  dependencies; it depends on nothing above it.
- **FR-027**: This feature MUST NOT modify `xtriever-core` traits, `xtriever-lexical`,
  `xtriever-dense`, the 003 metric/dataset layers, or `deny.toml`. Harness changes are additive.
- **FR-028**: This feature MUST NOT implement re-ranking, LTR, grouping of chunk hits by source,
  an FFI surface, or on-device packaging.
- **FR-029**: Acceptance tests MUST be committed failing before implementation; tests needing the
  model MUST be separated from tests using stub stages, so the offline suite runs with no model.
- **FR-030**: Cross-compilation for the iOS and Android targets MUST still be verified.

### Key Entities

- **Hybrid Index**: A directory holding the pipeline descriptor, the id map, the lexical index
  and the dense index; created with a schema, a dense field selection and an embedder.
- **Pipeline Descriptor**: Format version, schema, embedder fingerprint, dense field selection,
  candidate depth, fusion constant, and the last fully committed generation marker.
- **Id Map**: The bijection between external string ids and internal ids, persisted and
  versioned; live ids only after deletes.
- **Search Options**: Candidate depth per stage, strict mode, budget, optional monotonic time
  source, explanation requested.
- **Hybrid Hit**: External id, fused score, optional chunk provenance, optional explanation.
- **Response**: The hits plus the stage-level explanation and the degradation marker.
- **Fusion Goldens**: Committed reference fusions of hand-made candidate lists.
- **Hybrid Evaluation Configuration**: The 003/004 recipes combined — lexical fields and boosts,
  dense passage construction, candidate depth, `k`, fusion constant.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every fusion golden matches the reference exactly in ids and order, with fused
  scores within 1e-9 — 100 % of cases including every tie and every one-list-only case.
- **SC-002**: Round-trip: 1,000 synthetic documents added, committed, reopened — every external
  id resolves to the same document; replace and delete leave exactly the expected live set (0
  discrepancies).
- **SC-003**: Two searches of the same query on the same index, before and after reopening, give
  identical hit lists — 0 differences — for every golden query.
- **SC-004**: A filtered search equals the fusion of the two stages each restricted to the
  resolved set, every hit is in the set, and no stage score changes under the filter — 0
  differences on every golden filter. *(Corrected at implementation, see report F-001.)*
- **SC-005**: A failing dense stage yields the lexical ranking with the degraded marker set in
  the default mode and the stage error in strict mode — both asserted in tests; a lexical failure
  is an error in both.
- **SC-006**: Explanations reproduce the stage scores and ranks for every hit in every golden
  query and never change the ranking — 0 differences.
- **SC-007**: The hybrid baseline exists for all three datasets, verified by `--verify-run`
  within 1e-6, with delta tables against both stage baselines, and re-runs byte-identically.
- **SC-008**: The hybrid evaluation re-embeds 0 documents when the 004 cache is present.
- **SC-011**: Fused nDCG@10 is not below the better single-stage baseline on at least 2 of the 3
  datasets (a miss is a reported finding, not a tuned threshold).
- **SC-009**: The offline suite passes with no model present; `xtriever-core`, `xtriever-lexical`,
  `xtriever-dense`, the 003 metric/dataset layers and `deny.toml` are unchanged; the pipeline
  crate checks on the three mobile targets with no C/C++ dependency.
- **SC-010**: Ingest of the largest dataset through the pipeline (with cached embeddings)
  completes, and the wall time and peak memory are recorded with method.

## Assumptions

- **Fusion is reciprocal rank fusion with constant 60** (the core's `fused.score` is documented
  as the RRF score; 60 is the constant of the original formulation). Score-based fusion needs
  score normalisation across two incomparable scales and is a separate, delta-gated decision.
- **Candidate depth defaults to 100 per stage** — Recall@100 is the metric, so `k ≤ 100` covers
  the harness; the caller can raise it.
- **The tie-break is ascending internal id**, which is ingestion order — stated so it is
  reproducible; external-id ordering would make ranking depend on string comparison of
  caller-chosen ids.
- **The dense passage text is the configured text fields joined by a single space in schema
  order**, matching Feature 004's baseline recipe when the fields are `title`, `text`.
- **The lexical query is `Match` over all text fields** with the schema's boosts, as in the 003
  baseline; richer lexical queries are passed through unchanged when the caller supplies one.
- **Degrade-by-default, strict opt-in**, exactly the constitution's wording.
- **The id map is stored by the pipeline, not by either stage**: the lexical index keeps only
  internal ids (Feature 002's hidden id column), the dense index only internal ids (004).
- **The reference implementation for fusion is a short independent Python script** over
  hand-made candidate lists; RRF has no floating-point subtlety worth a tolerance beyond 1e-9.
- **The three BEIR datasets are unchunked**, so Story 2 scenario 5's decision does not change the
  baseline numbers.
- **Feature 004's embedding cache is reused as-is** (same key: configuration, dataset, fingerprint,
  corpus hash) so the hybrid baseline costs minutes, not hours, on a machine that has run 004.
- **No public API stability is promised yet**; the FFI feature will shape the final surface.
