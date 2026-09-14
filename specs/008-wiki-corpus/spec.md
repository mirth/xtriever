# Feature Specification: The Wikipedia Corpus and Shipped Index

**Feature Branch**: `008-wiki-corpus`

**Created**: 2026-09-13

**Status**: Draft

**Input**: User description: "Feature 008: Wikipedia corpus, chunking, and a shipped hybrid index for the iOS demo" — the second of three features toward the iOS Wikipedia demo (007 built the Swift surface; 009 builds the app at `apps/ios-wiki-demo/`). This feature turns a pinned snapshot of the **whole Simple English Wikipedia** into passages, builds the hybrid index on a host with one resumable command, stages it for the app with its attribution, and measures it on a phone against the constitution's ceiling. *Owner decisions 2026-09-13: the whole edition (Q1 = C); no relevance oracle for this corpus (Q2).*

## Why This Spec Reads Technically

The "user" is twice over: the Xtriever developer who builds the corpus artefact, and — through
the demo — a person typing a question into a phone and expecting a Wikipedia passage back.
The first needs a build that is reproducible, pinned and honest about its size and cost; the
second needs passages that read as passages, carry their article and link, and come back for
the obvious questions. The engine already indexes chunks with provenance (parent, ordinal, byte
range) and returns passage text with hits; what it lacks is everything upstream of that: a
snapshot reader, a chunker, a build tool, and a corpus identity. The constitution's reference
configuration for the memory ceiling — "a 100k-chunk hybrid index with both models loaded"
(v1.4.0, ADR-0010) — has never been measured; this corpus is several times larger, so the
device run here is a stricter test than the ceiling asks for. The owner chose not to have a
relevance oracle for this corpus (Q2): the BEIR baselines remain the ranking stages' guard, and
this corpus is measured for reproducibility, parity, size, time and footprint, not quality.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - One command builds a pinned, reproducible Wikipedia index on a host (Priority: P1)

A developer runs one command on a laptop. It fetches a pinned Wikipedia snapshot (verified
against a manifest, never committed), selects the articles the corpus is defined as, splits
them into passages, embeds them, writes a hybrid index in the pipeline's existing on-disk
format, and records the artefact's identity: which snapshot, which selection, which chunker,
which models, how many articles and passages, how long it took and what it cost.

**Why this priority**: Without the artefact nothing else exists; without the identity nobody
can tell two artefacts apart or reproduce one.

**Independent Test**: Build the index twice from the same manifest; the two artefacts have the
same identity and the same passages in the same order, and every search returns the same hits
from both.

**Acceptance Scenarios**:

1. **Given** a manifest pinning the snapshot (URL, size, hash) and the exclusion rules,
   **When** the developer runs the build command, **Then** the snapshot is downloaded once,
   verified, cached, and a hash mismatch stops the build before any article is read.
2. **Given** the verified snapshot, **When** the build runs to completion, **Then** the output
   directory is a hybrid index the existing surface opens unchanged, plus a build record
   stating the corpus identity, the counts, the wall time per phase, and the artefact's size.
3. **Given** the same manifest and models, **When** the build is run again on the same host,
   **Then** the corpus identity is identical and every passage is byte-identical in the same
   order; search results for the fixed measurement queries are identical.
4. **Given** a build interrupted at any point, **When** the developer looks at the output
   directory, **Then** it is either absent or a complete, openable index — never a partial one
   the surface would accept; **and When** the build is re-run, **Then** passages already
   embedded are not embedded again (the embedding phase is hours long and MUST resume).
5. **Given** the built index, **When** the developer opens it through the Feature 007 surface,
   **Then** it reports the passage count, the embedder and re-ranker identities, and searches.

---

### User Story 2 - Articles become passages a phone can show (Priority: P1)

An article is split into passages a person can read on a phone screen and the embedder can
see in full: bounded in length, aligned to the article's own paragraph structure, each
carrying its parent article's title and link and its position in the article. A hit is a
passage, and the demo can show "from *Article* — paragraph 3" with a link to the source.

**Why this priority**: The chunker decides what the index contains. A passage the embedder
truncates is content the dense stage never sees; a passage cut mid-sentence reads as broken;
a passage without its article is a quote without a source.

**Independent Test**: A golden fixture of articles and their expected passages (generated by
an independent reference implementation) is reproduced exactly by the chunker; every passage
in the built index tokenises to at most the embedder's window.

**Acceptance Scenarios**:

1. **Given** an article, **When** it is chunked, **Then** every passage is at most the stated
   length, starts and ends on a paragraph or sentence boundary, and the concatenation of the
   passages' byte ranges covers the article's body without overlap or gap.
