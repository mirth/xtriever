# Quickstart: validating the Evaluation Harness

**Feature**: `003-eval-harness` | **Date**: 2026-09-12 | **Plan**: [plan.md](./plan.md)

> **Status**: plan-time. The Constitution Check passes on all 14 rows with no ADR needed. Nothing
> below has been executed except the Phase 0 measurements recorded in research.md.

## Step 0 — Toolchain provenance

```bash
./scripts/check-toolchain.sh
```

## Step 1 — Python reference environment (metrics oracle only)

```bash
./scripts/setup-reference-venv.sh 003        # reference/.venv-003: pytrec_eval 0.5 + numpy; no torch
source reference/.venv-003/bin/activate
```

## Step 2 — Metric goldens (offline)

```bash
python3 reference/gen_003_fixtures.py --seed 3 --out reference/fixtures/003/
```

Eleven synthetic cases (data-model `MetricGoldens`), each with the reference's per-query and mean
values. The generator asserts its own probe of the conventions in research D3 before writing —
if `pytrec_eval` ever changes behaviour, the generator refuses rather than emitting new goldens.

## Step 3 — Red checkpoint (Rule 4)

```bash
cargo nextest run -p xtriever-eval
```

Expected after PR 1: `fixtures_valid` green, everything else failing on `not implemented`.

## Step 4 — Datasets

```bash
./scripts/fetch-beir.sh                      # all three; ~23 MB; idempotent; verifies every hash
./scripts/fetch-beir.sh scifact              # just one
cargo run -p xtriever-eval --example beir -- verify scifact nfcorpus fiqa
```

Expected counts (research D1): 5,183 / 300 · 3,633 / 323 · 57,638 / 648. Tamper check:

```bash
printf 'x' >> reference/datasets/beir/scifact/qrels/test.tsv
cargo run -p xtriever-eval --example beir -- verify scifact    # must fail naming the file and both hashes
./scripts/fetch-beir.sh scifact                                # restores by re-extracting
```

## Step 5 — Offline suite green (after PR 2)

```bash
cargo nextest run -p xtriever-eval                              # no network, no cache needed
```

## Step 6 — The baseline (after PR 3)

```bash
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config lexical-baseline-v1 \
    --out specs/003-eval-harness/baselines/lexical-baseline-v1.$d.json --export-run target/run-$d.jsonl
  python3 reference/gen_003_fixtures.py --verify-run target/run-$d.jsonl \
    --qrels reference/datasets/beir/$d/qrels/test.tsv --report specs/003-eval-harness/baselines/lexical-baseline-v1.$d.json
done
```

`--verify-run` must report agreement with the Rust means within 1e-6 (SC-001 at scale). Then the
FR-020 band: each `mean_ndcg_10` within ±0.10 of 0.665 / 0.325 / 0.236 — a miss is a finding for
`report.md`, not a widened band. Run SciFact twice and `diff` the two reports (SC-004).

FiQA observations (FR-018), measured outside the process and copied into the report by hand:

```bash
# --index-dir keeps the index on disk after the run (the default is a temp dir deleted on exit),
# so the `du` below measures THIS run's index, not a stale one.
/usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- run --dataset fiqa --config lexical-baseline-v1 \
  --out /tmp/fiqa.json --index-dir target/xt-eval-index/fiqa 2>&1 | grep 'maximum resident'
du -sk target/xt-eval-index/fiqa                                 # bytes = KiB × 1024
```

Timing for SC-008: `time` around the SciFact run; record the number.

## Step 7 — Delta and smoke (after PR 3)

```bash
cargo run -p xtriever-eval --example beir -- delta specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json /tmp/scifact-again.json
cargo run -p xtriever-eval --example beir -- smoke --dataset scifact --baseline specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json
# SC-006: a degraded copy must fail
jq '.mean_ndcg_10 += 0.01' specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json > /tmp/higher.json
cargo run -p xtriever-eval --example beir -- smoke --dataset scifact --baseline /tmp/higher.json; echo "exit=$? (expect 2)"
```

## Step 8 — Full gate (Rule 5)

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
cargo nextest run --workspace
cargo deny check
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo tree -p xtriever-eval -e normal --prefix none | grep -Ei '(-sys|^cc |onig|zstd|tantivy)' && echo "IMPURE" || echo "pure (library graph)"
git diff --stat main -- crates/xtriever-core crates/xtriever-lexical deny.toml   # must be empty
```

## Step 9 — CI (after PR 4)

Push a branch touching `crates/xtriever-lexical/` and confirm the `eval-smoke` job runs, hits the
cache on the second run, and passes; touch only `docs/` and confirm it does not run (Story 4
scenario 4).

## PR description contents

- nextest summary; the baseline table (3 datasets × 2 metrics, full precision and BEIR-rounded);
  the FR-020 band verdicts; the FiQA observations with method; SciFact wall time.
- The line **"eval delta: this feature establishes the baseline (ADR-0006 condition 2 discharged)"**.
