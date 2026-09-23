# Feature Specification: Optional Sparse Lexical Expansion

**Feature Branch**: `027-sparse-lexical-expansion`

**Created**: 2026-09-23

**Status**: Draft

**Input**: User description: "Optional sparse lexical expansion (learned sparse retrieval, SPLADE-style, "inference-free" on the query side), off by default. At index time, each document is encoded by the pinned OpenSearch neural sparse document encoder (opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill @ babf71f3, Apache-2.0, already pinned in reference/models/manifest-sparse-doc-v3.json; a DistilBERT masked-LM whose logits give per-token weights) into weighted vocabulary tokens, written as a second lexical field whose term frequency carries the weight (weight x scale, rounded; the spike's best scale is 10). At query time no model runs: the query's own wordpiece token ids match that field, scored by BM25 beside the existing contents field at a boost (spike's best 1.0). Measured in the spike (engine's own fusion and re-ranker, reproducing hybrid-rerank-v3 exactly): full pipeline re-ranked nDCG@10 SciFact 0.72194 -> 0.72166, NFCorpus 0.36247 -> 0.35767, FiQA 0.38964 -> 0.40703 (+0.0174), three-set mean +0.0041, below Feature 016's +0.005 default-stage floor -- so it ships as an opt-in per index for FiQA-shaped corpora (no titles, vocabulary mismatch), the owner's Feature 016 decision. Lexical index grows about 2.2-3.6x at scale 10. The document encoder must run inside the Rust build (candle 0.9.2 ships DistilBERT with its masked-LM head); devices need only the tokenizer, never the 268 MB encoder. The quality gate and the three BEIR datasets are measured locally; CI stays SciFact-only and runs no model-backed job."

## Why This Spec Reads Technically

The user is the roadmap, and this feature is the end of a line of measurements. Feature 012
found a learned sparse encoder worth +1.6 points of mean nDCG@10 and could not build it in
Rust; Feature 016 re-measured it as a third fused list on the current pipeline, found +0.28
points, below the +0.5 floor fixed in advance, and the owner kept it as an **opt-in stage, off
by default**, for corpora shaped like FiQA. The spike after Feature 026 found a better way to
use the same encoder: its weights written into a **second lexical field** scored by BM25
beside the existing one, rather than a separate list with its own scorer. On the full pipeline
that gains +1.74 points on FiQA and +0.41 on the three-set mean, still below the floor — so
this feature builds the owner's opt-in, in the cheaper design, and does not change the default.

Two facts found while specifying remove Feature 012's blockers. The pinned inference engine
ships the encoder's architecture with its masked-language-model head, so the index build can
encode in Rust. And the encoder's tokenizer has exactly the embedder's vocabulary, normalizer
and pre-tokenizer, so the query side needs no model: the encoder's own tokenizer and its
published query-side table, which decides which query tokens count, are 1.6 MB together and
travel inside the index (plan research D6).

## Clarifications

### Session 2026-09-23

- Q: Which surfaces carry the option in this feature? → A: The engine, the evaluation harness
  and the command-line build, plus the platform bindings — FFI, Python, Swift and Kotlin open
  and search a sparse index, and the Python package can build one. No new demonstration. Two
  pull requests: the engine and its measurement first, the surfaces second.

### Plan amendments (2026-09-23, research D6 and D11)

- The query side travels **inside the index**: the encoder's tokenizer and query-side table are
  copied into it at build, verified by hash at open. An installation needs no artefact beyond
  the index to search it, and there is no tokenizer to mismatch. FR-006, SC-005, User Story 1
  scenario 4 and User Story 4 scenario 3 are reworded to match.
- The work lands in **three** pull requests to keep each within the size rule, the engine and
  its measurement before the surfaces (FR-014).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An installation opts an index into sparse expansion and searches it (Priority: P1)

An integrator whose corpus is FiQA-shaped — no titles, questions phrased unlike the documents
— creates an index with sparse expansion switched on. The build encodes every document with the
pinned encoder and stores the weighted tokens beside the document's own terms. Every later
search of that index uses them automatically; no search option has to be remembered, and no
model runs at query time.

**Why this priority**: This is the feature: without it there is nothing to measure or ship.

**Independent Test**: Build a small index with the option on and one with it off from the same
documents; search both. The option-on index records the option and the encoder's identity and
ranks by both fields; the option-off index is unchanged in every byte and result from today.

**Acceptance Scenarios**:

1. **Given** documents and the option on, **When** the index is built, **Then** each document
   carries its expansion, the index records the option, the encoder's identity and the
   expansion's parameters, and the build reports how many documents were truncated to the
   encoder's window.
