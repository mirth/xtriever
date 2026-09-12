# Phase 0 Research: The Hybrid Pipeline

**Feature**: `005-hybrid-pipeline` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

This feature composes two crates this repository wrote and measured (002, 004) and one it
measures with (003); every item it relies on was read from those crates' sources on 2026-09-13
and is cited as `crate/path:line` (Agent Operating Rule 1). No new external dependency is
introduced (D9), so there is nothing to read from the registry.

---

## D1. The pipeline is the composition root: concrete stage types, a boxed embedder

**Decision**: `HybridIndex` owns a `TantivyIndex` (`xtriever-lexical/src/index.rs:80` `create`,
`:104` `open`; the `LexicalIndex` impl at `:283` `search`, `:300` `resolve_filter`, `:310`
`stats`) and a `FlatIndex` (`xtriever-dense/src/index/mod.rs:50` `create`, `:74` `open`, `:100`
`open_for`, `:112` `open_mapped_for`, `:127` `vector`) as **concrete** types — it must create and
persist them under its own directory — and a `Box<dyn Embedder>` supplied by the caller, because
the embedder is loaded from model files the caller locates (`MiniLmEmbedder::load(dir,
LoadPath)`) and tests substitute a table-driven stub. No trait of the pipeline's own, no generics
over storage (Rule 7).

**Rationale**: Principle V's direction `core ← stage crates ← pipeline` says the pipeline may
name stage types; a pipeline generic over `LexicalIndex + VectorIndex` would be trait gymnastics
for one instantiation. The embedder is the one component with a legitimate second implementation
today (the test stub), so it is the one thing abstracted.

## D2. Directory layout and the two persisted maps

```
<dir>/
├── xtriever-pipeline.json   descriptor (D3)
├── ids.json                 id map (D4)
├── lexical/                 TantivyIndex directory (002 descriptor inside)
└── dense/                   FlatIndex directory (index.bin, 004 format v1)
```

Stage directories are the stages' own formats; the pipeline never writes into them. Both
pipeline files are written whole to `<name>.tmp` and `rename`d (the 002/004 discipline), so a
crash never leaves a torn descriptor or id map.

## D3. Descriptor and the partial-commit check (FR-005, FR-006)

`xtriever-pipeline.json`:

```json
{ "format_version": 1, "schema": { … core Schema … }, "embedder_fingerprint": "…",
  "dense_fields": ["title", "text"], "candidate_depth": 100, "rrf_k": 60,
  "live_docs": 5183, "generation": 3 }
```

**Commit order**: `lexical.commit()` → `dense.commit()` → `ids.json` → descriptor
(`generation + 1`, `live_docs` = id-map live count). Each stage commit is durable on its own
(002 and 004 guarantee that), so the only inconsistent states a crash can leave are *lexical
ahead of dense* or *both stages ahead of the id map/descriptor*.

**Open-time check**: `descriptor.live_docs`, the id map's live count, `lexical.stats().num_docs`
(`xtriever-lexical/src/stats.rs:82`, live documents) and `dense.len()` must all agree; a
disagreement is `Error::Corrupt` naming all four numbers. This is cheap (one `stats()` call, one
`len()`), detects every partial commit the order above can produce, and needs no journal. It
does not *repair* — repair is a later feature with a measured need; FR-005 asks only that a
half-committed generation is never served silently.

**Identity check**: `schema` equals the lexical index's `schema()`; `embedder_fingerprint`
equals `embedder.fingerprint()` and the dense index opens through `open_for(dir, embedder)`
(which itself enforces fingerprint, dim and metric — 004 FR-015); `format_version == 1` else
`Corrupt` naming both versions.

## D4. Id map: external `String` ↔ internal `DocId`, ids never reused

`ids.json`: `{ "format_version": 1, "external": ["d1", null, "d3", …] }` — position = internal
id, `null` = deleted. On open the reverse `HashMap<String, u32>` is rebuilt (57,638 entries for
FiQA: milliseconds, ~1 MB on disk). Internal ids are assigned sequentially in ingestion order
and **never reused** after a delete: reuse would let a stale stage row (e.g. a dense vector
from a crashed commit) silently attach to a new document. `u32` gives 4.29 billion ids; the
constitution's index is 100k chunks.

