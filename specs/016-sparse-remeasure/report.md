# Report: Sparse Stage Re-measurement

**Feature**: `016-sparse-remeasure` | **Date**: 2026-09-16 | **Status**: done — **not the default** under the fixed rule (re-ranked three-way mean 0.4941 vs the 0.4963 floor); **kept as an opt-in stage by the owner's decision** (below)

## Verdict

On the pipeline as it stands — joined lexical field, interpolated re-rank — the sparse
expansion list adds **+0.0028** mean nDCG@10 (0.4913 → 0.4941): +0.23 points on SciFact,
+0.02 on NFCorpus, +0.58 on FiQA. Nothing is hurt, and nothing clears the +0.005 floor the
rule set before the numbers were read. The +1.6 points 012 measured against the 2026-09-15
pipeline were real, and 013 and 015 took them by cheaper means: the joined lexical field
closed the SciFact/NFCorpus gap the sparse list was filling, and the interpolating re-rank
took FiQA from 0.369 to 0.391 — the sparse list's remaining contribution there (+0.6 after
re-ranking) is a third of what it looked like. A stage costing 150–255 MB of postings on the
phone and a corpus encoder that cannot run in Rust does not earn +0.3 mean points. The GO is
withdrawn; the conditions that would reopen it are below.

The measurement is exact where it can be: fusing the engine's own lexical-v2 and dense lists
reproduces `hybrid-baseline-v2` list for list, re-ranking that reproduces `hybrid-rerank-v3`
list for list with **zero** reference-scored pairs, and on the 3–8 % of head pairs the sparse
list introduces the torch reference agrees with the engine's cross-encoder to **1.2 × 10⁻⁵**
at worst (200 covered pairs sampled per dataset).

## The table (nDCG@10; Δ against the matching anchor — plain vs `hybrid-baseline-v2`, rr vs `hybrid-rerank-v3`)

| row | scifact | nfcorpus | fiqa | mean | reference share |
|---|---|---|---|---|---|
| hybrid-baseline-v2 (anchor, plain) | 0.7144 | 0.3535 | 0.3692 | 0.4790 | — |
| 012 rrf-lex+dense+dot (v1 lexical, plain) | 0.7139 | 0.3508 | 0.3881 | 0.4843 | — |
| hybrid-rerank-v3 (anchor, rr) | 0.7207 | 0.3622 | 0.3910 | 0.4913 | — |
| lex2+dense-plain | 0.7144 (+0.0000) | 0.3535 (-0.0000) | 0.3692 (-0.0000) | 0.4790 (+0.0000) | — |
| lex2+dense-rr | 0.7207 (+0.0000) | 0.3622 (-0.0000) | 0.3910 (-0.0000) | 0.4913 (-0.0000) | 0%, 0%, 0% |
| lex2+dense+dot-plain | 0.7234 (+0.0091) | 0.3544 (+0.0009) | 0.3884 (+0.0192) | 0.4887 (+0.0097) | — |
| lex2+dense+dot-rr | 0.7230 (+0.0023) | 0.3624 (+0.0002) | 0.3968 (+0.0058) | 0.4941 (+0.0028) | 3%, 8%, 6% |
| dense+dot-plain | 0.6993 (-0.0151) | 0.3366 (-0.0169) | 0.4020 (+0.0328) | 0.4793 (+0.0003) | — |
| dense+dot-rr | 0.7136 (-0.0071) | 0.3511 (-0.0112) | 0.4101 (+0.0192) | 0.4916 (+0.0003) | 13%, 14%, 18% |
| lex2+dot-plain | 0.7134 (-0.0010) | 0.3458 (-0.0077) | 0.3239 (-0.0453) | 0.4611 (-0.0180) | — |
| lex2+dot-rr | 0.7152 (-0.0055) | 0.3599 (-0.0023) | 0.3489 (-0.0420) | 0.4747 (-0.0166) | 13%, 23%, 18% |

Recall@100 (plain = rr for every variant, checked): three-way 0.9617 / 0.3219 / 0.7006 against
v2's 0.9550 / 0.3216 / 0.7071 — the sparse list widens SciFact's candidate set (+0.7 points)
and narrows FiQA's (−0.6), so its FiQA gain is an ordering effect inside the same recall.

## Exactness

| check | result |
|---|---|
| `lex2+dense-plain` vs the explain's fused order (300 / 323 / 648 queries) | equal, list for list |
| `lex2+dense-plain` vs `hybrid-baseline-v2.<d>.json` per query | equal to 1e-6 |
| `lex2+dense-rr` vs `target/xt-rr3-run.<d>.jsonl` | equal, list for list; **0 reference pairs** |
| `lex2+dense-rr` vs `hybrid-rerank-v3.<d>.json` per query | equal to 1e-6 |
| Recall@100 plain = rr, every variant | equal |
| reference vs engine on 200 covered pairs | max \|Δ\| 6 × 10⁻⁶ / 1.2 × 10⁻⁵ / 8 × 10⁻⁶, mean 2 × 10⁻⁶ (the 006 tolerance is 10⁻³) |

