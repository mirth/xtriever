# Feature Specification: The Lexical Stage

**Feature Branch**: `002-lexical-stage`

**Created**: 2026-09-11

**Status**: Draft

**Input**: User description: "Feature 002 — the lexical stage. Implement the `LexicalIndex` trait from `xtriever-core` in the `xtriever-lexical` crate, backed by tantivy 0.26.2 using the C-free feature set Feature 001 measured … Scope is the whole trait … Three things Feature 001 deliberately avoided and this spec cannot: multi-segment indexes, ADR-0005's tie-break contract, and concurrency … Principle II applies in full … Out of scope: the pipeline, fusion, dense retrieval, re-ranking, and anything on-device."

## Why This Spec Reads Technically

Like Feature 001, the "user" here is an Xtriever developer and the subject matter is a Rust trait
contract. `LexicalIndex` and its eight methods are requirement content, not leaked implementation
detail: the feature *is* making that contract true against a real backend. Success criteria stay
measurable and outcome-shaped, and no crate API items are cited — under Agent Operating Rule 1
those must be read from the pinned versions' documentation and cited in `plan.md`.

The difference from 001 is that this is **production code behind a published contract**, not a
throwaway spike. Feature 001's FFI surface is provisional by its own FR-031 and must not be built
on or extended.

## Clarifications

### Session 2026-09-11

- Q: How should a score tie spanning the `k`-th and `k+1`-th positions be handled? → A: Accept the
  backend's ordering at the boundary; do not over-fetch. FR-013's `DocId` tie-break guarantees the
  *order* of the returned set, not its *membership* (FR-014).
- Q: Should `term_stats` and `stats` count deleted-but-not-merged documents? → A: No — live documents
  only, one rule for both (FR-024). `IndexStats::num_docs` is already documented in `xtriever-core`
  as "Live (non-deleted) documents", so this extends an existing core decision to `term_stats` rather
  than inventing one. The divergence from the backend's own scoring statistics is measured and
  recorded, not hidden (FR-025).
- Q: What concurrency model should the stage provide? → A: One handle with exclusive writes;
  `&mut self` keeps its ordinary Rust meaning, with no interior mutability or added locking
  (FR-028). This matches `LexicalIndex`'s existing doc comment, which states that a pipeline needing
  concurrent readers wraps the index in a lock.
- Q: Should this feature add a read-only way to open an index, or should callers wrap the single
  handle in a lock as `xtriever-core` already suggests? → A: No read-only handle. Exactly one index
  type, the simplest interface for a library user, accepting that indexing blocks queries (FR-029).
  An earlier draft required a read-only open path; it was cut here, and the blocking consequence is
  stated in the requirement rather than left to be discovered.
- Q: When a schema says a text field uses analyzer `"standard_en"`, where should the lexical stage
  get that analyzer from? → A: A built-in table. The stage recognizes a small fixed set of
  `AnalyzerId`s mapped onto the backend's own tokenizer chains; an unknown id fails at index creation
  with a schema error, never a silent default (FR-006). The constructor takes only a `Schema`.
  Analyzer pluggability via core's `Analyzer` trait is `xtriever-analysis`'s work, not this feature's.
- Q: When a schema gives a text field a `boost` other than 1.0, which queries should that boost
  affect? → A: Every scored query on that field — `Match`, `Phrase`, `Term`, `Fuzzy` — multiplied by
  the field boost, with `LexicalQuery::Boost` multiplying on top; a non-neutral boost on a non-text
  field is a schema error at creation (FR-005).
- Q: What should the lexical stage do with a document's `chunk` provenance — the `ChunkInfo`
  carrying the parent's external id, ordinal and byte range? → A: Nothing. Not indexed, not stored;
  accepted and ignored, documented as such (FR-008b). `ChunkInfo.parent` is an external string id
  and Principle V keeps those out of backends; grouping by parent is the pipeline's job.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Documents go in and come back ranked (Priority: P1)

A developer constructs an index from a schema, adds a corpus, commits, and runs a query. They get
back the top `k` hits, highest score first, with the scores and ordering the contract promises. This
is the smallest thing that makes `xtriever-lexical` a retrieval stage rather than an empty crate.

**Why this priority**: Nothing else in the pipeline can exist without it. Fusion has nothing to
fuse, re-ranking has nothing to re-rank, and the `Ranker` has no features to score. It is also the
story that proves the schema mapping works, since a query cannot succeed unless fields were indexed
correctly.

