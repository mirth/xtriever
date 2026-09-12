# Data Model: The Hybrid Pipeline

**Feature**: `005-hybrid-pipeline` | **Date**: 2026-09-13 | **Spec**: [spec.md](./spec.md) |
**Research**: [research.md](./research.md)

Core types are `xtriever-core`'s, unchanged (FR-027): `DocId`, `Document`, `ChunkInfo`, `Schema`,
`FieldName`, `Value`, `Filter`, `DocSet`, `LexicalQuery`, `Hit`, `Budget`, `Embedder`,
`features::*`, `Error`.

---

## Hybrid Index (`xtriever_pipeline::HybridIndex`)

### In memory

| field | type | notes |
|---|---|---|
| `dir` | `PathBuf` | |
| `descriptor` | `Descriptor` | below |
| `ids` | `IdMap` | below |
| `lexical` | `xtriever_lexical::TantivyIndex` | at `dir/lexical` |
| `dense` | `xtriever_dense::FlatIndex` | at `dir/dense` |
| `embedder` | `Box<dyn Embedder>` | caller-supplied |
| `pending_live` | `u64` | live count after the staged changes (for the descriptor) |

Constructors: `create(dir, config: HybridConfig, embedder)`, `open(dir, embedder)`,
`open_mapped(dir, embedder)` (feature `mmap`).

### On disk

```
<dir>/xtriever-pipeline.json     Descriptor
<dir>/ids.json                   IdMap
<dir>/lexical/                   002 format (TantivyIndex)
<dir>/dense/                     004 format v1 (FlatIndex)
```

Both JSON files are replaced by `<name>.tmp` + `rename`, never modified in place.

## Descriptor (`xtriever-pipeline.json`, format version 1)

| key | type | meaning |
|---|---|---|
| `format_version` | `u32` = 1 | rejected if ≠ 1 (`Corrupt`, both versions named) |
| `schema` | core `Schema` | must equal `lexical.schema()` at open |
| `embedder_fingerprint` | `String` | must equal `embedder.fingerprint()`; the dense stage re-checks via `open_for` |
| `dense_fields` | `Vec<FieldName>` | text fields joined by `" "` in this order into the passage |
| `candidate_depth` | `usize` (default 100) | per-stage candidates when the caller does not override |
| `rrf_k` | `u32` (default 60) | fusion constant |
| `live_docs` | `u64` | live documents at the last full commit |
| `generation` | `u64` | incremented per full commit |

**Consistency at open** (FR-005): `live_docs == ids.live() == lexical.stats().num_docs ==
dense.len()`, else `Corrupt` naming all four.

## HybridConfig (creation-time)

| field | default | rule |
|---|---|---|
| `schema` | — | validated by the lexical stage (002 rules) |
| `dense_fields` | — | non-empty; every name a `Text` field of the schema, else `Schema` |
| `candidate_depth` | 100 | ≥ 1 |
| `rrf_k` | 60 | ≥ 1 |

## IdMap (`ids.json`, format version 1)

```json
{ "format_version": 1, "external": ["d1", null, "d3"] }
```

| rule | error |
|---|---|
| external id non-empty | `Schema("external id must not be empty")` |
| position = internal `DocId`; `null` = deleted; ids never reused | — |
| `live()` = non-null count | — |
| reverse map `HashMap<String, u32>` rebuilt at open | — |

## SourceDocument (input)

| field | type |
|---|---|
| `external_id` | `String` |
| `fields` | `BTreeMap<FieldName, Value>` |
| `chunk` | `Option<ChunkInfo>` |

`add(&[SourceDocument])`: assigns or reuses the internal id, builds the passage from
`dense_fields` (missing or empty fields skipped; all absent ⇒ empty passage), embeds it
(`TextKind::Passage`), `lexical.add(&[Document])`, `dense.add(id, &vector)`. Repeated external
id within a batch: last wins. `add_embedded(&[(SourceDocument, Vec<f32>)])`: same, with the
supplied vector (width checked against `embedder.dim()` ⇒ `DimensionMismatch`).

`delete(&[&str])`: unknown ids ignored; known ⇒ slot `null`, both stages `delete`.

`commit()`: `lexical.commit()` → `dense.commit()` → write `ids.json` → write descriptor
(`generation += 1`, `live_docs`). No-op when nothing is pending.

### State transitions

```
create ─► committed(gen 0, empty) ─add/add_embedded/delete─► pending ─commit─► committed(gen n+1)
                                                              └─drop─► pending discarded (stages discard theirs too)
open ─► committed (four-count check) | Corrupt
```

## SearchOptions

