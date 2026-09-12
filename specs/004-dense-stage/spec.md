# Feature Specification: The Dense Stage

**Feature Branch**: `004-dense-stage`

**Created**: 2026-09-12

**Status**: Draft

**Input**: User description: "Feature 004" — the dense stage proposed at the close of Feature 003: implement `xtriever-core`'s `Embedder` and `VectorIndex` in `xtriever-dense`, productionising what Feature 001 proved on device (candle 0.9.2, `all-MiniLM-L6-v2` pinned by revision and SHA-256, tokenization and embedding oracles), so that a corpus can be embedded, stored with its embedder fingerprint, and searched exactly by similarity with the same determinism guarantees the lexical stage gives; record the dense stage's absolute BEIR baseline through the Feature 003 harness under the now-real eval gate.

## Why This Spec Reads Technically

As in Features 001–003, the "user" is an Xtriever developer and the deliverable is two named core
traits made true — `Embedder` and `VectorIndex` — against a pinned model. Those names, the model's
identity, the metric names and the harness are requirement content the constitution and earlier
specs already fix. No crate API items are cited; they belong in `plan.md` under Rule 1.

Two things make this feature different from the lexical stage. First, it is the first stage whose
correctness depends on **floating-point numerics** rather than on exact terms: the oracle is a
tolerance, and determinism has to be engineered rather than inherited. Second, it is the first
stage that runs a **model**, so it is where the constitution's memory budget stops being an
extrapolation and starts being a number this feature has to observe.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Text becomes vectors that match the reference model (Priority: P1)

A developer loads the pinned embedding model from a set of files whose identity is verified, hands
it a batch of texts, and gets back one vector per text — of the promised dimensionality, normalised
as the model card specifies, and agreeing with the reference implementation within the tolerance
Feature 001 established.

**Why this priority**: Nothing else in this feature means anything without it. It is also the
piece Feature 001 already proved feasible; the work here is turning a measurement into a contract.

**Independent Test**: Testable on a host with the model files present and no index at all: embed
the fixture sentences, compare against committed reference vectors.

**Acceptance Scenarios**:

1. **Given** the pinned model files, **When** the embedder is loaded, **Then** it reports the
   expected dimensionality, metric and a fingerprint that identifies the model revision, weight
   hash, pooling, normalisation and maximum input length.
2. **Given** model files whose size or hash differs from the pins, **When** loading is attempted,
   **Then** it fails naming the file and both hashes — never a warning, never a silent load.
3. **Given** the fixture sentences with committed reference vectors, **When** they are embedded,
   **Then** every vector has the promised dimensionality, unit length within the stated tolerance,
   and cosine similarity to its reference of at least the stated minimum.
4. **Given** the same texts embedded twice — in one batch, in two batches, in a different order —
   **When** the vectors are compared, **Then** each text's vector is bit-identical every time.
5. **Given** a text longer than the model's maximum input length, **When** it is embedded,
   **Then** it is truncated the way the reference implementation truncates it, and the golden set
   includes such a text.
6. **Given** an empty text, **When** it is embedded, **Then** the result is the reference's
   result for an empty input, and the golden set includes it.
7. **Given** a batch of texts, **When** embedded as queries and as passages, **Then** the results
   are identical, because the pinned model is symmetric — recorded so that a future asymmetric
   model's prefixes are a visible change, not a silent one.

---

### User Story 2 - Vectors are stored, found exactly, and survive reopening (Priority: P1)

A developer adds vectors under document ids, commits, and searches with a query vector; the
top-`k` come back highest similarity first, ties by ascending id, restricted to an allowed set when
one is given — exactly, not approximately. The index is written to disk with the embedder's
fingerprint and a format version, and reopening it with a different embedder is refused.

**Why this priority**: P1 alongside Story 1 because a vector index that returns approximately the
right documents cannot be verified against an oracle, and one that forgets which embedder produced
it will silently mix incompatible vectors — the exact failure Principle VI names.

**Independent Test**: Testable with synthetic vectors and no model: committed goldens generated
by an independent exact search over the same vectors.

**Acceptance Scenarios**:

1. **Given** a committed set of vectors and query vectors with independently computed exact
   top-`k` results, **When** the index is searched, **Then** ids and order match exactly and
   scores agree within the stated tolerance.