2. **Given** a passage in the index, **When** it comes back as a hit, **Then** it carries the
   article's title, the article's URL, its ordinal within the article, and its byte range.
3. **Given** an article that is a redirect, a disambiguation page, or has an empty body,
   **When** the corpus is selected, **Then** it is excluded and the exclusion is counted in
   the build record.
4. **Given** the golden chunking fixtures, **When** the chunker runs over them, **Then** the
   passages match the reference byte for byte.
5. **Given** every passage in the built index, **When** each is tokenised by the embedder's
   tokenizer, **Then** none exceeds the embedder's input window (100 %, not "almost all").

---

### User Story 3 - The index ships with the app (Priority: P2)

The built index and its attribution are staged by the existing package build script into the
app's resources, with its size stated. The app copies it out on first launch (the 007 lock-file
caveat), opens it read-only, and shows the corpus's licence attribution — Wikipedia text is
CC BY-SA and requires it.

**Why this priority**: The artefact's reason to exist is to be on the phone; its size and its
licence are the two facts the app author must know before shipping.

**Independent Test**: The build script stages the index; the harness opens it from the staged
location on the simulator and searches; the attribution file is present beside the descriptor.

**Acceptance Scenarios**:

1. **Given** a built index, **When** the package build script is asked to stage it, **Then**
   the index directory, its build record and an attribution file (licence, snapshot, date) are
   placed in the resources and the staged size is printed.
2. **Given** the staged resources, **When** the 007 harness opens the index on the simulator,
   **Then** it opens and a search returns passages with titles and URLs.
3. **Given** the staged size, **When** it is compared with the stated bundle budget, **Then**
   the build fails loudly if it is over — the app author never discovers it at submission.

---

### User Story 4 - The corpus is measured on a device (Priority: P3)

The 007 device harness runs over the Wikipedia index instead of SciFact: peak footprint
against the constitution's 600 MB ceiling (ADR-0010's review trigger), open time, first-launch
copy-out time and disk use, latency at re-rank depths 0 / 5 / 20 for the measurement queries,
and parity with the host. Run records are committed verbatim.

**Why this priority**: 600 MB was derived, not measured, and this corpus is several times the
configuration it was derived for. The exact dense scan touches every vector on every query;
what that costs in footprint and latency on a phone is the number the demo lives with.

**Independent Test**: A run record from a physical device with a footprint verdict and the
parity counts.

**Acceptance Scenarios**:

1. **Given** the harness with the Wikipedia index and both models mapped on a device,
   **When** the measurement runs, **Then** peak footprint is recorded with a verdict against
   600 MB and the record is committed.
2. **Given** the same run, **When** the measurement queries are searched at each depth,
   **Then** latency per query is recorded and the per-pair re-rank cost derived, beside 007's
   SciFact numbers; the depth-0 latency is the cost of the full dense scan on the phone.
3. **Given** the run, **When** parity is checked, **Then** lexical hits are bit-identical and
   dense / re-rank scores are within the 007 tolerance for every query, matched by id.
5. **Given** the app's first launch, **When** the index is opened, **Then** the run record
   states whether it was opened in place or copied out, the copy time if any, and the
   on-device disk use.
4. **Given** a footprint over 600 MB, **When** it is observed, **Then** it is ⛔ stop-and-report
   with the same breakdown 007 produced — the ceiling is not moved again by this feature.

---

### Edge Cases

- The snapshot URL disappears or changes: the manifest's hash catches a change; a vanished
  snapshot is a build failure naming the manifest, not a silent substitution.
- An article longer than the whole selection budget, a paragraph longer than one passage (a
  long run-on without sentence boundaries), a body that is only markup residue: each produces
  defined output — split at the best available boundary, never truncated silently — and is
  counted.
- Titles that collide after normalisation, articles with identical text: external ids come from
  the snapshot's own identifiers, never from titles.
- Text with combining characters, right-to-left runs, or invalid UTF-8 sequences in the dump:
  the passage store requires UTF-8; the reader replaces or rejects invalid bytes and counts
  them.
- The build is run on a host with a different thread count: passage order and content must not
  depend on it; embeddings must agree with the single-thread build within the dense stage's
  determinism contract.
- The staged index exceeds the bundle budget, or the device lacks the space to copy it out
  (the index is on the order of a gigabyte; the copy doubles it): loud failures at build time
  and at first launch respectively.
- A measurement query returns no hits (a query about something the edition lacks): recorded
  as zero hits, not an error; parity compares empty with empty.

## Requirements *(mandatory)*

### Functional Requirements

**The snapshot and its selection**

