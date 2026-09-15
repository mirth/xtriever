# Report: Lexical Quality — One Field for BM25

**Feature**: `013-lexical-quality` | **Started**: 2026-09-16 | **Status**: in progress

## Red checkpoint (Rule 4, T004)

`cargo nextest run -p xtriever-eval -E 'test(v2) | test(title_and_text)'` does not compile:

```text
error[E0599]: no variant or associated item named `TitleAndText` found for enum `Source`
error[E0599]: no function or associated item named `lexical_baseline_v2` found for struct `EvalConfig`   (×4)
error[E0599]: no function or associated item named `hybrid_baseline_v2` found for struct `HybridConfig`
error[E0599]: no function or associated item named `hybrid_rerank_v2` found for struct `RerankConfig`
```

The four tests: `v2_config_is_v1_with_one_joined_field`, `v2_hybrid_and_rerank_wrap_the_v2_lexical`,
`v1_is_unchanged` (`tests/run.rs`); `title_and_text_joins_with_one_space_and_omits_empty_sides`
(`tests/hybrid_run.rs`). Before any change, `lexical-baseline-v1` on SciFact reproduced its
committed baseline exactly (nDCG@10 0.627044, Recall@100 0.887556; T001).

## Lexical baselines (T007, SC-001)

`lexical-baseline-v2` on the three sets, each run verified by the 003 reference scorer
(`--verify-run … PASS`, 1e-6), `beir compare` against `specs/003-eval-harness/baselines/lexical-baseline-v1.<d>.json`:

| dataset | metric | v1 | v2 | abs | rel | SC-001 floor |
|---|---|---|---|---|---|---|
| scifact | nDCG@10 | 0.627044 | 0.685602 | **+0.058559** | +9.34% | ≥ +0.05 ✓ |
| scifact | Recall@100 | 0.887556 | 0.921333 | +0.033778 | +3.81% | — |
| nfcorpus | nDCG@10 | 0.311523 | 0.322688 | **+0.011165** | +3.58% | ≥ +0.008 ✓ |
| nfcorpus | Recall@100 | 0.247820 | 0.247331 | −0.000489 | −0.20% | — |
| fiqa | nDCG@10 | 0.250238 | 0.250238 | +0.000000 | +0.00% | ±0.001 ✓ |
| fiqa | Recall@100 | 0.551775 | 0.551775 | +0.000000 | +0.00% | — |

The engine's joined field lands on the spike's prediction (0.6867 / 0.3228 / 0.2473 in the
spike's BM25; 0.6856 / 0.3227 / 0.2502 in the engine — within 0.1 / 0.0 / 0.3 points, the
tokenizer's share). FiQA, which has no titles, is bit-identical to v1: the one field is the
`text` field under another name. NFCorpus Recall@100 slips by 0.0005 (one document in one
query's tail); nDCG@10 is the target metric and gains 1.1 points.

## Fused baselines (T008–T009, SC-002)

Every run verified by the 003 reference scorer (PASS, 1e-6). `dense-baseline-v1` on SciFact
re-run after the `dense_fields` change equals the 004 baseline (0.645082 / 0.925000): the
dense list is untouched. nDCG@10 / Recall@100:

| configuration | dataset | v1 | v2 | Δ nDCG@10 | Δ Recall@100 |
|---|---|---|---|---|---|
| hybrid-baseline | scifact | 0.689727 / 0.941667 | 0.714369 / 0.955000 | **+0.024642** | +0.013333 |
| hybrid-baseline | nfcorpus | 0.345008 / 0.320720 | 0.353510 / 0.321648 | **+0.008501** | +0.000929 |
| hybrid-baseline | fiqa | 0.369210 / 0.707111 | 0.369210 / 0.707111 | +0.000000 | +0.000000 |
| hybrid-baseline | **mean** | 0.467982 | 0.479030 | **+0.011048** | — |
| hybrid-rerank | scifact | 0.703862 / 0.941667 | 0.695430 / 0.955000 | **−0.008431** | +0.013333 |
| hybrid-rerank | nfcorpus | 0.360287 / 0.320720 | 0.360854 / 0.321648 | +0.000567 | +0.000929 |
| hybrid-rerank | fiqa | 0.374214 / 0.707111 | 0.374214 / 0.707111 | +0.000000 | +0.000000 |
| hybrid-rerank | **mean** | 0.479454 | 0.476833 | **−0.002622** | — |

**SC-002 passes for `hybrid-baseline-v2` and fails for `hybrid-rerank-v2`** (mean −0.0026,
all of it SciFact). ⛔ Rule 6 stop-point — reported, not adjusted.

### The SciFact re-rank drop is the re-ranker, not the field change

On SciFact the fused list improves by 2.5 points (0.6897 → 0.7144) and its Recall@100 by
1.3, yet re-ranking the better list at depth 20 lands *below* re-ranking the worse one
(0.6954 vs 0.7039) — and 1.9 points below its own input (0.7144). In v1 the cross-encoder
added 1.4 points to the fused list; in v2 it removes 1.9. The ms-marco MiniLM-L-6 cross-encoder
orders SciFact's top-20 worse than RRF over the joined-field BM25 and dense lists does, so
the more RRF gets right, the more the re-ranker has to lose. NFCorpus and FiQA are unaffected
(+0.0006, ±0). This is a 006 finding surfaced by a stronger candidate list: the re-ranker's
value on scientific-claim queries is negative once the first stage is good enough, and the
best known SciFact configuration is now `hybrid-baseline-v2` (0.7144), not a re-ranked one.
