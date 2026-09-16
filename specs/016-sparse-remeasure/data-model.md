# Data Model: Sparse Stage Re-measurement

**Feature**: `016-sparse-remeasure` | **Date**: 2026-09-16

| entity | shape / rule |
|---|---|
| **Candidate lists** (per query, per dataset) | `lex2`, `dense`: the explain export's `lexical` / `dense` `[[rank, id]…]` (100 each); `dot`: 012's dot run `doc_ids` (100); ids external, tie key = corpus position |
| **Fused list** | `fuse(lists)`: score = Σ over lists containing the id of `1/(60 + rank)`; sorted by `(−score, position)`; cut at 100; carries `(id, fused_score)` |
| **Variant** | `lex2+dense` (anchor) · `lex2+dense+dot` (the decision row) · `dense+dot` · `lex2+dot`; each `plain` (un-re-ranked) and `rr` (re-ranked) |
| **Head scores** | for the first 20 fused candidates: the explain `rerank` score when the (query, id) pair is covered, else the 006 reference's logit; `source ∈ {engine, reference}` per pair |
| **Re-ranked list** | `rerank_study.order_lin(head, rest, 100, 0.5)` over the head built from `(fused_score, s_c, r_f, position)`; ties by fused rank |
| **Reference-score cache** | `target/xt-sparse-remeasure/<d>/reference-scores.jsonl`: `{query_id, doc_id, score}` — scored once, reused across variants |
| **Agreement sample** | the first 200 covered pairs per dataset (query order): `max_abs_diff`, `mean_abs_diff` between reference and engine scores |
| **Cell** | `runs/<variant>-<plain\|rr>.<d>.json`: `{variant, reranked, dataset, ndcg_10, recall_100, per_query, scored_queries, reference_scored_pairs, reference_share, agreement: {sampled, max_abs_diff, mean_abs_diff}}` (last three on `rr` cells) |
| **Anchors** | `hybrid-baseline-v2.<d>.json` (013), `hybrid-rerank-v3.<d>.json` (015), `target/xt-rr3-run.<d>.jsonl`; 012 `rrf-lex+dense+dot` 0.7139 / 0.3508 / 0.3881 |
| **Table** | `runs/table.{json,md}`: rows variant × plain/rr; columns per dataset nDCG@10 (Δ vs the matching anchor), mean, reference share |
| **Decision** | `runs/decision.json`: `{rule: {row: "lex2+dense+dot-rr", mean_floor: 0.4963, max_drop: 0.005, anchor: "hybrid-rerank-v3"}, row, qualifies, statement, reopen: [...]}` |

Invariants: `lex2+dense-plain` = `hybrid-baseline-v2` per query (1e-6) and its list = the
explain's `fused`; `lex2+dense-rr` = `hybrid-rerank-v3` per query and list for list
(`target/xt-rr3-run.<d>.jsonl`), with **zero** reference-scored pairs (every v2 head pair is
covered); every `rr` cell's Recall@100 = its `plain` cell's; no file under `crates/` changes.
