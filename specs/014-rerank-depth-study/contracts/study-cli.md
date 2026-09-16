# Contract: the study's commands

**Feature**: `014-rerank-depth-study` | **Date**: 2026-09-16

## `beir run` (Rust example, `crates/xtriever-eval/examples/beir.rs`)

```text
beir run --dataset D --config hybrid-rerank-v2 --rerank-depth N --cache-dir target/xt-dense-cache \
         --index-dir target/xt-rerank-index-v2/D --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 \
         --out F --export-run R --export-explain E
```

- `--rerank-depth N` overrides the configuration's `rerank_depth` (validated as before:
  `1 ≤ N ≤ k`); accepted only with a re-rank configuration (an error otherwise). When `N`
  differs from the constructor's depth the report's `config` is `hybrid-rerank-v2@dN`; when
  equal, the name is unchanged (so `--rerank-depth 20` reproduces the baseline byte for byte).
- The explain line gains `"fused_scores"`, an array of the fused scores parallel to `"fused"`.
  Existing keys and their order are unchanged.

## `reference/rerank_study.py` (Python, `reference/.venv-012`)

```text
rerank_study.py derive --dataset D --explain E [--out-dir target/xt-rerank-study/D]     # every variant × depth → JSONL runs
rerank_study.py score  --dataset D [--runs-dir …] [--out-dir specs/014-rerank-depth-study/runs]   # 003 scorer → cells
rerank_study.py check  --dataset D --baseline-run target/xt-rr2-run.D.jsonl --baseline-report specs/013-…/hybrid-rerank-v2.D.json [--e2e-run R --e2e-depth 5]
rerank_study.py table  [--runs-dir …]                                          # table.json + table.md over the three sets
rerank_study.py decide [--runs-dir …]                                          # decision.json, printed
rerank_study.py all    --dataset D --explain E …                               # derive + score + check
```

- `derive` refuses an explain file without `fused_scores` ("explain export predates 014; re-run
  with the current `beir`") and one whose `rerank` head is shorter than the deepest requested
  depth for any query with ≥ that many candidates.
- `check` exits non-zero on any list-for-list or per-query metric mismatch, printing the first
  differing query.
- `decide` applies research D7 exactly and prints the qualifying rows and the winner or
  "no configuration qualifies; the default stays".
- Datasets: `scifact | nfcorpus | fiqa`; qrels from `reference/datasets/beir/D/qrels/test.tsv`;
  corpus order from `reference/datasets/beir/D/corpus.jsonl` (the `DocId` map).