2. **Given** two vectors with identical similarity to a query, **When** they compete for a rank —
   including the `k`-th rank — **Then** the lower id wins, in every storage layout and across
   reopening.
3. **Given** an allowed set, **When** the index is searched with it, **Then** every hit is in the
   set and the hits are exactly the unrestricted results filtered to the set.
4. **Given** a vector added under an id that already exists, **When** committed, **Then** the
   new vector replaces the old one and the id appears once.
5. **Given** deleted ids, **When** searched after commit, **Then** they never appear and are not
   counted by the size.
6. **Given** a committed index on disk, **When** it is reopened, **Then** it exposes exactly the
   committed vectors and searching it gives the same results as before closing.
7. **Given** an index written with one embedder fingerprint, **When** it is opened alongside an
   embedder with a different fingerprint, **Then** the mismatch is a hard error naming both
   fingerprints.
8. **Given** a query vector of the wrong dimensionality, **When** searched or added, **Then** the
   call fails naming both dimensions.

---

### User Story 3 - The dense stage's baseline is measured through the eval harness (Priority: P2)

A developer runs the Feature 003 harness with a dense configuration: the corpus is embedded once
(and cached, keyed by the embedder fingerprint and the dataset hash), each query is embedded and
searched, and nDCG@10 / Recall@100 are recorded as the dense stage's absolute baseline.

**Why this priority**: The eval gate ADR-0006 restored is now real, and this is the first ranking
stage to land under it. Its number is an absolute baseline for a *new* stage — like the lexical
baseline was — not a delta against the lexical stage: a dense score below the lexical score is not
a regression, and the report must say so, because the number that governs the constitution's
regression rule will be the fused pipeline's (Feature 005).

**Independent Test**: Run the harness on SciFact with the dense configuration; the report exists
with every required field and reproduces on re-run.

**Acceptance Scenarios**:

1. **Given** the harness and the dense configuration, **When** SciFact is evaluated, **Then** a
   report in the Feature 003 format is produced with the embedder fingerprint recorded alongside
   the dataset hashes.
2. **Given** a corpus already embedded and cached, **When** the evaluation is re-run, **Then** no
   document is re-embedded, and the report is byte-identical to the first run.
3. **Given** the cached embeddings, **When** the embedder fingerprint or the dataset hash differs,
   **Then** the cache is not used.
4. **Given** the dense report, **When** it is read, **Then** it states that the number is an
   absolute baseline for a new stage and is not compared against the lexical baseline for the
   regression rule.
5. **Given** the three constitution datasets, **When** the dense baseline is produced, **Then**
   SciFact, NFCorpus **and** FiQA each have a report, with FiQA's corpus embedded once and its
   cache reused for every later run on that machine (FR-020); the roughly one hour of CPU that
   FiQA's 57,638 documents cost is paid by whoever produces the baseline and recorded in the
   report, not budgeted.

---

### User Story 4 - Memory and time are observed, not assumed (Priority: P3)

A developer running the dense stage on the largest evaluated corpus records how much memory the
loaded model and the index occupy, and how long embedding and search take — as observations with
their method, the way Feature 003 did for the lexical index.

**Why this priority**: P3 because no threshold applies yet, but this is the feature that turns the
constitution's "300 MB for a 100k-chunk index including loaded models" from an extrapolation into
a curve with two real points (lexical from 003, dense from here).

**Independent Test**: The observations are present in the report with units and method.

**Acceptance Scenarios**:

1. **Given** the largest evaluated corpus, **When** the index is built, **Then** the on-disk size
   of the vector index and the peak resident memory of the process are recorded with the method.
2. **Given** the model, **When** it is loaded through each of the two load paths (FR-008), **Then**
   the memory attributable to the loaded weights is recorded per path, with the difference
   between them.
3. **Given** the recorded observations, **When** extrapolated to the constitution's 100k-chunk
   configuration, **Then** the extrapolation is labelled as such and the assumptions listed.

---

### Edge Cases

