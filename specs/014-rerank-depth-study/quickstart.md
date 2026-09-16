# Quickstart: validating the Re-rank Depth Study

**Feature**: `014-rerank-depth-study` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## Step 1 — Red (Rule 4)

```bash
reference/.venv-003/bin/python -m pytest reference/tests_014 -q      # ImportError: no module rerank_study
```

## Step 2 — The end-to-end runs (≈ 2 h; RAYON_NUM_THREADS=4)

```bash
export RAYON_NUM_THREADS=4; S=target/xt-rerank-study; mkdir -p $S/{scifact,nfcorpus,fiqa} specs/014-rerank-depth-study/runs
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-rerank-v2 --rerank-depth 50 \
    --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/$d --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 \
    --out specs/014-rerank-depth-study/runs/e2e-d50.$d.json --export-run $S/$d/e2e-d50.jsonl --export-explain $S/$d/explain-d50.jsonl
  reference/.venv-003/bin/python reference/gen_003_fixtures.py --verify-run $S/$d/e2e-d50.jsonl --qrels reference/datasets/beir/$d/qrels/test.tsv --report specs/014-rerank-depth-study/runs/e2e-d50.$d.json
done
cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config hybrid-rerank-v2 --rerank-depth 5 \
  --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/scifact --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 \
  --out specs/014-rerank-depth-study/runs/e2e-d5.scifact.json --export-run $S/scifact/e2e-d5.jsonl
```

Expected: `config: "hybrid-rerank-v2@d50"` / `"@d5"`; every `verify-run: PASS`; each run's
`stages.rerank.scored` = 50 (or the candidate count) per query.

## Step 3 — Derive, score, check, table, decide (seconds)

```bash
for d in scifact nfcorpus fiqa; do
  reference/.venv-003/bin/python reference/rerank_study.py all --dataset $d --explain $S/$d/explain-d50.jsonl \
    --baseline-run target/xt-rr2-run.$d.jsonl --baseline-report specs/013-lexical-quality/baselines/hybrid-rerank-v2.$d.json \
    $( [ $d = scifact ] && echo "--e2e-run $S/scifact/e2e-d5.jsonl --e2e-depth 5" )
done
reference/.venv-003/bin/python reference/rerank_study.py table && reference/.venv-003/bin/python reference/rerank_study.py decide
```

Expected: `check` PASS on every dataset (derived `replace-d20` = the 013 run and report;
`replace-d5` = the end-to-end SciFact run; `replace-d50` = the `hits` of its own source);
Recall@100 identical to depth 0 in every cell; the table and the decision printed and written
under `runs/`. **⛔** Any `check` mismatch: stop, report, and (if the per-pair assumption is
what failed) run depths 5 / 10 / 20 end to end instead of deriving.

## Step 4 — Gate (Rule 5)

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo nextest run --workspace && cargo deny check
cargo check --workspace --target aarch64-apple-ios && cargo check --workspace --target aarch64-apple-ios-sim && cargo check --workspace --target aarch64-linux-android
reference/.venv-003/bin/python -m pytest reference/tests_014 -q
git diff --stat main -- crates/ ':!crates/xtriever-eval' specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage specs/013-lexical-quality/baselines   # empty
cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config hybrid-rerank-v2 --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/scifact --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 --out /tmp/rr2.json
diff <(jq '.mean_ndcg_10,.mean_recall_100,.config' /tmp/rr2.json) <(jq '.mean_ndcg_10,.mean_recall_100,.config' specs/013-lexical-quality/baselines/hybrid-rerank-v2.scifact.json)   # empty: no flag → unchanged
```
