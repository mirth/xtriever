# Quickstart: validating the Dense Stage

**Feature**: `004-dense-stage` | **Date**: 2026-09-12 | **Plan**: [plan.md](./plan.md)

> **Status**: **executed 2026-09-12** — every step below ran; results in [report.md](./report.md).
> Measured: FiQA embeds in **5,433 s** (94 ms/passage, 4 threads; 8 threads was slower); model
> memory from cold buffered 226.3 MB vs mapped 197.4 MB (−13 %, ADR-0007 condition 5: the mmap
> weight path stays); thread-count and load-path bit-identity both hold. Step 8's containment
> greps: one `unsafe {}` in `bytes.rs`, one item-scoped allow.

## Step 0 — Toolchain provenance

```bash
./scripts/check-toolchain.sh
```

## Step 1 — The model (87 MiB, git-ignored, pinned)

```bash
./scripts/fetch-model.sh                      # → reference/models/all-MiniLM-L6-v2/ ; verifies 3 files
export XTRIEVER_MODEL_DIR=$PWD/reference/models/all-MiniLM-L6-v2   # optional; this is the default
```

Expected: three `verified` lines with sizes 612 / 466,247 / 90,868,376 and the hashes in research
D4. Corrupt one byte of a copy and re-run against it to see the size-or-hash failure name the file
and both values (FR-003).

## Step 2 — Python reference environment (embedding + search oracles)

```bash
./scripts/setup-reference-venv.sh 004         # reference/.venv-004: 001's exact torch/transformers pins + numpy
reference/.venv-004/bin/python reference/gen_004_fixtures.py --seed 4 --out reference/fixtures/004/
```

Produces `embeddings.json` (≥ 12 cases incl. `long_over_256`, `empty`, `oov_unicode`),
`search.json`, `mutations.json`, `manifest.json`. The generator refuses to emit a search case with
an undesigned near-tie (research D7); if it rerolls, it says so.

## Step 3 — Red checkpoint (Rule 4)

```bash
cargo nextest run -p xtriever-dense                      # offline suite: no model needed
cargo nextest run -p xtriever-dense --run-ignored only   # model-backed suite: needs Step 1
```

Expected after PR 1: `fixtures_valid` and `model_pins` green; every other test failing on
`NotImplemented` — a runtime failure, not a compile error (`./scripts/check-no-stubs.sh` must
FAIL at this point and PASS after PR 3).

## Step 4 — The embedder (after PR 2)

```bash
RAYON_NUM_THREADS=1 cargo nextest run -p xtriever-dense --run-ignored only
cargo nextest run -p xtriever-dense --features mmap --run-ignored only    # adds the load-path parity test
```

Expected: every golden within cosine ≥ 0.9999 / max-abs ≤ 1e-3 (SC-001); three batch arrangements
bit-identical (SC-002); the cross-process test with `RAYON_NUM_THREADS=1` and `4` bit-identical
(research D3 — if this fails, **stop and report**, Rule 6); buffered vs mapped bit-identical
(ADR-0007 condition 3); two loads report the same fingerprint (SC-010).

Model memory, per path, each in its own process (FR-023, research D12):

```bash
/usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- model-memory --load-path buffered 2>&1 | grep 'maximum resident'
/usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- model-memory --load-path mmap 2>&1 | grep 'maximum resident'
# (the example's dev-dependency on xtriever-dense enables `mmap`, so no feature flag is needed here)
```

Record both; the expected difference is roughly the 90.9 MB transient (D1), **not** 001's 39.5×.
ADR-0007 condition 5 decides the weights-mmap path's fate from these two numbers.

## Step 5 — The vector index (after PR 3)

```bash
cargo nextest run -p xtriever-dense                       # goldens, mutations, persistence, errors, properties
cargo nextest run -p xtriever-dense --features mmap       # same suite through open_mapped (parity)
```

