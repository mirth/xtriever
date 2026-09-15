# Data Model: Python Interface and Bindings

**Feature**: `011-python-bindings` | **Date**: 2026-09-15

## The package

| entity | fields | notes |
|---|---|---|
| `xtriever` (Python package) | `__version__` (= the crate version), the re-exported names below, `__all__` | `python/src/xtriever/__init__.py`; nothing but re-exports |
| `xtriever._ffi` (generated) | `xtriever_ffi.py`, the shared library | produced by the build, never committed |

## Existing wire types (unchanged; research D1)

`IndexHandle` (`open`, `info`, `search`), `SearchOptions` (`k`; `depth`, `rerank_depth`,
`max_time_ms`, `max_items` default `None`; `strict`, `explain` default `False`),
`SearchResponse { hits, stages, elapsed_ms }`, `Hit { external_id, text, score, rerank_score,
chunk, explain }`, `HitExplain` (seven features, absent = `None`), `ChunkInfo { parent,
ordinal, byte_start, byte_end }`, `StageReport`, `RerankReport`, `Degradation`,
`DegradeReason`, `IndexInfo`, `LoadPath { BUFFERED, MMAP }`, `XtrieverError` and its eleven
nested classes.

## New wire types (research D4)

| type | fields | constraints (the engine's) |
|---|---|---|
| `FieldKind` (enum) | `Text { analyzer: str }`, `Keyword`, `U64`, `I64`, `F64`, `Bool`, `DateMillis` | unknown analyzer → `Schema` at create |
| `FieldDef` (record) | `name: str`, `kind: FieldKind`, `indexed: bool = True`, `stored: bool = False`, `boost: float = 1.0` | names unique within a schema |
| `IndexConfig` (record) | `fields: [FieldDef]`, `dense_fields: [str]`, `candidate_depth: int = 100`, `rrf_k: int = 60`, `rerank_depth: int = 20` | `dense_fields` non-empty, each a `Text` field in `fields` → else `Schema`; `candidate_depth ≥ 1`, `rrf_k ≥ 1` |
| `FieldValue` (enum) | `Text(str)`, `Keyword(str)`, `U64(int)`, `I64(int)`, `F64(float)`, `Bool(bool)`, `DateMillis(int)` | value kind must match the field's kind → else the lexical stage's `Schema` |
| `Document` (record) | `external_id: str`, `fields: {str: FieldValue}`, `chunk: ChunkInfo \| None = None` | `external_id` non-empty → else `Schema`; unknown field name → `UnknownField`/`Schema` (the stage's) |

## New operations on `IndexHandle`

| op | semantics (the pipeline's; contract §2) |
|---|---|
| `create(index_dir, config, embedder_dir, reranker_dir, load_path)` | directory absent or empty → created with an empty generation; non-empty → `Corrupt` |
| `add([Document])` | staged; embedded by the embedder; a known id replaces |
| `add_embedded([Document], [[float]])` | staged with the caller's vectors; `len(vectors) != len(docs)` → `Schema`; wrong width → `DimensionMismatch` |
| `delete([str])` | staged; unknown ids ignored |
| `commit()` | lexical → dense → passages → id map → descriptor; no-op when nothing staged |
| `merge()` | commit, then one lexical segment |
| `contains(external_id) -> bool` | the committed view |

State: `search`/`contains`/`info` read the committed view; staged changes become visible at
`commit`; a handle opened read-only (a directory the process cannot lock) refuses every write
with `Io` "read-only index".

## Tests' data

- Goldens: `swift/Xtriever/Tests/Fixtures/expected.json` — `queries[]` with `id`, `text`, `k`,
  `rerank_depth`, `with_reranker` / `without_reranker` → `hits[]` (`external_id`,
  `score_bits` hex f64, `rerank_score_bits` / `bm25_score_bits` / `dense_score_bits` hex f32,
  `rerank_rank`) and `stages`.
- Documents for the Python build: `reference/fixtures/005/hybrid.json` — `schema`,
  `dense_fields`, `documents[]` (`external_id`, `fields`, `chunk`), `queries[]`.
- The fixture index: `swift/Xtriever/Tests/Fixtures/index` (built by
  `cargo run --release -p xtriever-ffi --example fixture_index -- swift/Xtriever/Tests/Fixtures`; git-ignored).
