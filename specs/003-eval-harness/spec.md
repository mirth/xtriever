# Feature Specification: The Evaluation Harness

**Feature Branch**: `003-eval-harness`

**Created**: 2026-09-12

**Status**: Draft

**Input**: User description: "Feature 003" — the evaluation harness proposed at the close of Feature 002: implement `xtriever-eval` so that ranking quality can be measured on the constitution's fixed benchmark set (BEIR SciFact, NFCorpus, FiQA) as nDCG@10 and Recall@100, record the first absolute baseline for the lexical stage against the commit ADR-0006 names, and restore the eval gate that ADR-0006 deferred.

## Why This Spec Reads Technically

As with Features 001 and 002, the "user" is an Xtriever developer and the subject matter is
measurement machinery named by the constitution: Principle II fixes the benchmark set and the two
metrics; the Quality Gates table names the CI smoke; ADR-0006 names the commit to be measured. Those
are the requirement, not leaked implementation. No crate API items are cited; under Agent Operating
Rule 1 they belong in `plan.md`.

What is different here is that this feature **produces numbers that other features will be judged
by**. A wrong metric implementation would silently misjudge every later stage, so the metric
computation itself is verified against an independent reference the same way BM25 was in 001 and
002.

## Clarifications

### Session 2026-09-12

- Q: Which configuration is *the* baseline? → A: One, `lexical-baseline-v1` — title + body under
  `standard_en`, title boost 2.0, `Match(None, query)`, `k = 100`. Others informational only (FR-016).
- Q: Self-validate against BEIR's published BM25 figures? → A: Yes, as a harness-validity check
  with a ±0.10 nDCG@10 band per dataset; published figures pinned with their source at planning,
  a miss is a finding, never a widened band (FR-020).
- Q: Is the CI smoke job in scope? → A: Yes, blocking, on the ranking crates' paths, with a
  hash-verified dataset cache. May be demoted to non-blocking for one feature cycle if flaky, by a
  commit that names the re-blocking point (FR-023). The repository owner noted this may be revisited.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A developer measures a retriever on a benchmark and gets trustworthy numbers (Priority: P1)

A developer points the harness at a retrieval stage and a named benchmark dataset. The harness
indexes the corpus, runs every test query, scores the results against the relevance judgements,
and reports nDCG@10 and Recall@100 — numbers that agree with the reference implementation the
retrieval community uses.

**Why this priority**: This is the feature. Without correct metrics nothing else in the spec means
anything, and the constitution's "measured, not asserted" principle has no instrument.

**Independent Test**: Fully testable on a host with no network, using small synthetic runs whose
metric values are computed independently and committed as goldens; the real datasets are a
separate story.

**Acceptance Scenarios**:

1. **Given** a set of ranked results and relevance judgements computed by the reference
   implementation, **When** the harness scores the same inputs, **Then** its nDCG@10 and Recall@100
   agree with the reference to the tolerance this spec states, for every golden case.
2. **Given** a query with no relevant documents in the judgements, **When** it is scored, **Then**
   it is handled exactly as the reference handles it, and the choice is documented — never silently
   counted as a perfect or a zero score by accident.
3. **Given** a query for which the retriever returns fewer than 10 (or 100) results, **When** it is
   scored, **Then** the metric is computed over what was returned, as the reference does.
4. **Given** graded relevance judgements (values above 1), **When** nDCG is computed, **Then** the
   gain formula matches the reference's, and the golden set includes at least one graded case.
5. **Given** the same retriever and dataset, **When** the evaluation runs twice, **Then** the
   reported numbers are bit-identical.
6. **Given** a retriever that returns results for a query id absent from the judgements, **When**
   scored, **Then** the harness follows the reference's treatment and says which it chose.

---

### User Story 2 - The three benchmark datasets are obtained, pinned and loaded reproducibly (Priority: P1)

A developer runs one command that fetches SciFact, NFCorpus and FiQA into a local cache, verifies
each file against a recorded size and content hash, and loads corpus, queries and judgements in the
form the harness needs. A second developer on another machine gets byte-identical inputs.

**Why this priority**: P1 alongside Story 1 because a metric over unpinned data is not
reproducible, and Principle IV requires pinned dataset versions. It is the same discipline Feature
001 applied to model weights.

