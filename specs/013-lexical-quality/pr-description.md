# 013 lexical quality — one joined field for BM25

**One change**: the evaluation's BM25 gains `lexical-baseline-v2`, which indexes
`title + " " + text` as a single `contents` field (boost 1.0, same analyzer, query and depth)
instead of `title` × 2.0 beside `text`; `hybrid-baseline-v2` and `hybrid-rerank-v2` are the
v1 recipes over it. Nothing in the engine changes.

**Attribution** (012 spike's BM25, nDCG@10 SciFact / NFCorpus / FiQA): a replica of the
engine's layout scores 0.6207 / 0.3115 / 0.2473 — the engine's numbers; the same BM25 over
one joined field scores 0.6867 / 0.3228 / 0.2473; k1/b changes, stop words and an extra title
field all lose. Full table in [report.md](report.md).

## Baselines (nDCG@10 / Recall@100, every run verified by the 003 reference scorer to 1e-6)

| configuration | dataset | v1 | v2 | Δ nDCG@10 |
|---|---|---|---|---|
| lexical | scifact | 0.627044 / 0.887556 | 0.685602 / 0.921333 | **+0.058559** |
| lexical | nfcorpus | 0.311523 / 0.247820 | 0.322688 / 0.247331 | **+0.011165** |
| lexical | fiqa | 0.250238 / 0.551775 | 0.250238 / 0.551775 | +0.000000 |
| hybrid | scifact | 0.689727 / 0.941667 | 0.714369 / 0.955000 | **+0.024642** |
| hybrid | nfcorpus | 0.345008 / 0.320720 | 0.353510 / 0.321648 | **+0.008501** |
| hybrid | fiqa | 0.369210 / 0.707111 | 0.369210 / 0.707111 | +0.000000 |
| hybrid-rerank | scifact | 0.703862 / 0.941667 | 0.695430 / 0.955000 | **−0.008431** |
| hybrid-rerank | nfcorpus | 0.360287 / 0.320720 | 0.360854 / 0.321648 | +0.000567 |
| hybrid-rerank | fiqa | 0.374214 / 0.707111 | 0.374214 / 0.707111 | +0.000000 |

Means: lexical 0.3963 → 0.4195; hybrid 0.4680 → 0.4790; re-rank 0.4795 → **0.4768**.

**SC-002 fails for the re-ranked configuration** — landed under
[ADR-0011](../../docs/adr/0011-rerank-v2-scifact-regression.md): the SciFact fused list
improves by 2.5 points, and re-ranking it at depth 20 with the pinned cross-encoder lands
1.9 points below its own input (in v1 the re-ranker added 1.4). A re-ranker finding, not a
field-layout one; follow-up spec. Nothing was tuned to pass.

## What stays v1

Every v1 configuration and baseline (byte-identical, re-run and reproduced); the engine,
the Wikipedia index schema, the FFI, the fixture goldens. The `beir` example's default
configuration remains `lexical-baseline-v1`.

## CI

The `eval-smoke` job compares against `specs/013-lexical-quality/baselines/lexical-baseline-v2.scifact.json`
(`--config lexical-baseline-v2`); still SciFact only, no model. Path filter gains the 013 baselines.

## Changes

- `crates/xtriever-eval/src/run.rs`: `Source::TitleAndText`, `lexical_baseline_v2`,
  `hybrid_baseline_v2`, `hybrid_rerank_v2`; `examples/beir.rs`: the three names, `dense_fields`
  derived from the lexical schema (was hard-coded `["title","text"]`; the dense list is
  unchanged — re-run and equal to the 004 baseline).
- `join_title_text` (review round 1): one join behind the dense passage and the `contents`
  field, so the two are equal for every document; no corpus document is title-only, so no
  passage, cache entry or baseline changes.
- Tests first (committed red): `tests/run.rs` (three), `tests/hybrid_run.rs` (one: join, empty
  title, empty text, both empty, equality with the dense passage).
- Docs: `python/README.md` schema recommendation, `xtriever-cli` `chunking.rs` comment (the
  only non-eval diff under `crates/`, `///` lines only — SC-006 narrowed in review to permit
  exactly it), `xtriever-eval` crate docs, 012 report
  F-002 resolved.

Gate: fmt · clippy (host + Windows target) · nextest workspace · deny · iOS / iOS-sim / Android
checks · no-stubs · smoke v2 PASS.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
