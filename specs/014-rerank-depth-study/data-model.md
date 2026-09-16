# Data Model: Re-rank Depth Study

**Feature**: `014-rerank-depth-study` | **Date**: 2026-09-16

| entity | shape |
|---|---|
| **Explain line** (per query, `beir --export-explain`, one JSON object per line) | `query_id`; `lexical`, `dense`: `[[rank, id], …]`; `fused`: `[id, …]` in fused order; **`fused_scores`: `[f64, …]` parallel to `fused` (new)**; `rerank`: `[[rank, id, score], …]` for the scored head; `hits`: the response |
| **Head** | the first `d` entries of `fused` with their `fused_scores` and the `rerank` scores joined by id; `r_f` = 1-based fused rank; `DocId` = corpus position of the id (from `corpus.jsonl` order) |
| **Variant** | `replace` · `rrf` · `lin-0.25` · `lin-0.5` · `lin-0.75` (research D4); plus `depth0` = the fused list as is |
| **Depth** | `d ∈ {5, 10, 20, 50}`; `d = 0` only for `depth0` |
| **Derived run** | `target/xt-rerank-study/<dataset>/<variant>-d<d>.jsonl` in the harness export shape `{"query_id", "doc_ids"}` (≤ 100 ids) |
| **Cell** | `runs/<variant>-d<d>.<dataset>.json`: `{variant, depth, dataset, ndcg_10, recall_100, per_query, scored_queries, calls_per_query = d, source: "derived" \| "end-to-end"}` — the 003 scorer's numbers |
| **Table** | `runs/table.json` (rows = variant × depth, columns = datasets + `mean`, each with `ndcg_10`, `recall_100`, `delta_vs_depth0`, `delta_vs_default`) and `runs/table.md` (the same, markdown) |
| **Decision** | `runs/decision.json`: `{rule: {mean_floor: 0.4818, max_drop: 0.005, baseline_default: "replace-d20", default_mean: 0.4768}, qualifying: [row, …], winner: row \| null, statement}` |
| **End-to-end run** | `beir run --config hybrid-rerank-v2 --rerank-depth 50` → report `config: "hybrid-rerank-v2@d50"`; `--rerank-depth 5` on SciFact → `hybrid-rerank-v2@d5`; both under `runs/` with their `--verify-run` PASS noted in the report |

Invariants: `recall_100` of every cell equals `depth0`'s for that dataset (SC-002); the derived
`replace-d20` run equals `target/xt-rr2-run.<dataset>.jsonl` list for list and its cell equals
`specs/013-lexical-quality/baselines/hybrid-rerank-v2.<dataset>.json` per query to 1e-6; the
derived `replace-d5` on SciFact equals the end-to-end `@d5` run list for list (SC-003);
`replace-d50` derived equals the end-to-end `@d50` run's `hits` (the source of the scores —
a tautology that checks the parser); every earlier baseline byte-identical.
