# Data Model: The FFI Surface

**Feature**: `007-ffi-surface` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

Entities from the spec's "Key Entities", made concrete by [research.md](./research.md). Nothing
here is persisted: the surface reads an existing pipeline directory and never writes. Every
type below is a wire type (crosses to Swift) unless marked *Rust-only* or *Swift-only*.

## Index Handle (`XtrieverIndex`, uniffi object)

| Field (Rust-only) | Type | Notes |
|---|---|---|
| `inner` | `Mutex<xtriever_pipeline::HybridIndex>` | the lock is the per-handle serialisation (FR-006) and makes the object `Sync` (research D1) |
| `embedder_load`, `reranker_load` | `Duration` | how long each model took to load, reported by `info()` |

**Constructor** `open(index_dir: String, embedder_dir: String, reranker_dir: Option<String>,
load_path: LoadPath) -> Result<Arc<Self>, XtrieverError>`: loads `MiniLmEmbedder` (buffered or
mapped), `HybridIndex::open` / `open_mapped`, then `MiniLmCrossEncoder` if a directory is given
and attaches it. **Validation**: the pipeline's own — format version 2, fingerprint, marker,
five counts, store integrity; the models' pins. A re-rank directory without an embedder
directory is impossible by signature.

**Methods**: `info() -> IndexInfo`; `search(query: String, options: SearchOptions) ->
Result<SearchResponse, XtrieverError>` — takes the lock, starts an `Instant`, builds the
pipeline options with `elapsed: Some(&|| start.elapsed())` when `max_time_ms` is set, calls
`HybridIndex::search`, converts. No other method: no add, delete, commit (FR-002).

**Swift-only wrapper** `XtrieverIndex` (final class): holds the generated object and a serial
`DispatchQueue`; `static func open(...) async throws`, `func search(...) async throws ->
SearchResponse`, `var info: IndexInfo`. Cancellation cooperative (FR-008).

## SearchOptions (record)

| Field | Type | Pipeline meaning | Validation |
|---|---|---|---|
| `k` | `u32` | hits returned | 0 ⇒ empty response |
| `depth` | `Option<u32>` | candidates per stage; `None` = index default (100) | — |
| `rerank_depth` | `Option<u32>` | `None` = index default (20); `Some(0)` = no re-ranking | — |
| `max_time_ms` | `Option<u64>` | `Budget.max_time`; the FFI supplies the clock | — |
| `max_items` | `Option<u32>` | `Budget.max_items` | — |
| `strict` | `bool` | errors instead of degradation | — |
| `explain` | `bool` | attach `HitExplain` | — |

Swift default: `SearchOptions(k: 10, explain: true)` with everything else `nil`/`false`.

## SearchResponse (record)

