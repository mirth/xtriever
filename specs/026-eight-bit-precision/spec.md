# Feature Specification: Eight-Bit Precision End to End

**Feature Branch**: `026-eight-bit-precision`

**Created**: 2026-09-20

**Status**: Draft

**Input**: User description: "Eight-bit precision end to end: store the dense vectors as int8 with per-vector scales and an exact rescoring pass, and load both pinned models from the owner-supplied eight-bit GGUF artefacts (leliuga/all-MiniLM-L6-v2-GGUF, cstr/ms-marco-MiniLM-L-6-v2-GGUF). One feature so the artefacts are regenerated once."

**Clarifications (owner, 2026-09-20)**: the float vectors are **not** kept — the eight-bit
scores are the final ones, and the measured 0.0006 nDCG@10 is accepted (Q1 = B). Float weights
**remain loadable** beside eight-bit ones, chosen by what the manifest pins (Q2 = B). The
artefacts pinned are the **plain eight-bit file from each repository** (Q3 = A).

## Why This Spec Reads Technically

The engine's footprint on a device is dominated by two things it keeps in memory: the stored
vectors and the two model weight files. For the shipped Wikipedia artefact that is 661 MB of
vectors and 174 MB of weights, against a 600 MB device ceiling the project set in
[ADR-0010](../../docs/adr/0010-device-rss-ceiling-600mb.md). Every measurement this project has
taken on a phone has had to report that ceiling "for comparison only", because the full corpus
does not fit inside it.

Eight bits changes that arithmetic. Three studies under `reference/` measured it before this
spec was written, and they are what the requirements below are built on rather than on
expectations:

- **The vectors** (`int8_vectors_study.py`, SciFact and NFCorpus): quantised vectors cost
  0.0006 nDCG@10 on both datasets and nothing at all in Recall@100; an exact rescoring pass over
  the top twenty restores the float ranking exactly. Storage falls from 1,544 to 396 bytes a
  row. A kernel probe outside the repository scanned a Wikipedia-sized matrix in 3.7–7.8 ms
  against 16 ms for a fair float kernel.
- **The models** (`int8_model_study.py`, `int8_reranker_study.py`): simulated eight-bit weights
  per output channel left the embedder and the cross-encoder within noise of float — the
  embedder moved +0.0020 nDCG@10, the re-ranker 0.0000 — while the cruder per-tensor scheme cost
  the embedder 0.0052 and is not viable. Those were simulations; **this feature uses the
  owner-supplied artefacts instead**, whose eight-bit weights sit beside float norms and biases.

Both halves force every artefact to be rebuilt: the vectors because the dense format changes,
the models because an index records the embedder's fingerprint and a different embedder means a
different index. Doing them as one feature rebuilds the corpus once rather than twice.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The stored vectors take a quarter of the space (Priority: P1)

An index built by this engine keeps its dense vectors as eight-bit codes with the scale needed
to read them back. A search over that index returns what the float index returned, within the
small and measured difference that rounding introduces.

**Why this priority**: It is the larger of the two footprint wins, it is self-contained in one
crate, and it is the half whose quality cost has been measured on two datasets.

**Independent Test**: Build the fixture index and a BEIR index, compare each query's hits with
the float index's by the thresholds FR-009 sets, and compare the file sizes.

**Acceptance Scenarios**:

1. **Given** an index built by this engine, **When** its dense file is inspected, **Then** each
   row holds an eight-bit code per dimension and the scale that recovers it, and the file is
   under a third of the float file's size for the same documents.
2. **Given** the same corpus and query set, **When** the eight-bit index and a float index are
   both searched, **Then** the evaluation metrics stay within FR-009's thresholds; the rankings
   are close but not identical, which is what dropping the float vectors buys.
3. **Given** an index in the previous dense format, **When** it is opened, **Then** the engine
   refuses it by name and says to rebuild, as it did when the format last changed.
4. **Given** the same index, query and configuration, **When** a search runs twice, **Then** the
   scores and the order are identical — quantisation changes what the scores are, never whether
   they are reproducible.

---

### User Story 2 - Both models ship at eight bits (Priority: P1)

The engine loads the embedder and the cross-encoder from the eight-bit artefacts the owner
pinned, instead of the as-published float weights, and retrieval quality holds.

**Why this priority**: It is the other half of the footprint, it is what makes the corpus fit
under the device ceiling, and it must land in the same cycle so the corpus is embedded once.

