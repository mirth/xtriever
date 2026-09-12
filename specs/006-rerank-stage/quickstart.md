# Quickstart: validating the Re-rank Stage

**Feature**: `006-rerank-stage` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

> **Status**: not yet executed — filled in by `/speckit-implement`; results go to `report.md`.

## Step 0 — Toolchain, models, cache

```bash
./scripts/check-toolchain.sh
ls reference/models/all-MiniLM-L6-v2/model.safetensors        # 004 embedder (scripts/fetch-model.sh)
scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json
#   → reference/models/ms-marco-MiniLM-L-6-v2/{config.json,tokenizer.json,model.safetensors}, verified
ls target/xt-dense-cache/{scifact,nfcorpus,fiqa}/index.bin    # 004 cache; Step 6 needs it (0 documents embedded)
ls reference/.venv-004/bin/python                             # the 004 oracle venv (scripts/setup-reference-venv.sh 004)
```

## Step 1 — Goldens

```bash
reference/.venv-004/bin/python reference/gen_006_fixtures.py --out reference/fixtures/006/
```

Produces `rerank.json` (~10 queries × 4–6 passages with reference logits, ids, type ids and
order; the over-length, empty-passage, empty-query, both-empty and near-tie cases; refuses any
query whose smallest gap is below 0.01), `pipeline_order.json` (10 ordering cases),
`manifest.json`. Prints the model identity string; it must equal `xtriever_rerank::model::MODEL_ID`.

## Step 2 — Red checkpoint (Rule 4)

```bash
cargo nextest run -p xtriever-rerank                           # offline: fixtures_valid, model_pins green; budget failing on NotImplemented
cargo nextest run -p xtriever-pipeline                         # 005 suites green; rerank/passages suites failing
cargo nextest run -p xtriever-eval                             # rerank_run failing
./scripts/check-no-stubs.sh                                    # FAILs here, PASSes after PR 3
```

## Step 3 — The cross-encoder (after PR 2)

```bash
cargo nextest run -p xtriever-rerank
cargo nextest run -p xtriever-rerank --run-ignored only        # model-backed: load, goldens, determinism, time budget
cargo nextest run -p xtriever-rerank --features mmap --run-ignored only   # + load_paths: buffered vs mapped bit-identical
RAYON_NUM_THREADS=1 cargo nextest run -p xtriever-rerank --run-ignored only -E 'test(score_determinism)'
```

Expected: every golden pair within 1e-3 and every per-query order exact, including the four
edge cases (SC-001); tokenization parity on every pair; 0 differing bits across three
arrangements and two thread counts (SC-002); item limits `{0, 1, half, all}` exact and the
100 ms limit over 40 max-length passages scores ≥ 1 and < 40 (SC-003); a tampered file is
refused naming the file and both values; mapped scores equal buffered scores to the bit
(ADR-0009 condition 3).

## Step 4 — The pipeline with stub re-rankers (after PR 3)

```bash
cargo nextest run -p xtriever-pipeline
cargo nextest run -p xtriever-pipeline --features mmap
cargo nextest run -p xtriever-pipeline --run-ignored only      # real embedder + real cross-encoder round trip
```

Expected: every `pipeline_order.json` case exact and the ordering property holds (SC-004);
depth 0 / no re-ranker responses equal 005's (SC-004); failing stub ⇒ fused order + `skipped`
in the default mode, the error in strict; wrong length ⇒ `Model` in both (SC-005);
explanations carry `rerank.score` / `rerank.rank` exactly where scored and never change hits
(SC-006); a 005 (version 1) directory is refused naming `1` and `2`; a torn `passages.bin` is
refused naming the counts; the captured budget equals `limit − elapsed`.

## Step 5 — Lexical smoke still green

```bash
cargo run --release -p xtriever-eval --example beir -- smoke --dataset scifact --baseline specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json
```

## Step 6 — The re-ranked baseline and the delta (after PR 4)

