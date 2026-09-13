# Data Model: The Re-rank Stage

**Feature**: `006-rerank-stage` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

Entities from the spec's "Key Entities", made concrete by [research.md](./research.md). Two
crates are touched: `xtriever-rerank` (new types) and `xtriever-pipeline` (extended types, one
new file, format version 2); the harness gains one configuration and optional report keys.

## Pinned Re-rank Model (`xtriever_rerank::model::PINNED`, research D2)

```rust
pub struct PinnedFile { pub name: &'static str, pub bytes: u64, pub sha256: &'static str }
pub struct PinnedModel {
    pub repository: &'static str,   // "cross-encoder/ms-marco-MiniLM-L-6-v2"
    pub revision: &'static str,     // "233902d25c440f23af6f7d6e94d2946bac0bee0a"
    pub files: [PinnedFile; 3],     // config.json 794 B, tokenizer.json 711,396 B, model.safetensors 90,870,598 B
    pub hidden: usize,              // 384
    pub max_tokens: usize,          // 512 — the pair's total, [CLS]/[SEP]s included
    pub f32_tensors: usize,         // 105 (+ one I64 index buffer, bert.embeddings.position_ids)
}
pub const MODEL_ID: &str = "cross-encoder/ms-marco-MiniLM-L-6-v2@233902d…;weights=sha256:821d1a…;max_tokens=512;trunc=longest_first;head=cls-pooler-tanh-linear;act=identity;dtype=f32;engine=candle-0.9.2";
```

Assembled from the same literal pieces as the pins (the 004 `concat!` device). Mirrored by
`reference/models/manifest-rerank.json` (`schema_version 1`, `repository`, `revision`,
`local_dir: "ms-marco-MiniLM-L-6-v2"`, `files[3]`, `hidden`, `max_tokens`, `f32_tensors`);
`tests/model_pins.rs` keeps the two equal, and `gen_006_fixtures.py` emits `MODEL_ID` so the
Rust constant is asserted against the oracle's.

**Validation at load** (FR-002/FR-003, in order): each file's size then SHA-256 (streamed);
`config.json` parsed and asserted (`hidden_size 384`, `vocab_size 30522`, `pad_token_id 0`,
`max_position_embeddings ≥ 512`, `model_type "bert"`, `architectures` contains
`BertForSequenceClassification`, `id2label` has exactly one entry); tokenizer built from the
verified bytes with `TruncationParams { max_length: 512, LongestFirst }`, no padding, `[PAD] → 0`,
`[CLS] → 101`, `[SEP] → 102`; safetensors header asserted (105 `F32`, only
`bert.embeddings.position_ids` as `I64`, `classifier.weight` shape `[1, 384]`); weights read through `bytes::read`
(buffered `std::fs::read`, or the read-only map under `mmap`) and loaded through
`VarBuilder::from_slice_safetensors`; encoder + pooler + classifier built.

## Cross-Encoder (`xtriever_rerank::MiniLmCrossEncoder`)

| Field | Type | Notes |
|---|---|---|
| `tokenizer` | `tokenizers::Tokenizer` | pair encoding, `longest_first` at 512 (research D4) |
| `encoder` | `candle_transformers::models::bert::BertModel` | loaded under the `bert` prefix |
| `pooler` | `candle_nn::Linear` | `bert.pooler.dense`, 384 → 384, followed by `tanh` |
| `classifier` | `candle_nn::Linear` | `classifier`, 384 → 1 |
| `device` | `candle_core::Device` | `Cpu` |
| `load_path` | `LoadPath` | how the weights were read; recorded by the harness |

**Operations**: `load(dir, LoadPath) -> Result<Self>` (`LoadPath::{Buffered, Mmap}`, the
dense crate's type mirrored; `Mmap` is `#[cfg(feature = "mmap")]` and carries the external-writer
precondition, ADR-0009); `load_path()`; `score(query, passage) -> Result<f32>` (one pair,
one forward at its own length; `Error::Model` on a non-finite logit); `tokenize_for_test(query,
passage) -> Result<(Vec<u32>, Vec<u32>)>` (`input_ids`, `token_type_ids`, `#[doc(hidden)]`);
`thread_count()`; and the `Reranker` impl: `model_id() = MODEL_ID`, `rerank(query, passages,
budget)` = the budget loop of research D6 over `score`.

**Invariants**: `rerank` returns exactly `passages.len()` entries; `Some` entries form a prefix
of the input order (the loop stops, it never skips); a pair's score is a pure function of
`(query, passage)` on a given architecture (FR-005).