**Independent Test**: Fully testable on a host with no device and no other crate. Build a schema,
add the fixture corpus, commit, query, compare against a committed golden ranking.

**Acceptance Scenarios**:

1. **Given** a schema with a text field and a keyword field, **When** an index is created from it,
   **Then** `schema()` returns a schema equal to the one supplied.
2. **Given** a committed corpus, **When** a `Match` query runs with `k = 10`, **Then** exactly ten
   hits return, ordered by descending score.
3. **Given** a query matching fewer than `k` documents, **When** it runs, **Then** every matching
   document returns and no more, without error.
4. **Given** a query matching nothing, **When** it runs, **Then** an empty result returns — this is
   a legitimate outcome at the trait level, unlike the fixture-specific guarantee in Feature 001.
5. **Given** a `Document` whose field is absent from the schema, **When** it is added, **Then** the
   call fails with a schema error naming the field, rather than silently dropping data.
6. **Given** each `LexicalQuery` variant in turn — `Match`, `Phrase`, `Term`, `Fuzzy`, `Bool`,
   `Boost` — **When** it runs against the fixture corpus, **Then** the result matches that
   variant's committed golden.
7. **Given** a schema whose text field names an analyzer id the stage does not recognize, **When** an
   index is created from it, **Then** creation fails with a schema error naming the field and the id
   — no index is produced and no default analyzer is substituted.
8. **Given** a schema with two text fields where one carries `boost = 2.0`, **When** a `Match` query
   with no field runs, **Then** each hit's score equals the reference implementation's per-field
   BM25 sum (FR-017) with the boosted field's contribution doubled — and a `Term` query naming
   only the boosted field returns scores exactly double those of the same query with `boost = 1.0`.
9. **Given** a schema with `boost = 2.0` on a `Keyword` field, **When** an index is created from it,
   **Then** creation fails with a schema error naming the field.
10. **Given** two documents identical except that one carries `ChunkInfo` and the other does not,
    **When** both are indexed and queried, **Then** they score identically, and no term derived from
    the parent id, ordinal or byte range exists anywhere in the index.

---

### User Story 2 - Ties, segments and repeated runs behave identically (Priority: P1)

A developer commits documents in several batches, producing an index with more than one segment,
and gets exactly the same ranking as an equivalent single-segment index — including for documents
whose scores tie. Running the same query twice returns identical results.

**Why this priority**: P1 alongside Story 1 because it is the contract Feature 001 could not test.
001 pinned a single writer thread and asserted one segment purely so its golden was comparable; that
control is not available to production code. ADR-0005 was accepted on the strength of an argument
and has never been executed. If the tie-break is wrong, every downstream ranking is subtly
non-deterministic in a way that only shows up under load.

**Independent Test**: Testable entirely on a host by building the same corpus two ways — one commit
versus several — and comparing rankings. Delivers value independently: it is the first executable
evidence for ADR-0005.

**Acceptance Scenarios**:

1. **Given** a corpus committed in one batch and the same corpus committed in several batches,
   **When** the same query runs against both, **Then** the returned hits are identical in order,
   identity and score.
2. **Given** an index with more than one segment, **When** a query produces score ties, **Then**
   tied hits appear in ascending `DocId` order (ADR-0005).
3. **Given** any index, **When** the same query runs twice with no mutation between, **Then** the
   results are identical.
4. **Given** an index where a merge has occurred, **When** the same query runs before and after the
   merge, **Then** the results are identical.
5. **Given** a tie that spans the `k`-th and `k+1`-th positions, **When** the query runs, **Then**
   *which* of the tied documents come back is the backend's choice (FR-014), while *the order of
   those that do* is still ascending `DocId`. The test asserts exactly that, and not a
   `DocId`-minimal selection the stage does not promise.

---

### User Story 3 - Documents can be replaced and removed (Priority: P2)

A developer adds a document, adds it again with different content under the same `DocId`, commits,
and sees only the new version. They delete documents and those documents stop appearing in results
and stop counting toward corpus statistics.

**Why this priority**: P2 because retrieval is demonstrable without it, but it is not optional for
long: an index that cannot express change is a fixture, not a stage. The trait documents
`add` as add-or-replace and `delete` as ignoring unknown ids, and both are easy to implement subtly
wrong in a segmented store where deletion is a tombstone rather than an erasure.

