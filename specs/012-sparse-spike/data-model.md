# Data Model: Sparse Expansion Spike

**Feature**: `012-sparse-spike` | **Date**: 2026-09-15

| entity | where | fields / shape | invariants |
|---|---|---|---|
| Model manifest | `reference/models/manifest-sparse-doc-v2.json`, `…-v3.json` (committed) | `schema_version`, `repository`, `revision` (commit hash), `files[] {name, bytes, sha256}` for `config.json`, `tokenizer.json`, `model.safetensors`, `idf.json`; `license`, `parameters`, `activation` (`log1p_relu` / `log1p_log1p_relu`) | fetched and verified by `scripts/fetch-model.sh --manifest`; weights never committed |
| Sparse vector | `target/xt-sparse-cache/<model-key>/<dataset>/docs-NNNNN.npz`, `queries.npz` | CSR pieces: `ids` (str), `indptr` (int64), `indices` (int32 vocab id), `data` (f32 ≥ 0); shard = 1,000 documents in corpus order | complete shards only (`.part` → rename); `model-key = <repo-basename>@<revision[:8]>`; `data > 0` (zeros dropped) |
| Run | `target/xt-sparse-runs/<dataset>/<name>.jsonl` | one line per judged query: `{"query_id": str, "doc_ids": [str]}` at most 100 ids, query ids ascending | the harness's export shape (`crates/xtriever-eval/src/run.rs:163`); ties by ascending doc id; empty lists allowed and counted |
| Report | `…/<name>.json` | the 003 scorer's `reference()` output: `per_query`, `scored_queries`, `dropped_identical`, `mean_ndcg_10`, `mean_recall_100`, `beir_rounded` + the spike's `provenance` (model key, variant, parameters, timestamps) | produced only by `reference/gen_003_fixtures.reference` |
| Variant | a name → a run | `engine-lexical`, `engine-dense`, `engine-hybrid` (imported); `dot`, `dot-q10`, `dot-q100`, `dot-q1000`, `bm25x`, `bm25x+text-b0.5/1/2`, `rrf-lex+dot`, `rrf-dense+dot`, `rrf-lex+dense+dot`, `rrf-lex+bm25x`, `rrf-dense+bm25x`, `rrf-lex+dense+bm25x` — each per model where model-dependent | parameters fixed by research D5 |
| Costs record | `specs/012-sparse-spike/runs/costs-<model-key>.json` (committed) | per dataset: `documents`, `wall_s`, `docs_per_s`, `device`, `threads`, `nnz_doc {mean, p95, max}`, `nnz_query {mean, p95, max}`, `truncated_docs`, `empty_queries`; model: `parameters`, `bytes_on_disk`, `license`, `revision` | measured, not derived, except the Wikipedia projection which names its method |
| Summary table | `specs/012-sparse-spike/runs/summary.json` (committed) + the report's tables | `{dataset: {variant: {ndcg_10, recall_100, scored_queries}}}` for every model | every cell traceable to a run file by name |
| Decision | `report.md` | the FR-011 rule applied: go / no-go, model revision, scoring, quantisation scale | written after the table, the rule before |
