# Contract: the public surface of `xtriever-rerank` and the pipeline / harness extensions

**Feature**: `006-rerank-stage` | **Date**: 2026-09-13 | **Plan**: [../plan.md](../plan.md)

Everything public after Feature 006. `xtriever-rerank` is listed whole; for `xtriever-pipeline`
and `xtriever-eval` only the additions and changes to the 005 contract are listed
([005 contract](../../005-hybrid-pipeline/contracts/hybrid-pipeline.md) remains in force for
the rest). No `candle` or `tokenizers` type appears in any signature. The stable contracts are
the **model identity string**, the **budget semantics**, the **ordering rule** and the
**on-disk format** (data-model); API stability is not promised yet.

## `xtriever-rerank`

```rust
pub mod model {
    pub struct PinnedFile { pub name: &'static str, pub bytes: u64, pub sha256: &'static str }
    pub struct PinnedModel { pub repository: &'static str, pub revision: &'static str, pub files: [PinnedFile; 3], pub hidden: usize, pub max_tokens: usize, pub f32_tensors: usize }
    pub const PINNED: PinnedModel;          // research D2
    pub const MODEL_NAME: &str = "ms-marco-MiniLM-L-6-v2";
    pub const MODEL_ID: &str;               // "cross-encoder/ms-marco-MiniLM-L-6-v2@233902d…;weights=sha256:821d1a…;max_tokens=512;trunc=longest_first;head=cls-pooler-tanh-linear;act=identity;dtype=f32;engine=candle-0.9.2"
    pub fn verify_files(dir: &Path) -> Result<()>;   // size then sha256, every file, first failure wins
}

/// How the weights are brought into memory — the dense crate's type, mirrored (ADR-0009).
pub enum LoadPath { Buffered, #[cfg(feature = "mmap")] Mmap /* caller's precondition: no external writer while mapped */ }

pub struct MiniLmCrossEncoder { /* private */ }
impl MiniLmCrossEncoder {
    pub fn load(dir: &Path, load_path: LoadPath) -> Result<Self>;  // verify → assert config → tokenizer → assert header → weights via load_path → encoder + pooler + classifier
    pub fn load_path(&self) -> LoadPath;
    pub fn score(&self, query: &str, passage: &str) -> Result<f32>; // one pair, one forward at its own length; Error::Model on a non-finite logit
    pub fn thread_count() -> usize;                                // candle's effective RAYON_NUM_THREADS
    #[doc(hidden)] pub fn tokenize_for_test(&self, query: &str, passage: &str) -> Result<(Vec<u32>, Vec<u32>)>;   // (input_ids, token_type_ids)
}
impl Reranker for MiniLmCrossEncoder {
    fn model_id(&self) -> &str;                                    // MODEL_ID
    fn rerank(&self, query: &str, passages: &[Passage<'_>], budget: &Budget) -> Result<Vec<Option<f32>>>;
}
```

One type, one enum, one module of pins, one identity string. The crate's only clock use is
`std::time::Instant` inside `rerank` (a leaf crate, Principle III); its only `unsafe` is
`bytes::map_readonly` under the non-default `mmap` feature (ADR-0009), tested bit-for-bit
against the buffered path.

### Cargo features

| feature | default | effect |
|---|---|---|
| (none) | yes | buffered weights; zero `unsafe` compiled from this crate |
| `mmap` | no | `= ["dep:memmap2"]`; enables `LoadPath::Mmap` (the caller owns the external-writer precondition, as for the dense stage) |

### Semantics