- A text that tokenises to nothing (only whitespace or only characters the tokenizer drops).
- A batch of zero texts.
- Vectors containing non-finite values (NaN, ±∞) offered to the index.
- A query vector that is not unit length when the metric assumes normalised inputs.
- `k` of zero; `k` larger than the number of live vectors.
- An allowed set that is empty, or that contains ids the index has never seen.
- An index directory written by a different format version, or by a different dimensionality.
- Two handles on the same index directory, one writing.
- The model files present but the tokenizer file missing, or vice versa.
- Adding a vector before any commit, then reopening without committing — the vector must be gone.
- Exactly tied scores at the very last rank when `k` equals the number of live vectors.

## Requirements *(mandatory)*

### Functional Requirements

**The embedder**

- **FR-001**: `xtriever-dense` MUST provide a type implementing `xtriever-core`'s `Embedder` in
  full: `dim`, `metric`, `fingerprint`, `max_input_tokens`, `embed`.
- **FR-002**: The pinned model MUST be `sentence-transformers/all-MiniLM-L6-v2` at the revision
  and weight hash Feature 001 recorded, with 384 dimensions, attention-mask-weighted mean pooling,
  L2 normalisation, a maximum input length of 256 model tokens, and 32-bit weights. Every one of
  those MUST be asserted at load, never assumed.
- **FR-003**: Loading MUST verify each model file against a recorded size and content hash before
  use, failing with an error that names the file and both hashes (the Feature 001 and 003
  discipline).
- **FR-004**: `fingerprint()` MUST be a stable string composed of the model identity (repository,
  revision, weight hash), the pooling, the normalisation, the maximum input length and the weight
  precision — every input whose change would change the vectors. Two embedders with the same
  fingerprint MUST produce bit-identical vectors for the same text.
- **FR-005**: `embed` MUST be deterministic: the same text MUST yield a bit-identical vector
  regardless of batch composition, batch size, order within the batch, or thread count.
- **FR-006**: `embed` MUST agree with the reference implementation on the committed golden set
  within the Feature 001 tolerance (cosine ≥ 0.9999, max absolute difference ≤ 1e-3); the golden
  set MUST include a text longer than the maximum input length, an empty text, and a text with
  characters outside the tokenizer's vocabulary.
- **FR-007**: `embed` with `TextKind::Query` and `TextKind::Passage` MUST produce identical vectors
  for this model, and the fingerprint MUST encode that no prefixes are applied, so a future
  asymmetric model is a fingerprint change.
