# Report: Re-rank Depth Study

**Feature**: `014-rerank-depth-study` | **Date**: 2026-09-16 | **Status**: done — decision: **`lin-0.5` at depth 20 replaces replace-order re-ranking at depth 20** (follow-up feature)

## Verdict

The cross-encoder is not the problem; letting it *replace* the fused order is. Re-ranking the
v2 fused list by cross-encoder score alone loses on SciFact at every depth (−0.4 points at
depth 5, −1.9 at 20, −2.0 at 50) and no replace-order depth qualifies under the rule.
Combining the fused signal with the cross-encoder score inside the head does: linear
interpolation at α = 0.5, depth 20 scores 0.7207 / 0.3622 / 0.3910 nDCG@10 — above the
un-re-ranked list on all three sets (+0.6 / +0.9 / +2.2 points) and a three-set mean of
0.4913 against the current default's 0.4768 (+0.0145), for the same 20 cross-encoder calls per
query. Fifteen of the sixteen interpolation rows qualify (only `rrf-d5`, 0.4812, misses the floor); the winner is the highest mean,
depth 50 (0.4911) offering nothing more for 2.5× the calls. Every cell was verified by the
003 reference; the offline derivation reproduced the 013 depth-20 run and an end-to-end
depth-5 run exactly. Nothing in the pipeline changes here; the follow-up is named below.

## The table (nDCG@10; runs/table.md, runs/table.json)

| variant | depth | scifact | nfcorpus | fiqa | mean | Δ vs depth 0 | Δ vs replace-d20 | calls/query |
|---|---|---|---|---|---|---|---|---|
| depth0 (hybrid-baseline-v2) | 0 | 0.7144 | 0.3535 | 0.3692 | 0.4790 | — | +0.0022 | 0 |
| replace | 5 | 0.7104 (-0.0040) | 0.3589 (+0.0053) | 0.3747 (+0.0055) | 0.4813 | +0.0023 | +0.0045 | 5 |
| replace | 10 | 0.7056 (-0.0087) | 0.3608 (+0.0073) | 0.3747 (+0.0055) | 0.4804 | +0.0014 | +0.0036 | 10 |
| replace | 20 | 0.6954 (-0.0189) | 0.3609 (+0.0073) | 0.3742 (+0.0050) | 0.4768 | -0.0022 | +0.0000 | 20 |
| replace | 50 | 0.6943 (-0.0201) | 0.3551 (+0.0016) | 0.3722 (+0.0030) | 0.4739 | -0.0052 | -0.0030 | 50 |
| rrf | 5 | 0.7149 (+0.0005) | 0.3555 (+0.0020) | 0.3732 (+0.0040) | 0.4812 | +0.0021 | +0.0043 | 5 |
| rrf | 10 | 0.7177 (+0.0033) | 0.3586 (+0.0051) | 0.3757 (+0.0065) | 0.4840 | +0.0050 | +0.0072 | 10 |
| rrf | 20 | 0.7133 (-0.0011) | 0.3590 (+0.0055) | 0.3854 (+0.0162) | 0.4859 | +0.0069 | +0.0090 | 20 |
| rrf | 50 | 0.7097 (-0.0047) | 0.3605 (+0.0070) | 0.3858 (+0.0166) | 0.4854 | +0.0063 | +0.0085 | 50 |
| lin-0.25 | 5 | 0.7214 (+0.0070) | 0.3574 (+0.0039) | 0.3735 (+0.0043) | 0.4841 | +0.0051 | +0.0073 | 5 |
| lin-0.25 | 10 | 0.7258 (+0.0115) | 0.3588 (+0.0053) | 0.3772 (+0.0080) | 0.4873 | +0.0083 | +0.0105 | 10 |
| lin-0.25 | 20 | 0.7228 (+0.0084) | 0.3578 (+0.0043) | 0.3820 (+0.0128) | 0.4875 | +0.0085 | +0.0107 | 20 |
| lin-0.25 | 50 | 0.7214 (+0.0071) | 0.3596 (+0.0061) | 0.3820 (+0.0128) | 0.4877 | +0.0086 | +0.0108 | 50 |
| lin-0.5 | 5 | 0.7228 (+0.0084) | 0.3578 (+0.0043) | 0.3757 (+0.0065) | 0.4854 | +0.0064 | +0.0086 | 5 |
| lin-0.5 | 10 | 0.7223 (+0.0079) | 0.3612 (+0.0077) | 0.3809 (+0.0117) | 0.4881 | +0.0091 | +0.0113 | 10 |
| lin-0.5 | 20 | 0.7207 (+0.0063) | 0.3622 (+0.0087) | 0.3910 (+0.0218) | 0.4913 | +0.0123 | +0.0145 | 20 |
| lin-0.5 | 50 | 0.7186 (+0.0042) | 0.3630 (+0.0095) | 0.3918 (+0.0226) | 0.4911 | +0.0121 | +0.0143 | 50 |
| lin-0.75 | 5 | 0.7153 (+0.0010) | 0.3597 (+0.0061) | 0.3784 (+0.0092) | 0.4844 | +0.0054 | +0.0076 | 5 |
| lin-0.75 | 10 | 0.7174 (+0.0030) | 0.3614 (+0.0079) | 0.3821 (+0.0129) | 0.4870 | +0.0080 | +0.0102 | 10 |
| lin-0.75 | 20 | 0.7123 (-0.0021) | 0.3619 (+0.0084) | 0.3921 (+0.0229) | 0.4887 | +0.0097 | +0.0119 | 20 |
| lin-0.75 | 50 | 0.7176 (+0.0032) | 0.3609 (+0.0074) | 0.3940 (+0.0248) | 0.4908 | +0.0118 | +0.0140 | 50 |