`check <d>: PASS (plain + rr)` on all three datasets.

## The reference's contribution

| dataset | head pairs | engine-covered | reference-scored | share |
|---|---|---|---|---|
| scifact, `lex2+dense+dot` | 6,000 | 5,806 | 194 | 3.2 % |
| nfcorpus, `lex2+dense+dot` | 6,460 | 5,950 | 510 | 7.9 % |
| fiqa, `lex2+dense+dot` | 12,960 | 12,130 | 830 | 6.4 % |
| the pairings (`dense+dot`, `lex2+dot`) | — | — | 13–23 % | — |

With agreement at 10⁻⁵ the estimate is, for practical purposes, what the engine would produce.

## The decision (spec FR-007, research D6)

Rule, fixed before any run: specify the sparse stage only if `lex2+dense+dot-rr`'s three-set
mean nDCG@10 ≥ 0.4913 + 0.005 = 0.4963 **and** no dataset is more than 0.005 below
`hybrid-rerank-v3`. Cells: 0.7230 / 0.3624 / 0.3968, mean **0.4941**; deltas vs v3 +0.0023 /
+0.0002 / +0.0058 — the drop bound holds, the floor does not. **Outcome under the rule: not the default pipeline stage** (012's GO as stated — a default stage — is withdrawn).

**Owner's decision (2026-09-16)**: keep the sparse stage as an **opt-in option**, off by default —
built only for indexes that carry the sparse field and switched per search. The rule decided the
default; the FiQA-shaped case (`dense+dot-rr` 0.4101, +1.9 over v3) is the use case the option
serves. Its specification is a feature of its own (a sparse field and scorer, fusion over a
selectable set of first-stage lists, a descriptor flag with an ADR, the surfaces).

**What would reopen it** (`runs/decision.json`):
- a corpus shaped like FiQA — no titles, colloquial queries, heavy vocabulary mismatch —
  where a *lexical-free* configuration is wanted: `dense+dot-rr` reaches 0.4101 on FiQA
  (+1.9 over v3) while losing 0.7 / 1.1 on the titled sets;
- an encoder cheap enough to run at index time inside the Rust pipeline (012 F-004);
- a measured need at the head the cross-encoder does not cover (a phone that cannot afford
  depth 20);
- a change to the fused signal (new dense model, new lexical layout) that re-opens the
  candidate-set question.

## Findings

- **F-001 — 013 and 015 absorbed 012's gain.** 012: three-way fusion +1.6 mean points over
  the then-pipeline. Now: +0.97 before re-ranking (SciFact +0.9, NFCorpus +0.1, FiQA +1.9),
  +0.28 after. The joined field removed the NFCorpus contribution entirely (+0.6 → +0.1) and
  the interpolating re-ranker, which already reads the cross-encoder's view of the head,
  leaves the sparse list little to add on SciFact.
- **F-002 — Sparse replaces lexical on FiQA, and only there.** `dense+dot` beats every other
  row on FiQA (0.4101 rr) and is the worst on the titled sets; `lex2+dot` (sparse replacing
  dense) loses everywhere. The sparse list behaves like a better BM25 for vocabulary-mismatch
  corpora, not like a second dense model.
- **F-003 — The candidate-set effect is small and mixed.** Recall@100 moves +0.7 / +0.03 /
  −0.6 points; the nDCG gains are ordering gains within nearly the same top-100.
- **F-004 — The re-ranker compresses first-stage differences.** Every plain gap shrinks after
  re-ranking (three-way +0.97 → +0.28; `dense+dot` on FiQA +3.3 → +1.9): with the cross-encoder
  informing the head, the first stage's job is recall, and the current two lists already
  supply it.

## Deliberately not done (research D8)

No new encoding, no quantised or `bm25x` variants, no Rust, no baseline change, no device
measurement. The 012 encodings and manifests stay on disk and in the tree for the reopening
cases.


## Red checkpoint (Rule 4, T006)

`reference/.venv-012/bin/python -m pytest reference/tests_016 -q`: 4 errors during collection —
`ModuleNotFoundError: No module named 'sparse_remeasure'` in `test_fuse.py`, `test_scores.py`,
`test_decide.py`, `test_end_to_end.py`.

Inputs confirmed first (T001): every explain line of the three 014 exports has `fused_scores`
and exactly 50 `rerank` entries; the explain `lexical` lists equal the 013 `lex2` runs on all
300 / 323 / 648 queries; the 012 dot runs, the 015 v3 runs, the model and the torch
environment are present.
