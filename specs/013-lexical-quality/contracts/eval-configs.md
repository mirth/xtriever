# Contract: the v2 evaluation configurations

**Feature**: `013-lexical-quality` | **Date**: 2026-09-16

```text
beir run   --dataset D --config lexical-baseline-v2 [--out F] [--export-run F]
beir run   --dataset D --config hybrid-baseline-v2 --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/D [--out F] [--export-run F]
beir run   --dataset D --config hybrid-rerank-v2  --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/D --rerank-model-dir R [--out F] [--export-run F]
beir smoke --dataset scifact --config lexical-baseline-v2 --baseline specs/013-lexical-quality/baselines/lexical-baseline-v2.scifact.json
beir compare specs/003-eval-harness/baselines/lexical-baseline-v1.D.json specs/013-lexical-quality/baselines/lexical-baseline-v2.D.json
```

- `lexical-baseline-v2` indexes exactly one field, `contents`, whose value for a BEIR
  document is `title + " " + text` with an empty title contributing nothing (no separator);
  analyzer `standard_en`, boost 1.0, `Match(None, query)`, depth 100 — otherwise v1.
- `hybrid-baseline-v2` / `hybrid-rerank-v2` are their v1s with the lexical configuration
  replaced; the dense list is identical to v1's (same cache, same passage text).
- Every v1 name keeps working and reproduces its committed baseline.
- Reports name their configuration (`config`), so a v2 report can never be mistaken for v1.