**Replace**: adding an external id that exists reuses its internal id, and both stages' own
replace-by-id semantics do the rest (002 FR-008, 004 FR-014). **Delete**: the slot becomes
`null`; both stages get `delete(&[id])`. **Empty external id**: rejected (`Error::Schema`) — an
empty key is almost always a bug and would be indistinguishable in JSON exports. **Repeated
external id inside one `add` batch**: last wins, like a replace.

**Why JSON, not a binary column**: boring, human-inspectable, and the size is small at every
scale this project has measured; the dense index is 85 MB at the same row count. A binary map is
a later change with a measured reason.

## D5. Reciprocal rank fusion, computed and ordered in `f64`

**Decision** (FR-009, FR-011): `fused(d) = Σ_stages 1 / (rrf_k + rank_stage(d))`, ranks 1-based
within each stage's candidate list, `rrf_k = 60` (Cormack, Clarke & Buettcher 2009's constant;
the core's `features::FUSED_SCORE` is documented as "Fused (RRF) score",
`xtriever-core/src/types.rs:346-360`). The sum is taken in a fixed order — lexical term, then
dense term, absent terms contribute nothing — in `f64`, and hits are sorted by `(fused DESC,
DocId ASC)` on the `f64` value. `HybridHit.score` is **`f64`**, so the spec's 1e-9 tolerance is
meaningful (RRF values are ~0.01–0.03; an `f32` would round at ~1e-9 relative and could
collapse distinct sums). The `f32` view exists only in the explanation's feature vector, where
the LTR feature matrix is `f32` by core definition.

**Oracle** (FR-010): `reference/gen_005_fixtures.py` implements the same three lines in Python
over hand-made candidate lists; Python's `float` is IEEE `f64` and the two terms are added in
the same order, so results are bit-identical and every designed tie (same rank pair) is exact
on both sides. Cases: both lists, lexical-only, dense-only, disjoint lists, identical lists,
reversed lists, ties at the `k`-th rank, depth < `k`, `k = 0`, empty lists.

**Why not score fusion**: BM25 and cosine live on incomparable scales; normalising them is a
modelling decision with parameters, which the constitution makes delta-gated. RRF has one
constant and is the documented default.

## D6. Filters: resolve once, apply as one set to both stages (FR-012)

**Decision**: `lexical.resolve_filter(filter)` (`index.rs:300`) → `DocSet`; if empty, return the
empty response without running either stage. Otherwise the lexical stage is searched with
`Filter::Ids(set.iter().collect())` (`DocSet::iter`, `xtriever-core/src/types.rs:185`; 002
evaluates `Ids` as a term set on the hidden id column, `xtriever-lexical/src/filter.rs:145`) and
the dense stage with `Some(&set)`. Both stages therefore see literally the same allowed set, and
Story 2 scenario 3's "unrestricted result filtered to the set" holds by construction because
neither stage's scores change under a filter (002 FR-017 / 004 FR-013).

**Cost**: `Ids` materialises the set as a term-set query; at 100k ids that is a large query, but
filters that admit most of the corpus are the pathological case for any allow-list design.
Recorded as an observation, not budgeted; a `LexicalIndex::search_in(&DocSet)` would need a core
trait change and an ADR.

## D7. Degradation and the time source (FR-014–FR-018; spec Q2 = A)

**Decision**: `SearchOptions<'a> { depth, strict, budget: Budget, elapsed: Option<&'a dyn Fn()
-> Duration>, explain }`. Sequence:

1. Resolve the filter (any error is an error in every mode — it is the lexical stage).
2. Lexical search (error ⇒ error in every mode, FR-017).
3. **Check point A**: if `elapsed` is given and `budget.max_time` is set and `elapsed() >
   max_time`, skip the dense stage: `Degradation { stage: "dense", reason: BudgetExceeded }`.
