# Feature Specification: The Python Wikipedia Demo

**Feature Branch**: `019-python-wiki-demo`

**Created**: 2026-09-16

**Status**: Draft

**Input**: User description: "The Python Wikipedia demo app: a Python application at `apps/python-wiki-demo/` that searches all of Simple English Wikipedia — the 008 index at `target/xt-wiki/` with both pinned models — through the 011 Python package (`xtriever` wheel) and shows the pipeline working the way the iOS demo (009) does on the phone: the fused (lexical + dense) list first, then the re-ranked order with what moved per hit (moved up / down / new / dropped), each hit's title, passage, rank and article link, each hit's eight explained features under the engine's names ("not seen by this stage" where absent), the engine's stage report (candidate counts, degradation and reason, re-rank candidates/scored/skipped, time-limit flag, elapsed ms), settings (re-rank depth 0/5/10/20 with 10 as the app default per 018, time budget, strict, re-rank mode replace/interpolate), and an About with the corpus identity, snapshot and counts from `corpus.json`, the embedder and re-ranker identities and format version from `info()`, this session's open time, and the attribution text verbatim with the licence link. The app owns no retrieval logic and makes no network call for retrieval; every number on screen is the engine's or a wall clock around an engine call. Title / passage / article URL are derived from the hit text by the 008 convention exactly as the Swift package's `titleAndPassage` / `wikipediaURL` do (split at the first blank line; percent-encode the title outside `A–Z a–z 0–9 - _ . ~ /`), verified against the CLI's eleven URL cases. The demo's own logic (preparation → ready → searching → results; fused-then-reranked merge; change marks; settings; title/URL derivation; About fields) is covered by tests that run against the 40-document 007 fixture index and its goldens so no 1 GB artefact is needed; one host run over the 20 measurement queries (`reference/fixtures/008/queries.json`) is checked for parity against the host goldens (`target/xt-wiki/expected.json`, score bits at depths 0/5/10/20) and its latency and footprint recorded beside 009/018's device records. Measured today on this laptop: open 0.34 s, fused ~250 ms warm, re-ranked depth 10 ~0.8–1.0 s. No engine, FFI, format, Python-package-wire or baseline change; the app is run from a checkout with the wheel installed, the models fetched and the 008 artefact built; CI runs nothing that needs the models or the artefact (standing rule). The open question is the UI form: a local web page served by the app and opened in the browser (stdlib only — no framework dependency), a framework web UI (Gradio/Streamlit), or a terminal UI."

**Clarification (Q1, owner)**: the simplest textual interface possible — a command line
(`demo search "sky color"` and the like), and the demo must also demonstrate building the
index from scratch.

## Why This Spec Reads Technically

The iOS demo (009) proved the pipeline on a phone; this feature proves the same thing on the
platform where RAG systems are actually assembled — a Python process on a laptop or a
server — through the surface Feature 011 built for exactly that audience. It is two
demonstrations in one small command-line program: *building* an index from the raw
Wikipedia snapshot with nothing but the package (read the articles, drop the
disambiguation pages, cut each article into passages that fit the embedder's window, add
them, commit, merge — the whole recipe on one screen of Python), and *searching* it while
watching the pipeline work: the fused list, then the re-ranked order with what moved, every
hit explaining itself in the engine's names, the stage report and the attribution. The
program is deliberately thin, as 009's is: it owns no retrieval logic, so what it
demonstrates is the engine, and every number it prints is the engine's or a wall clock
around an engine call. What makes it a *measured* claim rather than a search box is the same
discipline as 009's, with one addition the build makes possible: the demo's own logic is
tested against the 40-document fixture and its goldens; its search over the shipped
Wikipedia index is checked against the host goldens the phone is checked against; and its
own build of the first N articles is checked against the Rust build of the same N — the same
queries give the same hits in the same order, so the Python recipe and the shipped index are
demonstrably the same recipe.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A person searches Wikipedia from the command line and gets passages (Priority: P1)

From a checkout with the package installed, the models fetched and a Wikipedia index on
disk, one command with a question prints a ranked list of passages: each with its rank, the
article title, the passage text, and the article's link. The command says what it opened
and how long that took; nothing in the retrieval path touches the network.

