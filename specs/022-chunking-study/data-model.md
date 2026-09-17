# Data Model: The Chunking Study

| Entity | Fields | Rule |
|---|---|---|
| **Document** | `_id`, `title`, `text` | from `corpus.jsonl`; `contents` = `title + " " + text` (empty parts omitted) |
| **Passage** | `doc_id`, `ordinal`, `text` (the chunk), `positions` (of `title + " " + chunk`) | `external_id = f"{doc_id}#{ordinal}"`, `ChunkInfo(parent=doc_id, ordinal=ordinal)`; `whole`: `external_id = doc_id`, no chunk info |
| **Variant** | `whole` \| `contract` \| `chonky` \| `chonky-bounded` | research D5 |
| **Build record** (`build.json`) | `variant`, `dataset`, `documents`, `passages`, `over_window`, `under_16`, `title_fills_window`, `split_s`, `embed_s`, `passages_per_doc_median` | one per index |
| **Cell** | `(variant, dataset, rerank_depth ∈ {0, 20}, k ∈ {100, 300})` → `runs/<variant>-d<depth>@<k>.<dataset>.jsonl` (run) and `.json` (scores) | `k = 100` only for `whole` (the anchor) |
| **Run** | `{query_id: [doc_id, …]}` — at most 100 documents, each once, first-occurrence order | MaxP |
| **Scores** | the 003 report: `per_query{qid: {ndcg_10, recall_100}}`, `scored_queries`, `mean_ndcg_10`, `mean_recall_100`; plus `short_queries` (fewer than 100 documents) | pytrec_eval |
| **Anchor check** | `whole-d0@100` vs `hybrid-baseline-v2`, `whole-d20@100` vs `hybrid-rerank-v3`, per dataset | tolerance 1e-6, every query and the means |
| **Decision** | per variant: `datasets`, `scope`, `mean_ndcg_10`, `delta_mean`, `deltas{dataset}`, `delta_recall`, `recommended`; `best_chunker`; `constants` | the rule of spec US4; `owner-decision.json` |