**Independent Test**: Run the three BEIR datasets through the evaluation harness with the
eight-bit models and compare nDCG@10 and Recall@100 with the committed baselines.

**Acceptance Scenarios**:

1. **Given** the pinned eight-bit artefacts on disk, **When** the engine loads them, **Then**
   both models load and report fingerprints that name the eight-bit artefact, not the float one.
2. **Given** an artefact whose bytes do not match the pinned checksum, **When** it is loaded,
   **Then** the engine refuses it and names the mismatch, as it does for the float weights today.
3. **Given** an index built with the float embedder, **When** it is opened with the eight-bit
   embedder, **Then** the fingerprint mismatch is a hard error at open, never a silent
   re-interpretation.
4. **Given** the three evaluation datasets, **When** they are run with both eight-bit models,
   **Then** no dataset's nDCG@10 or Recall@100 falls more than 0.005 below its committed
   baseline, and the deltas are reported per dataset.

---

### User Story 3 - The device footprint falls below the ceiling (Priority: P2)

The shipped Wikipedia corpus, which today exceeds the project's device memory ceiling and is
reported "for comparison only", fits under it.

**Why this priority**: It is the reason for the feature, but it follows arithmetically from the
first two stories rather than needing work of its own.

**Independent Test**: The host measurement over the shipped artefact reports a peak resident
size below the ceiling, recorded in a run file as the previous measurements were.

**Acceptance Scenarios**:

1. **Given** the regenerated artefact, **When** the host measurement runs, **Then** its peak
   resident size is recorded and compared with both the ceiling and the previous record.
2. **Given** the regenerated artefact, **When** the measurement queries run, **Then** the parity
   check against the goldens for that artefact passes.

---

### Edge Cases

- **A vector whose components are all zero**, where a per-vector scale would be zero.
- **An index built before this feature**, in the previous dense format.
- **A float model beside an eight-bit index**, or the reverse: the fingerprint must catch it.
- **A rescoring pass asked for more candidates than the index holds.**
- **An eight-bit artefact that fails its checksum**, or a file that is not the expected format.
- **The re-ranker artefact missing its classification head**: a cross-encoder without one
  produces embeddings, not relevance scores, and must be refused rather than used.
- **A corpus where two documents' eight-bit codes tie** but their float scores do not.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The dense stage MUST store each vector as one eight-bit code per dimension
  together with the scale required to recover it, and MUST refuse a file written in an earlier
  dense format by name, instructing a rebuild.
- **FR-002**: A search over an eight-bit index MUST return substantially what the float index
  returns, judged by FR-009's thresholds rather than by identity: with the float vectors dropped
  (FR-003) the two rankings differ for a small minority of hits, and the evaluation metrics are
  what decide whether that is acceptable.
- **FR-003**: The scores the engine reports for returned hits are the eight-bit scores; the
  float vectors are not retained and no rescoring pass exists. **Owner's decision (2026-09-20)**:
  the measured cost — 0.0006 nDCG@10 on SciFact and NFCorpus, no change in Recall@100 — is
  accepted in exchange for a dense file that is small on disk as well as in memory. Scores MUST
  still be deterministic: the same index, query and configuration give the same scores.
- **FR-004**: The dense file for a given corpus MUST be no larger than a third of the float file
  for the same corpus.
- **FR-005**: Both models MUST be loaded from the eight-bit artefacts pinned by the owner, by
  repository, revision and per-file checksum, exactly as the float weights are pinned today; a
  mismatch MUST be refused with the mismatch named.
- **FR-006**: Each model's fingerprint MUST name the eight-bit artefact it was loaded from, so
  that an index records which weights produced it.
- **FR-007**: Opening an index whose recorded embedder fingerprint differs from the loaded
  embedder's MUST remain a hard error at open.
- **FR-008**: The re-ranker artefact MUST be verified to carry its classification head at load,
  and refused if it does not.
- **FR-009**: Evaluation on all three BEIR datasets MUST report nDCG@10 and Recall@100 against
  the committed baselines, and no dataset may fall more than **0.005** below its baseline on
  either metric. A larger drop is a stop-and-report, never a threshold to widen.
- **FR-010**: Every shipped artefact — the Wikipedia corpus, the fixture index, the demo slices
  and the evaluation caches — MUST be regenerated once within this feature, and the measurement
  records refreshed.
