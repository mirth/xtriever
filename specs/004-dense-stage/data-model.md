# Data Model: The Dense Stage

**Feature**: `004-dense-stage` | **Date**: 2026-09-12 | **Spec**: [spec.md](./spec.md) |
**Research**: [research.md](./research.md)

Core types are `xtriever-core`'s and are not changed (FR-025): `DocId(u32)`, `Vector = Vec<f32>`,
`Hit { id, score: f32 }`, `DocSet`, `Metric::{Cosine, Dot, Euclidean}`, `TextKind`, `Error`.

---

## Pinned Model (`xtriever_dense::model`)

| field | value | source |
|---|---|---|
| `repository` | `sentence-transformers/all-MiniLM-L6-v2` | 001 |
| `revision` | `1110a243fdf4706b3f48f1d95db1a4f5529b4d41` | 001 |
| `files[0]` | `config.json`, 612 B, `953f9c0d…08b41` | research D4 |
| `files[1]` | `tokenizer.json`, 466,247 B, `be50c362…2037` | D4 |
| `files[2]` | `model.safetensors`, 90,868,376 B, `53aa5117…9db9` | 001, D4 |
| `dim` | 384 | asserted from `config.hidden_size` |
| `max_tokens` | 256 | sentence-transformers config; asserted `≤ config.max_position_embeddings` |
| `pooling` | `mean-mask` | ours (D4) |
| `normalisation` | `l2` | ours |
| `dtype` | `f32`, 103 tensors | asserted from the safetensors header |
| `engine` | `candle-0.9.2` | ADR-0001 |

`FINGERPRINT: &str` is assembled from these at compile time (research D6). A committed
`reference/models/manifest.json` holds the same pins for `scripts/fetch-model.sh`; test
`model_pins::manifest_matches_compiled_pins` keeps them equal.

**Validation at load** (FR-002/FR-003, in this order): size of each file → SHA-256 of each file →
parse `config.json` → assert fields → parse safetensors header → assert dtype/count → build
tokenizer from bytes → assert pad id 0 → build model. Any failure: `Error::Model { model:
"all-MiniLM-L6-v2", message }` naming the file and both values.

## Embedder (`xtriever_dense::MiniLmEmbedder`)

| field | type | notes |
|---|---|---|
| `tokenizer` | `tokenizers::Tokenizer` | truncation 256, fixed padding 256, pad id 0 |
| `model` | `candle_transformers::models::bert::BertModel` | CPU device |
| `load_path` | `LoadPath` | recorded for observations |

`impl Embedder`: `dim() = 384`, `metric() = Cosine`, `fingerprint() = FINGERPRINT`,
`max_input_tokens() = Some(256)`, `embed(texts, kind)` = per-text forward (D2/D5), `kind` ignored
(FR-007). Output: one `Vector` of 384 `f32` per input, in input order; an empty input slice yields
an empty `Vec`.

### `LoadPath`

```
enum LoadPath { Buffered, #[cfg(feature = "mmap")] Mmap }
```

`Buffered`: `std::fs::read` → `Vec<u8>`. `Mmap`: `memmap2::Mmap` of the opened, verified file. Both
feed `VarBuilder::from_slice_safetensors`. The single hand-written `unsafe` block is
`xtriever_dense::bytes::map_readonly(&File) -> io::Result<Mmap>`, compiled only under `mmap`,
under `#[allow(unsafe_code)]` scoped to that item (ADR-0007).

## Embedding Goldens (`reference/fixtures/004/embeddings.json`)

```
{ "schema_version": 1, "model": { revision, weights_sha256 }, "fingerprint": "…",
  "tolerance": { "cosine_min": 0.9999, "max_abs_diff": 0.001, "unit_norm_abs": 1e-5 },
  "cases": [ { "id": "short", "text": "…", "input_ids": [..256], "attention_mask": [..256],
               "n_real_tokens": n, "vector": [..384] }, … ] }
```

Required case ids: `short`, `long_over_256` (> 256 tokens, `n_real_tokens == 256`), `empty`
(`""`, 2 real tokens), `whitespace_only`, `oov_unicode` (emoji + CJK + accented Latin),
`punctuation_only`, `beir_like_title_text`, `duplicate_of_short` (same text as `short`), plus
≥ 4 varied sentences — ≥ 12 cases. `manifest.json` carries each fixture file's SHA-256 and the
generator's own hash (002/003 pattern; `tests/fixtures_valid.rs`).

## Vector Index (`xtriever_dense::FlatIndex`)

### In memory

| field | type | notes |
|---|---|---|
| `dir` | `PathBuf` | holds `index.bin` |
| `header` | `Header { format_version: 1, dim, metric, fingerprint, count }` | |
| `committed` | `Generation { ids: Vec<u32>, norms: Vec<f32>, rows: Rows }` | `Rows::Owned(Vec<f32>)` or, under `mmap`, `Rows::Mapped { map: Mmap, offset: usize }` |
| `pending` | `BTreeMap<DocId, Option<Vec<f32>>>` | `None` = delete; invisible until `commit` |

`impl VectorIndex`: `dim`, `metric`, `fingerprint` from the header; `add`, `delete`, `commit`,
`search`, `len` per research D7–D9. Constructors: `create(dir, dim, metric, fingerprint: &str)`,
`open(dir)` (buffered) / `open_mapped(dir)` (feature `mmap`), `open_for(dir, &dyn Embedder)` →
`FingerprintMismatch` on disagreement, plus `dim`/`metric` agreement checks → `Corrupt`.

