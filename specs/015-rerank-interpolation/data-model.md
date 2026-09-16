# Data Model: Interpolated Re-ranking as the Default

**Feature**: `015-rerank-interpolation` | **Date**: 2026-09-16

| entity | shape / rule |
|---|---|
| `RerankMode` (pipeline, `xtriever_pipeline::RerankMode`) | `Replace` \| `Interpolate { alpha: f64 }`; `Default` = `Interpolate { alpha: 0.5 }`; serde externally tagged, snake_case: `"replace"` / `{"interpolate": {"alpha": 0.5}}`; α valid iff finite and `0 ≤ α ≤ 1` |
| Order rule `Replace` | `order_reranked` unchanged: scored head by `(−s_c, DocId)`, unscored/tail in fused order, cut at `k`; `rerank_combined = None` |
| Order rule `Interpolate { α }` | over the scored head (positions with `Some(s_c)` among the first `depth`): `f = minmax(fused)`, `c = minmax(s_c as f64)`, `minmax(v) = (v − lo)/(hi − lo)`, all zeros when `hi == lo`; `combined = (1 − α)·f + α·c`; head sorted by `(−combined, fused position)`; tail in fused order; cut at `k`; `rerank_combined = Some(combined)` for head hits |
| `HybridConfig.rerank_mode` | default `Interpolate { 0.5 }`; validated with the schema at `create` |
| `Descriptor.rerank_mode` | key after `rerank_depth`; `#[serde(default)]`; **missing key = `Interpolate { 0.5 }`** (Q1 = A); `format_version` stays 2; not part of `check_identity` |
| `SearchOptions.rerank_mode: Option<RerankMode>` | `None` = the index's mode; `Some` overrides for this call; α validated per call |
| `HitExplain.rerank_combined: Option<f64>` | the ordering score under `Interpolate`; `features()` → 8 entries, the eighth `("rerank.combined", f32)` (`NaN` when absent) |
| `Response::hits` order | without re-ranking: fused; `Replace`: 006's order; `Interpolate`: the head by combined score, then fused |
| FFI `RerankMode` (uniffi Enum) | `Replace` \| `Interpolate { alpha: f64 }`; `IndexConfig.rerank_mode: Option<RerankMode>` (default `None` = engine default), `SearchOptions.rerank_mode: Option<RerankMode>` (default `None` = index mode), `IndexInfo.rerank_mode: RerankMode`, `HitExplain.rerank_combined: Option<f64>` |
| Python | `xtriever.RerankMode.REPLACE()` / `.INTERPOLATE(alpha=…)` (uniffi's enum classes), same fields; README documents the default and the override |
| Eval `RerankMode` (own enum, serde) | `Replace` \| `Interpolate { alpha }`; `RerankConfig.mode`; `hybrid_rerank_v1/v2` → `Replace`; `hybrid_rerank_v3()` = `{ name "hybrid-rerank-v3", hybrid: hybrid_baseline_v2(), rerank_depth 20, mode Interpolate { 0.5 } }`; reports carry `mode` inside the configuration |
| 006 golden `pipeline_order.json` | `cases` (unchanged bytes) + `interpolate_cases: [{name, fused: [[id, fused_score]…], scores: [s_c \| null…], d, k, alpha, expected: [[id, combined \| null]…]}]` |
| 007 golden `expected.json` | regenerated; `info.rerank_mode` added; `without_reranker` responses byte-identical; `with_reranker` heads may change |
| Baselines | `specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.<d>.json`, per query equal (1e-6) to `specs/014-rerank-depth-study/runs/lin-0.5-d20.<d>.json` |

Invariants: `Replace` results are bit-identical to before this feature; Recall@100 of v3
equals `hybrid-baseline-v2`'s; `hybrid-rerank-v2` reproduces its 013 baselines; an index
written by this version round-trips its mode; an index without the key reads as the default
and, searched with `Some(Replace)`, reproduces the v2 baseline.