- **FR-001**: The corpus MUST be defined by a committed manifest pinning the snapshot (source
  URL, byte size, content hash, edition, date); the snapshot itself MUST NOT be committed and
  MUST be verified against the manifest before any article is read.
- **FR-002**: The corpus is the **whole edition** (Simple English Wikipedia, one pinned
  snapshot) minus the exclusions of FR-003 — no sampling, no size cap; the corpus is a function
  of the snapshot and the exclusion rules, not of the run. *(Owner decision 2026-09-13, Q1 = C.)*
- **FR-003**: Redirects, disambiguation pages and articles with an empty body MUST be excluded
  and counted; every exclusion class MUST appear in the build record.
- **FR-004**: External ids MUST be the snapshot's own stable article identifiers; titles are
  a field, never an id.

**Passages**

- **FR-005**: Articles MUST be split into passages bounded by a stated maximum length, aligned
  to paragraph boundaries and, within a paragraph, to sentence boundaries; passages MUST
  tile the article body (ordered byte ranges, no overlap, no gap) so a passage can be located
  in its source.
- **FR-006**: Every passage MUST fit the embedder's input window in full; a passage the
  embedder would truncate is a defect, and the build MUST verify this for 100 % of passages.
- **FR-007**: Each passage MUST carry its article's title and URL as fields and its provenance
  (parent id, ordinal, byte range) so a hit can be shown as "from *Title*, passage *n*" with a
  link.
- **FR-008**: The chunker MUST be verified against golden fixtures produced by an independent
  reference implementation under `reference/` (Principle II), byte for byte, including the
  edge cases above.

**The build**

- **FR-009**: One host command MUST perform the whole build — fetch/verify, select, chunk,
  embed, index, commit, record — and MUST NOT leave a partial index the surface would open.
  The embedding phase (hours at this corpus size) MUST resume from a cache keyed by the
  passage's content and the embedder's identity, so an interrupted or repeated build never
  re-embeds a passage it has already embedded; the cache is local and never committed.
- **FR-010**: The build MUST be reproducible: the same manifest and models on the same host
  give the same corpus identity, byte-identical passages in the same order, and identical
  search results for a fixed query set; the identity MUST be recorded in the index directory
  and MUST change if the snapshot, selection, chunker, or either model changes.
- **FR-011**: The build record MUST state counts (articles read, excluded per class, selected,
  passages), wall time per phase, host thread count, artefact size per component, and the
  identities of the models used; it MUST be committed with the feature.
- **FR-012**: The build tool MUST live in the existing command-line crate or the evaluation
  harness (the plan decides) and MUST NOT add a stage, change a core trait, change the on-disk
  format, or touch `deny.toml` (Rule 2); the chunker MUST live in a pure crate.

**Measurement queries (no relevance oracle — owner decision, Q2)**

- **FR-013**: A committed set of 20 fixed measurement queries MUST exist for reproducibility
  (SC-001), host/device parity and latency; it carries **no expected answers and no relevance
  claim**. The queries are plain questions a person might type; their only requirement is to
  be fixed. The feature MUST state, in its report, that this corpus's retrieval quality is
  unmeasured.

**Shipping**

- **FR-014**: The package build script MUST stage the index, its build record and an
  attribution file (licence name and URL, snapshot edition and date, "text from Wikipedia")
  into the resources on request, print the staged size, and fail if it exceeds the bundle
  budget stated in the plan.
- **FR-015**: The 007 harness MUST be able to open the staged Wikipedia index on the simulator
  and on a device — in place, inside the read-only bundle, if the plan can make the surface
  open a read-only directory (007 F-001 was a lock file, not a content write), otherwise
  through the existing writable copy; the FFI wire contract does not change.

**Measurement**

- **FR-016**: The 007 device harness MUST run over the Wikipedia index with both models
  mapped: peak footprint against 600 MB (ADR-0010), open time, on-device disk use (and the
  first-launch copy time, or that the index is opened in place), latency at depths 0 / 5 / 20
  over the measurement queries, parity with
  the host matched by id; run records committed verbatim. A footprint over the ceiling is ⛔
  stop-and-report; this feature does not amend it.
- **FR-017**: CI MUST NOT fetch the snapshot, embed the corpus or run the build; the artefact
  is built locally and its record committed (standing rule: CI runs SciFact only). CI MAY run
  the chunker's fixture tests over committed files.

**Discipline**

- **FR-018**: Acceptance tests MUST be committed failing before implementation; the chunker
  fixtures and the measurement queries are written first.

### Key Entities

- **Snapshot manifest**: the pinned source — edition, date, URL, size, hash — plus the
  exclusion rules; the corpus is a pure function of it.