| field | type | default | notes |
|---|---|---|---|
| `depth` | `Option<usize>` | descriptor's `candidate_depth` | per stage |
| `strict` | `bool` | `false` | FR-015 |
| `budget` | `Budget` | none | `max_items` caps the dense depth; `max_time` needs `elapsed` |
| `elapsed` | `Option<&dyn Fn() -> Duration>` | `None` | monotonic time since the call started, caller-owned |
| `explain` | `bool` | `false` | FR-019 |

## Response

```
Response {
  hits: Vec<HybridHit>,               // ≤ k, (fused DESC, DocId ASC); degraded: lexical order, lexical scores
  stages: StageReport,
}
HybridHit { external_id: String, id: DocId, score: f64, chunk: Option<ChunkInfo>, explain: Option<HitExplain> }
HitExplain { bm25_score: Option<f32>, bm25_rank: Option<u32>, dense_score: Option<f32>, dense_rank: Option<u32>, fused: f64 }
  .features() -> [(FeatureName, f32); 5]   // NaN for absent (core convention)
StageReport { lexical_candidates: usize, dense_candidates: Option<usize>, degraded: Option<Degradation>, time_limit_ignored: bool }
Degradation { stage: "dense", reason: DegradeReason }
DegradeReason { StageError(String), BudgetExceeded { elapsed_ms: u64, limit_ms: u64 } }
```

### Search algorithm (research D5–D7)

1. `k == 0` ⇒ empty response (stages not run; `lexical_candidates = 0`). Query validation is
   the stages' (an empty query is fine: lexical matches nothing, dense embeds it).
2. Filter: `lexical.resolve_filter` → `DocSet`; empty ⇒ empty response.
3. Lexical: `search(Match(None, query), Some(Filter::Ids(set)), depth)`; error ⇒ error.
4. Check point A (time) → maybe degrade.
5. Dense: `embedder.embed(&[query], Query)` then `dense.search(&v, Some(&set), min(depth,
   max_items))`; error ⇒ degrade (default) / error (strict).
6. Check point B (time) → maybe degrade, discarding the dense candidates.
7. Fuse: `score(id) = lex_term + dense_term` in `f64`, `term = 1/(rrf_k + rank)`; sort
   `(score DESC, id ASC)`; truncate `k`; map ids to external strings (an unknown id ⇒
   `Corrupt`, FR edge case "stale or foreign index").
8. Degraded: hits = lexical list in lexical order, `score` = lexical score, `bm25_*` set,
   `dense_*` absent.

## Fusion Goldens (`reference/fixtures/005/fusion.json`)

```
{ "schema_version": 1, "rrf_k": 60, "score_abs_tol": 1e-9,
  "cases": [ { "id": "both_lists", "lexical": [ids in rank order], "dense": [ids], "k": 10,
               "expected": [ { "id", "score" } ] }, … ] }
```

Case ids required: `both_lists`, `lexical_only`, `dense_only`, `disjoint`, `identical`,
`reversed`, `tie_at_k`, `depth_below_k`, `k_zero`, `both_empty`. Ties are exact: same rank pair
⇒ same sum, both sides adding in the same order.

## Hybrid fixture (`reference/fixtures/005/hybrid.json`)

```
{ "schema_version": 1, "dim": 8, "fingerprint": "table-fp",
  "schema": { … two Text fields (title, text, analyzer "standard"), one Keyword field "source" … },
  "documents": [ { "external_id": "d1", "fields": {…}, "chunk": null | {parent, ordinal, byte_range},
                   "passage": "title text", "vector": [..8] } ],
  "queries": [ { "id": "q1", "text": "…", "vector": [..8], "filter": null | Filter,
                 "expected_dense": [ { "id": "d3", "score" } ] } ] }
```

The `passage` string is what the pipeline must hand the embedder for that document (the
`TableEmbedder` looks it up); `expected_dense` is the 004-style `fsum` oracle over the vectors
(cosine), asserted through the pipeline's explanation to prove the dense stage saw the right
vectors. Lexical expectations are not re-oracled (002).

## Harness additions (`xtriever-eval`)

| item | shape |
|---|---|
| `run::HybridConfig` | `{ name: "hybrid-baseline-v1", lexical: EvalConfig, dense: DenseConfig, candidate_depth: 100, rrf_k: 60, k: 100 }`; `validate()`: `k ≥ 100`, `candidate_depth ≥ k` |
| `run::build_external(dataset, &EvalConfig)` | `Vec<(String, BTreeMap<FieldName, Value>)>` in corpus order, empty fields omitted per `omit_empty_fields` |
| `run::execute_external(dataset, name, k, &mut dyn FnMut(&str) -> Result<Vec<String>>)` | `Run` over judged queries ascending |
| `report::compare(a, b)` | `Comparison { a_config, b_config, rows: Vec<DeltaRow> }` — no ADR trigger; `to_markdown()` names both configurations |
| `stage.kind` | `"hybrid"`, `baseline: "guarded"` |

Baselines: `specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.{scifact,nfcorpus,fiqa}.json`.
