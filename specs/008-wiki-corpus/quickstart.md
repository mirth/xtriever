# Quickstart: validating the Wikipedia Corpus and Shipped Index

**Feature**: `008-wiki-corpus` | **Date**: 2026-09-13 | **Plan**: [plan.md](./plan.md)

## Step 0 — Snapshot, models, tooling

```bash
scripts/setup-reference-venv.sh 008                            # pyarrow + tokenizers 0.23.2, pinned
scripts/fetch-wiki.sh                                          # 157 MB parquet → verified → simple.jsonl → verified (reference/datasets/wiki/, gitignored)
scripts/fetch-model.sh && scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json
```

Expected: both hashes in `reference/datasets/wiki-manifest.json` verified; a second run
downloads nothing.

## Step 1 — Red checkpoint (Rule 4)

```bash
reference/.venv-008/bin/python reference/gen_008_fixtures.py   # chunker goldens (sets A and B) → reference/fixtures/008/
cargo nextest run -p xtriever-analysis                          # chunker goldens + properties: red
cargo nextest run -p xtriever-lexical -E 'test(read_only)'      # read-only open: red
cargo nextest run -p xtriever-pipeline -E 'test(open_with) | test(merge) | test(sidecar)'   # red
cargo nextest run -p xtriever-dense --release --run-ignored only -E 'test(token_count)'     # red
cargo nextest run -p xtriever-cli                               # manifest/exclusion/cache/url unit tests: red
```

## Step 2 — Chunker and read-only open (after PR 1–2)

```bash
cargo nextest run -p xtriever-analysis -p xtriever-lexical -p xtriever-pipeline
cargo nextest run -p xtriever-dense --release --run-ignored only -E 'test(token_count)'
cargo nextest run -p xtriever-ffi --release --run-ignored only -E 'test(readonly)'
```

Expected: every golden byte-identical (text, ranges, cost) for sets A and B; properties hold;
a `chmod 0o555` index opens read-only and searches, mutations say "read-only index"; the FFI
opens a read-only directory.

## Step 3 — A development build (minutes, not hours)

```bash
cargo run --release -p xtriever-cli -- wiki build --manifest reference/datasets/wiki-manifest.json \
  --snapshot-dir reference/datasets/wiki --embedder-dir reference/models/all-MiniLM-L6-v2 \
  --out target/xt-wiki-dev --cache-dir target/xt-wiki-cache --limit 2000
cargo run --release -p xtriever-cli -- wiki verify --index target/xt-wiki-dev/index \
  --embedder-dir reference/models/all-MiniLM-L6-v2 --snapshot-dir reference/datasets/wiki
```

Expected: ~4–5k passages, `passages_over_window: 0`, `url_mismatches: 0`, a second `build`
reports every shard as a cache hit and a byte-identical `corpus.json`; interrupting the first
build mid-embedding (Ctrl-C) leaves no `target/xt-wiki-dev/`, and the re-run skips the
embedded shards.

## Step 4 — The full build (one unattended run, resumable; ~12 h estimated)

```bash
RAYON_NUM_THREADS=4 cargo run --release -p xtriever-cli -- wiki build … --out target/xt-wiki --cache-dir target/xt-wiki-cache 2>&1 | tee target/xt-wiki-build.log   # 4 threads: 004 F-005 — bit-identical at any count, fastest at ~4
cp target/xt-wiki/wiki-build.json specs/008-wiki-corpus/build-record.json
cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki/index --embedder-dir … --reranker-dir reference/models/ms-marco-MiniLM-L-6-v2 \
  --queries reference/fixtures/008/queries.json --out target/xt-wiki/expected.json
```

Expected: `wiki-build.json` with all phases timed, `verify.verdict: PASS`, artefact sizes;
`expected.json` for 20 queries × 3 depths. Run `build` again afterwards: all shards hit,
identity identical (SC-001).