**Independent Test**: Add, replace, delete, commit, and assert on search results and `stats()`.

**Acceptance Scenarios**:

1. **Given** a committed document, **When** a document with the same `DocId` and different content
   is added and committed, **Then** searches return the new content and the document appears once.
2. **Given** a committed document, **When** it is deleted and committed, **Then** it no longer
   appears in any search result.
3. **Given** a delete for a `DocId` that was never added, **When** it is committed, **Then** the
   call succeeds and nothing changes.
4. **Given** deletions that have been committed, **When** `stats()` is called, **Then**
   `num_docs` counts live documents only.
5. **Given** mutations that have not been committed, **When** a search runs, **Then** it does not
   observe them.

---

### User Story 4 - Filters restrict results and can be shared with other stages (Priority: P2)

A developer expresses a constraint over metadata — equality, set membership, ranges, existence, and
boolean combinations — and either applies it to a search or resolves it into a document set to hand
to another stage.

**Why this priority**: P2 because search works without it, but `xtriever-core` assigns metadata and
filter evaluation to the lexical index in v0, so no other crate can supply it. `resolve_filter`
exists precisely so a dense or vector stage can be constrained by the same predicate.

**Independent Test**: Build a corpus with known metadata, resolve each filter shape, and compare
against sets computed independently from the fixture.

**Acceptance Scenarios**:

1. **Given** a corpus with metadata fields, **When** each filter shape is resolved in turn — `Eq`,
   `In`, `Range`, `Exists`, `And`, `Or`, `Not`, `Ids` — **Then** each returns exactly the document
   set the fixture defines.
2. **Given** a filter and a query, **When** the search runs with both, **Then** the results are the
   query's results restricted to the filter's set, with scores unchanged by the restriction.
3. **Given** a filter over a field absent from the schema, **When** it is resolved, **Then** the
   call fails naming the field rather than returning an empty set — an empty set and an unknown
   field are different answers and must not be conflated.
4. **Given** a `Range` filter with an open bound, **When** it is resolved, **Then** the open side is
   unbounded and the closed side is inclusive of its endpoint.
5. **Given** a filter whose value type does not match the field's kind, **When** it is resolved,
   **Then** the call fails with a type error rather than coercing silently.
6. **Given** deleted documents that would otherwise match, **When** a filter is resolved, **Then**
   they are absent from the result.

---

### User Story 5 - Corpus statistics are available for scoring (Priority: P3)

A developer reads term-level and corpus-level statistics — document frequency, total term frequency,
document count, average field length — so that a later stage can compute features or re-derive
scores.

**Why this priority**: P3 because nothing in this feature consumes them yet. They exist because the
`Ranker` and the fusion stage will need them, and because they are nearly free to expose once the
index exists. Getting them wrong is cheap now and expensive later, when a feature vector silently
carries a wrong IDF.

**Independent Test**: Compare against statistics computed independently from the fixture corpus.

**Acceptance Scenarios**:

1. **Given** an indexed term, **When** `term_stats` is called, **Then** document frequency and total
   term frequency match the values computed from the fixture.
2. **Given** a term absent from the corpus, **When** `term_stats` is called, **Then** it returns
   nothing rather than zeroes — "unseen" and "seen zero times" are different answers.
3. **Given** a committed corpus, **When** `stats()` is called, **Then** `num_docs` and the average
   field lengths match the fixture.
4. **Given** documents that have been deleted and committed but not merged away, **When** statistics
   are read, **Then** both `term_stats` and `stats` report the live corpus only (FR-024) — a term
   occurring solely in deleted documents reports as unseen.
5. **Given** two indexes holding the same live documents, one built by adding them and the other by
   adding more and deleting the difference, **When** statistics are read from both, **Then** they
   agree.
6. **Given** an index with pending deletions, **When** `term_stats` is compared against the document
   frequency the backend actually used to score the same query, **Then** the divergence is measured
   and recorded (FR-025) rather than either number being quietly adjusted to match the other.

---

### User Story 6 - One index can be shared across threads under a lock (Priority: P3)

A developer puts one index behind a read-write lock, queries it from several threads, and indexes
through it from another. Nothing corrupts and no query sees a half-applied commit. The cost is the
one this design admits openly: queries wait while an indexing batch holds the write lock.