4. Embed the query; dense search with depth `min(depth, budget.max_items)`. An `Err` from either
   ⇒ `Degradation { reason: StageError(message) }` in the default mode, or the error unchanged in
   strict mode (FR-015).
5. **Check point B**: after the dense stage returns, the same time check; exceeded ⇒ the dense
   candidates are **discarded** and the response degrades — so a budget is a promise about the
   result, not just about the attempt.
6. Fuse (or, degraded, take the lexical list in lexical order with lexical scores as the hits'
   scores — the "previous stage's results" of Principle VI), truncate to `k`.

`max_time` set with `elapsed` absent ⇒ ignored, and `StageReport.time_limit_ignored = true`
(spec Story 3 scenario 3). `core::Budget` (`types.rs:305`) is reused as-is: it is already a
`Duration`, not an `Instant`, for exactly this reason. Hosts pass `|| start.elapsed()`; wasm
passes its own; tests pass a stub returning a chosen value.

**Strict + time budget zero**: check point A fires before the dense stage; in strict mode a
budget exceedance is *also* an error (`Error::BudgetExhausted`, `xtriever-core/src/error.rs`) —
the constitution's strict mode means "I would rather have an error than a degraded answer".

## D8. Explanation (FR-019–FR-021)

`HitExplain { bm25_score: Option<f32>, bm25_rank: Option<u32>, dense_score: Option<f32>,
dense_rank: Option<u32>, fused: f64 }` on every hit when `explain` is requested;
`features() -> [(FeatureName, f32); 5]` renders it under `features::{BM25_SCORE, BM25_RANK,
DENSE_SCORE, DENSE_RANK, FUSED_SCORE}` with `NaN` for absent (the core's documented missing-value
encoding for the feature matrix, `types.rs:348`, `:368`). `StageReport { lexical_candidates,
dense_candidates: Option<usize>, degraded: Option<Degradation>, time_limit_ignored }` on every
response, explanation requested or not — it is cheap and FR-018 needs the degraded marker
regardless. Explanation never changes ranking because it is derived from the same candidate
lists after ordering (FR-020 test: two searches, one explained, `hits` equal).

## D9. Dependencies

`xtriever-pipeline`: `xtriever-core`, `xtriever-lexical`, `xtriever-dense` (path), `serde`,
`serde_json` (descriptor and id map), `thiserror` (workspace; for the `NotImplemented` scaffold
only). Optional feature `mmap = ["xtriever-dense/mmap"]` so a consumer can open the dense stage
mapped through the same flag. Dev: `tempfile`, `proptest`, `serde_json`, `sha2`. **No new
external crate**; `cargo deny`, `deny.toml` unchanged. Purity: lexical (tantivy) and dense
(candle) are pure Rust; the pipeline adds no threads, no async, no clock.

`xtriever-eval`: **dev**-dependency on `xtriever-pipeline` for the example (which then reaches
both stage crates through it; the direct dev-deps on lexical and dense stay for the 003/004
configurations). Library graph unchanged (`core + serde + serde_json + sha2`).

## D10. Harness: stage-agnostic runner, cached embeddings by value (FR-022–FR-024)

- `run::execute_external(dataset, config_name, k, retrieve: &mut dyn FnMut(&str) ->
  Result<Vec<String>>) -> Run` — the runner takes a closure from query text to external ids,
  so the library names no pipeline type. `execute` and `execute_dense` stay as they are.
- `run::HybridConfig { name: "hybrid-baseline-v1", lexical: EvalConfig (the 003 fields and
  boosts), dense: DenseConfig (the 004 passage recipe), candidate_depth: 100, rrf_k: 60, k: 100
  }` and `run::build_external(dataset, &EvalConfig) -> Vec<(String, BTreeMap<FieldName, Value>)>`
  — the same field construction as `build` but keyed by the BEIR id instead of a `DocId`.