```bash
export RAYON_NUM_THREADS=4
for d in scifact nfcorpus fiqa; do
  /usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-rerank-v1 \
    --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index/$d \
    --out specs/006-rerank-stage/baselines/hybrid-rerank-v1.$d.json \
    --export-run target/xt-rerank-run.$d.jsonl --export-explain target/xt-rerank-explain.$d.jsonl
  reference/.venv-003/bin/python reference/gen_003_fixtures.py --verify-run target/xt-rerank-run.$d.jsonl \
    --qrels reference/datasets/beir/$d/qrels/test.tsv --report specs/006-rerank-stage/baselines/hybrid-rerank-v1.$d.json
  reference/.venv-004/bin/python reference/gen_006_fixtures.py --verify-rerank target/xt-rerank-explain.$d.jsonl
  cargo run --release -p xtriever-eval --example beir -- compare \
    specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.$d.json specs/006-rerank-stage/baselines/hybrid-rerank-v1.$d.json
done
for p in buffered mmap; do for i in 1 2 3; do
  /usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- model-memory --model rerank --load-path $p
done; done
```

Expected: `embedded 0 documents`; three reports with `stage.kind = "hybrid-rerank"`, the model
id and depth 20; `--verify-run` within 1e-6 (SC-007); `--verify-rerank` agrees on every query;
Recall@100 byte-equal to `hybrid-baseline-v1` per dataset; **nDCG@10 not below the fused
baseline on ≥ 2 of 3** (SC-008 — a miss is ⛔ stop-and-report); a second run of one dataset
byte-identical; FiQA observations recorded (SC-010): per-query and per-pair re-rank time,
`rerank_pairs`, peak RSS, directory bytes including `passages.bin`, and the fresh-process load
peak per load path (median of three each; ADR-0009 condition 5 — recorded, no deletion clause).

## Step 7 — Full gate (Rule 5)

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
RUSTFLAGS="-D warnings" cargo clippy -p xtriever-pipeline --features mmap --all-targets
RUSTFLAGS="-D warnings" cargo clippy -p xtriever-rerank --features mmap --all-targets
cargo nextest run --workspace
cargo nextest run -p xtriever-pipeline --features mmap
cargo nextest run -p xtriever-rerank --features mmap
cargo nextest run -p xtriever-rerank --run-ignored only
cargo nextest run -p xtriever-rerank --features mmap --run-ignored only
cargo nextest run -p xtriever-pipeline --run-ignored only
cargo deny check
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo check --workspace --target wasm32-unknown-unknown   # best-effort; fails at getrandom via candle — tracked
./scripts/check-no-stubs.sh
grep -rn 'unsafe' crates/xtriever-rerank/src/ ; echo "(expect exactly bytes.rs: the allow attribute and the one block — ADR-0009)"
grep -rn 'unsafe' crates/xtriever-pipeline/src/ ; echo "(expect nothing)"
grep -rn 'Instant\|SystemTime\|std::thread' crates/xtriever-pipeline/src/ ; echo "(expect nothing — Principle III)"
cargo tree -p xtriever-eval -e normal | grep -E 'candle|tantivy|memmap2|xtriever-pipeline|xtriever-rerank' ; echo "(expect nothing)"
cargo tree -p xtriever-pipeline -e normal | grep xtriever-rerank ; echo "(expect nothing — pipeline takes Box<dyn Reranker>)"
git diff --stat main -- crates/xtriever-core crates/xtriever-lexical crates/xtriever-dense deny.toml crates/xtriever-eval/src/metrics.rs crates/xtriever-eval/src/dataset.rs   # expect empty
```

## Step 8 — Report

`specs/006-rerank-stage/report.md`: three re-ranked baselines with `--verify-run` and
`--verify-rerank`, the three `compare` tables against `hybrid-baseline-v1`, the SC-008 verdict,
observations with method (FiQA), the model-memory numbers per load path, the FR-022 diff,
findings.