**Why this priority**: P3 because nothing in this feature consumes it yet. It is nevertheless
required rather than optional, because FR-028 and FR-029 push the writer/reader split onto the
caller, and a caller can only perform that split if the index is genuinely safe to share this way. An
index that satisfies `Send + Sync` in its signature but carries hidden shared state would make the
whole decision unsound while appearing to work.

**Independent Test**: Wrap one index in a lock, drive reads and writes from several threads, and
assert on what the readers see. Separately, open the same directory twice and assert on the outcome.

**Acceptance Scenarios**:

1. **Given** one index behind a read-write lock, **When** several threads query it concurrently,
   **Then** all queries succeed and return the same results they would return single-threaded.
2. **Given** one index behind a read-write lock, **When** a thread indexes and commits while others
   query, **Then** every query returns a result consistent with a single commit — the state before or
   the state after, never a mix.
3. **Given** two indexes open in one process on different directories, **When** both are used,
   **Then** neither affects the other's results, statistics or ranking.
4. **Given** a directory already open, **When** it is opened a second time, **Then** the outcome is
   whatever FR-031 records — succeeding or failing with a named `xtriever-core` error — and it is the
   same outcome every time rather than a race.
5. **Given** a search running while a background merge is in progress, **When** it returns, **Then**
   its result is consistent with a single commit.

---

### Edge Cases

- A document carries a value whose type does not match its field's declared kind.
- A document omits a field the schema declares; a document is added with no fields at all.
- Two documents are added with the same `DocId` within a single uncommitted batch.
- `search` is called with `k = 0`, or with `k` larger than the corpus.
- A `Fuzzy` query with a distance large enough to match most of the vocabulary.
- A `Phrase` query on a `Keyword` field — it carries no positions, so the query cannot be
  satisfied and must say so rather than returning silence.
- Two text fields in one schema name different analyzer ids; a query with no field (`Match(None, …)`)
  spans both.
- A `Boost` wrapping a `Bool` whose `should` clauses touch fields with different field boosts.
- A field boost of `0.0`, or a negative one.
- A `Bool` query with no clauses at all, or with only negative clauses.
- `Not` applied to a filter matching everything, or to one matching nothing.
- An index directory that exists but was written by a different schema or a different format
  version.
- Two processes, or two handles in one process, opening the same index directory at once.
- The process is interrupted between `add` and `commit`; the index must open cleanly afterwards
  without the uncommitted documents.
- A merge runs concurrently with a search.
- A field declared `indexed: false` is queried; a field declared `stored: false` is read back.

## Requirements *(mandatory)*

### Functional Requirements

**The contract**

- **FR-001**: `xtriever-lexical` MUST provide a type implementing `xtriever-core`'s `LexicalIndex`
  trait in full: `schema`, `add`, `delete`, `commit`, `search`, `resolve_filter`, `term_stats`,
  `stats`.
- **FR-002**: The `xtriever-core` traits, types and error semantics MUST NOT change. If the
  implementation cannot satisfy the contract as written, the work stops and an ADR is raised rather
  than the contract being adjusted to fit. The single exception is the doc-comment amendment FR-014
  requires, which changes no signature, type or behaviour and is itself ADR-gated.
- **FR-003**: All errors MUST be `xtriever-core`'s existing `Error` variants. A failure mode with no
  suitable variant is a contract question, not a licence to invent one locally.
- **FR-004**: The crate MUST remain pure Rust with no C/C++ build dependencies, no `async`, no
  `tokio`, and no `std::time::Instant` in library code, verified per target rather than assumed.
  Threads are **not** forbidden: the constitution's "no unconditional threads" rule names five pure
  crates and `xtriever-lexical` is deliberately not among them, because the backend's writer is
  thread-based by design. The stage MUST instead fix every thread count the backend exposes to its
  minimum (one indexing worker, one merge worker) so the count is a constant of the crate rather
  than a function of the host, and MUST NOT spawn threads of its own. *(An earlier draft of this
  requirement forbade threads outright; that was stricter than the constitution and unsatisfiable
  by the chosen backend, and was corrected during planning.)*

**Schema and documents**

- **FR-005**: An index MUST be creatable from an `xtriever-core` `Schema`, honouring each
  `FieldDef`'s `kind`, `indexed`, `stored` and `boost`. `boost` is a **query-time multiplier applied
  uniformly**: every scored query touching a text field — `Match`, `Phrase`, `Term`, `Fuzzy` — has
  its score multiplied by that field's boost, and `LexicalQuery::Boost` multiplies on top, so the two
  compose as a product. Within one field a constant multiplier cannot reorder hits; its effect is in
  `Match(None, …)`, where fields compete. A `boost` other than 1.0 on a non-text field MUST fail at
  index creation with a schema error naming the field, since core defines boost for text fields only
  and silently ignoring it would be a silent drop (FR-007). The reference implementation (FR-016)
  MUST model both boosts, or the goldens cannot be independently checked.