- **FR-008**: Both weight load paths MUST exist, selected by a Cargo feature of `xtriever-dense`:
  the default is safe buffered loading (the whole file read into memory, no `unsafe`); an opt-in
  `mmap` feature memory-maps the weights. The `mmap` path is admitted only under a new ADR that
  names the one `unsafe` block, its `// SAFETY:` argument, and how the Principle VII wording ("SIMD
  kernels only") is reconciled — ADR-scoped exception or a constitution amendment is the plan's
  decision to put to the human. Both paths MUST produce bit-identical vectors for the golden set,
  the `mmap` path MUST be tested in the gate, and the reported model memory MUST be measured for
  each. Rationale: Feature 001 measured 101 MB → 2.6 MB resident from mapping the weights, but
  ADR-0002's permission "expires with the spike … a production `xtriever-dense` … needs its own
  ADR"; a default consumer pays nothing in `unsafe`, and an on-device consumer opts in.

**The vector index**

- **FR-009**: `xtriever-dense` MUST provide a type implementing `VectorIndex` in full: `dim`,
  `metric`, `fingerprint`, `add`, `delete`, `commit`, `search`, `len`.
- **FR-010**: `search` MUST be **exact**: the top-`k` by the index's metric over all live vectors,
  with no approximation. An approximate structure is a later feature and requires the measured
  justification Principle I demands.
- **FR-011**: `search` MUST return hits ordered by descending score with ties broken by ascending
  `DocId`, **including at the `k`-th rank**, in every storage layout and across reopening — the
  obligation ADR-0005's resolution places on `VectorIndex` implementations.
- **FR-012**: Scores MUST be deterministic: the same index and query MUST give bit-identical
  scores across calls, reopening, and thread counts; the reduction order MUST be fixed.
- **FR-013**: `search` with an `allowed` set MUST return exactly the unrestricted results filtered
  to the set, with unchanged scores.
- **FR-014**: `add` MUST replace an existing vector with the same `DocId`; `delete` MUST ignore
  unknown ids; neither MUST be visible until `commit`; dropping without commit MUST discard
  (the Feature 002 semantics, applied to vectors).
- **FR-015**: The index MUST be persisted on disk with a format version and the embedder
  fingerprint it was built for. Opening with a mismatched version MUST fail; opening alongside an
  embedder whose fingerprint differs MUST fail naming both fingerprints; a dimensionality mismatch
  on `add` or `search` MUST fail naming both dimensions.
- **FR-016**: Vectors MUST be stored at the precision the model produces (32-bit); quantisation is
  a later, delta-gated feature.
- **FR-017**: Non-finite components in an added or query vector MUST be rejected, never stored or
  compared.
- **FR-018**: The vector index's search MUST be verified against an independent exact
  implementation through committed goldens generated by a script in `reference/`: ids and order
  exact, scores within the stated tolerance, on vector sets that include exact ties and an
  `allowed` restriction.

**Evaluation and observation**

- **FR-019**: The Feature 003 harness MUST gain a dense evaluation configuration that embeds the
  corpus and queries with the `Embedder` and searches the `VectorIndex`, producing a report in the
  Feature 003 format with the embedder fingerprint recorded.
- **FR-020**: Corpus embeddings MUST be cached on disk keyed by the embedder fingerprint and the
  dataset's file hashes, so a re-run embeds nothing; any key mismatch MUST bypass the cache.
- **FR-021**: The dense baseline report MUST state that it is an absolute baseline for a new stage
  and MUST NOT be presented as a delta against the lexical baseline; the constitution's
  regression rule applies to the pipeline's number once Feature 005 exists.
- **FR-022**: The dense baseline MUST be verified against the reference metric implementation
  through the Feature 003 `--verify-run` path.
- **FR-023**: For the largest evaluated corpus, the report MUST record the on-disk vector index
  size, the peak resident memory of the evaluating process, the wall time to embed the corpus and
  to answer all queries, and the memory attributable to the loaded model under each load path
  (FR-008) — as observations with their method, with no threshold attached.

**Constraints and scope**

- **FR-024**: `xtriever-dense` is a leaf crate: it MAY carry native dependencies only behind
  non-default features enforced by `deny.toml`; the default feature set MUST stay free of C/C++
  dependencies, as Feature 001 measured for the pinned inference engine.
- **FR-025**: This feature MUST NOT modify `xtriever-core`, `xtriever-lexical`, `xtriever-eval`'s
  metric or dataset layers, or `deny.toml`. The harness extension is additive.
- **FR-026**: Model files MUST NOT be committed; they are fetched by a committed script into a
  git-ignored cache and pinned by hash (the Feature 001 arrangement, made a first-class script).
- **FR-027**: This feature MUST NOT implement fusion, the pipeline, re-ranking, LTR, an
  approximate index, quantisation, SIMD kernels, or any on-device packaging; it MUST NOT extend the
  provisional Feature 001 FFI surface.
- **FR-028**: Acceptance tests MUST be committed failing before implementation, failing for want of
  an implementation. Tests needing the model files MUST be separated from tests that do not, so the
  offline suite (vector index goldens, persistence, fingerprint checks) runs with no model present.
- **FR-029**: Cross-compilation for the iOS and Android targets MUST still be verified for the
  default feature set, as Feature 001 proved feasible; nothing runs on hardware.

### Key Entities

- **Pinned Model**: Repository, revision, weight file size and hash, tokenizer and config files
  with their hashes, dimensionality 384, pooling, normalisation, maximum length, precision. The
  values are Feature 001's; this feature makes them a committed manifest with a fetch script.
- **Fingerprint**: The string derived from the Pinned Model plus preprocessing choices; the
  identity every vector index carries.
- **Embedding Goldens**: Committed reference vectors for a fixed set of texts, generated by the
  reference implementation Feature 001 pinned, including the edge-case texts FR-006 lists.
- **Vector Index**: A committed, versioned on-disk store of `(DocId, vector)` pairs plus the
  fingerprint, dimensionality and metric; exact search over it.
- **Search Goldens**: Synthetic vector sets and queries with independently computed exact top-`k`,
  including ties and `allowed` restrictions.
- **Dense Evaluation Configuration**: The recipe for Story 3 — which document fields are
  concatenated into the passage text, the truncation, `k`, and the embedder fingerprint.
- **Embedding Cache**: Per dataset, the corpus embeddings keyed by fingerprint and dataset hashes,
  with its own integrity check.
- **Observations**: Index size, peak memory, model memory, embedding and query wall times — with
  method.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every golden text embeds within the tolerance: 100 % of the golden set at cosine ≥
  0.9999 and max abs diff ≤ 1e-3, including the over-length, empty and out-of-vocabulary texts.
- **SC-002**: Embedding is bit-identical across batch composition, batch order and batch size —
  0 differing bits over the golden set in at least three arrangements.
- **SC-003**: Every search golden matches exactly in ids and order — 100 % of cases, including
  every tie case and every `allowed` case — with scores within tolerance.
- **SC-004**: A committed index reopened from disk returns bit-identical results to the pre-close
  results for every golden query — 0 differences.
- **SC-005**: A fingerprint mismatch, a format-version mismatch and a dimensionality mismatch are
  each rejected by a named error in a test — 3 of 3.
- **SC-006**: The dense baseline report exists for all three datasets (SciFact, NFCorpus,
  FiQA), in the Feature 003 format, with the fingerprint recorded, and re-running it with the
  cache warm reproduces it byte-for-byte and embeds 0 documents.
- **SC-007**: The dense baseline agrees with the reference metric implementation via
  `--verify-run` within 1e-6.
- **SC-008**: The observations in FR-023 are present with units and method; the model-memory
  figure is measured, not copied from Feature 001.
- **SC-009**: The offline suite passes with no model files present; `xtriever-core`,
  `xtriever-lexical`, the 003 metric/dataset layers and `deny.toml` are unchanged; the default
  feature set checks on the three mobile targets with no C/C++ dependency.
- **SC-010**: Two embedders loaded from the same pinned files report the same fingerprint and
  produce bit-identical vectors — 0 differing bits.

## Assumptions

- **The inference engine and its pin are Feature 001's** (candle 0.9.2 under ADR-0001; the
  `onig_sys` ban and the RUSTSEC-2024-0436 ignore under ADR-0004 remain exactly as they are).
  Changing the engine version is a measured decision, not an upgrade.