**Independent Test**: Fetch into an empty cache, verify hashes, load, and assert document, query
and judgement counts against the values this spec records.

**Acceptance Scenarios**:

1. **Given** an empty cache, **When** the fetch runs, **Then** each dataset is downloaded once,
   its archive and extracted files match the recorded hashes, and a manifest records what was
   verified.
2. **Given** a populated cache, **When** the fetch runs again, **Then** nothing is downloaded and
   the hashes are re-verified.
3. **Given** a file whose hash does not match, **When** loading is attempted, **Then** it fails
   naming the file and both hashes — never a warning, never a silent reload.
4. **Given** a loaded dataset, **When** counts are read, **Then** documents, queries and judgement
   pairs equal the numbers recorded in this spec's Key Entities.
5. **Given** the datasets, **When** they are loaded, **Then** every judged query exists in the
   query set and every judged document exists in the corpus, or the harness reports which do not.
6. **Given** the repository, **When** it is inspected, **Then** no dataset content is committed —
   only hashes, counts and the fetch script.

---

### User Story 3 - The lexical stage's baseline is recorded against the commit ADR-0006 names (Priority: P2)

A developer runs the full evaluation of the lexical stage on all three datasets and records the
absolute nDCG@10 and Recall@100 per dataset, with the configuration that produced them, as the
baseline every future ranking change is compared against.

**Why this priority**: This discharges ADR-0006 condition 2 and gives the project its first
retrieval-quality number. It is P2 only because it is meaningless until Stories 1 and 2 are
trustworthy.

**Independent Test**: Run the evaluation; the recorded report contains one number per metric per
dataset per configuration, plus the commit hashes it refers to.

**Acceptance Scenarios**:

1. **Given** the three datasets and the lexical stage as merged at `94ddbe6`, **When** the
   baseline evaluation runs, **Then** it reports nDCG@10 and Recall@100 for each dataset and each
   baseline configuration, and records both the lexical commit and the harness commit.
2. **Given** the baseline report, **When** it is read, **Then** the evaluation configuration is
   complete enough to reproduce the run exactly: which fields were indexed, which analyzer, how a
   query text becomes a query, `k`, and the dataset hashes.
3. **Given** the baseline run repeated on the same commit, **When** the reports are compared,
   **Then** every number is identical.
4. **Given** the FiQA run (the largest corpus), **When** it completes, **Then** the index directory
   size and the process's peak memory are **recorded as observations** — the first data point on
   the curve Feature 002 deferred — without any requirement being attached to them.

---

### User Story 4 - A ranking-affecting change gets its delta automatically (Priority: P2)

A developer changing a ranking stage runs the harness before and after, and gets a delta table —
nDCG@10 and Recall@100 per dataset, absolute and relative — in a form that can be pasted into a
PR description, as Agent Operating Rule 5 requires. In CI, the SciFact smoke runs on every change
to a ranking crate and fails the build when the metrics fall below the recorded baseline.

**Why this priority**: This is what restores the gate ADR-0006 deferred. It is P2 because the
numbers must exist (Stories 1–3) before deltas can.

**Independent Test**: Compute the delta between two committed reports and compare to a golden
delta table; run the smoke against a report with deliberately lowered numbers and see it fail.

**Acceptance Scenarios**:

1. **Given** two evaluation reports, **When** the delta is computed, **Then** the table shows each
   metric's before, after, absolute and relative change per dataset, in a stable text format.
2. **Given** a change that lowers nDCG@10 on SciFact below the recorded baseline, **When** the smoke
   runs, **Then** it fails and names the metric and the two values.
3. **Given** a change that leaves the metrics unchanged or raises them, **When** the smoke runs,
   **Then** it passes and prints the delta table.
4. **Given** a change to a crate outside the ranking set, **When** CI runs, **Then** the smoke does
   not run.
5. **Given** the constitution's rule that a change lowering nDCG@10 on the majority of datasets
   needs an ADR, **When** the full three-dataset delta is computed, **Then** the report states
   whether that rule is triggered.

---

### Edge Cases

- A judgement references a document id absent from the corpus, or a query absent from the query
  set (BEIR data has known instances of the former).
- A query text is empty after analysis, so the retriever returns nothing.
- Two documents tie at rank 10 or rank 100 — the tie-break is the retriever's (Feature 002 made it
  deterministic), and the metric must be computed over exactly the returned order.