**Why this priority**: This is the demo. Without it nothing else matters.

**Independent Test**: `demo search "why is the sky blue"` prints passages with titles, text
and article links within the measured latency; the same command offline prints the same.

**Acceptance Scenarios**:

1. **Given** the inputs on disk, **When** a question is given, **Then** the command prints
   what it opened (index, both models, load path) with the engine's timings, then the
   ranked passages — rank, title, passage, link — and exits 0.
2. **Given** the machine offline, **When** the same question is given, **Then** the output is
   identical.
3. **Given** a query with no hits (an empty string, only stop-words), **When** given, **Then**
   the command says so plainly and exits 0 — never an error for an ordinary empty result.
4. **Given** a checkout missing the index, a model directory or the package, **When** any
   command runs, **Then** it names exactly which input is missing and the command that
   produces it, and exits non-zero.
5. **Given** an index the engine refuses (format version, fingerprint, interrupted commit),
   **When** opened, **Then** the engine's message is printed and the exit is non-zero.

---

### User Story 2 - The person sees the pipeline work (Priority: P1)

A search shows its stages. The fused (lexical + dense) list is printed first, as soon as it
is available; the re-ranked order follows with the change marked per hit (moved up / moved
down / new / dropped). With the explain flag each hit lists its eight pipeline features
under the engine's names, "not seen by this stage" where a stage did not retrieve it, and
the combined re-rank score named as the score the head was ordered by. The stage report
closes every search: how many candidates each stage produced, whether a stage was skipped
or degraded and why, how many pairs the re-ranker scored, whether a time limit was ignored,
and the elapsed time — all from the engine's report, none computed by the demo. The re-rank
depth, the time budget, strict mode and the re-rank mode are flags, and the report reflects
them.

**Why this priority**: "Demonstrates the whole pipeline functionality" is the owner's brief
for both demos; a list alone demonstrates a search box.

**Independent Test**: Run a search with explanations; the fused list prints, then the
re-ranked list with marks, each hit with eight features, then the report's counts and time.

**Acceptance Scenarios**:

1. **Given** a question, **When** searched, **Then** the fused list is printed, labelled as
   the fused stage, before the re-ranked list is printed, labelled as such, with each hit
   whose position changed marked by the rule the iOS demo uses.
2. **Given** the explain flag, **When** searched, **Then** every hit lists the eight features
   under the engine's names with absent stages marked as such.
3. **Given** any completed search, **When** the report is read, **Then** it shows lexical and
   dense candidate counts (or that the dense stage was skipped and why), re-rank candidates
   and scored count (or that re-ranking was skipped and why), whether a time limit was
   ignored, and the elapsed milliseconds.