- **The reference implementation for embeddings is Feature 001's** Python `sentence-transformers`
  pipeline with the pinned `torch`/`transformers`/`tokenizers` versions in `requirements-001`, and
  the tolerance is Feature 001's: cosine ≥ 0.9999, max absolute difference ≤ 1e-3 — defensible only
  because the weights are 32-bit (001 FR-033).
- **The reference implementation for exact vector search is a small independent Python
  computation** (matrix product plus stable sort with id tie-break) over the same committed
  vectors; tolerance 1e-6 on scores, exact on ids and order.
- **The metric is cosine over L2-normalised vectors**, which the model card specifies and which
  makes score = dot product; the index stores normalised vectors and the query is normalised at
  search time if it is not already.
- **The vector index is a flat, exact store of 32-bit vectors.** At the constitution's 100k-chunk
  configuration that is 100,000 × 384 × 4 bytes ≈ 154 MB of vectors — under the 300 MB ceiling
  with the mapped model (2.6 MB in 001) and the lexical index (17.5 MiB at 57.6k documents in
  003), but the number is *observed* in this feature, not asserted. Its read path follows the
  same single Cargo feature as the weights (FR-008): buffered by default, mapped under `mmap`,
  both covered by the same ADR and the same bit-identical test — one flag, one ADR, not two.
- **Embedding runs on the CPU, single-threaded by default**, with thread count a recorded
  parameter; the pinned engine reads `RAYON_NUM_THREADS` (Feature 001 corrected this from a
  different variable name — the record matters).
- **Passage text for evaluation is `title` + `text`** joined by a single space when a title
  exists, matching how BEIR's own dense baselines feed the model; truncation at 256 tokens is the
  model's, not the harness's.
- **The Feature 003 smoke stays lexical.** The dense baseline is a full run, not a CI job; adding
  a dense smoke would require the model in CI and is a later decision.
- **No `xtriever-dense` public API stability is promised**; the pipeline feature will shape it.