- A dataset archive downloads but extraction produces an unexpected file layout.
- The cache directory is read-only, missing, or on a different filesystem than expected.
- The retriever returns duplicate document ids for one query.
- A judgement file contains a relevance grade of 0 (BEIR includes explicit zeros in some sets).
- The download source is unreachable — the harness must fail clearly, never fall back to a partial
  or differently-versioned copy.
- The evaluation is run against an index built by a different lexical configuration than the one
  the report claims.

## Requirements *(mandatory)*

### Functional Requirements

**Metrics**

- **FR-001**: The harness MUST compute **nDCG@10** and **Recall@100** for a set of queries given
  ranked results and relevance judgements, and MUST report the mean over queries per dataset.
- **FR-002**: Both metrics MUST be verified against an independent reference implementation
  through committed golden fixtures generated by a script in `reference/`, with the tolerance
  stated in Assumptions. The goldens MUST include: a query with no relevant documents, a query with
  fewer results than the cutoff, graded relevance, a result for an unjudged query, duplicate result
  ids, and a run with ties at the cutoff.
- **FR-003**: The treatment of queries with no relevant documents and of results for unjudged
  queries MUST match the reference implementation's and MUST be stated in the harness's
  documentation, because the two choices change the mean.
- **FR-004**: Metric computation MUST be deterministic: the same inputs MUST produce bit-identical
  output regardless of input ordering, and the mean MUST be computed in a fixed order.
- **FR-005**: The harness MUST NOT depend on any retrieval stage crate to compute metrics; the
  metric layer takes ranked ids and judgements only, so it can score any stage — lexical now,
  dense and fused later.

**Datasets**

- **FR-006**: The harness MUST obtain BEIR **SciFact**, **NFCorpus** and **FiQA-2018** from their
  published source into a local, git-ignored cache, and MUST verify every archive and every
  extracted file it reads against a **recorded size and content hash** before use (Principle IV).
- **FR-007**: A hash mismatch MUST be a hard error naming the file and both hashes. The harness
  MUST NOT download a different version, retry with a different URL, or proceed with a warning.
- **FR-008**: Dataset content MUST NOT be committed to the repository; hashes, counts, source URLs
  and the fetch script are.
- **FR-009**: Loading MUST produce documents (id, title, text), queries (id, text) and judgements
  (query id, document id, grade), and MUST report the counts. The counts for each dataset are
  recorded in Key Entities and MUST be asserted.
- **FR-010**: Judgements referencing an unknown document or query MUST be reported by count and
  handled the way the reference implementation handles them; they MUST NOT abort the run.
- **FR-011**: The BEIR **test** split of judgements MUST be used for every dataset, since that is
  what published BEIR numbers refer to.

**Running a retriever**

- **FR-012**: The harness MUST be able to evaluate any implementation of `xtriever-core`'s
  `LexicalIndex` given an **evaluation configuration**: which document fields are indexed under
  which analyzer and boost, how a query text is turned into a query, and `k` (which MUST be at
  least 100 so Recall@100 is well-defined).
- **FR-013**: The evaluation configuration MUST be part of the report, complete enough that a
  reader can reproduce the run without consulting the code.
- **FR-014**: The document id used by the harness MUST be the dataset's external string id; the
  mapping to internal ids is the harness's own and MUST NOT leak into any backend (Principle V).
- **FR-015**: An evaluation run MUST be reproducible: two runs of the same configuration on the
  same commit and dataset hashes MUST produce identical reports (FR-004 plus Feature 002's
  determinism guarantees).
- **FR-016**: The lexical baseline MUST be evaluated with exactly **one** named configuration,
  `lexical-baseline-v1`: `title` and `body` indexed as text under the `standard_en` analyzer, title
  boost 2.0, body boost 1.0, query = a match over both fields of the query text as given, `k = 100`.
  One number per metric per dataset is the gate's reference. Other configurations MAY be run and
  reported as informational, but MUST NOT be presented as the baseline.

**Baseline and reporting**

- **FR-017**: The baseline report MUST record, per dataset and configuration: nDCG@10, Recall@100,
  the number of queries scored, the number skipped and why, the lexical stage commit (`94ddbe6`,
  per ADR-0006 condition 1), the harness commit, and every dataset hash.