- **Article**: one snapshot entry — stable id, title, URL, body — and its exclusion class if
  any.
- **Passage**: the indexed unit — text, parent article id, ordinal, byte range; the thing a hit
  returns and a phone displays.
- **Corpus identity**: a fingerprint over snapshot hash, selection rule, chunker version and
  parameters, embedder and re-ranker identities; recorded in the index directory.
- **Build record**: counts, timings, sizes, host facts, identities — committed evidence of one
  build.
- **Measurement queries**: 20 fixed queries with no expected answers; the reproducibility,
  parity and latency probe.
- **Embedding cache**: local, keyed by passage content and embedder identity; what makes a
  ten-hour build resumable; never committed.
- **Attribution file**: what the app must show for the text it ships.
- **Device run record**: the 007 record shape over this corpus.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Two builds from the same manifest on the same host produce the same corpus
  identity, byte-identical passages in the same order, and identical top-10 hits for every
  measurement query.
- **SC-002**: 100 % of passages in the built index fit the embedder's window; the chunker
  reproduces every golden fixture byte for byte.
- **SC-003**: The build completes on the reference laptop with wall time per phase in the
  build record, and a build interrupted after the embedding phase has started resumes without
  re-embedding any passage already cached; the artefact's size per component is stated.
- **SC-004**: The staged index opens on the simulator through the unchanged 007 surface and
  returns passages with title, URL and provenance; the attribution file is present.
- **SC-005**: On a physical device the run record carries a footprint verdict against 600 MB,
  on-device disk use (in place or copied), latency at three depths for every measurement query,
  and parity counts: lexical bit-identical for 100 % of queries, dense and re-rank scores
  within the 007 tolerance, matched by id.
- **SC-006**: No change under `crates/xtriever-core`, `-lexical`, `-dense`, `-rerank`,
  `-pipeline`, `-ffi` or `deny.toml` beyond what the plan names, and the on-disk format
  version is unchanged.

## Assumptions

- **Edition**: Simple English Wikipedia, whole — about 250k articles in a snapshot of a few
  hundred megabytes; articles are short and plain. The English edition's full dump is tens of
  gigabytes and out of the question for a shipped index. The manifest makes the edition and
  date explicit so they can change without changing this spec.
- **Licence**: Wikipedia text is CC BY-SA 4.0; the attribution file and per-passage URL are
  the minimum the demo needs to comply, and the demo (009) shows them.
- **Passage length**: about 200 words, paragraph-aligned, merging short consecutive paragraphs
  up to the bound and splitting a long one at sentence boundaries — the largest passage the
  embedder sees whole (its window is 256 word-pieces) and a phone shows without scrolling.
  The plan states the exact bound after tokenising a sample.
- **Cost**: the dense stage embedded 57k FiQA documents in about 90 minutes on the reference
  laptop; the whole edition — an estimated 350–450k passages after exclusions — is on the
  order of **ten hours**, done once, cached (FR-009), and stated in the build record.
- **Size**: SciFact's 5,183 documents make a 19 MB index; 400k passages extrapolate to roughly
  600 MB of vectors, 250 MB of passage text and 200 MB of lexical index — **about 1.1 GB** —
  beside 174 MB of models: an app of about 1.3 GB, installed by sideloading — and the same
  again on the phone after a first-launch copy-out, unless the plan removes the copy (only the
  lexical lock file needs a writable directory — 007 F-001). The plan's research replaces these
  figures with measured ones; the bundle budget is set against the measured artefact.
- **Footprint**: the dense stage's exact scan touches every vector on every query; with the
  vectors memory-mapped, clean file-backed pages are not charged to the process footprint the
  way heap is (007 F-002), so the 600 MB verdict is expected to hinge on the models and the
  re-rank transient, as in 007, while the scan shows up in depth-0 latency. Expected, not
  known: the device run decides.
- **No quality oracle** (owner decision, Q2): this corpus's retrieval quality is not measured.
  The ranking stages remain guarded by the BEIR baselines; a defect specific to this corpus
  (chunking, selection, a field mapping) would surface only by use. The report states this.
- **Determinism**: passage order and content are a function of the snapshot and the rule;
  embeddings follow the dense stage's cross-process determinism contract (Feature 004); the
  lexical index is written with one writer thread as the eval harness does.
- **No new stage, no LTR**: the index is the 006 pipeline's format v2; LTR (a later feature)
  is not built here, and this corpus offers it no training signal (no oracle).
- **The demo app is Feature 009**: this feature ends at a staged, measured artefact and the
  harness proving it opens; the app that shows attribution and passages is next.