- **FR-006**: Every `FieldKind` MUST be supported: `Text(AnalyzerId)`, `Keyword`, `U64`, `I64`,
  `F64`, `Bool`, `DateMillis`. A `Text` field's `AnalyzerId` MUST be resolved against a **built-in
  table** of ids this stage recognizes, each mapped onto one of the backend's own tokenizer chains
  and documented in the plan with the exact chain it names. The table MUST include the id whose
  behaviour Feature 001's reference transcription already covers, so that oracle carries over
  unchanged. An id absent from the table MUST fail at index creation with a schema error naming the
  field and the id; falling back to a default analyzer is forbidden, because a silently substituted
  analyzer changes every score while every test still passes. The constructor takes a `Schema` and
  nothing else — no analyzer registry, no caller-supplied `Analyzer` implementations. Pluggable
  analyzers through core's `Analyzer` trait are `xtriever-analysis`'s concern and out of scope here.
- **FR-007**: A `Document` field absent from the schema, or carrying a `Value` whose type does not
  match the field's `FieldKind`, MUST fail with a schema error naming the field. Silent coercion and
  silent dropping are both forbidden.
- **FR-008**: `add` MUST replace any existing document with the same `DocId`, observable after
  `commit`.
- **FR-008a**: A field declared `stored: true` MUST have its original value written to the index and
  survive a round-trip through `commit` and reopen. This feature provides **no read path** for it:
  `Hit` carries only `id` and `score`, `LexicalIndex` has no method returning stored values, and
  adding one is a core change FR-002 forbids. The value is stored so that a later feature can add
  retrieval without re-indexing; the absence of a read path is recorded here as a named gap, and the
  round-trip MUST still be tested through whatever inspection the backend offers so the write is not
  silently lost.
- **FR-008b**: `Document.chunk` MUST be **neither indexed nor stored**. The stage accepts a document
  with or without `ChunkInfo` and treats the two identically; this is documented on the index type
  so it is a stated contract rather than a discovered omission. `ChunkInfo.parent` is an external
  string id, which Principle V keeps out of every backend; grouping hits by parent is the pipeline's
  job through its own document store, exactly as core's doc comment on `ChunkInfo` assigns it. A
  later feature wanting parent-aware behaviour inside this stage re-indexes; that is the accepted
  cost of keeping an external id off a backend's disk. This is not a silent drop in FR-007's sense:
  `chunk` is outside the schema, and the behaviour is specified and tested.
- **FR-009**: `delete` MUST remove documents by `DocId` and MUST ignore ids that are not present.
- **FR-010**: Mutations MUST NOT be visible to `search`, `resolve_filter`, `term_stats` or `stats`
  until `commit` returns.
- **FR-011**: An index MUST survive process restart: reopening a directory MUST expose exactly the
  committed state, and uncommitted mutations MUST be absent.
- **FR-012**: An index directory MUST carry a format version and the schema it was created with.
  Opening it with a mismatched version or schema MUST be a hard error at open time, never a silent
  reinterpretation.

**Ranking and determinism**

- **FR-013**: `search` MUST return at most `k` hits ordered by descending score, with ties broken by
  **ascending `DocId`** (ADR-0005), regardless of how many segments the index has or how documents
  are distributed across them.
- **FR-014**: A score tie spanning the `k`-th and `k+1`-th positions MUST be resolved by the
  backend's own ordering, not by `DocId`. The stage MUST NOT over-fetch to close this gap. FR-013's
  `DocId` tie-break is therefore a guarantee about the **order of the returned set**, not about
  **which documents are in it**: given a tie group straddling the boundary, the members that appear
  are whichever the backend selected, and those that appear are ordered by ascending `DocId`. This
  MUST be tested as specified behaviour, not left undefined.

  This narrows a promise `xtriever-core` currently states without qualification, so it MUST be
  recorded before the implementation lands: ADR-0005 is amended with the decision and its rationale,
  and `LexicalIndex::search`'s doc comment is amended to state the boundary caveat. That doc comment
  is the **only** permitted change to `xtriever-core` in this feature — documentation only, no
  change to any signature, type or behaviour — and it is permitted solely because the ADR amendment
  satisfies Principle V's gate. Any other core change stops the work (FR-002).
