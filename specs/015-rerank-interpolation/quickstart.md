# Quickstart: validating Interpolated Re-ranking as the Default

**Feature**: `015-rerank-interpolation` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## Step 1 — Red (Rule 4)

```bash
reference/.venv-012/bin/python reference/gen_006_fixtures.py --out reference/fixtures/006     # writes interpolate_cases; `cases` byte-identical
git diff --stat reference/fixtures/006/pipeline_order.json                                    # only additions
cargo nextest run -p xtriever-pipeline -p xtriever-eval -p xtriever-ffi -E 'test(interpolat) | test(rerank_mode) | test(v3)'   # does not compile
```

## Step 2 — Green

```bash
cargo nextest run -p xtriever-pipeline -p xtriever-eval -p xtriever-ffi
```

## Step 3 — The goldens (007 parity) and the Python suite

```bash
cargo run --release -p xtriever-ffi --example fixture_index -- --out swift/Xtriever/Tests/Fixtures   # regenerates expected.json + the fixture index
git diff --stat swift/Xtriever/Tests/Fixtures/expected.json
reference/.venv-012/bin/python reference/rerank_study.py golden-diff --old <(git show main:swift/Xtriever/Tests/Fixtures/expected.json) --new swift/Xtriever/Tests/Fixtures/expected.json
#   → "without_reranker identical (8/8); with_reranker changed: N; info gains rerank_mode"
(cd python && .venv/bin/maturin build) && uv pip install --force-reinstall target/wheels/xtriever-*.whl && python/.venv/bin/pytest python/tests -q
# Swift, on the simulator (owner; the id only on the command line):
# scripts/build-ios-package.sh && cd swift/Xtriever && xcodebuild test -scheme Xtriever -destination 'platform=iOS Simulator,id=XXXXX' -configuration Release ARCHS=arm64 -skip-testing:XtrieverTests/DeviceMeasurementTests
```

## Step 4 — The baselines (≈ 50 min) and the per-query oracle

```bash
export RAYON_NUM_THREADS=4; mkdir -p specs/015-rerank-interpolation/baselines
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-rerank-v3 --cache-dir target/xt-dense-cache \
    --index-dir target/xt-rerank-index-v2/$d --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 \
    --out specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.$d.json --export-run target/xt-rr3-run.$d.jsonl
  reference/.venv-012/bin/python reference/gen_003_fixtures.py --verify-run target/xt-rr3-run.$d.jsonl --qrels reference/datasets/beir/$d/qrels/test.tsv --report specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.$d.json
  reference/.venv-012/bin/python reference/rerank_study.py check-cell --dataset $d --report specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.$d.json \
    --cell specs/014-rerank-depth-study/runs/lin-0.5-d20.$d.json --run target/xt-rr3-run.$d.jsonl --derived target/xt-rerank-study/$d/lin-0.5-d20.jsonl
done
cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config hybrid-rerank-v2 --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/scifact --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 --out /tmp/rr2.json   # still 0.695430 / 0.955000
```

Expected: v3 = 0.7207 / 0.3622 / 0.3910 nDCG@10, Recall@100 0.955000 / 0.321648 / 0.707111,
every `check-cell: PASS` (per query to 1e-6; list for list where the derived run exists);
`hybrid-rerank-v2` reproduces. **⛔** Any difference from 014's cells is a defect in the
engine's rule, not in the cell.

## Step 5 — Gate (Rule 5)

fmt · clippy workspace · nextest workspace · deny · iOS / iOS-sim / Android checks · wheel +
pytest · Swift suite (owner) · `git diff --stat main -- specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage specs/013-lexical-quality/baselines specs/014-rerank-depth-study/runs` empty · the 006 `cases` array unchanged · no identifiers in the tree.