2. **Given** an index built with the option, **When** it is searched with plain text, **Then**
   the query's own tokens are matched against the expansion as well as the document text, with
   no model call.
3. **Given** documents and the option off (the default), **When** the index is built and
   searched, **Then** the files and every result are identical to what today's engine produces.
4. **Given** an index built with the option whose stored tokenizer or query-side table has been
   altered or damaged, **When** it is opened, **Then** it is refused naming the file and both
   hashes, never silently searched with mismatched tokens.

---

### User Story 2 - The encoder's output is checked against a reference (Priority: P1)

The document weights the engine computes are compared with weights the pinned model produces in
its reference implementation, on fixture documents, at a stated tolerance.

**Why this priority**: The constitution requires every learned behaviour to be verified against
an executable oracle; an encoder that drifts would degrade quality invisibly.

**Independent Test**: A fixture set of documents, including one longer than the encoder's
window and one with no expansion, encoded by the reference and by the engine; weights compared
within the tolerance, and the term frequencies derived from them compared exactly except where
the reference's value sits within the tolerance of a rounding boundary.

**Acceptance Scenarios**:

1. **Given** the fixture documents, **When** encoded by the engine, **Then** every weight is
   within the stated tolerance of the reference's, and every stored term frequency equals the
   reference's apart from documented rounding-boundary cases.
2. **Given** the same documents encoded twice, on any thread count, **When** compared, **Then**
   the outputs are identical.

---

### User Story 3 - The option's quality is measured and recorded (Priority: P1)

The evaluation harness gains a configuration with the option on, run on SciFact, NFCorpus and
FiQA through fusion and re-ranking, against the default pipeline.

**Why this priority**: The option ships only with its measured cost and benefit on record, as
the constitution requires of any ranking change.

**Independent Test**: The option-on configuration's nDCG@10 and Recall@100 per dataset, beside
`hybrid-rerank-v3`; the option-off configuration reproduces `hybrid-rerank-v3` exactly.

**Acceptance Scenarios**:

1. **Given** the three datasets, **When** the option-on configuration runs, **Then** its
   numbers per dataset and their mean are recorded beside the default's, with the index sizes
   and the build's encoding throughput.
2. **Given** the option off, **When** the default configurations run, **Then** every committed
   baseline is reproduced exactly.

---

### User Story 4 - Bindings build and search sparse indexes (Priority: P2)

An application opens an index built with the option through the FFI surface, Python, Swift
or Kotlin and searches it exactly as any other index, and sees the option in the index's
information; a Python integrator builds one by switching the option on at creation. No
demonstration is added: the existing demos serve titled Wikipedia, where the option does not
help.

**Why this priority**: The option serves applications, but the engine and its measurement
stand on their own; the bindings are a second increment.

**Independent Test**: An index built with the option opens and searches through each surface
in scope, with the same results as the engine's own search.

**Acceptance Scenarios**:

1. **Given** a sparse index, **When** opened through FFI, Python, Swift or Kotlin, **Then** it
   searches with the expansion, returns the engine's own results for the same query, and
   reports the option, its parameters and the encoder's identity in its index information.
2. **Given** the Python package and the option on at creation, **When** documents are added
   and committed, **Then** the index is built with the expansion, as the engine builds it.
3. **Given** a device package, **When** it searches a sparse index, **Then** it needs nothing
   beyond the index it stages already, and it carries no encoder.

### Edge Cases

- A document longer than the encoder's window: encoded from its first window, truncation
  counted in the build record (the reference does the same).
- A document whose expansion is empty (every weight rounds to zero): stored with an empty field;
  it is still found by its own text.
- A query with no token in the query-side table: searched on the document text alone.
- An index with the option built by an earlier engine version, or opened by one: refused by
  name through the index format's version, never read with the field ignored.
- The encoder artefact missing, damaged or different at build time: the build refuses before
  encoding anything, naming the file and the expected size and hash.
- Adding documents to an existing sparse index: each new document is encoded; the index never
  holds a mix of expanded and unexpanded documents.
- The pinned encoder is 268 MB and far slower than the embedder: an installation that never
  switches the option on never loads, fetches or ships it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Sparse expansion MUST be an option chosen when an index is created, off by
  default, recorded in the index, and applied to every document the index ever receives.
- **FR-002**: With the option off, index files, search results and every committed baseline
  MUST be identical to the engine before this feature.
- **FR-003**: The build MUST encode each document with the pinned encoder — repository,
  revision and every file's size and hash verified before use, as every pinned model is — using
  the encoder's own recipe: its logits masked by the document's tokens, the maximum over token
  positions per vocabulary entry, and the encoder's activation (log of one plus the log of one
  plus the positive part), special tokens excluded; a document longer than the encoder's window
  is truncated to it.