- **FR-015**: The same index, query, filter and `k` MUST produce identical results across repeated
  calls, across process restarts, and across segment layouts that contain the same live documents
  **reached by the same mutation history**. Whether two indexes holding identical live documents but
  reached by *different* mutation histories — one built by adding 1,000 documents, the other by
  adding 1,100 and deleting 100 — also agree MUST be measured and reported. It is expected that they
  do not, because scoring consumes the backend's corpus statistics and those retain deleted documents
  until a merge (see FR-024). If they disagree, that is a Principle VI finding to be recorded with
  its magnitude, not a reason to loosen this requirement (Agent Operating Rule 6).
- **FR-016**: Scoring MUST be verified against an independent reference implementation, not only
  against the backend's own output, extending Feature 001's Python BM25 transcription to the query
  shapes this feature adds.
- **FR-017**: Every `LexicalQuery` variant MUST be supported: `Match` (with and without a field),
  `Phrase` (with slop), `Term`, `Fuzzy`, `Bool` (must / should / must_not), and `Boost`. `Match`
  follows core's documented semantics — the text is analyzed by the field's analyzer and the
  resulting terms are OR-ed — and `Match(None, …)` extends that disjunction across every indexed text
  field, analyzing the text once per field with that field's analyzer. A hit's score is therefore the
  **sum** of its per-term, per-field contributions, each multiplied by its field's boost (FR-005).
  `Fuzzy` uses the distance core names — **Levenshtein**, in which a transposition costs two edits,
  not one — with no prefix exemption; both are stated here because the backend offers them as knobs
  and a golden cannot be independently checked unless the setting is fixed.
- **FR-018**: A query that cannot be satisfied by the schema MUST fail with an error naming the
  field, rather than returning an empty result. Because `FieldDef` carries no positions switch, the
  rule is derived from `FieldKind`: every indexed `Text` field records term positions, so `Phrase`
  is always satisfiable on one — the index-size cost of that is accepted; `Keyword` fields are not
  analyzed and carry no positions, so `Phrase` on a `Keyword` field is an invalid query; and any
  query on a field declared `indexed: false` is invalid regardless of kind.

**Filters**

- **FR-019**: Every `Filter` variant MUST be supported: `Eq`, `In`, `Range`, `Exists`, `And`, `Or`,
  `Not`, `Ids`.
- **FR-020**: `resolve_filter` MUST return exactly the set of live documents satisfying the
  predicate, excluding deleted documents.
- **FR-021**: A filter over an unknown field, or with a value whose type does not match the field's
  kind, MUST fail rather than returning an empty set.
- **FR-022**: Applying a filter to a search MUST restrict which documents can be returned without
  altering the scores of those that are.
- **FR-023**: The filter algebra MUST be property-tested, which Principle II names explicitly. At
  minimum: `And`/`Or` are commutative and associative over their operands, `Not` is an involution,
  and a filter resolved to a set agrees with the same filter applied during search.

**Statistics**

- **FR-024**: `term_stats` and `stats` MUST both report **live documents only**, excluding documents
  that are deleted but not yet merged away. One rule, no exceptions, matching FR-020's treatment of
  filters and Story 3's live `num_docs`. Statistics MUST be a function of the live corpus alone, so
  that two indexes holding the same live documents report the same statistics regardless of how many
  deletions each absorbed getting there.
- **FR-025**: FR-024 MUST be reconciled with scoring rather than left as a silent inconsistency. The
  backend computes BM25 from its own corpus statistics, which retain deleted documents until a merge,
  so a document frequency read from `term_stats` may not be the one that produced the scores from
  `search` on the same index. The size of that divergence MUST be measured on an index with pending
  deletions and reported. Whichever way it goes, the result is recorded: agreement closes the
  question; disagreement is a finding with a documented magnitude and an ADR if it constrains the
  `Ranker`'s feature extraction. Reporting `term_stats` as-scored instead, to make the numbers match,
  is explicitly not the escape hatch — that is the option FR-024 rejected.
- **FR-026**: `term_stats` MUST return nothing for an unseen term, distinguishably from a term seen
  zero times.