## Re-rank Goldens (`reference/fixtures/006/rerank.json`, research D14)

```json
{
  "model_id": "cross-encoder/ms-marco-MiniLM-L-6-v2@…",
  "max_tokens": 512,
  "tolerance_abs": 0.001,
  "min_gap": 0.01,
  "queries": [
    {
      "name": "berlin-population",
      "query": "How many people live in Berlin?",
      "passages": [
        { "text": "Berlin has a population of …", "input_ids": [101, …], "token_type_ids": [0, …], "truncated": false, "score": 8.845856666564941 },
        …
      ],
      "order": [0, 3, 1, 2]
    },
    …
  ]
}
```

`order` is the reference's descending-score order of passage indexes; the generator refuses a
query whose adjacent gap is below `min_gap` (10 × the tolerance), so "exact order" is a fair
test. Named cases present: `over-length` (a 600-word passage, `truncated: true`, 512 ids),
`empty-passage`, `empty-query`, `both-empty`, `near-tie` (gap in `[0.01, 1.0]`, asserted).
Scores are written with Python `repr` and read under `float_roundtrip`.

## Pipeline Order Goldens (`reference/fixtures/006/pipeline_order.json`)

```json
{ "cases": [ { "name": "partial-m-lt-d", "fused": [7, 3, 9, 1, 4], "scores": [0.5, null, 2.0, null, null], "d": 4, "k": 5,
               "expected": [[9, 2.0], [7, 0.5], [3, null], [1, null], [4, null]] }, … ] }
```

`scores[i]` is the stub's answer for `fused[i]` (`null` = not scored; entries at `i ≥ d` are
absent by construction). `expected` is research D8's rule computed independently in Python:
scored first by `(score DESC, id ASC)`, then the unscored in fused order, cut at `k`. Cases:
`full`, `partial-m-lt-d`, `none-scored`, `d-gt-k`, `d-lt-k`, `d-ge-len`, `d-zero`, `ties-by-id`,
`k-lt-scored`, `single`.

## Passage Store (`<dir>/passages.bin`, store format version 1; research D7, ADR-0008)

| Region | Encoding |
|---|---|
| magic | `XTPASS01` (8 bytes) |
| header length | `u64` LE |
| header | JSON `{"format_version": 1, "count": N}` |
| offsets | `(N + 1) × u64` LE, non-decreasing, `off[0] = 0`, `off[N] = text block length` |
| text | UTF-8, `text(i) = block[off[i]..off[i+1]]` |

Position = internal id; `N` = the id map's length (assigned-id space, deleted slots included);
a deleted or empty passage is an empty range. **In memory**: `PassageStore { path, offsets:
Vec<u64>, text_start: u64, pending: BTreeMap<u32, Option<String>> }`. `read(id) -> Result<String>`
opens the file, seeks, reads `off[i+1] − off[i]` bytes, validates UTF-8 (`Corrupt` otherwise);
an id `≥ N` is `Corrupt` ("stage returned id unknown to the passage store"). `commit(next_len)`
streams the previous block and the pending texts into `passages.bin.tmp`, syncs, renames, and
reloads the offsets. **Validation at open**: magic, header version, `count` = id map length,
offsets monotone and within the file — any failure is `Corrupt` naming the file.

## HybridConfig / Descriptor (format version 2)

`HybridConfig` gains `rerank_depth: usize` (default **20**; `HybridConfig::new` sets it;
`validate` accepts any value — 0 means "never re-rank by default"). `Descriptor` gains
`rerank_depth` after `rrf_k`; `format_version` is written as 2 and any other value is refused
at open naming both (ADR-0008). `open` rebuilds `HybridConfig` including `rerank_depth`.

**Commit order**: marker → lexical → dense → **passages** → id map → descriptor → marker
removed. **Open check** (five counts): descriptor `live_docs` = id map live = lexical live =
dense live, **and** passage store `count` = id map length.

## HybridIndex additions

| Item | Type | Notes |
|---|---|---|
| `reranker` | `Option<Box<dyn Reranker>>` | attached per handle by `set_reranker`; not persisted |
| `passages` | `PassageStore` | created empty by `create`, opened by `open`/`open_mapped` |
| `reranker()` | `Option<&dyn Reranker>` | accessor |

`add` / `add_embedded` stage the dense passage text (`HybridIndex::passage`) into the store's
pending map under the assigned id; `delete` stages `None`.

