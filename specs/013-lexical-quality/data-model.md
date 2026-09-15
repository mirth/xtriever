# Data Model: Lexical Quality — One Field for BM25

**Feature**: `013-lexical-quality` | **Date**: 2026-09-16

| entity | change |
|---|---|
| `Source` (eval) | + `TitleAndText`: the value is `title + " " + text`; an empty title contributes nothing and no separator; an empty text likewise; both empty → empty value (skipped under `omit_empty_fields`) |
| `EvalConfig::lexical_baseline_v2()` | `name "lexical-baseline-v2"`, `fields [FieldSpec { name "contents", from TitleAndText, analyzer "standard_en", boost 1.0 }]`, `omit_empty_fields true`, `query MatchAll`, `k 100` — equal to v1 in every field but `fields` |
| `HybridConfig::hybrid_baseline_v2()` | v1 with `lexical: lexical_baseline_v2()` |
| `RerankConfig::hybrid_rerank_v2()` | v1 with the hybrid v2 |
| `beir` example | names `lexical-baseline-v2`, `hybrid-baseline-v2`, `hybrid-rerank-v2`; `dense_fields` = the lexical configuration's field names |
| Baselines | `specs/013-lexical-quality/baselines/<config>.<dataset>.json`, the report shape of 003 (`mean_ndcg_10`, `mean_recall_100`, `per_query`, `beir_rounded`, `config`, `stage`, …) |
| CI smoke | `--config lexical-baseline-v2 --baseline specs/013-lexical-quality/baselines/lexical-baseline-v2.scifact.json` |

Invariants: v1 configurations and baselines byte-identical; `TitleAndText` for a document
equals the dense passage `dense_baseline_v1` builds for it (title-then-text, one space, empty
title omitted).
