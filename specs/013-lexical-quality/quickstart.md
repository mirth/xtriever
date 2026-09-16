# Quickstart: validating Lexical Quality — One Field for BM25

**Feature**: `013-lexical-quality` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## Step 1 — Red (Rule 4)

```bash
cargo nextest run -p xtriever-eval -E 'test(v2)'            # the new tests: do not compile (no Source::TitleAndText, no *_v2)
```

## Step 2 — Green, then the baselines (Rule 5; local; all three datasets)

```bash
cargo nextest run -p xtriever-eval
export RAYON_NUM_THREADS=4
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config lexical-baseline-v2 \
    --out specs/013-lexical-quality/baselines/lexical-baseline-v2.$d.json --export-run target/xt-lex2-run.$d.jsonl
  reference/.venv-003/bin/python reference/gen_003_fixtures.py --verify-run target/xt-lex2-run.$d.jsonl \
    --qrels reference/datasets/beir/$d/qrels/test.tsv --report specs/013-lexical-quality/baselines/lexical-baseline-v2.$d.json
  cargo run --release -p xtriever-eval --example beir -- compare specs/003-eval-harness/baselines/lexical-baseline-v1.$d.json specs/013-lexical-quality/baselines/lexical-baseline-v2.$d.json
done
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-baseline-v2 --cache-dir target/xt-dense-cache \
    --index-dir target/xt-rerank-index-v2/$d --out specs/013-lexical-quality/baselines/hybrid-baseline-v2.$d.json --export-run target/xt-hyb2-run.$d.jsonl
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-rerank-v2 --cache-dir target/xt-dense-cache \
    --index-dir target/xt-rerank-index-v2/$d --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 \
    --out specs/013-lexical-quality/baselines/hybrid-rerank-v2.$d.json --export-run target/xt-rr2-run.$d.jsonl
  # verify-run and compare for both, as above, against specs/005-…/hybrid-baseline-v1.$d.json and specs/006-…/hybrid-rerank-v1.$d.json
done
```

Expected: SC-001 (lexical: SciFact ≥ +0.05, NFCorpus ≥ +0.008, FiQA ±0.001), SC-002 (fused
means not below v1), every verify-run PASS. **⛔** A lexical gain below the floor, or a fused
mean below v1, is stop-and-report.

## Step 3 — v1 untouched (SC-004)

```bash
cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config lexical-baseline-v1 --out /tmp/lex1.json
diff <(jq .metrics /tmp/lex1.json) <(jq .metrics specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json)   # empty
git diff --stat main -- specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage   # empty
```

## Step 4 — Gate

fmt · clippy (host, Windows target) · nextest workspace · deny · cross-target checks ·
no-stubs · `git diff --stat main -- crates/ ':!crates/xtriever-eval' ':!crates/xtriever-cli/src/wiki/chunking.rs'` empty and the `chunking.rs` diff `///` lines only
(`git diff main -- crates/xtriever-cli/src/wiki/chunking.rs | grep '^[-+]' | grep -v '^[-+]\{3\}' | grep -v '^[-+] *///'` empty) · the CI smoke
green against the v2 baseline on push.