Recall@100 is identical to depth 0 in every cell (0.955000 / 0.321648 / 0.707111): re-ordering
inside the first 100 cannot change the set. `Δ` columns are three-set means; the per-dataset
deltas in parentheses are against depth 0.

## Exactness (SC-001, SC-003)

| check | result |
|---|---|
| end-to-end `--rerank-depth 20` (SciFact) | `config: "hybrid-rerank-v2"`, 0.695430 / 0.955000 — the 013 baseline, no suffix |
| derived `replace-d20` vs `target/xt-rr2-run.<d>.jsonl` (013's exported runs) | list for list equal, all three sets |
| derived `replace-d20` vs `specs/013-…/hybrid-rerank-v2.<d>.json` per query | equal to 1e-6, all three sets |
| derived `replace-d5` vs end-to-end `hybrid-rerank-v2@d5` (SciFact, 1,500 pairs) | list for list equal |
| derived `replace-d50` vs the explain's own `hits` | equal, all three sets |
| `check <d>` | `PASS (4 exactness checks, 20 Recall@100 checks)` scifact; `(2, 20)` nfcorpus, fiqa |

The per-pair contract of the re-ranker (`crates/xtriever-rerank/src/scorer.rs`: every pair
scored alone, at its own length) held: no score depended on the depth it was produced at.

End-to-end runs (`runs/e2e-d50.<d>.json`, `runs/e2e-d5.scifact.json`, each `verify-run: PASS`):

| run | pairs | wall | ms / pair | nDCG@10 |
|---|---|---|---|---|
| scifact @d50 | 15,000 | 2,261 s | 150.7 | 0.694280 |
| nfcorpus @d50 | 16,150 | 2,261 s | 140.0 | 0.355128 |
| fiqa @d50 | 32,400 | 2,777 s | 85.7 | 0.372213 |
| scifact @d5 | 1,500 | 222 s | 147.8 | 0.710409 |

(`RAYON_NUM_THREADS=4`, M1 laptop; FiQA's passages are shorter.)

## The decision (spec FR-006, research D7)

Rule, fixed before any run: a configuration replaces the default only if its three-set mean
nDCG@10 ≥ 0.4768 + 0.005 = 0.4818 **and** no dataset is more than 0.005 below depth 0; the
highest qualifying mean wins, ties by the smaller depth; none → the default stays.

`runs/decision.json`: qualifying — `lin-0.5-d20` 0.4913, `lin-0.5-d50` 0.4911, `lin-0.75-d50`
0.4908, `lin-0.75-d20` 0.4887, `lin-0.5-d10` 0.4881, `lin-0.25-d50` 0.4877, `lin-0.25-d20`
0.4875, `lin-0.25-d10` 0.4873, `lin-0.75-d10` 0.4870, `rrf-d20` 0.4859, `lin-0.5-d5` 0.4854,
`rrf-d50` 0.4854, `lin-0.75-d5` 0.4844, `lin-0.25-d5` 0.4841, `rrf-d10` 0.4840. Not
qualifying: every `replace` row (`replace-d5` 0.4813 misses the floor by 0.0005; the rest
fail SciFact's drop bound or the floor), `rrf-d5` (0.4812), `lin-0.75-d20`'s SciFact −0.0021
is within the bound so it qualifies on the mean.

**Winner: `lin-0.5-d20`** — 0.7207 / 0.3622 / 0.3910, mean 0.4913, 20 calls per query.
Cost: unchanged from today's default. What it saves: nothing on the phone; what it buys:
+1.45 mean points, and the re-ranker stops hurting SciFact.

**Landed in 015** (`specs/015-rerank-interpolation/`, ADR-0012): the interpolating rule at
α 0.5, depth 20 is the pipeline default; `hybrid-rerank-v3` reproduces the `lin-0.5-d20` cells
list for list.

**Follow-up (a feature of its own, not this study)**: the pipeline's re-rank stage gains a
score-combination mode — `(1 − α)·minmax(fused) + α·minmax(cross-encoder)` within the head,
α = 0.5, ties by fused rank — as the default in `crates/xtriever-pipeline` (descriptor field
beside `rerank_depth: 20` at `src/descriptor.rs:88`; the re-rank order rule in
`src/rerank.rs`; `explain` reporting both terms), the FFI default following it, the 007 Swift
and 011 Python goldens re-generated for the new order (they were built on replace-order
re-ranking), `hybrid-rerank-v3` baselines in the harness, and an ADR because the on-disk
descriptor and the default behaviour change. Until then the default stays replace-order at
depth 20; ADR-0011's SciFact loss stands as recorded.

## Findings

- **F-001 — Where the SciFact loss lives: replacing, not depth.** With replace-order
  re-ranking the loss grows with depth (−0.4 → −0.9 → −1.9 → −2.0 points at 5 / 10 / 20 /
  50) — every deeper head gives the cross-encoder more of RRF's correct ordering to undo.
  With the fused signal kept in the mix, every depth is positive on SciFact (`lin-0.5`: +0.8 /
  +0.8 / +0.6 / +0.4). The cross-encoder's *scores* carry information; its *order* alone does
  not beat two-retriever RRF on scientific claims.
- **F-002 — Interpolation lifts NFCorpus and FiQA too, by more than replace ever did.** FiQA
  gains +2.2 points at `lin-0.5-d20` against +0.5 for replace-d20 — larger than the sparse
  stage's projected FiQA gain in 012 (+1.9 in three-way fusion). NFCorpus +0.9 vs +0.7. The
  re-ranker was under-used on every set, not only mis-used on SciFact.
- **F-003 — Depth 50 buys nothing.** For every variant the depth-50 mean is within ±0.0021 of
  depth 20 (replace: −0.0029), at 2.5× the calls. Depth 10 already captures most of the gain
  (`lin-0.5`: 0.4881 vs 0.4913); on a phone, depth 10 with interpolation is a defensible
  cheaper point — the follow-up can keep 20 (the winner) or take 10 (−0.3 mean points, half
  the calls); the rule picks 20.
- **F-004 — α is not knife-edge.** 0.25 / 0.5 / 0.75 at depth 20 give 0.4875 / 0.4913 /
  0.4887; the parameter-free `rrf` (0.4859) is 0.5 points behind. α = 0.5 is the middle of a
  plateau, not a fitted optimum; the follow-up should not re-tune it per dataset.
- **F-005 — 012's GO for the sparse stage should be re-read against 0.4913, not 0.468.** The
  three-way fusion the spike measured (0.484 mean with the v1 lexical list) is now below the
  interpolated re-rank alone. The sparse stage's remaining case is FiQA-shaped corpora
  (no titles, vocabulary mismatch) and the phone's cost profile; its expected gain needs
  re-measuring from `lin-0.5-d20` before it is specified.

## Review of the derivation's precision

The derivation reads the harness's own exported `fused_scores` (f64) and `rerank` scores
(f32 as JSON); the pipeline's tie rule (ascending `DocId` = corpus position) is applied with
positions from `corpus.jsonl` order. The four exactness checks are the evidence that no
rounding or ordering differs from the engine.

## Deliberately not done (research D9)

No conditional re-ranking policy; no other cross-encoder; no depth beyond 50; no default,
pipeline, FFI or golden change (the follow-up); no Wikipedia or device measurement; no
per-dataset α.


## Red checkpoint (Rule 4, T005)

`reference/.venv-012/bin/python -m pytest reference/tests_014 -q`:

```text
ImportError while importing test module '…/reference/tests_014/test_order.py'
ImportError while importing test module '…/reference/tests_014/test_table.py'
ImportError while importing test module '…/reference/tests_014/test_variants.py'
E   ModuleNotFoundError: No module named 'rerank_study'
3 errors during collection
```

The suites: `test_order.py` (the replace rule against the 006 reference's `ORDER_CASES`, the
fixture at every depth, depth 0, ties by position, `derive` reproducing `hits`),
`test_variants.py` (rank fusion and linear interpolation hand-computed, α 0 / 1, the single
candidate and the constant column, no duplicates and the tail in fused order for every
variant × depth, Recall@100 equal to depth 0), `test_table.py` (the decision rule's floor,
drop, tie and none cases; the reader's refusal of a pre-014 export; the run round trip).