### On disk — `index.bin`, format version 1 (research D8)

| section | bytes | content |
|---|---|---|
| magic | 8 | `XTDENSE1` |
| `hdr_len` | 8 | `u64` LE |
| header | `hdr_len` | UTF-8 JSON `{"format_version":1,"dim":384,"metric":"cosine","fingerprint":"…","count":n}`; keys in this order |
| ids | 4·n | `u32` LE, strictly ascending |
| norms | 4·n | `f32` LE, Euclidean norm of the row (computed in `f64`, rounded once) |
| vectors | 4·n·dim | `f32` LE, row-major, row `i` ↔ `ids[i]` |

`metric` serialises as `cosine` / `dot` / `euclidean`. Open-time validation: magic, `format_version
== 1` (else `Corrupt("format version v, this build reads 1")`), `hdr_len` and total length
consistent, `count` fits `usize`, ids ascending. The file is only ever replaced by `rename` of a
fully written `index.bin.tmp`; never modified in place.

### State transitions

```
create ──► committed (empty file written) ──add/delete──► pending ──commit──► committed (new file)
                                                             │
                                                           drop ──► pending discarded
open(dir) ──► committed (file read or mapped)
```

`len()` and `search` see `committed` only. A second handle opened on the same directory sees the
generation current at *its* open; a handle never observes another handle's commit until reopened.

### Validation rules

| operation | rule | error |
|---|---|---|
| `add` | `vector.len() == dim` | `DimensionMismatch { expected, actual }` |
| `add` | every component finite | `Schema` |
| `add` | norm > 0 when `metric == Cosine` | `Schema` |
| `search` | `query.len() == dim` | `DimensionMismatch` |
| `search` | every component finite; norm > 0 under `Cosine` | `InvalidQuery` |
| `search` | `k == 0` or empty `allowed` | `Ok(vec![])` |
| `open` | magic / version / size / order | `Corrupt` |
| `open_for` | header fingerprint == embedder fingerprint | `FingerprintMismatch { index, current }` |

### Search result

`Vec<Hit>` of length `min(k, live ∩ allowed)`, ordered by `score` descending then `id` ascending,
`score` = the metric's value computed in `f64` and rounded to `f32` once (D7).

## Search Goldens (`reference/fixtures/004/search.json`, `mutations.json`)

```
search.json: { "schema_version": 1, "score_abs_tol": 1e-6, "tie_margin": 1e-5,
  "sets": [ { "id": "dim8_ties", "dim": 8, "metric": "cosine",
              "rows": [ { "id": 0, "vector": [..] }, … ],          // ids ascending, may contain duplicate vectors
              "queries": [ { "id": "q0", "vector": [..],
                             "cases": [ { "k": 5, "allowed": null | [ids], "expected": [ { "id", "score" } ] } ] } ] } ] }

mutations.json: { "schema_version": 1, "dim": 8, "metric": "dot", "fingerprint": "test-fp",
  "steps": [ { "op": "add"|"delete"|"commit"|"reopen"|"expect", … } ] }
   expect: { "len": n, "query": [..], "k": 10, "results": [ { "id", "score" } ] }
```

Every tie in `expected` is between rows with identical vectors (generator-enforced, research D7).

## Dense Evaluation Configuration (`xtriever_eval::run::DenseConfig`)

| field | value in `dense-baseline-v1` |
|---|---|
| `name` | `dense-baseline-v1` |
| `passage.title_then_text` | true |
| `passage.separator` | `" "` |
| `passage.omit_empty_title` | true |
| `k` | 100 (validated ≥ 100, as `EvalConfig`) |

`build_passages(dataset, cfg) -> (Vec<String>, IdMap)`; `execute_dense(embedder, index, ids,
dataset, cfg) -> Run` (queries in ascending id order, `TextKind::Query`, `search(v, None, k)`).

## Embedding Cache (`<cache-dir>/<dataset>/`)

| file | content |
|---|---|
| `index.bin` | the `FlatIndex` generation holding one row per corpus document, `DocId(i)` = corpus position |
| `cache.json` | `EmbeddingCacheKey { config, dataset, embedder_fingerprint, corpus_sha256, documents, format_version: 1 }` |

Valid iff `cache.json` parses, every key field equals the current run's, and `open_for(dir,
&embedder)` succeeds; otherwise the directory is removed and rebuilt. The cache is git-ignored
(`target/xt-dense-cache/` by default).

## Report extension (`xtriever_eval::report`)

`EvalReport` gains an optional trailing `stage`:

```
"stage": { "kind": "dense", "embedder_fingerprint": "…", "load_path": "buffered"|"mmap",
           "thread_count": n, "baseline": "absolute" }
```

`Observations` gains optional `embed_corpus_ms`, `search_ms`, `model_bytes_buffered`,
`model_bytes_mmapped` (all `u64`, omitted when `None`). Existing lexical reports round-trip
byte-for-byte (test `report::lexical_reports_round_trip_unchanged`).

Baselines: `specs/004-dense-stage/baselines/dense-baseline-v1.{scifact,nfcorpus,fiqa}.json`.