| item | behaviour | errors |
|---|---|---|
| `load` | `verify_files` (FR-003) → `config.json` parsed and asserted (`hidden_size 384`, `vocab_size 30522`, `pad_token_id 0`, `max_position_embeddings ≥ 512`, `model_type "bert"`, `architectures ∋ "BertForSequenceClassification"`, one label) → tokenizer from the verified bytes with `longest_first` truncation at 512, no padding, `[PAD]=0`, `[CLS]=101`, `[SEP]=102` → safetensors header asserted (105 `F32`; only `bert.embeddings.position_ids` as `I64`; `classifier.weight` is `[1, 384]`) → weights through `load_path` → model built; buffered and mapped scores are bit-identical | `Error::Model { model: "ms-marco-MiniLM-L-6-v2", message }` naming the file and both values for a pin violation; the assertion for a config/header violation |
| `score` | if `passage` is empty, encode `query` alone; else encode the pair (research D4 — the reference's rule); `input_ids`, `token_type_ids`, mask all ones (`None`); `BertModel::forward`; CLS row → pooler → `tanh` → classifier → `f32` | `Error::Model` on encode/forward failure or a non-finite logit (FR-008) |
| `rerank` | `out = vec![None; passages.len()]`; `start = Instant::now()`; for each passage in input order: stop if `max_items` reached or `start.elapsed() >= max_time`; else `out[i] = Some(score(query, text)?)`. The time check precedes every pair including the first; a zero limit of either kind scores nothing; no budget scores everything (FR-007) | the first `score` error aborts the call (the pipeline degrades on it) |
| `model_id` | `MODEL_ID` — every input whose change would change a score (FR-004) | — |

**Determinism** (FR-005): a pair's score is bit-identical across call composition, passage order,
call boundaries and thread count, on one CPU architecture. Across architectures agreement is
within the golden tolerance, not bit-for-bit (the 004 rule).

## `xtriever-pipeline` (changes to the 005 contract)

```rust
pub struct HybridConfig { …, pub rerank_depth: usize /* default 20; 0 = never by default */ }

pub struct SearchOptions<'a> { …, pub rerank_depth: Option<usize> /* None = index default; Some(0) = none this call */ }

pub struct HybridHit { pub external_id: String, pub id: DocId, pub score: f64, pub rerank_score: Option<f32>, pub text: String, pub chunk: Option<ChunkInfo>, pub explain: Option<HitExplain> }
pub struct HitExplain { …, pub rerank_score: Option<f32>, pub rerank_rank: Option<u32> }
impl HitExplain { pub fn features(&self) -> [(FeatureName, f32); 7]; }   // + rerank.score (core), rerank.rank (below); NaN when absent
pub const RERANK_RANK: &str = "rerank.rank";

pub struct StageReport { …, pub rerank: Option<RerankReport> }
pub struct RerankReport { pub candidates: usize, pub scored: usize, pub skipped: Option<DegradeReason> }

impl HybridIndex {
    pub fn set_reranker(&mut self, reranker: Option<Box<dyn Reranker>>);   // per handle; not persisted
    pub fn reranker(&self) -> Option<&dyn Reranker>;
    // create / open / open_mapped / add / add_embedded / delete / commit / search / search_lexical: signatures unchanged
}

pub const FORMAT_VERSION: u32 = 2;   // ADR-0008
```

### Semantics (changed rows)

| method | behaviour | errors |
|---|---|---|
| `create` | as 005, plus an empty `passages.bin` (count 0) and `rerank_depth` in the descriptor | as 005 |
| `open` / `open_mapped` | as 005, plus: refuse `format_version ≠ 2` naming both and "rebuild the index"; open `passages.bin` (magic, header, offsets); **five-count check** — the four live counts agree and the store's `count` equals the id map's length | `Corrupt` |
| `add` / `add_embedded` | as 005, plus the dense passage text staged for the store under the assigned id | as 005 |
| `delete` | as 005, plus the slot staged as empty in the store | as 005 |
| `commit` | marker → lexical → dense → **passages** (`passages.bin.tmp` + `rename`) → id map → descriptor → marker removed | `Io`, stage errors |
| `search` / `search_lexical` | steps 1–8 as 005 with the fused list built to `max(k, d)`; **step 9** (data-model): if a re-ranker is attached and `d > 0` and the list is non-empty — check point C; `Passage`s for the first `min(d, len)` from the store; `rerank(dense_text, passages, Budget { max_items: opts.budget.max_items, max_time: remaining or None })`; length and finiteness validated; ordered per the rule below; `stages.rerank` filled; truncated to `k`. Every hit carries its `text` | as 005, plus: `Model` for a wrong-length or non-finite result (every mode); the re-ranker's error (strict only); `BudgetExhausted("rerank stage: …")` (strict + spent at C); `Corrupt` if a candidate's id is beyond the store |

**Ordering rule** (spec FR-011/FR-012, Q1 = A): hits with `rerank_score: Some` first, ordered
`(rerank_score DESC, id ASC)`; then every hit without one — not reached by the budget, or
beyond `d` — in fused order; at most `k`. `score` keeps the 005 meaning (fused, or lexical when
degraded) on every hit.

**Degradation** (FR-014/FR-015): a re-ranker `Err` ⇒ default mode returns the fused order with
`stages.rerank = Some({ candidates: 0, scored: 0, skipped: Some(StageError(msg)) })`; strict
returns the error. A partial result is **not** a degradation (`skipped: None`, `scored < candidates`)
and is not an error in strict mode. The dense stage degrading does not skip the re-ranker; the
lexical list is re-ranked the same way. Without a re-ranker, with `d = 0`, with `k = 0` or an
empty list: `stages.rerank = None` and the response equals 005's (plus `text` on hits and
`rerank_score: None`).

