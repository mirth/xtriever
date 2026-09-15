# Report: Sparse Expansion Spike

**Feature**: `012-sparse-spike` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Pins

| model | revision | licence | parameters | activation | files |
|---|---|---|---|---|---|
| `opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill` | `babf71f3c48695e2e53a978208e8aba48335e3c0` | apache-2.0 | 66,985,530 | `log(1+log(1+relu))` | config, tokenizer, model.safetensors (~267 MB), idf.json (889,360 B) |
| `opensearch-project/opensearch-neural-sparse-encoding-doc-v2-distill` | `8921a26c78b8559d6604eb1f5c0b74c079bee38f` | apache-2.0 | 66,985,530 | `log(1+relu)` | same layout |

Manifests: `reference/models/manifest-sparse-doc-v{2,3}.json`; fetched and verified by
`scripts/fetch-model.sh --manifest …` (its file loop is generic; no script change).

## Red checkpoint (C1)

`reference/.venv-012/bin/pytest reference/tests_012 -q` → **1 passed** (the 003 scorer's
convention probe), **12 failed** (9 engine-run reproductions — no exports yet; 3 BM25/RRF/top-k
— functions missing), **3 errors** (the recipe checks — no encoder). The recipe oracle is the
v3 model card's own example: `What's the weather in ny now?` vs `Currently New York is rainy.`
→ similarity 11.1105 with six printed (query, document) weights.