`hits: Vec<Hit>`, `stages: StageReport`, `elapsed_ms: u64` (the FFI's wall time for the call,
lock wait excluded — for the UI's latency readout).

## Hit (record)

| Field | Type | From |
|---|---|---|
| `external_id` | `String` | `HybridHit.external_id` |
| `text` | `String` | `HybridHit.text` (the stored passage) |
| `score` | `f64` | fused (or lexical when degraded) — meaning unchanged from 005/006 |
| `rerank_score` | `Option<f32>` | where the re-ranker scored it |
| `chunk` | `Option<ChunkInfo>` | `{ parent, ordinal, byte_start: Option<u64>, byte_end: Option<u64> }` |
| `explain` | `Option<HitExplain>` | when requested |

Ordering is the pipeline's (006 data-model): scored-by-re-ranker first `(rerank_score DESC,
id ASC)`, then the rest in fused order.

## HitExplain (record)

`bm25_score: Option<f32>`, `bm25_rank: Option<u32>`, `dense_score: Option<f32>`, `dense_rank:
Option<u32>`, `fused: f64`, `rerank_score: Option<f32>`, `rerank_rank: Option<u32>`. Swift-only
`features() -> [(String, Float)]` under `bm25.score, bm25.rank, dense.score, dense.rank,
fused.score, rerank.score, rerank.rank` with `.nan` for absent — the same seven names as
`HitExplain::features()`.

## StageReport / RerankReport / Degradation / DegradeReason

`StageReport { lexical_candidates: u32, dense_candidates: Option<u32>, degraded:
Option<Degradation>, rerank: Option<RerankReport>, time_limit_ignored: bool }`;
`RerankReport { candidates: u32, scored: u32, skipped: Option<DegradeReason> }`;
`Degradation { stage: String, reason: DegradeReason }`; enum `DegradeReason { StageError {
message: String }, BudgetExceeded { elapsed_ms: u64, limit_ms: u64 } }` — one-to-one with the
pipeline's types.

## IndexInfo (record)

`documents: u64`, `format_version: u32`, `embedder_fingerprint: String`, `reranker_model_id:
Option<String>`, `candidate_depth: u32`, `rerank_depth: u32`, `rrf_k: u32`,
`embedder_load_ms: u64`, `reranker_load_ms: Option<u64>`.

## LoadPath (enum)

`Buffered`, `Mmap` — both models use the same path; `Mmap` carries the external-writer
precondition of ADR-0007/0009 (documented on the Swift API).

## XtrieverError (error enum)

One case per core variant, each with the engine's message (research D4):

| Case | Fields | Core variant |
|---|---|---|
| `Schema` | `message` | `Schema(String)` |
| `InvalidQuery` | `message` | `InvalidQuery(String)` |
| `UnknownField` | `field` | `UnknownField(FieldName)` |
| `DimensionMismatch` | `expected, actual` | same |
| `NotFound` | `id: u32` | `NotFound(DocId)` |
| `Model` | `model, message` | same |
| `Corrupt` | `message` | `Corrupt(String)` |
| `FingerprintMismatch` | `index, current` | same |
| `BudgetExhausted` | `message` | same |
| `Io` | `message` | `Io(std::io::Error)` (Display) |
| `Backend` | `message` | `Backend(Box<dyn Error>)` (Display) — and the wildcard for any future core variant |

## Parity goldens (`swift/Xtriever/Tests/Fixtures/expected.json`, committed; research D8)

```json
{
  "generated_by": "xtriever-ffi examples/fixture_index.rs @ <git head>",
  "info": { "documents": 40, "format_version": 2, "embedder_fingerprint": "…", "reranker_model_id": "…", "candidate_depth": 100, "rerank_depth": 20, "rrf_k": 60 },
  "queries": [
    { "id": "q1", "text": "…", "k": 10, "rerank_depth": 5,
      "with_reranker":    { "hits": [ { "external_id": "d007", "score_bits": "…u64 hex…", "rerank_score_bits": "…u32 hex…", "rerank_rank": 1 }, … ], "stages": { "lexical_candidates": 12, "dense_candidates": 40, "rerank": { "candidates": 5, "scored": 5 } } },
      "without_reranker": { "hits": [ … ], "stages": { … } } }
  ]
}
```

Bits are hex strings so JSON floats never round; Swift compares `bitPattern`. The fixture
index itself (`Tests/Fixtures/index/`) is built by the same example and gitignored (it needs
the models).

## Device run record (`specs/007-ffi-surface/runs/<device>-<date>-<n>.json`, committed; research D9)

001's shape extended: `device`, `os`, `build` (Release, `RAYON_NUM_THREADS`), `index`
(SciFact identity: documents, fingerprints), `models` (load path, load ms each), `open_ms`,
`footprint` (`peak_bytes`, `peak_method`, `sampled_max_bytes`, `ledger_peak_bytes`,
`observed_limit_bytes`, `verdict` vs 300 MB), `queries` (20 × { id, depth, elapsed_ms,
rerank_candidates, rerank_scored }), `parity` (lexical bit-identical count, dense/re-rank max
abs diff vs the host's `expected-scifact.json`), `notes`.