Expected: every search golden exact in ids and order, scores within 1e-6, including every
designed tie at the `k`-th rank (SC-003); reopen bit-identical (SC-004); the three named errors
(SC-005); stale-handle test (a second handle does not see the first's commit until reopened).

## Step 6 — Datasets and the dense baseline (after PR 4)

```bash
./scripts/fetch-beir.sh scifact nfcorpus fiqa
export RAYON_NUM_THREADS=4                                # recorded in stage.thread_count; any value is valid
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config dense-baseline-v1 \
    --cache-dir target/xt-dense-cache \
    --out specs/004-dense-stage/baselines/dense-baseline-v1.$d.json --export-run target/dense-run-$d.jsonl
  reference/.venv-003/bin/python reference/gen_003_fixtures.py --verify-run target/dense-run-$d.jsonl \
    --qrels reference/datasets/beir/$d/qrels/test.tsv --report specs/004-dense-stage/baselines/dense-baseline-v1.$d.json
done
```

The first FiQA run embeds 57,638 passages — expect on the order of an hour (research D2; the
example prints progress every 1,000 and the total to stderr). Then:

```bash
# SC-006: warm cache ⇒ 0 embedded, byte-identical report
cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config dense-baseline-v1 \
  --cache-dir target/xt-dense-cache --out /tmp/scifact-again.json 2>&1 | grep 'embedded 0'
diff specs/004-dense-stage/baselines/dense-baseline-v1.scifact.json /tmp/scifact-again.json && echo identical

# FR-021: a delta against the lexical baseline is refused
cargo run -p xtriever-eval --example beir -- delta specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json \
  specs/004-dense-stage/baselines/dense-baseline-v1.scifact.json; echo "exit=$? (expect non-zero: different configurations)"
```

FiQA observations (FR-023), measured outside the process and copied into the report by hand:

```bash
rm -rf target/xt-dense-cache/fiqa      # so the timed run embeds, not just opens
/usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- run --dataset fiqa --config dense-baseline-v1 \
  --cache-dir target/xt-dense-cache --out /tmp/fiqa.json 2>&1 | grep -E 'embedded|searched|maximum resident'
stat -f %z target/xt-dense-cache/fiqa/index.bin
```

Spot-check the real corpus embeddings against torch (D14):

```bash
cargo run --release -p xtriever-eval --example beir -- export-vectors --cache-dir target/xt-dense-cache --dataset scifact --sample 50 --out /tmp/sample.jsonl
reference/.venv-004/bin/python reference/gen_004_fixtures.py --verify-embed /tmp/sample.jsonl
```

## Step 7 — Lexical smoke still green

```bash
cargo run --release -p xtriever-eval --example beir -- smoke --dataset scifact --baseline specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json
```

Unchanged numbers: this feature touches no lexical code. CI's `eval-smoke` runs because
`crates/xtriever-dense/**` is in its path filter.

## Step 8 — Full gate (Rule 5)

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
RUSTFLAGS="-D warnings" cargo clippy -p xtriever-dense --features mmap --all-targets
cargo nextest run --workspace
cargo nextest run -p xtriever-dense --features mmap
cargo deny check
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo check -p xtriever-dense --features mmap --target aarch64-apple-ios
cargo check --workspace --target wasm32-unknown-unknown   # best-effort; still fails at errno (tracked)
./scripts/check-no-stubs.sh
# containment: exactly one hand-written unsafe block, in bytes.rs, only under the feature
grep -rn 'unsafe' crates/xtriever-dense/src/ | grep -v '^.*//' ; grep -c 'unsafe {' crates/xtriever-dense/src/bytes.rs   # expect 1
grep -rn 'allow(unsafe_code)' crates/xtriever-dense/src/                                                              # expect bytes.rs only
# purity of the eval library graph (003 Step 8, unchanged)
cargo tree -p xtriever-eval -e normal | grep -E 'candle|tantivy|memmap2' ; echo "(expect nothing)"
git diff --stat main -- crates/xtriever-core crates/xtriever-lexical deny.toml crates/xtriever-eval/src/metrics.rs crates/xtriever-eval/src/dataset.rs   # expect empty
```

## Step 9 — Report

`specs/004-dense-stage/report.md`: three baselines (absolute; no band), `--verify-run` agreement,
SC-002/SC-010 bit-identity evidence, the thread-count result, the two model-memory numbers and
the ADR-0007 condition 5 verdict, FiQA index size / peak RSS / embed and search wall times with
method, the 100k extrapolation labelled as such, and the FR-025 "core untouched" diff.