- **FR-004**: Each weight MUST become a term frequency equal to the weight times the index's
  scale, rounded to the nearest integer; entries that round to zero are not stored. The scale
  MUST be recorded per index, with a default of 10.
- **FR-005**: A query MUST contribute its distinct token ids that have a positive entry in the
  encoder's query-side table, special tokens excluded, each once, matched against the expansion
  field and scored by the same relevance function as the document text, at a boost recorded per
  index with a default of 1.0. No model MUST run at query time.
- **FR-006**: A sparse index MUST carry the encoder's tokenizer and query-side table, copied
  byte for byte at creation, with their hashes recorded; opening MUST verify both and refuse a
  mismatch naming the file and both hashes. Searching MUST need nothing outside the index.
- **FR-007**: Identical documents, option and parameters MUST produce an identical index and
  identical results on one architecture, independent of thread count and batching.
- **FR-008**: The engine's document weights MUST agree with a reference implementation's on
  fixture documents within a tolerance stated in the plan, and the stored term frequencies MUST
  equal the reference's except where the reference value lies within that tolerance of a
  rounding boundary; the fixtures and their generator live in `reference/`.
- **FR-009**: The index format change MUST carry an ADR and a version the older engine refuses
  by name; indexes without the option MUST keep today's version or be read unchanged.
- **FR-010**: The evaluation harness MUST offer the option as a configuration and measure it on
  the three datasets through fusion and re-ranking, recording per-dataset nDCG@10 and
  Recall@100, the lexical index size and the encoder's build throughput.
- **FR-011**: Continuous integration MUST NOT run the encoder or any job that needs its weights;
  the model-free parts (the term-frequency rule, the query-side rule, the recorded identities,
  the refusal paths) MUST be tested without it.
- **FR-012**: The option MUST be available through the FFI surface and its bindings: every
  binding (Python, Swift, Kotlin) MUST open and search a sparse index with the engine's own
  results and report the option in the index information, and the Python package MUST be able
  to create one. The device packagers MUST NOT stage the encoder; a sparse index needs no
  artefact beyond itself. No new demonstration is in scope.
- **FR-014**: The feature MUST land in pull requests each within the repository's size rule,
  the engine and its measurement before the surfaces (the plan splits it in three).
- **FR-013**: The encoder MUST NOT be required by any installation that does not build sparse
  indexes: not fetched, not loaded, not packaged.

### Key Entities

- **Sparse expansion option**: per index — on or off, the scale, the boost, the encoder's
  identity, the tokenizer's identity, the query-side table's identity.
- **Document expansion**: per document — the vocabulary entries the encoder weights above zero
  after rounding, each with its term frequency.
- **Pinned sparse encoder**: the model artefact, its configuration, tokenizer and query-side
  table, pinned by revision and file hashes, used at build time only.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the option on, FiQA's full-pipeline nDCG@10 rises by at least 0.010 over
  the default pipeline (the spike measured +0.0174).
- **SC-002**: With the option on, no dataset's full-pipeline nDCG@10 or Recall@100 falls more
  than 0.005 below the default pipeline (the spike's nearest: NFCorpus −0.0048).
- **SC-003**: With the option off, every committed baseline is reproduced exactly.
- **SC-004**: The engine's document weights agree with the reference on every fixture document
  within the stated tolerance.
- **SC-005**: An installation that searches a sparse index needs no artefact beyond the index,
  which carries 1.6 MB for its query side; the lexical index size with the option on is
  recorded for each dataset, and stays within 4× the size without it at the default scale.

## Assumptions

- The encoder is the one Features 012 and 016 measured, pinned since Feature 012
  (`reference/models/manifest-sparse-doc-v3.json`, Apache-2.0); its weights come from the owner's
  pinned link, as all model weights do, and run at full precision at build time — the build host
  is not memory-bounded like a phone.
- The spike's settings are the defaults: scale 10, boost 1.0, query tokens unweighted. They were
  chosen on the same three datasets they are measured on; SC-001 and SC-002 are therefore a
  reproduction check, not a generalisation claim, and the report says so.
- The option is not the default and not applied to the shipped Wikipedia artefact: that corpus
  has titles, where the spike measured no gain.
- The query-side table is the encoder's published `idf.json`; only its positive entries' token
  ids matter to the query side.
- Building with the option is much slower than without: the encoder is about five times the
  embedder's cost per document. The build records its throughput; no speed claim is made.
- The design's measured basis is the spike (`crates/xtriever-eval/examples/splade_field.rs`,
  `rerank_runs.rs`), whose re-rank stage reproduced `hybrid-rerank-v3` to every printed digit.