- **FR-011**: The device measurement for the shipped corpus MUST be recorded and compared with
  the project's 600 MB ceiling and with the previous record.
- **FR-012**: Float and eight-bit model artefacts MUST both be loadable, with the manifest
  deciding which one an installation uses, and any artefact in an unsupported format MUST be
  refused by name. **Owner's decision (2026-09-20)**: keeping both paths preserves a fallback if
  an eight-bit artefact disappoints on some corpus, at the cost of two loading paths and two
  fingerprint families in the tests.
- **FR-013**: The change to the on-disk dense format and to the model loading contract MUST be
  recorded in an architecture decision record before it lands.
- **FR-014**: The artefacts pinned are the plain eight-bit files: `all-MiniLM-L6-v2.Q8_0.gguf`
  (25.0 MB) from `leliuga/all-MiniLM-L6-v2-GGUF` and `ms-marco-MiniLM-L-6-v2-q8_0.gguf`
  (24.7 MB) from `cstr/ms-marco-MiniLM-L-6-v2-GGUF`. **Owner's decision (2026-09-20)**. Both
  were inspected on 2026-09-20: eight-bit weight matrices with float norms and biases, and the
  re-ranker carries its `classifier.weight` and `classifier.bias`.

### Key Entities

- **The eight-bit dense row**: a document's vector as codes plus the scale that recovers it,
  alongside the identifier and norm the row already carries.
- **The manifest entry**: which artefact an installation uses for each model, float or
  eight-bit, and the checksums that pin it.
- **The pinned model artefacts**: two eight-bit files, each named by repository, revision and
  checksum in the manifest the fetch script reads.
- **The fingerprints**: the strings an index records to say which embedder and re-ranker
  produced and scored it.
- **The regenerated artefacts**: the Wikipedia corpus, the fixture index, the demo slices and
  the evaluation caches, all rebuilt once.

## Success Criteria *(mandatory)*

- **SC-001**: For the same corpus and query set, the eight-bit index's ranking agrees with the
  float index's on at least 99% of the first hundred candidates, measured on a BEIR dataset —
  the agreement the study measured (99.5% at depth 100) held to within a point.
- **SC-002**: The dense file for the shipped corpus falls from about 661 MB to under 200 MB.
- **SC-003**: Both model artefacts together occupy under 60 MB, against 174 MB today.
- **SC-004**: On all three BEIR datasets, nDCG@10 and Recall@100 stay within 0.005 of their
  committed baselines, with the deltas reported per dataset.
- **SC-005**: The peak resident size for the shipped corpus falls below the project's 600 MB
  device ceiling, recorded in a run file.
- **SC-006**: An index or model artefact from before this feature is refused with a message that
  names what is wrong and what to do, never silently accepted.

## Assumptions

- **The owner supplies the model artefacts.** They are fetched and pinned like every other
  model; nothing is quantised locally.
- **The quality evidence comes from the real artefacts, not the simulations.** The studies under
  `reference/` justified attempting this; the gate is the evaluation harness on the actual files.
- **The vector quantisation is symmetric with a scale per vector**, the scheme the study
  measured; a global scale measured slightly better on quality but leaves no headroom for a
  corpus with a different distribution.
- **No rescoring pass exists** (FR-003), so the dense file holds eight-bit rows only and the
  float vectors are gone once an index is built. An installation that needs exact float scoring
  keeps a float index, which the format version distinguishes.
- **Search results change.** Eight-bit models produce different embeddings, so this feature's
  indexes are not bit-comparable with today's, and every golden that records a score must be
  regenerated. The parity claims in this feature are eight-bit against eight-bit.
- **No accelerator is used.** This stays on the pinned inference engine and the processor;
  neural accelerators are a separate question with their own decision record.
- **Arbitrary user-supplied models are out of scope**, and deliberately so (owner's decision,
  2026-09-20). The engine would support any BERT-family encoder whose file describes itself —
  block count, embedding length, head count, pooling — and the eight-bit loading this feature
  adds is most of that work, but the quality guarantee cannot follow: every committed baseline
  belongs to the pinned pair. It is the natural feature after this one, and this one's loader
  should be shaped so that it mostly relaxes assertions rather than rewriting anything.
- **Continuous integration is unchanged.** The full three-dataset evaluation stays local, as the
  standing resource rule requires.