- **Cached embeddings**: the 004 cache (`target/xt-dense-cache/<dataset>/{index.bin,
  cache.json}`) holds `DocId(i)` = corpus position. The pipeline assigns internal ids in
  ingestion order starting at 0, and the harness adds documents in corpus order, so position
  `i` in the cache **is** the pipeline's internal id `i`. The example verifies the cache key
  (config `dense-baseline-v1`, fingerprint, corpus hash, count), opens the cached `FlatIndex`,
  and feeds `HybridIndex::add_embedded(&[(SourceDocument, Vec<f32>)])` with `vector(DocId(i))`
  — "bring your own embedding", a legitimate ingest path for callers that embed offline. The
  pipeline verifies each supplied vector's width against the embedder. 0 documents embedded
  (SC-008); the dense sub-index is rewritten (~85 MB for FiQA, seconds).
- Report: `stage { kind: "hybrid", embedder_fingerprint, load_path, thread_count, baseline:
  "guarded" }`; `observations` unchanged in shape.
- **`beir compare a.json b.json`**: the delta table across *different* configurations, labelled
  with both names and **without** the ADR-trigger line — an explicit comparison, so `delta`'s
  FR-021 refusal stands. Story 5 scenario 3 (SC-011) is evaluated from three `compare` outputs
  against the better stage per dataset: dense on all three today (0.645 / 0.317 / 0.369).
- `--export-explain F`: per judged query `{query_id, lexical: [ext ids], dense: [ext ids],
  fused: [ext ids]}` from the explained response; `gen_005_fixtures.py --verify-fusion F`
  recomputes RRF in Python over the two lists and checks the fused order — the end-to-end
  fusion oracle on real data (in addition to the metric `--verify-run`).

## D11. Test strategy without a model

`tests/support::TableEmbedder`: an `Embedder` whose `embed` looks each text up in a fixture table
(`hybrid.json`: documents with fields and 8-d vectors, queries with vectors; unknown text ⇒
`Error::Model`, so a test that forgets a fixture fails loudly), fingerprint `"table-fp"`,
`Metric::Cosine`. `FailingEmbedder` (always `Error::Model`) and `SlowClock` (a closure returning a
fixed `Duration`) drive Story 3. With the real `TantivyIndex` and `FlatIndex` underneath, the
offline suite exercises the full pipeline; the model-backed suite is one `#[ignore]` test that
builds a tiny hybrid index with `MiniLmEmbedder` and checks a search round-trips (US1 scenario 5
uses the real fingerprint mismatch). The fusion goldens (`fusion.json`) are pure list inputs.

**Composition check** (US2): for each golden query, `pipeline.search(explain)` must equal
`rrf(lexical.search(...), dense.search(...))` where the two stage searches are run **directly**
on the sub-indexes opened from `<dir>/lexical` and `<dir>/dense`, and `rrf` is the crate's fusion
(itself verified against the Python oracle). Ranking of the stages is 002's and 004's business
and is not re-oracled here.

## D12. Observations to record (SC-010), no budgets

FiQA through the pipeline with cached embeddings: ingest wall time (`add_embedded` + commit of
both stages), `ids.json` size, directory size, peak RSS of the run, and search wall time per
query split into lexical / embed / dense / fuse (the example's stderr, `Instant` in the binary).
The `Filter::Ids` cost (D6) is measured once with a filter admitting half of SciFact.

## Risks

| # | risk | mitigation |
|---|---|---|
| R1 | Fused nDCG@10 below the better stage on ≥ 2 datasets (SC-011) | stop-and-report with the numbers and the `compare` tables; candidates to investigate: `rrf_k`, depth, a fusion defect — never a moved bar |
| R2 | The 004 cache positions do not equal the pipeline's internal ids (e.g. a document skipped) | the example asserts `cache.documents == corpus.len()` and every `vector(DocId(i))` is `Some`; a mismatch is an error, never a silent re-embed |
| R3 | `Filter::Ids` over a large allowed set is slow | measured and recorded (D12); the alternative needs a core change and an ADR |
| R4 | Partial commit not caught | the four-count check (D3) covers every state the commit order can leave; tested by deleting `dense/index.bin`'s generation between commits in a test |
| R5 | `HybridHit.score` as `f64` differs from core `Hit.score: f32` | deliberate and documented (D5); the FFI feature decides the wire type |
