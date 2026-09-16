# Contract: the re-rank mode across the surfaces

**Feature**: `015-rerank-interpolation` | **Date**: 2026-09-16

## Pipeline (`xtriever_pipeline`)

```rust
pub enum RerankMode { Replace, Interpolate { alpha: f64 } }          // Default: Interpolate { alpha: 0.5 }
pub fn order_reranked(fused: &[(DocId, f64)], scores: &[Option<f32>], k: usize) -> Vec<(DocId, f64, Option<f32>)>            // unchanged
pub fn order_interpolated(fused: &[(DocId, f64)], scores: &[Option<f32>], k: usize, alpha: f64) -> Vec<(DocId, f64, Option<f32>, Option<f64>)>
pub struct HybridConfig { …, pub rerank_depth: usize, pub rerank_mode: RerankMode }
pub struct SearchOptions<'a> { …, pub rerank_depth: Option<usize>, pub rerank_mode: Option<RerankMode>, … }
pub struct HitExplain { …, pub rerank_score: Option<f32>, pub rerank_rank: Option<u32>, pub rerank_combined: Option<f64> }
pub const RERANK_COMBINED: &str = "rerank.combined";                  // beside RERANK_RANK; features() has 8 entries
```

- `HybridIndex::create` rejects α ∉ [0, 1] or non-finite: `Error::Schema("rerank alpha must be within [0, 1], got …")`; `search` rejects the same in an override.
- Descriptor: `"rerank_mode": {"interpolate": {"alpha": 0.5}}` after `"rerank_depth"`; absent → the default; `format_version` 2.
- `HybridIndex::config().rerank_mode` reports the effective recorded mode (the default for an index without the key).

## FFI (Swift via uniffi) and Python

```text
enum RerankMode { Replace, Interpolate { alpha: Double } }
record IndexConfig  { …, rerank_depth: u32 = 20, rerank_mode: RerankMode? = None }   // None → Interpolate(0.5)
record SearchOptions{ …, rerank_depth: u32? = None, rerank_mode: RerankMode? = None } // None → the index's mode
record IndexInfo    { …, rerank_depth: u32, rerank_mode: RerankMode, … }
record HitExplain   { …, rerank_score: f32?, rerank_rank: u32?, rerank_combined: f64? }
```

- Errors: an invalid α at build or search → `XtrieverError.Schema`.
- Python: `xtriever.RerankMode.INTERPOLATE(alpha=0.5)`, `xtriever.RerankMode.REPLACE()`; `SearchOptions(k=10, rerank_mode=xtriever.RerankMode.REPLACE())` reproduces the pre-015 order.
- Explain feature names, in order: `bm25.score, bm25.rank, dense.score, dense.rank, fused.score, rerank.score, rerank.rank, rerank.combined`.

## Harness (`xtriever-eval`, `beir`)

```text
beir run --dataset D --config hybrid-rerank-v3 --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/D --rerank-model-dir R [--rerank-depth N] [--out F] [--export-run F] [--export-explain F]
beir run --dataset D --config hybrid-rerank-v2 …        # replace-order, unchanged, reproduces 013
```

- `RerankConfig { name, hybrid, rerank_depth, mode }`; `hybrid_rerank_v3()` = v2's hybrid at depth 20, `Interpolate { alpha: 0.5 }`; reports embed the configuration including `mode`.
- The example passes the mode as a `SearchOptions` override, so the eval index directory's descriptor is irrelevant to which rule runs.

## Reference (`reference/gen_006_fixtures.py`, `reference/rerank_study.py`)

```text
gen_006_fixtures.py: order_interpolated(fused_ids, fused_scores, scores, d, k, alpha) -> [[id, combined | None], …]; INTERPOLATE_CASES → fixtures/006/pipeline_order.json["interpolate_cases"]; ["cases"] unchanged
rerank_study.py check-cell --dataset D --report R --cell C [--run RUN --derived DERIVED]     # per-query metrics to 1e-6; lists exact when both given; exit 1 on mismatch
```