## SearchOptions (extended)

```rust
pub struct SearchOptions<'a> {
    pub depth: Option<usize>,
    pub rerank_depth: Option<usize>,       // NEW: None = the index's configured depth; Some(0) = no re-ranking this call
    pub strict: bool,
    pub budget: Budget,
    pub elapsed: Option<&'a dyn Fn() -> Duration>,
    pub explain: bool,
}
```

`Default` unchanged in meaning (`rerank_depth: None`).

## Response (extended)

```rust
pub struct HybridHit {
    pub external_id: String,
    pub id: DocId,
    pub score: f64,                    // fused (or lexical when degraded) — meaning unchanged
    pub rerank_score: Option<f32>,     // NEW: the cross-encoder's score where the stage scored it
    pub text: String,                  // NEW: the stored passage text (FR-010)
    pub chunk: Option<ChunkInfo>,
    pub explain: Option<HitExplain>,
}
pub struct HitExplain {
    pub bm25_score: Option<f32>, pub bm25_rank: Option<u32>,
    pub dense_score: Option<f32>, pub dense_rank: Option<u32>,
    pub fused: f64,
    pub rerank_score: Option<f32>,     // NEW
    pub rerank_rank: Option<u32>,      // NEW: 1-based position among the scored candidates
}
impl HitExplain { pub fn features(&self) -> [(FeatureName, f32); 7]; }   // + rerank.score, rerank.rank (NaN when absent)
pub const RERANK_RANK: &str = "rerank.rank";   // pipeline-defined; rerank.score is the core's

pub struct StageReport {
    pub lexical_candidates: usize,
    pub dense_candidates: Option<usize>,
    pub degraded: Option<Degradation>,     // the dense stage, as in 005
    pub rerank: Option<RerankReport>,      // NEW: None = the stage did not run (no re-ranker, depth 0, empty list, or k == 0)
    pub time_limit_ignored: bool,
}
pub struct RerankReport {
    pub candidates: usize,                 // passages handed to the re-ranker (≤ d)
    pub scored: usize,                     // Some(_) entries returned
    pub skipped: Option<DegradeReason>,    // stage error or budget spent at check point C ⇒ candidates 0, scored 0
}
```

**Ordering invariant of `hits`** (research D8): the hits with `rerank_score: Some` form a
prefix ordered `(rerank_score DESC, id ASC)`; the rest follow in fused order; `len ≤ k`; the
hits are the first `k` entries after re-ordering the first `d` candidates of the fused list
(built to `max(k, d)`). When `d > k` a candidate at fused rank in `(k, d]` can be promoted into
the top `k` and displace a fused top-`k` candidate — the intended effect (research D8), so the
set is *not* always the fused top `k`; every hit is a fused candidate and none is duplicated.

### Search algorithm (steps 1–8 as 005; step 9 new)

9. **Re-rank** — if a re-ranker is attached and `d = rerank_depth > 0` and the list is
   non-empty: check point **C** (`check_budget`); build `Passage`s for the first `min(d, len)`
   candidates from the store; call `rerank(dense_text, &passages, &remaining_budget)`; validate
   the length and finiteness (`Error::Model` in every mode); split scored / unscored; order per
   D8; `stages.rerank = Some(RerankReport { candidates, scored, skipped: None })`. On `Err`
   from the stage or a spent budget: default mode ⇒ fused order unchanged,
   `stages.rerank = Some(RerankReport { 0, 0, skipped: Some(reason) })`; strict ⇒ the error /
   `BudgetExhausted`. The fused list is built to `max(k, d)` before this step and truncated to
   `k` after it.

## Harness additions (`xtriever-eval`)

- `RerankConfig { name: "hybrid-rerank-v1", hybrid: HybridConfig, rerank_depth: 20 }`;
  `validate`: `1 ≤ rerank_depth ≤ hybrid.k`, `hybrid.validate()`. `rerank_v1()` constructor.
- `StageInfo` + `reranker_model_id: Option<String>`, `rerank_depth: Option<usize>` (trailing,
  omitted when absent); `kind = "hybrid-rerank"`.
- `Observations` + `rerank_ms`, `rerank_pairs`, `rerank_model_bytes_buffered`,
  `rerank_model_bytes_mmapped` (optional).
- Baselines `specs/006-rerank-stage/baselines/hybrid-rerank-v1.{scifact,nfcorpus,fiqa}.json`.
