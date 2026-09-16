# Contract: `reference/sparse_remeasure.py`

**Feature**: `016-sparse-remeasure` | **Date**: 2026-09-16 | Python 3.12, `reference/.venv-012`

```text
sparse_remeasure.py fuse   --dataset D [--out-dir target/xt-sparse-remeasure/D]        # 4 plain runs from the explain lists + the dot run
sparse_remeasure.py rerank --dataset D [--out-dir …] [--model-dir reference/models/ms-marco-MiniLM-L-6-v2] [--agreement-sample 200]
                                                                                        # 4 rr runs; reference scores for uncovered pairs (cached); the agreement sample
sparse_remeasure.py score  --dataset D [--runs-dir …] [--out-dir specs/016-sparse-remeasure/runs]   # 003 scorer → 8 cells
sparse_remeasure.py check  --dataset D                                                  # lex2+dense-plain = hybrid-baseline-v2 & explain fused; lex2+dense-rr = hybrid-rerank-v3 & xt-rr3-run; Recall@100 plain = rr; exit 1 on mismatch
sparse_remeasure.py table  [--runs-dir specs/016-sparse-remeasure/runs]                 # table.json + table.md
sparse_remeasure.py decide [--runs-dir …]                                               # decision.json, printed
sparse_remeasure.py all    --dataset D [...]                                            # fuse + rerank + score + check
```

- Inputs (fixed paths, refused with a message when absent): `target/xt-rerank-study/D/explain-d50.jsonl`
  (must carry `fused_scores` and 50 `rerank` entries per query), `target/xt-sparse-runs/D/dot@opensearch-neural-sparse-encoding-doc-v3-distill@babf71f3.jsonl`,
  `reference/datasets/beir/D/{corpus.jsonl,queries.jsonl,qrels/test.tsv}`, the 013 and 015 baselines, `target/xt-rr3-run.D.jsonl`.
- `rerank` prints per dataset: pairs in heads, covered by the engine, scored by the reference (count, share); the agreement figures; refuses to run without the pinned model directory.
- `decide` applies FR-007 exactly to the `lex2+dense+dot-rr` row and prints the rule, the row's cells, the outcome; the other rows are printed as findings.
- Datasets: `scifact | nfcorpus | fiqa`. Exit codes: 0 / 1 (check or decide inputs missing) / 2 (input refused).