4. **Given** a re-rank depth, a time budget, strict mode or the replace mode on the command
   line, **When** the search runs, **Then** the report reflects it: fewer pairs scored, a
   budget exhausted shown as degradation (or as the engine's error under strict), the
   replace order when selected.
5. **Given** the default settings, **When** the first search of a process runs, **Then** it is
   labelled as the warm-up it is, and the elapsed time and the process footprint after the
   search are printed.

---

### User Story 3 - The person builds the index from scratch (Priority: P1)

One command builds a Wikipedia index from the raw snapshot with nothing but the package:
it reads the articles, drops the disambiguation pages by the 008 rules, cuts each article
into passages that fit the embedder's window by the 008 chunking contract, adds the passages
with their provenance, commits, merges, and writes the corpus sidecar and the attribution
file beside the index so the search and About commands work on it. The build takes a limit
(the first N articles) for a slice that builds in minutes; without a limit it builds the
whole corpus and says up front what that costs. Progress is printed as it goes.

**Why this priority**: The owner's brief: the demo must show how the index is built, not
only how it is searched. It is the recipe a person copies into their own RAG system.

**Independent Test**: Build the first N articles; search the result; the same searches
through the Rust build of the same N give the same passages in the same order.

**Acceptance Scenarios**:

1. **Given** the snapshot and the models, **When** the build runs with a limit, **Then** an
   index directory appears with the corpus sidecar and the attribution file beside it, the
   command prints the article, excluded, selected and passage counts and the phase timings,
   and a search over it works.
2. **Given** a slice built by the demo and the same slice built by the Rust command,
   **When** the twenty measurement queries run through both, **Then** every query gives the
   same external ids in the same order at every depth.
3. **Given** the build without a limit, **When** started, **Then** it states the full corpus's
   cost (hours on a laptop, from 008's record) before it begins.
4. **Given** a build interrupted, **When** looked at, **Then** no half-built index sits at the
   output path (the output appears only when complete).

---

### User Story 4 - The person sees what they are searching and on what terms (Priority: P2)

An About command prints the corpus: Simple English Wikipedia, the snapshot date, the
article and passage counts, the corpus identity, the two models' identities, the index's
format version, the recorded re-rank mode and depth beside the demo's default depth (both
labelled), this session's open and model load times, and the licence attribution the corpus
requires — the text shipped beside the index, verbatim, with its licence link.

**Why this priority**: Attribution is a licence obligation (CC BY-SA); the identities are what
make the demo a measured claim.

**Independent Test**: Run About; every item is present, and the attribution text equals the
shipped file byte for byte.

**Acceptance Scenarios**:

1. **Given** About, **When** run, **Then** the attribution text is printed verbatim with the
   licence link.
2. **Given** About, **When** run, **Then** the corpus identity, counts, model identities,
   format version and re-rank configuration are what the engine and the corpus sidecar
   report for the open index — not constants typed into the demo.

---

### User Story 5 - The laptop agrees with the phone, on record (Priority: P2)

One measurement command runs the twenty measurement queries at re-rank depths 0 / 5 / 10 /
20 over the shipped Wikipedia index, checks every hit against the host goldens the device
parity check uses — the same external ids in the same order with the same score bits — and
writes a record with this machine's latency per depth and its footprint, committed beside
the 009 and 018 device records.

**Why this priority**: The demo's claim is "the same engine"; the goldens are how that claim
is checked rather than asserted.

**Independent Test**: A record under this feature's `runs/` with parity PASS at all four
depths, per-depth latency medians, and a footprint figure.

**Acceptance Scenarios**:

1. **Given** the shipped artefact and its host goldens, **When** the measurement runs,
   **Then** every query at every depth matches the goldens and the record says PASS.
2. **Given** the record, **When** set beside 009's and 018's, **Then** the fused and re-ranked
   medians are stated for this machine at the demo default (depth 10) and at the engine
   default (20).

---

### Edge Cases

- A hit whose text has no title line (an index built by someone else): printed without a
  title and a link, never as an error.
- Very long passages or titles, right-to-left text, CJK: printed whole, not truncated
  silently (a snippet flag may shorten passages, and says so).
- A query longer than the embedder's window: the engine truncates; the demo neither
  restricts the length nor needs to know.
- A build whose snapshot is missing or fails its hash check: the demo names the fetch
  command; it never reads an unverified snapshot.
- A build over an existing output path: refused, never overwritten.
- A limit larger than the corpus: the whole corpus, marked as such in the sidecar; a limit
  of zero: refused.
- The measurement command on a demo-built slice: refused with the reason — the host goldens
  describe the full corpus; the slice's oracle is the Rust build of the same slice.

## Requirements *(mandatory)*

### Functional Requirements

**The program**

- **FR-001**: The demo MUST live at `apps/python-wiki-demo/`, consume the 011 package as an
  ordinary installed dependency, and locate the index, the two models and the snapshot at
  the checkout's standard locations by default, each overridable by flag or environment
  variable; it MUST NOT contain retrieval logic of its own or make any network call.
- **FR-002**: The interface MUST be a command line with subcommands for search, build, about
  and measure; every search or about invocation opens the index with both models
  memory-mapped and prints what it opened with the engine's timings.
- **FR-003**: Any missing input MUST be reported by name with the command that produces it,
  before any work starts, with a non-zero exit.

**Search**

- **FR-004**: A search MUST run the full pipeline with explanations requested under the
  flags' re-rank depth (0 / 5 / 10 / 20; default 10 with the 018 trade-off stated in the
  help and the engine's default of 20 named), time budget in ms (or none), strict flag and
  re-rank mode (the engine's interpolating default, or replace).
- **FR-005**: The fused result MUST be printed as soon as it is available and the re-ranked
  result after it, each labelled with its stage; position changes between the two MUST be
  marked per hit (moved up / moved down / new / dropped) by the rule the iOS demo uses.
- **FR-006**: Each hit MUST show the article title, the passage body, its rank, its position
  in the article, and the article's URL — all derived from the engine's hit (its text by
  the 008 convention: title line, blank line, passage; its chunk provenance) with the URL
  derived from the title exactly as the Swift package and the CLI derive it, never from a
  lookup the demo maintains.
- **FR-007**: With the explain flag each hit MUST list the eight pipeline features under the
  engine's names, absent stages marked as such.
- **FR-008**: The stage report MUST close every search, as reported by the engine: lexical
  and dense candidate counts, degradation with its reason, the re-rank report (candidates,
  scored, skipped reason), the time-limit-ignored flag, elapsed milliseconds; the process
  footprint after the search MUST be printed with it; the first search of a process MUST be
  labelled as the warm-up.
- **FR-009**: An empty result MUST be stated as such with exit 0; an engine error MUST be
  printed with the engine's message and a non-zero exit.

**Build**

- **FR-010**: The build MUST read the pinned snapshot only after verifying it against the
  008 manifest, apply the manifest's exclusion rules in order, chunk each article by the
  008 chunking contract priced with the embedder's own tokenizer, and add every passage
  with its external id (`<article id>#<ordinal>`), its fields and its chunk provenance
  through the package; then commit and merge.
- **FR-011**: The build MUST write the corpus sidecar (schema, corpus identity, snapshot,
  counts, marked partial when limited) and the attribution text beside the index in the
  008 shapes, so search and about work on a demo-built index exactly as on the shipped one.
- **FR-012**: The build MUST take a limit (first N articles, N ≥ 1); without one it MUST
  state the full corpus's cost from 008's record before starting; it MUST build into a
  staging path and move the result into place only when complete, and MUST refuse an
  existing output path.
- **FR-013**: The build MUST print progress (articles read, excluded, passages added,
  elapsed) and, at the end, the counts and phase timings.
- **FR-014**: A slice built by the demo MUST give, for the twenty measurement queries at
  depths 0 / 5 / 10 / 20, the same external ids in the same order as the Rust build of the
  same slice; the check is part of the feature's evidence (a difference is stop-and-report).

**About and measure**

- **FR-015**: About MUST print, from the engine's report and the shipped files: the corpus
  edition and snapshot date, article and passage counts, corpus identity (and whether the
  index is a limited slice), embedder and re-ranker identities, index format version, the
  recorded re-rank mode and depth beside the demo's default (both labelled), this
  session's open and model load times, and the attribution text verbatim with its licence
  link.
- **FR-016**: Measure MUST run the twenty measurement queries at depths 0 / 5 / 10 / 20 over
  the shipped index, compare every hit with the host goldens (external ids, order and
  score bits, by the device check's comparison), and write a record with per-depth latency
  medians and maxima, the footprint, the machine's model name, threads and load path, and
  the verdict — in a shape comparable with the 009/018 records; a parity failure is
  stop-and-report, never a loosened comparison.

**Discipline**

- **FR-017**: The demo's own logic — the missing-input checks; the fused-then-reranked merge
  and change marks; title / passage / URL derivation (the CLI's eleven URL cases); the
  exclusion rules and the chunker against the 008 fixtures; the sidecar and attribution
  writers; the About fields; the output formatting — MUST be covered by tests that run
  against the 40-document 007 fixture index, its goldens and the 008 chunker fixtures,
  needing no snapshot and no 1 GB artefact; the tests MUST be committed failing before the
  code (Principle II).
- **FR-018**: One host measurement record (FR-016) and one slice parity check (FR-014) MUST
  be committed under this feature's `runs/`.
- **FR-019**: No change under `crates/`, `swift/`, the Python package's wire, the formats or
  any baseline; the demo's own dependencies (the package, and the tokenizer library at the
  version the Rust crate pins, for the build) declared in the demo's own manifest; CI runs
  nothing that needs the models, the snapshot or the artefact.
- **FR-020**: The demo README MUST show the commands in the order a person would run them
  (fetch the snapshot and the models, install the package, build a slice, search it,
  explain, about, measure), the inputs and how each is produced, the flags and their
  defaults with the 018 trade-off, the full build's cost, and where the records are; the
  repository README and the 009 spec/report MUST point to it as the second demo.

### Key Entities

- **Search**: the query, the options (depth, budget, strict, mode, k, explain), the fused
  response, the re-ranked response, the wall times, the footprint sample.
- **Displayed hit**: title, passage, article URL, provenance (parent id, ordinal), the eight
  features, fused rank, re-ranked rank, change mark.
- **Build**: the snapshot path and manifest, the limit, the output path, the counts (articles,
  excluded per rule, selected, passages), the phase timings, the corpus identity.
- **Corpus sidecar and attribution**: the 008 shapes, written by the build and read by About.
- **Measurement record**: machine, build, load path, threads, per-depth latency and parity,
  footprint, verdicts — comparable with the 009/018 records.
- **Slice parity record**: N, the two builds' counts, the per-query comparison, the verdict.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On this laptop, after warm-up, a search over the shipped index prints the fused
  list within 1 s and the re-ranked list within 3 s at the default depth 10; the record
  states the medians (measured today: fused ~250 ms, re-ranked ~0.8–1.0 s).
- **SC-002**: Every hit's title, passage, URL and eight features printed equal the engine's
  values for that hit — verified by a test against the 007 fixture goldens.
- **SC-003**: About's identities and counts equal the engine's index information and the
  corpus sidecar; the attribution text equals the shipped file byte for byte.
- **SC-004**: The host measurement record shows parity PASS at depths 0 / 5 / 10 / 20 for all
  twenty queries and a footprint figure; committed verbatim.
- **SC-005**: A demo-built slice of at least 1,000 articles gives the same external ids in the
  same order as the Rust build of the same slice for all twenty queries at all four depths;
  the slice builds in under 30 minutes on this laptop.
- **SC-006**: From a prepared checkout, the first search of a process prints its fused list
  within 5 s of the command; a checkout missing an input is told which one within 1 s.
- **SC-007**: `git diff --stat main -- crates/ swift/ python/src specs/*/baselines` is empty.

## Assumptions

- **The build reproduces the shipped index's recipe** — the 008 schema (`title` boosted
  2.0 beside `text`, the dense field `text`), exclusion rules and chunking contract — so
  that the Rust build is its oracle and the phone's goldens apply to a full build. Feature
  013 measured one joined `contents` field better on the BEIR sets; the demo's README says
  so and shows the one-line schema change for a new corpus, but the demo's build keeps
  parity with the shipped index rather than diverging from it silently.
- **The chunker is the 008 reference implementation**: the contract-verified Python chunker
  the Rust chunker's goldens were minted from, priced with the tokenizer library at the
  pinned version — the same word-piece counts the Rust build sees.
- **Embedding goes through the package** (the engine embeds on add); the Rust build's
  embedding cache is not used, so a slice's build time is its embedding time (~93 ms per
  passage at four threads, 008).
- **Interaction**: a search is one command; the fused list prints before the re-ranked one
  because the demo makes two engine calls, as the iOS demo does. There is no persisted
  settings state — flags are the settings, and their defaults are the demo's defaults.
- **Inputs are produced by the existing tooling**: the wheel by the 011 build, the models by
  `scripts/fetch-model.sh`, the snapshot by `scripts/fetch-wiki.sh`, the shipped artefact by
  `xtriever wiki build`; the demo builds indexes but never fetches.
- **The host goldens are the oracle** for the measurement record (as for the phone) and the
  Rust slice build is the oracle for the demo's build. Score bits are expected to match
  between the two builds too; if they differ while the order holds, that is reported in the
  record, not hidden.
- **The footprint figure is the process's resident size** after a search; the 600 MB ceiling
  is a phone rule, recorded here for comparison only.
- **The demo is run from a checkout**; packaging it for distribution is out of scope.
- **No machine identifier beyond the model name** goes into any record.
