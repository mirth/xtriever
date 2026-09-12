# Contract: the public surface of `xtriever-pipeline` and the harness extension

**Feature**: `005-hybrid-pipeline` | **Date**: 2026-09-13 | **Plan**: [../plan.md](../plan.md)

This is the whole public API after Feature 005. Anything not listed is `pub(crate)`. No
`tantivy`, `candle` or `memmap2` type appears in a signature; the two stage types appear only
as the concrete stages the pipeline composes (Principle V's direction). The spec promises no API
stability yet; the stable contracts are the **directory layout and the two JSON formats**
(data-model), the **fusion rule** (research D5) and the **report extension**.

```rust
/// Creation-time configuration (data-model "HybridConfig").
pub struct HybridConfig {
    pub schema: Schema,
    pub dense_fields: Vec<FieldName>,
    pub candidate_depth: usize,   // default 100
    pub rrf_k: u32,               // default 60
}
impl HybridConfig { pub fn new(schema: Schema, dense_fields: Vec<FieldName>) -> Self; }

/// A document under its external id (data-model "SourceDocument").
pub struct SourceDocument { pub external_id: String, pub fields: BTreeMap<FieldName, Value>, pub chunk: Option<ChunkInfo> }

pub struct SearchOptions<'a> {
    pub depth: Option<usize>,
    pub strict: bool,
    pub budget: Budget,
    pub elapsed: Option<&'a dyn Fn() -> Duration>,
    pub explain: bool,
}
impl Default for SearchOptions<'_> { /* depth None, strict false, no budget, no clock, no explain */ }

pub struct Response { pub hits: Vec<HybridHit>, pub stages: StageReport }
pub struct HybridHit { pub external_id: String, pub id: DocId, pub score: f64, pub chunk: Option<ChunkInfo>, pub explain: Option<HitExplain> }
pub struct HitExplain { pub bm25_score: Option<f32>, pub bm25_rank: Option<u32>, pub dense_score: Option<f32>, pub dense_rank: Option<u32>, pub fused: f64 }
impl HitExplain { pub fn features(&self) -> [(FeatureName, f32); 5]; }   // NaN for absent
pub struct StageReport { pub lexical_candidates: usize, pub dense_candidates: Option<usize>, pub degraded: Option<Degradation>, pub time_limit_ignored: bool }
pub struct Degradation { pub stage: &'static str /* "dense" */, pub reason: DegradeReason }
pub enum DegradeReason { StageError(String), BudgetExceeded { elapsed_ms: u64, limit_ms: u64 } }

pub struct HybridIndex { /* private */ }
impl HybridIndex {
    pub fn create(dir: &Path, config: HybridConfig, embedder: Box<dyn Embedder>) -> Result<Self>;
    pub fn open(dir: &Path, embedder: Box<dyn Embedder>) -> Result<Self>;
    #[cfg(feature = "mmap")]
    pub fn open_mapped(dir: &Path, embedder: Box<dyn Embedder>) -> Result<Self>;
    pub fn config(&self) -> &HybridConfig;
    pub fn embedder(&self) -> &dyn Embedder;
    pub fn len(&self) -> u64;                     // committed live documents
    pub fn is_empty(&self) -> bool;
    pub fn contains(&self, external_id: &str) -> bool;   // committed
    pub fn add(&mut self, docs: &[SourceDocument]) -> Result<()>;
    pub fn add_embedded(&mut self, docs: &[(SourceDocument, Vec<f32>)]) -> Result<()>;
    pub fn delete(&mut self, external_ids: &[&str]) -> Result<()>;
    pub fn commit(&mut self) -> Result<()>;
    pub fn search(&self, query: &str, filter: Option<&Filter>, k: usize, options: &SearchOptions<'_>) -> Result<Response>;
    pub fn search_lexical(&self, query: &LexicalQuery, dense_text: &str, filter: Option<&Filter>, k: usize, options: &SearchOptions<'_>) -> Result<Response>;  // caller-built lexical query; the dense stage embeds `dense_text`
}

/// Reciprocal rank fusion over two ranked id lists (research D5). Public so the harness and
/// tests can call the exact rule the index uses.
pub fn rrf(lexical: &[Hit], dense: &[Hit], rrf_k: u32, k: usize) -> Vec<(DocId, f64)>;

pub const FORMAT_VERSION: u32 = 1;
```

One index type, one config, one document type, one options type, one response family, one free
function, one constant. No trait of its own, no generics, no async, no threads, no clock.

## Method semantics

| method | behaviour | errors |
|---|---|---|
| `create` | validate config (`dense_fields` non-empty and all `Text` fields of `schema`; `candidate_depth ≥ 1`; `rrf_k ≥ 1`); `dir` empty or absent; create `lexical/` and `dense/` (`dim = embedder.dim()`, `metric = embedder.metric()`, fingerprint) and write `ids.json` + descriptor (generation 0) | `Schema`, `Corrupt` (non-empty dir), `Io`, stage errors |
| `open` / `open_mapped` | read descriptor (version, identity checks) → id map → `TantivyIndex::open` → `FlatIndex::open_for`/`open_mapped_for` → four-count consistency check (research D3) | `Corrupt` (naming both versions / both schemas / all four counts), `FingerprintMismatch` (from the dense stage), `Io` |
| `add` | per document: reject empty `external_id`; assign or reuse internal id; passage from `dense_fields`; `embed(&[passage], Passage)`; `lexical.add(&[Document { id, fields, chunk }])`; `dense.add(id, &v)` | `Schema` (empty id, unknown/invalid fields via the lexical stage), `Model` (embedder), `DimensionMismatch` |
| `add_embedded` | as `add` with the supplied vector, width checked against `embedder.dim()` | as above |
| `delete` | unknown ids ignored; known ⇒ id map slot `null` (pending), both stages `delete` | stage errors |
| `commit` | `lexical.commit()` → `dense.commit()` → `ids.json` → descriptor; no-op if nothing pending. A failure between steps leaves a state `open` refuses (FR-005) | `Io`, stage errors |
| `search` | `LexicalQuery::Match(None, query)`; then as `search_lexical` | — |
| `search_lexical` | data-model "Search algorithm" steps 1–8; the dense stage embeds `dense_text` (a `LexicalQuery` has no single text to embed — added at implementation) | `InvalidQuery`/`UnknownField` (filter or query), lexical stage errors (every mode), dense stage errors (strict only), `BudgetExhausted` (strict + time exceeded), `Corrupt` (an id the map does not know) |
| `rrf` | `Σ 1/(rrf_k + rank)` over the lists, `f64`, `(score DESC, id ASC)`, first `k` | — (pure) |

**Determinism**: same directory contents + same query + same options ⇒ identical `hits` (ids,
scores, order) across calls and reopening; `explain` never changes `hits`.

**Degraded response**: `stages.degraded.is_some()`; `hits` are the lexical candidates in lexical
order with `score = f64::from(lexical score)`; `dense_candidates = None`.

## Error mapping (no new core variants)

| situation | variant |
|---|---|
| empty external id; `dense_fields` invalid | `Schema` |
| descriptor version / schema / counts disagree; unknown id returned by a stage | `Corrupt` |
| embedder fingerprint ≠ descriptor or dense index | `FingerprintMismatch` (dense stage's) |
| dense failure in strict mode | the stage's error, unchanged |
| time budget exceeded in strict mode | `BudgetExhausted` |
| filesystem | `Io` |

## Cargo features

| feature | default | effect |
|---|---|---|
| (none) | yes | buffered dense stage; zero `unsafe` anywhere in the pipeline crate |
| `mmap` | no | `= ["xtriever-dense/mmap"]`; enables `open_mapped` (the dense stage's precondition on external writers applies, see 004's contract) |

## Harness extension (`xtriever-eval`, additive)

```rust
pub mod run {
    pub struct HybridConfig { pub name: String, pub lexical: EvalConfig, pub dense: DenseConfig, pub candidate_depth: usize, pub rrf_k: u32, pub k: usize }
    impl HybridConfig { pub fn validate(&self) -> Result<()>; pub fn hybrid_baseline_v1() -> Self; }
    pub fn build_external(dataset: &Dataset, cfg: &EvalConfig) -> Result<Vec<(String, BTreeMap<FieldName, Value>)>>;
    pub fn execute_external(dataset: &Dataset, config_name: &str, k: usize,
                            retrieve: &mut dyn FnMut(&str) -> Result<Vec<String>>) -> Result<Run>;
}
pub mod report {
    pub struct Comparison { pub a_config: String, pub b_config: String, pub rows: Vec<DeltaRow> }
    impl Comparison { pub fn to_markdown(&self) -> String; }
    /// Cross-configuration comparison: same rows as `delta`, no ADR trigger, both names shown.
    pub fn compare(a: &[EvalReport], b: &[EvalReport]) -> Comparison;
}
```

### `beir` example commands (added)

| command | does | exit |
|---|---|---|
| `run --dataset D --config hybrid-baseline-v1 [--model-dir M] [--cache-dir C] [--index-dir DIR] [--load-path P] [--out F] [--export-run F] [--export-explain F]` | verify the 004 cache key for `D`; build a `HybridIndex` in `DIR` (default temp) via `add_embedded` from the cached vectors (0 embedded), one commit; search every judged query with `explain`; score; write | 0 / 1 |
| `compare a.json b.json` | cross-configuration table, no ADR line | 0 / 1 |
| `delta`, `smoke`, `verify`, `model-memory`, `export-vectors` | unchanged; `delta` still refuses mixed configurations | as before |

`--export-explain F`: one line per judged query `{"query_id", "lexical": [ext ids], "dense":
[ext ids], "fused": [ext ids]}` from the explained response, for
`gen_005_fixtures.py --verify-fusion F`.

### Report file

Key order unchanged; `stage { kind: "hybrid", embedder_fingerprint, load_path, thread_count,
baseline: "guarded" }`. Baselines at
`specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.{scifact,nfcorpus,fiqa}.json`.

## Scripts

- `reference/gen_005_fixtures.py` — `fusion.json` (RRF oracle), `hybrid.json` (synthetic corpus
  with vectors and the dense oracle), `manifest.json`; `--verify-fusion F` recomputes RRF over an
  exported explain file and checks the fused order per query. Runs in `reference/.venv-003`
  (needs only the standard library and NumPy for the cosine oracle).
- `scripts/check-no-stubs.sh` — extended to `crates/xtriever-pipeline/src/`.
