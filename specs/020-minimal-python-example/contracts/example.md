# Contract: `apps/python-minimal-demo/demo.py`

```
python apps/python-minimal-demo/demo.py "how do bees make honey"
```

Inputs: the `xtriever` wheel installed in the running Python; the embedder at
`reference/models/all-MiniLM-L6-v2` (`XTRIEVER_MODEL_DIR`) and the re-ranker at
`reference/models/ms-marco-MiniLM-L-6-v2` (`XTRIEVER_RERANK_MODEL_DIR`), relative to the
repository root.

| Case | Output | Exit |
|---|---|---|
| no argument / more than one | `usage: demo.py "your question"` | 2 |
| success | `indexed 10 documents` · blank · `fused (lexical + dense), 5 hits` + hits · blank · `re-ranked (depth 10), 5 hits` + hits | 0 |
| a missing model | the package's error, uncaught | 1 |

Hit line: ` 1. doc-03  score=0.0328  Honey bees` (fused) / ` 1. doc-03  score=0.0328  rerank=8.6573  Honey bees`
(re-ranked). Nothing is left on disk after the run.
