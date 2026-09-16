# Research: Re-rank Depth Study

**Feature**: `014-rerank-depth-study` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## D1 — The depth is already a per-search option; the harness only lacks a flag

`xtriever_core::SearchOptions::rerank_depth: Option<usize>` overrides the descriptor's depth
per search (`crates/xtriever-pipeline/src/search.rs:167–172`: `opts.rerank_depth.unwrap_or(
self.config.rerank_depth)`, the candidate list built to `max(k, depth)`). The `beir` example
already passes it (`examples/beir.rs:546–558`) from `RerankConfig::rerank_depth`, which the
`hybrid-rerank-v2` constructor fixes at 20. **Decision**: a `--rerank-depth N` flag on `beir
run` overriding the configuration's depth (the report's `config` name gains a `@dN` suffix
when the override differs from the constructor, so a depth-50 report can never be mistaken
for the baseline). No pipeline, descriptor or default changes (spec FR-007).

## D2 — The derivation is exact because the re-ranker scores every pair alone

`crates/xtriever-rerank/src/scorer.rs:1–9`: "Each query–passage pair goes through the model
**alone, at its own length**: a pair's tensor shapes depend only on the pair, so its score
cannot depend on the other passages in the call, their order or the call boundaries". So a
(query, document) score at depth 50 is the score at depth 5, 10 or 20 bit for bit, and the
shallower replace-order lists follow from the depth-50 scores by the engine's own rule —
`xtriever_pipeline::rerank::order_reranked` (`src/rerank.rs:13–35`: scored by `(-score,
DocId)` first, then the unscored in fused order, cut at `k`), which the 006 reference
mirrors as `order_reranked(fused, scores, d, k)` in `reference/gen_006_fixtures.py:311`.
**Decision**: one end-to-end run per dataset at depth 50 with `--export-explain`; depths 5 /
10 / 20 and every interpolation variant derived offline. Spec FR-004 checks the derivation
against two end-to-end runs: depth 20 (the 013 baseline, its exported run
`target/xt-rr2-run.<d>.jsonl` and per-query metrics) and depth 5 on SciFact (300 × 5 pairs,
minutes). If either disagrees the assumption is wrong and every depth runs end-to-end
(≈ 3.5 h instead of 2 h) — stop-and-report first.

## D3 — What the explain export carries, and the one field it lacks

`--export-explain` writes per query (`examples/beir.rs:591–599`): `lexical` and `dense`
`[rank, id]` lists from a second un-re-ranked search at `2 × candidate_depth`, `fused` (the
ids in fused order, sorted by `(−fused score, DocId)`), `rerank` `[rank, id, score]` for the
scored prefix, `hits` as returned. The **fused score** itself is not exported, and the linear
interpolation needs it. It is `Σ 1/(rrf_k + rank)` over the lists a candidate appears in
(`fusion.rs:23–45`, f64) and could be recomputed, but a recomputation is a second
implementation of the rule. **Decision**: add `"fused_scores": [f64, …]` (parallel to
`fused`) to the explain line — a new key, so `gen_005_fixtures.py` / `gen_006_fixtures.py`'s
`verify_*` readers (which index `fused`, `rerank`, `hits`) are unaffected. Internal ids are
not exported either; the tie rule needs them, and for the harness `DocId(i)` is the corpus
position (`run.rs` `build`), so the study maps external id → corpus position from
`corpus.jsonl` order (the 012 spike did the same).

## D4 — The variants, defined so they are testable

Within the head `H` = the first `d` fused candidates (fused rank `r_f`, 1-based; fused score
`s_f`; cross-encoder score `s_c`, f32 as exported):

- **replace** (the pipeline's rule): order `H` by `(−s_c, DocId)`, then the rest in fused
  order, cut at `k = 100`. Derived depth 20 must equal the 013 run exactly.
- **rrf**: order `H` by `(−(1/(60 + r_f) + 1/(60 + r_c)), DocId)` where `r_c` is the
  1-based rank by `(−s_c, DocId)`; rest in fused order. Parameter-free, the engine's fusion
  constant.
- **lin-α**, α ∈ {0.25, 0.5, 0.75}: `f = minmax(s_f over H)`, `c = minmax(s_c over H)`,
  `s = (1 − α)·f + α·c`; order `H` by `(−s, r_f)` — ties (including a single candidate or a
  constant column, where `minmax` yields 0 for every member) fall back to the fused order,
  which is itself tie-broken by `DocId` (spec edge case). Rest in fused order.
- **depth 0**: `hybrid-baseline-v2`, the un-re-ranked list (the 013 baseline).

Cost per query for every row = `d` cross-encoder calls (the pipeline scores exactly the head;
`StageReport.rerank.scored` in the depth-50 run confirms 50 per query, or fewer when a query
has fewer candidates).

## D5 — Scoring and verification

Every derived run is written in the harness's export shape (`{"query_id", "doc_ids"}` JSONL)
and scored by the 003 reference (`reference/gen_003_fixtures.py`'s `reference(qrels, run)`,
imported as 012 did); the end-to-end runs are scored by the harness and verified with
`--verify-run` as usual. The derived depth-20 replace run is compared with
`specs/013-lexical-quality/baselines/hybrid-rerank-v2.<d>.json` per query (metrics to 1e-6)
and with `target/xt-rr2-run.<d>.jsonl` list for list (exact). Recall@100 must equal depth 0's
in every cell (spec SC-002): re-ordering within the first `max(k, d)` = 100 cannot change the
set.

## D6 — Where the code lives

The study script `reference/rerank_study.py` (Python 3.12, `reference/.venv-003`, which holds
`numpy 2.5.3` and `pytrec_eval 0.5`; no new environment) with `reference/tests_014/` (pytest,
the 012 pattern): the derivation rules of D4 on hand-built cases including the 006
`ORDER_CASES`, the tie and constant-column edge cases, and a JSONL round trip. Committed
first, failing (spec FR-010). The Rust change is confined to `examples/beir.rs` (the flag,
the `fused_scores` key, the `@dN` name) — nothing in the `xtriever-eval` library, nothing in
any other crate (spec SC-005). Reports, the table and the decision go to
`specs/014-rerank-depth-study/{runs/,report.md}`; the explain exports and derived runs stay
under `target/xt-rerank-study/` (spec FR-009).

## D7 — The decision rule, restated for the script

`qualifies(row) = mean(row) ≥ 0.4768 + 0.005 and ∀ dataset: ndcg(row, dataset) ≥
ndcg(depth 0, dataset) − 0.005`; among qualifying rows the highest mean, ties by the smaller
`d`; none → "the default stays". The script prints the qualifying set and the winner; the
report quotes it. The current default (replace, 20) is a row like any other.

## D8 — Costs

Depth 50 end-to-end on the three sets: the 013 depth-20 runs took ~50 minutes at
`RAYON_NUM_THREADS=4`; 2.5 × the pairs ≈ 2 h (the re-ranker dominates). Depth 5 on SciFact
≈ 3 minutes. Derivation and scoring: seconds per row. Within spec SC-006.

## D9 — Not done

No conditional re-ranking policy, no other cross-encoder, no depth beyond 50 (the fused list
is 100 and depth 50 already re-orders half of it), no default change (a follow-up feature
with the 007/011 goldens), no Wikipedia or device measurement.