- **FR-018**: For the largest corpus (FiQA), the report MUST additionally record the on-disk index
  size and the process's peak resident memory **as observations**. No threshold applies; this is
  the first measured point on the index-memory curve Feature 002 explicitly deferred (its FR-038),
  and the number is recorded so that a later spec inherits data rather than an extrapolation.
- **FR-019**: The report MUST be a committed, human-readable file with a stable format, so that
  two reports can be diffed and a delta computed.
- **FR-020**: The harness MUST validate itself against BEIR's **published BM25 figures** as a
  harness-validity check, not a quality target: the `lexical-baseline-v1` nDCG@10 on each dataset
  MUST lie within **±0.10** of the published BM25 nDCG@10 for that dataset. The published figures
  and their exact source (paper, table, or leaderboard revision) MUST be pinned in the plan and
  quoted in the report; the values recalled while drafting this spec — about 0.665 SciFact, 0.325
  NFCorpus, 0.236 FiQA — are **provisional until verified at planning** and MUST NOT be used
  unverified. A miss is a finding to investigate and record (Rule 6), never a reason to widen the
  band, and it does not by itself say the lexical stage is bad: the published system used a
  different analyzer and multi-field weighting.

**Deltas and the gate**

- **FR-021**: The harness MUST compute a delta between two reports: per dataset and metric, the
  before value, after value, absolute change and relative change, formatted as a table suitable for
  a PR description (Agent Operating Rule 5).
- **FR-022**: The delta MUST state whether the constitution's ADR trigger applies — nDCG@10 lower
  on the majority of the three datasets.
- **FR-023**: This feature MUST deliver the **CI smoke job**: on changes touching
  `crates/xtriever-{lexical,dense,rerank,ltr,pipeline}/`, CI fetches SciFact into a hash-verified
  cache (cache hit ⇒ no download), runs `lexical-baseline-v1` on it, and applies FR-024. The job is
  **blocking**, which discharges ADR-0006 condition 2 in full. *Escape hatch, recorded so it cannot
  happen by drift*: if download or cache flakiness makes the job unreliable, it MAY be demoted to
  non-blocking for **one feature cycle only**, by a commit that states the reason and the commit at
  which it becomes blocking again.
- **FR-024**: The smoke, wherever it runs, MUST compare the current SciFact metrics against the
  recorded baseline and MUST fail when either metric is lower by more than the tolerance stated in
  Assumptions, naming the metric and both values; it MUST pass, printing the delta, otherwise.

**Constraints and scope**