**Determinism**: same directory contents + same query + same options + same re-ranker ⇒
identical `hits`; `explain` never changes `hits`.

### Error mapping (no new core variants)

| situation | variant |
|---|---|
| descriptor version ≠ 2; store magic/header/offsets/count wrong; candidate id beyond the store | `Corrupt` |
| re-ranker returned the wrong length or a non-finite score | `Model { model: model_id, … }` (every mode) |
| re-ranker error in strict mode | the stage's error, unchanged |
| time budget spent at check point C in strict mode | `BudgetExhausted` |

### Cargo features

Unchanged (`mmap` forwards to the dense stage). `xtriever-pipeline` does **not** depend on
`xtriever-rerank`; it takes `Box<dyn Reranker>` from core.

## `xtriever-eval` (additive)

```rust
pub mod run {
    pub struct RerankConfig { pub name: String, pub hybrid: HybridConfig, pub rerank_depth: usize }
    impl RerankConfig { pub fn validate(&self) -> Result<()>; /* 1 ≤ rerank_depth ≤ hybrid.k; hybrid valid */ pub fn hybrid_rerank_v1() -> Self; /* hybrid_baseline_v1 + depth 20 */ }
}
pub mod report {
    pub struct StageInfo { …, pub reranker_model_id: Option<String>, pub rerank_depth: Option<usize> }   // trailing, omitted when None
    pub struct Observations { …, pub rerank_ms: Option<u64>, pub rerank_pairs: Option<u64>, pub rerank_model_bytes_buffered: Option<u64>, pub rerank_model_bytes_mmapped: Option<u64> }   // omitted when None
}
```

Existing report files deserialise and re-serialise unchanged (every new key is optional and
skipped when absent). `delta` still refuses mixed configurations; the cross-configuration table
against `hybrid-baseline-v1` is `compare`.

### `beir` example commands (added / changed)

| command | does | exit |
|---|---|---|
| `run --dataset D --config hybrid-rerank-v1 [--model-dir M] [--rerank-model-dir R] [--cache-dir C] [--index-dir DIR] [--load-path P] [--out F] [--export-run F] [--export-explain F]` | as `hybrid-baseline-v1`, then `set_reranker(MiniLmCrossEncoder::load(R, P))` (one `--load-path` for both models), every judged query searched with `rerank_depth: Some(20)`, `k = 100`, `explain`; the re-ranker is wrapped in a timing decorator so `rerank_ms` / `rerank_pairs` are recorded; `stage.kind = "hybrid-rerank"` with the model id and depth | 0 / 1 |
| `model-memory --model embedder\|rerank --load-path buffered\|mmap` | `rerank`: fresh-process load of the cross-encoder through the given path (ADR-0009 condition 5) | 0 / 1 |
| `compare a.json b.json` | unchanged — used for `hybrid-baseline-v1 → hybrid-rerank-v1` | 0 / 1 |

`--export-explain` lines gain `"rerank": [[rank, ext id], …]` (the scored prefix with its
1-based re-rank position) so `gen_006_fixtures.py --verify-rerank F` can check the ordering rule
on real data: the scored prefix is sorted by the exported `rerank.score` (ties by external id
compared as blocks) and the suffix equals the fused order with the scored ids removed.

### Report file

`stage { kind: "hybrid-rerank", embedder_fingerprint, load_path, thread_count, baseline:
"guarded", reranker_model_id, rerank_depth: 20 }`. Baselines at
`specs/006-rerank-stage/baselines/hybrid-rerank-v1.{scifact,nfcorpus,fiqa}.json`.

## Scripts

- `xtriever-eval`'s dev-dependency on `xtriever-rerank` enables `mmap` (as it does for the
  dense crate) so the example can measure both paths.
- `scripts/fetch-model.sh [--manifest FILE] [DEST_DIR]` — unchanged default (the 004 manifest
  and directory); with `--manifest reference/models/manifest-rerank.json` the destination
  defaults to `reference/models/<local_dir>` from the manifest. Same verification, same exit
  codes.
- `reference/gen_006_fixtures.py` — `rerank.json` (reference scores, ids, order; refuses gaps
  below `min_gap`), `pipeline_order.json` (ordering oracle), `manifest.json`; `--verify-embed`
  analogue `--verify-scores F` re-scores an exported `(query, passage)` sample; `--verify-rerank F`
  checks an exported explain file. Runs in `reference/.venv-004`.
- `scripts/check-no-stubs.sh` — extended to `crates/xtriever-rerank/src/`.
- `.github/workflows/ci.yml` — the `eval-smoke` path filter gains `crates/xtriever-rerank/**`;
  the smoke itself stays SciFact-only and lexical (standing rule); no model in CI.