## Step 5 — Stage and open on the simulator

```bash
scripts/build-ios-package.sh --with-models --with-wiki
cd swift/Xtriever && xcodebuild test -scheme Xtriever -destination 'platform=iOS Simulator,id=…' -configuration Release ARCHS=arm64 \
  -skip-testing:XtrieverTests/DeviceMeasurementTests
TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test … -only-testing:XtrieverTests/WikipediaTests
```

Expected: staged size printed and under the 2.0 GB budget; `WikipediaTests` opens the bundled
index **in place** (no copy), searches a query, and the first hit's text splits into a title and
a passage whose derived URL matches; the 007 suite still passes (minus the removed
`WritableCopyTests`).

## Step 6 — Ranking crates touched → BEIR unchanged (Rule 5)

```bash
cargo run --release -p xtriever-eval --example beir -- run --dataset scifact  --config hybrid-rerank-v1 --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index/scifact  --rerank-model-dir … --out /tmp/scifact.json
cargo run --release -p xtriever-eval --example beir -- run --dataset nfcorpus --config hybrid-rerank-v1 … --out /tmp/nfcorpus.json
cargo run --release -p xtriever-eval --example beir -- run --dataset fiqa     --config hybrid-rerank-v1 … --out /tmp/fiqa.json
diff <(jq .metrics /tmp/scifact.json)  <(jq .metrics specs/006-rerank-stage/baselines/hybrid-rerank-v1.scifact.json)   # empty
```

Expected: nDCG@10 / Recall@100 identical on all three (no scoring code changed; the lexical
crate gained a read-only directory and the pipeline a merge that the eval does not call).

## Step 7 — Device measurement (manual, recorded)

```bash
scripts/build-ios-package.sh --with-models --with-wiki --app
TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj -scheme XtrieverHarnessApp -configuration Release \
  -destination 'platform=iOS,id=XXXXX' -skipMacroValidation -allowProvisioningUpdates ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX \
  -only-testing:XtrieverHarnessAppTests/DeviceMeasurementTests 2>&1 | tee /tmp/wiki-device.log
# TEST_RUNNER_* must be an *environment* variable of xcodebuild, not a trailing NAME=VALUE argument (that is a build setting and never reaches the test)
scripts/extract-device-run.py /tmp/wiki-device.log specs/008-wiki-corpus/runs/
```

Expected: a run record with `corpus: "wikipedia"`, `openedInPlace: true`, `index.bytes`,
footprint verdict against 600 MB, latency at depths 0 / 5 / 20 for 20 queries, parity
(lexical bit-identical 20 / 20, dense / re-rank within 1e-3, matched by id). Over 600 MB is
⛔ stop-and-report. One mapped / 1-thread run and one mapped / default-threads run.

## Step 8 — Full gate (Rule 5)

```bash
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
RUSTFLAGS="-D warnings" cargo clippy -p xtriever-ffi --features cli --all-targets
cargo nextest run --workspace
cargo nextest run -p xtriever-ffi -p xtriever-dense -p xtriever-cli --release --run-ignored only -j 1   # serial: the 007 budget test needs an unloaded CPU (report F-007)
cargo deny check
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo check --workspace --target wasm32-unknown-unknown   # best-effort
./scripts/check-no-stubs.sh && ./scripts/check-containment.sh
git diff --stat main -- crates/xtriever-core deny.toml       # empty (Rule 2)
git diff --stat main -- crates/xtriever-lexical crates/xtriever-dense crates/xtriever-pipeline crates/xtriever-ffi   # only the files the plan names
```

## Step 9 — Report

`specs/008-wiki-corpus/report.md`: build record summary (counts, phase times, sizes, cache),
the measured vs estimated table (D4/D9 estimates against reality), the simulator and device
results, the BEIR deltas (zero), F-001's resolution, the explicit "retrieval quality of this
corpus is unmeasured" statement (FR-013), findings.
