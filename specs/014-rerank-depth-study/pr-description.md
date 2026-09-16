# 014 re-rank depth study — the cross-encoder should inform the fused order, not replace it

**The question** (013 F-001, ADR-0011): with the v2 fused lists, re-ranking at depth 20 lowered
the three-set mean nDCG@10 below the un-re-ranked hybrid (0.4768 vs 0.4790) — all of it
SciFact. Is it the depth, or the fact that the cross-encoder's order replaces RRF's?

**Method**: one end-to-end run per dataset at depth 50 with the explain export; every
shallower depth (5 / 10 / 20) and every interpolation variant (rank fusion; linear at
α 0.25 / 0.5 / 0.75) derived offline from those scores — exact, because the re-ranker scores
each pair alone — and scored by the 003 reference. Two end-to-end checks: derived depth 20
equals the 013 run list for list and per query; derived depth 5 equals a fresh SciFact run.
Recall@100 identical to depth 0 in all 60 cells.

## The table (nDCG@10; per-dataset Δ vs depth 0 in parentheses)

| variant | depth | scifact | nfcorpus | fiqa | mean | Δ vs replace-d20 |
|---|---|---|---|---|---|---|
| depth 0 (hybrid-baseline-v2) | 0 | 0.7144 | 0.3535 | 0.3692 | 0.4790 | +0.0022 |
| replace | 5 | 0.7104 (−0.0040) | 0.3589 (+0.0053) | 0.3747 (+0.0055) | 0.4813 | +0.0045 |
| replace | 20 (today's default) | 0.6954 (−0.0189) | 0.3609 (+0.0073) | 0.3742 (+0.0050) | 0.4768 | — |
| replace | 50 | 0.6943 (−0.0201) | 0.3551 (+0.0016) | 0.3722 (+0.0030) | 0.4739 | −0.0030 |
| rrf | 20 | 0.7133 (−0.0011) | 0.3590 (+0.0055) | 0.3854 (+0.0162) | 0.4859 | +0.0090 |
| lin-0.25 | 20 | 0.7228 (+0.0084) | 0.3578 (+0.0043) | 0.3820 (+0.0128) | 0.4875 | +0.0107 |
| **lin-0.5** | **20** | **0.7207 (+0.0063)** | **0.3622 (+0.0087)** | **0.3910 (+0.0218)** | **0.4913** | **+0.0145** |
| lin-0.5 | 10 | 0.7223 (+0.0079) | 0.3612 (+0.0077) | 0.3809 (+0.0117) | 0.4881 | +0.0113 |
| lin-0.75 | 20 | 0.7123 (−0.0021) | 0.3619 (+0.0084) | 0.3921 (+0.0229) | 0.4887 | +0.0119 |

All 21 rows: `specs/014-rerank-depth-study/runs/table.md`; every cell verified to 1e-6.

## The decision (rule fixed in the spec before any run)

Qualifies iff mean ≥ 0.4768 + 0.005 and no dataset > 0.005 below depth 0; highest qualifying
mean wins, ties by the smaller depth. **Fifteen of the sixteen interpolation rows qualify (only `rrf-d5` misses the floor), no replace-order
row does. Winner: `lin-0.5` at depth 20** — +1.45 mean points over today's default at the same
20 cross-encoder calls per query, positive on every dataset.

Findings: the SciFact loss is replace-order re-ranking, not depth (it grows with depth:
−0.4 → −0.9 → −1.9 → −2.0); interpolation lifts FiQA by +2.2 points, more than the sparse
stage projected; depth 50 buys nothing over 20; α is a plateau (0.25–0.75 all qualify).
012's sparse GO needs re-reading against 0.4913.

## What changes here — and what does not

- `crates/xtriever-eval/examples/beir.rs`: `--rerank-depth N` (report `config` gets `@dN`
  unless N is the constructor's depth; without the flag the baseline reproduces byte for
  byte) and a `fused_scores` key in the explain export (existing readers unaffected).
- `reference/rerank_study.py` + `reference/tests_014/` (56 tests, committed red first).
- Committed: the 60 derived cells, 4 end-to-end reports, `table.{json,md}`, `decision.json`,
  the report; 013's F-001 marked "studied in 014".
- **Unchanged**: every pipeline / rerank / core crate, every default, every earlier baseline
  (`git diff --stat main -- crates/ ':!crates/xtriever-eval' specs/00{3,4,5,6}-* specs/013-*/baselines` empty).

**Follow-up feature**: the pipeline's re-rank stage gains the score-combination mode as its
default (descriptor field, order rule, explain terms), the FFI default follows, the 007 / 011
goldens are regenerated, `hybrid-rerank-v3` baselines, and an ADR (descriptor and default
behaviour change).

Gate: fmt · clippy · nextest 267/267 · deny · iOS / iOS-sim / Android checks · pytest 56/56.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
