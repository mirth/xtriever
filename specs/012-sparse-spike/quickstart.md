# Quickstart: running the Sparse Expansion Spike

**Feature**: `012-sparse-spike` | **Date**: 2026-09-15 | **Plan**: [plan.md](./plan.md)

## Step 0 — Environment and pins

```bash
/opt/homebrew/bin/uv venv --python 3.12 reference/.venv-012 && VIRTUAL_ENV=$PWD/reference/.venv-012 /opt/homebrew/bin/uv pip install -r reference/requirements-012.txt
reference/.venv-012/bin/python reference/sparse_spike.py pin --repo opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill --out reference/models/manifest-sparse-doc-v3.json
reference/.venv-012/bin/python reference/sparse_spike.py pin --repo opensearch-project/opensearch-neural-sparse-encoding-doc-v2-distill --out reference/models/manifest-sparse-doc-v2.json
scripts/fetch-model.sh --manifest reference/models/manifest-sparse-doc-v3.json      # → reference/models/<local_dir>, verified
scripts/fetch-model.sh --manifest reference/models/manifest-sparse-doc-v2.json
```

Expected: two manifests with revision hashes and four verified files each (~270 MB per
model, git-ignored under `reference/models/`).

## Step 1 — The oracle agrees with the engine (SC-001)

```bash
for d in scifact nfcorpus fiqa; do reference/.venv-012/bin/python reference/sparse_spike.py export --dataset $d; done
```

Expected: `engine-lexical`, `engine-dense`, `engine-hybrid` runs for each dataset, each
re-scored to its committed baseline to 1e-6 (`specs/003-…`, `specs/004-…`, `specs/005-…
/baselines`); FiQA's dense run comes from the 004 cache (no embedding).

## Step 2 — SciFact smoke, then the rest

```bash
M=reference/models/manifest-sparse-doc-v3.json
reference/.venv-012/bin/python reference/sparse_spike.py encode --manifest $M --dataset scifact       # minutes; prints docs/s
reference/.venv-012/bin/python reference/sparse_spike.py all    --manifest $M --dataset scifact       # every variant, every report
for d in nfcorpus fiqa; do reference/.venv-012/bin/python reference/sparse_spike.py all --manifest $M --dataset $d; done   # FiQA: the long one
for d in scifact nfcorpus fiqa; do reference/.venv-012/bin/python reference/sparse_spike.py all --manifest reference/models/manifest-sparse-doc-v2.json --dataset $d; done
```

Expected: `dot` on SciFact near the card's 0.708 nDCG@10 (the card's number; a gap is a
finding, not a threshold to move); a second `encode` prints "cached" for every shard.

## Step 3 — Summary and decision

```bash
reference/.venv-012/bin/python reference/sparse_spike.py summary
```

Expected: `specs/012-sparse-spike/runs/summary.json` and `costs-*.json`; the metrics table
(every variant × dataset × model), the quantisation table (loss per scale), the costs table,
and the FR-011 verdict printed — pasted into `report.md`.

## Step 4 — The gate (Rule 5, reduced: nothing under `crates/` changes)

```bash
git diff --stat main -- crates/ swift/ apps/ python/ .github/ deny.toml    # empty
cargo nextest run --workspace                                            # unchanged, 263 / 263
```

No eval delta is *claimed*: the spike measures; the engine's baselines are untouched.