- **FR-025**: `xtriever-eval` MUST stay pure Rust and `std`-only: no C/C++ dependencies, no
  `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code
  (Principle III names it among the pure crates). Fetching may live in a binary or example and use
  whatever a binary may use.
- **FR-026**: Dependencies MUST point downward: `xtriever-eval` may depend on `xtriever-core` and
  on stage crates it evaluates, never the reverse.
- **FR-027**: This feature MUST NOT modify `xtriever-core`, `xtriever-lexical`, or `deny.toml`. The
  lexical stage is measured as it is; changing it is a later, delta-gated feature.
- **FR-028**: This feature MUST NOT implement any retrieval stage, fusion, re-ranking or LTR, and
  MUST NOT set or imply a quality target for the lexical stage beyond what FR-020 decides.
- **FR-029**: Acceptance tests MUST be written and committed **failing** before the implementation,
  and MUST fail for want of an implementation rather than for want of a fixture or a dataset.
- **FR-030**: Tests that need the real datasets MUST be separated from tests that do not, so the
  offline suite runs in CI without network access and the dataset suite runs where the cache
  exists.

### Key Entities

- **Benchmark Dataset**: One of SciFact, NFCorpus, FiQA-2018 in BEIR form — a corpus of documents
  (external string id, title, text), a set of queries (id, text) and test-split judgements (query
  id, document id, integer grade). Identified by name, source URL, archive hash and per-file hashes.
  Expected counts, to be confirmed against the pinned files at planning and asserted at load:
  SciFact ≈ 5,183 documents / 300 test queries; NFCorpus ≈ 3,633 / 323; FiQA-2018 ≈ 57,638 / 648.
- **Dataset Manifest**: The committed record of what "pinned" means — per dataset: source URL,
  archive size and hash, each loaded file's size and hash, and the counts. The only thing the
  repository holds about the data.
- **Evaluation Configuration**: The recipe that turns a dataset into an index and a query text into
  a query: indexed fields with analyzer and boost, query construction, `k`. Named, so a report can
  cite it.
- **Run**: One retriever × one dataset × one configuration: the ranked results for every query, in
  order.
- **Metric Goldens**: Committed synthetic runs and judgements with reference-computed nDCG@10 and
  Recall@100 per query and mean, covering every edge case FR-002 lists.
- **Evaluation Report**: The committed output of a run set: per dataset and configuration, the two
  metrics, query counts, commits, dataset hashes, and — for FiQA — the memory and index-size
  observations.
- **Delta**: A comparison of two reports, per dataset and metric, with the ADR-trigger verdict.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: nDCG@10 and Recall@100 agree with the reference implementation on every golden case
  within the stated tolerance — 100 % of cases, including all six edge-case categories in FR-002.
- **SC-002**: All three datasets are fetched, hash-verified and loaded with document, query and
  judgement counts matching the manifest — 3 of 3, on a machine that has never seen them.
- **SC-003**: A tampered or truncated dataset file is rejected before any metric is computed — 0
  reports produced from unverified data.
- **SC-004**: The lexical baseline report exists for all three datasets with every FR-017 field
  present, and re-running it on the same commit reproduces it byte-for-byte.
- **SC-005**: A delta between two reports is produced as a table with before/after/absolute/relative
  for 2 metrics × 3 datasets, plus the ADR-trigger line.
- **SC-006**: A deliberately degraded SciFact result fails the smoke naming the metric; an unchanged
  one passes — both demonstrated by test.
- **SC-007**: The offline test suite (metrics, manifest, delta, report format) passes with no
  network access and no dataset cache present.
- **SC-008**: Evaluating SciFact end to end (fetch excluded) completes in under one minute on the
  development host, so the smoke is cheap enough to run on every ranking PR — recorded, and if it is
  slower that is a finding, not a failure.
- **SC-009**: The FiQA observations (index size, peak memory) are present in the baseline report as
  numbers with units.
- **SC-010**: `xtriever-eval` compiles for the host and the three mobile targets with zero C/C++
  dependencies reachable from it, and `xtriever-core`, `xtriever-lexical` and `deny.toml` are
  unchanged.

## Assumptions

- **The reference implementation for the metrics is the one BEIR itself uses** (the `pytrec_eval`
  package, a binding to `trec_eval`). Its conventions — queries absent from the judgements are
  skipped, queries with judgements but no relevant document are scored zero by `trec_eval`'s
  definition of the metric, gains are the raw grades with the standard discount — are what the
  harness matches. Where BEIR's own evaluation wrapper post-processes `pytrec_eval` output, the
  wrapper's behaviour is the reference, since that is what published numbers reflect. The plan cites
  the exact package versions.
- **Metric tolerance is 1e-6 absolute** on the per-query values and the mean. nDCG and recall are
  ratios of small sums; there is no legitimate reason for two correct implementations to differ by
  more than double-precision noise.
- **Datasets come from the BEIR public download location**, pinned by archive hash. If the
  location changes, the hash still identifies the content; if the content changes, that is a new
  pin recorded with a new baseline, never a silent update.
- **Datasets are never redistributed by this repository.** Each is used under its own licence from
  the original source; the repository holds hashes and a fetch script only.
- **The lexical stage is evaluated exactly as merged at `94ddbe6`.** The harness lives on a later
  commit, but Feature 002's crate is unchanged between the two (FR-027 keeps it so), so evaluating
  on the harness branch is evaluating that commit. Both hashes are recorded.
- **`k = 100`** for retrieval, which serves both metrics; nDCG@10 is computed over the first ten.
- **The smoke tolerance is zero**: any decrease in either SciFact metric fails. Metrics are
  deterministic (FR-004, FR-015), so there is no noise to allow for; a decrease is a real change.
- **Memory is observed, not budgeted.** FR-018's numbers are read from the operating system for
  the evaluation process on the development host; the method is recorded with the number.
- **No `xtriever-eval` public API stability is promised yet.** The dense and pipeline features will
  shape it; this feature's contract is the report format and the fixture set.