- **FR-027**: `stats` MUST report the live document count and per-field average lengths, consistent
  with FR-024.

**Concurrency**

- **FR-028**: The stage MUST provide **one handle with exclusive writes**: `&mut self` means what
  Rust says it means. A single writer at a time, enforced by the borrow checker rather than by a
  runtime lock; concurrent reads through shared references permitted; **no interior mutability and no
  locking added to satisfy the trait**. Splitting writer and reader into separate types is explicitly
  not done here — the trait is a floor rather than a ceiling, but adding public API beyond it is
  design work this feature does not undertake.
- **FR-029**: The stage MUST expose exactly **one** index type. No read-only handle, no separate
  reader type, no second open path. A caller needing concurrent readers wraps the index in a lock, as
  `LexicalIndex`'s doc comment already instructs. The consequence is stated rather than discovered:
  under a read-write lock an `add` + `commit` batch **blocks every query for its full duration**, so
  this stage cannot serve queries while indexing. That is accepted here as the price of the smaller
  interface; if a later feature needs concurrent serving, adding a reader type is that feature's work
  and does not require changing this one's contract.
- **FR-030**: The index type MUST be safely shareable under a caller-supplied lock — `Send + Sync` as
  the trait requires, with no hidden global or thread-local state that would make two indexes in one
  process interfere. Reads MUST NOT observe a torn or partial commit: a search returns a result
  consistent with a single commit, including while a background merge is running.
- **FR-031**: The behaviour when two handles open the same index directory simultaneously MUST be
  defined and tested, rather than left to the backend's locking to surface through an opaque error.
  Where the attempt must fail, it MUST fail with an `xtriever-core` `Error` variant, and the spec's
  plan MUST record which variant — the enum is `#[non_exhaustive]` with a `Backend` escape hatch, so
  "there is no suitable variant" is not a reason to add one (FR-003).

**Verification**

- **FR-032**: Acceptance tests MUST be written and committed **failing** before the implementation,
  and MUST fail for want of an implementation rather than for want of a fixture.
- **FR-033**: Golden fixtures MUST be generated by committed scripts in `reference/`, with the
  tolerance for each comparison stated in this spec, and MUST be tamper-evident in the way Feature
  001 established.
- **FR-034**: Analyzer determinism, index round-trips and the filter algebra MUST be property-tested
  (Principle II).
- **FR-035**: A regression that changes any committed golden MUST fail the suite. Regenerating a
  golden to make a test pass is forbidden without a recorded justification.

**Scope boundaries**

- **FR-036**: This feature MUST NOT implement the pipeline, fusion, dense retrieval, re-ranking,
  or LTR, and MUST NOT modify `xtriever-core` beyond the single doc-comment amendment FR-014 permits.
- **FR-037**: This feature MUST NOT build on Feature 001's FFI surface, which **Feature 001's**
  FR-031 declares provisional. Nothing in `xtriever-ffi` is a dependency of this work.
- **FR-038**: Index memory scaling is **explicitly deferred** and MUST NOT be measured here. Feature
  001 recorded 12.1 MB of footprint for 1,000 documents and that a naive ×100 extrapolation reaches
  roughly 1.2 GB against the constitution's 300 MB ceiling; that extrapolation is recorded as an open
  question and is not evidence of a problem, because the curve was never measured. Deciding it here
  would require a corpus two orders of magnitude larger than the correctness fixtures, which would
  make this feature about measurement rather than about the contract. The deferral is a decision, not
  an omission: this spec states it so a later spec inherits a question rather than an assumption.
- **FR-039**: On-device execution is out of scope. Cross-compilation for the iOS and Android targets
  MUST still be verified, because Principle III requires it of every PR, but nothing needs to run on
  hardware.

### Key Entities

- **Lexical Index**: The stage's central object. Owns a schema, a set of live documents, and the
  on-disk artifacts backing them. Created from a schema, reopened from a directory.
- **Index Descriptor**: What an index directory records about itself — a format version and the
  schema it was created with — so that opening it can fail loudly on mismatch (FR-012).
- **Document**: `xtriever-core`'s `Document`: a `DocId`, a field map, and optional chunk
  information. The `DocId` is supplied by the caller and is the identity used for replacement,
  deletion and tie-breaking. The chunk information is accepted and ignored (FR-008b).
- **Fixture Corpus**: Committed, deterministic documents with known text and metadata, generated
  from a fixed seed. Feature 001's corpus is the starting point; this feature needs metadata fields
  it does not have.
