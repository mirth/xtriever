# Contract: `reference/sparse_spike.py`

**Feature**: `012-sparse-spike` | **Date**: 2026-09-15

One script, subcommands, run from the repository root with `reference/.venv-012/bin/python`.
Every subcommand is idempotent over its cache and prints one line per artefact it wrote.

```text
sparse_spike.py pin      --repo <hf repo>  --out reference/models/manifest-sparse-doc-vN.json
                         # resolves the current main commit, downloads the four files to a temp dir,
                         # writes the manifest (bytes, sha256, revision, license, parameters, activation)
sparse_spike.py encode   --manifest <manifest> --dataset {scifact,nfcorpus,fiqa} [--device mps|cpu] [--batch 32]
                         # documents → docs-NNNNN.npz shards (resumable); judged queries → queries.npz;
                         # prints throughput and the costs record fields; refuses a model dir that fails the manifest
sparse_spike.py export   --dataset D
                         # runs the harness (cargo run … beir run … --export-run) for lexical/dense/hybrid into
                         # target/xt-sparse-runs/D/engine-*.jsonl and re-scores them; FAILS unless each equals its
                         # committed baseline to 1e-6 (SC-001)
sparse_spike.py score    --manifest <manifest> --dataset D --variant <name> [--scale 100] [--boost 1.0]
                         # one run + report for a variant of research D5; RRF variants read engine-*.jsonl
sparse_spike.py all      --manifest <manifest> --dataset D
                         # encode (if needed) + every variant; writes the summary rows
sparse_spike.py summary  # collects every report into specs/012-sparse-spike/runs/summary.json and prints the
                         # markdown tables of the report (metrics, quantisation, costs) and the FR-011 verdict
```

Exit codes: 0 success; 1 a refusal (manifest mismatch, an engine run that does not reproduce
its baseline, a query set that does not match the dataset); the failing check named on stderr.

Inputs it never modifies: `reference/datasets/beir/*`, `reference/models/*`, `target/xt-dense-cache`,
`target/xt-rerank-index`. Outputs under `target/xt-sparse-cache`, `target/xt-sparse-runs`, and the
committed `specs/012-sparse-spike/runs/{summary.json, costs-*.json}`.
