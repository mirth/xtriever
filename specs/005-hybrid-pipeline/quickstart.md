# Quickstart: validating the Hybrid Pipeline

**Feature**: `005-hybrid-pipeline` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

> **Status**: **executed 2026-09-13** — every step ran; results in [report.md](./report.md).
> SC-011 PASS 3 / 3 (fused nDCG@10 0.690 / 0.345 / 0.369 vs the dense stage's 0.645 / 0.317 /
> 0.369); `--verify-fusion` 1,271 / 1,271 real queries; FiQA ingest from the cache 5.2 s.

## Step 0 — Toolchain and prerequisites

```bash
./scripts/check-toolchain.sh
ls reference/models/all-MiniLM-L6-v2/model.safetensors      # from 004 (scripts/fetch-model.sh)
ls target/xt-dense-cache/{scifact,nfcorpus,fiqa}/index.bin  # 004's embedding cache (Step 5 needs it)
```

## Step 1 — Goldens

```bash
reference/.venv-003/bin/python reference/gen_005_fixtures.py --seed 5 --out reference/fixtures/005/
```

Produces `fusion.json` (10 named RRF cases), `hybrid.json` (synthetic corpus: ~40 documents
incl. chunked ones, 8-d vectors, ~8 queries with filters and the dense oracle), `manifest.json`.

## Step 2 — Red checkpoint (Rule 4)

```bash
cargo nextest run -p xtriever-pipeline                      # offline: no model needed
```

Expected after PR 1: `fixtures_valid` green, everything else failing on `NotImplemented`
(`./scripts/check-no-stubs.sh` FAILs here and PASSes after PR 3).

## Step 3 — Ingest, identity, fusion, filters, explain (after PR 2/3)

```bash
cargo nextest run -p xtriever-pipeline
cargo nextest run -p xtriever-pipeline --features mmap
cargo nextest run -p xtriever-pipeline --run-ignored only    # one model-backed round-trip
```

Expected: every fusion golden exact (SC-001); 1,000-document round-trip (SC-002); reopen
identical (SC-003); filtered = fusion of both stages restricted to the set (SC-004, corrected); failing/slow dense stage degrades in
the default mode and errors in strict (SC-005); explanations reproduce the stage lists and never
change ranking (SC-006); the partial-commit test opens with `Corrupt` naming four counts.

## Step 4 — Lexical smoke still green

```bash
cargo run --release -p xtriever-eval --example beir -- smoke --dataset scifact --baseline specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json
```

## Step 5 — The hybrid baseline (after PR 4)

```bash
export RAYON_NUM_THREADS=4
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-baseline-v1 \
    --cache-dir target/xt-dense-cache --index-dir target/xt-hybrid-index/$d \
    --out specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.$d.json \
    --export-run target/hybrid-run-$d.jsonl --export-explain target/hybrid-explain-$d.jsonl
  reference/.venv-003/bin/python reference/gen_003_fixtures.py --verify-run target/hybrid-run-$d.jsonl \
    --qrels reference/datasets/beir/$d/qrels/test.tsv --report specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.$d.json
  reference/.venv-003/bin/python reference/gen_005_fixtures.py --verify-fusion target/hybrid-explain-$d.jsonl
done
```

Expected on stderr: `embedded 0 documents (004 cache)`, then ingest and per-query timings; each
dataset minutes, not hours (only the 648 FiQA queries are embedded, ~114 ms each).

```bash
# SC-011: fused vs the better stage (dense on all three today) and vs lexical, per dataset
for d in scifact nfcorpus fiqa; do
  cargo run -p xtriever-eval --example beir -- compare specs/004-dense-stage/baselines/dense-baseline-v1.$d.json specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.$d.json
  cargo run -p xtriever-eval --example beir -- compare specs/003-eval-harness/baselines/lexical-baseline-v1.$d.json specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.$d.json
done
# FR-021 (004) still holds: delta refuses mixed configurations
cargo run -p xtriever-eval --example beir -- delta specs/004-dense-stage/baselines/dense-baseline-v1.scifact.json specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.scifact.json; echo "exit=$? (expect 1)"
# SC-007 reproducibility
cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config hybrid-baseline-v1 --cache-dir target/xt-dense-cache --out /tmp/hybrid-scifact-again.json
diff specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.scifact.json /tmp/hybrid-scifact-again.json && echo identical
```

**SC-011 verdict**: fused nDCG@10 ≥ 0.645082 (SciFact), ≥ 0.316673 (NFCorpus), ≥ 0.368671 (FiQA)
on at least two of the three. A miss is a ⛔ stop-and-report.

Observations (SC-010), copied into the FiQA report's `observations` by hand as in 003/004:

```bash
/usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- run --dataset fiqa --config hybrid-baseline-v1 \
  --cache-dir target/xt-dense-cache --index-dir target/xt-hybrid-index/fiqa --out /tmp/fiqa-hybrid.json 2>&1 | grep -E 'ingested|searched|maximum resident'
du -sk target/xt-hybrid-index/fiqa; stat -f %z target/xt-hybrid-index/fiqa/ids.json
```

## Step 6 — Full gate (Rule 5)

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
RUSTFLAGS="-D warnings" cargo clippy -p xtriever-pipeline --features mmap --all-targets
cargo nextest run --workspace
cargo nextest run -p xtriever-pipeline --features mmap
cargo nextest run -p xtriever-pipeline --run-ignored only
cargo deny check
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo check --workspace --target wasm32-unknown-unknown   # best-effort; fails at getrandom via candle — tracked
./scripts/check-no-stubs.sh
grep -rn 'unsafe' crates/xtriever-pipeline/src/ ; echo "(expect nothing)"
grep -rn 'Instant\|SystemTime\|std::thread' crates/xtriever-pipeline/src/ ; echo "(expect nothing — Principle III)"
cargo tree -p xtriever-eval -e normal | grep -E 'candle|tantivy|memmap2|xtriever-pipeline' ; echo "(expect nothing)"
git diff --stat main -- crates/xtriever-core crates/xtriever-lexical crates/xtriever-dense deny.toml crates/xtriever-eval/src/metrics.rs crates/xtriever-eval/src/dataset.rs   # expect empty
```

## Step 7 — Report

`specs/005-hybrid-pipeline/report.md`: three hybrid baselines with `--verify-run` and
`--verify-fusion`, the six `compare` tables, the SC-011 verdict, observations with method, the
`Filter::Ids` cost measurement, the FR-027 diff, findings.