- **Query Golden**: For each supported query shape, the expected ranking, committed and compared
  exactly.
- **Filter Golden**: For each filter shape, the expected document set, computed independently of the
  implementation under test.
- **Statistics Golden**: Expected term and corpus statistics, computed from the fixture.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All eight `LexicalIndex` methods are implemented and exercised by committed tests —
  8 of 8, with no method left to a future feature.
- **SC-002**: Every `LexicalQuery` variant and every `Filter` variant has at least one committed
  golden — 6 of 6 query shapes, 8 of 8 filter shapes.
- **SC-003**: A corpus committed as one batch and the same corpus committed as several produce
  identical rankings for every query golden — 0 differences in order, identity or score.
- **SC-004**: Repeating any query on an unchanged index returns identical results, across process
  restarts as well as within one process — 0 differences.
- **SC-005**: Scores agree with the independent reference implementation within the tolerance this
  spec states, for every query shape that the reference covers.
- **SC-006**: The filter algebra's stated properties hold under property testing with no falsifying
  case found in a run of at least 1,000 generated cases per property.
- **SC-007**: A developer can construct an index, add documents and retrieve ranked results using
  only `xtriever-core` types and this crate's constructor — no other Xtriever crate is required.
- **SC-008**: The crate compiles for the host and for `aarch64-apple-ios`, `aarch64-apple-ios-sim`
  and `aarch64-linux-android`, with zero C/C++ build dependencies reachable from it.
- **SC-009**: Every error path named in the requirements is reachable in a test and returns the
  documented error rather than a panic — 0 panics in library code.
- **SC-010**: The acceptance suite is committed failing before the implementation, and the failures
  are attributable to missing implementation rather than broken fixtures — 0 fixture-caused failures
  at that checkpoint.
- **SC-011**: Two indexes holding the same live documents, one built by adding them and the other by
  adding a superset and deleting the difference, report the same statistics — 0 differences in
  `num_docs`, average field lengths, or any term's document and total term frequency.
- **SC-012**: The divergence between `term_stats` and the statistics the backend used for scoring is
  measured on an index with pending deletions and recorded as a number, whether or not it is zero.
- **SC-013**: One lock-wrapped index driven by concurrent readers and a writer across a test that
  commits at least twice returns a single-commit-consistent view on every read — 0 torn reads — and
  the stage exposes exactly one index type, with 0 additional handle or reader types.

## Assumptions

- **The index is on-disk and memory-mapped.** Feature 001 established that the memory-mapped
  directory works on iOS and that dropping it would remove the on-disk path entirely. An in-memory
  index is assumed to exist only as a testing convenience.
- **The backend is tantivy 0.26.2 with Feature 001's measured C-free feature set** — default
  features minus the one that pulls a C dependency. Changing the version or the feature set is a
  measured decision, not a casual upgrade, and Feature 001's `deny.toml` bans remain in force.
- **`DocId` is supplied by the caller** and is the identity for replacement, deletion and
  tie-breaking. The stage maps it to whatever internal identifier the backend uses; external string
  ids are the pipeline's concern and never reach this crate.
- **Feature 001's fixture and oracle machinery is extended, not reinvented** — the `reference/`
  generator, the manifest hashing that makes goldens tamper-evident, and the independent Python BM25
  transcription that agreed with the backend to within 8.3e-08.
- **Scoring is BM25 with the backend's non-configurable parameters.** Feature 001 established that
  `k1` and `b` are private constants in tantivy 0.26.2, so no scoring configuration can drift, and
  none is exposed.
- **Score comparisons against the independent reference use a relative tolerance of 1e-5**, as
  Feature 001 established; comparisons of the implementation against its own committed goldens are
  exact.
- **Analyzers come from a built-in table, not from a plug-in point.** `FieldKind::Text` names an
  `AnalyzerId`, and this stage resolves it against a fixed set mapped onto the backend's own
  tokenizer chains (FR-006). The id covering the backend's default chain — the one Feature 001
  transcribed and verified — is in that table, so 001's oracle applies directly. Core's `Analyzer`
  trait is not wired in here; making analyzers pluggable is `xtriever-analysis`'s job.
- **No performance budget is set.** Feature 001 deliberately set none and produced baselines
  instead; this feature is about correctness of the contract. Any performance claim would need
  `criterion` benchmarks and a stated budget, which is a later spec's work.
