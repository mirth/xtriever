# Quickstart: Eight-Bit Precision End to End (Feature 026)

How to prove this feature works, in the order the tasks build it. Commands run from the
repository root. The long pole is the final artefact rebuild, which is hours; everything before
it is minutes.

## Prerequisites

| Input | Where | Producer |
|---|---|---|
| the float models | `reference/models/…` | `scripts/fetch-model.sh` (already present) |
| the eight-bit artefacts | `reference/models/…-q8/` | `scripts/fetch-model.sh --manifest reference/models/manifest-q8.json` |
| the three BEIR datasets | `reference/datasets/beir/` | `scripts/fetch-beir.sh` (already present) |
| the Wikipedia snapshot | `reference/datasets/wiki/` | `scripts/fetch-wiki.sh` (already present) |

The two artefacts were inspected on 2026-09-20 and are in `target/gguf-probe/` if you want to
look at them before the manifest exists.

## Step 1 — red (Rule 4)

The format tests and the artefact-loading tests are written first and committed failing.

```bash
cargo nextest run -p xtriever-dense format_v3     # expect failure: no version 3 yet
cargo nextest run -p xtriever-dense -p xtriever-rerank quantised   # expect failure: no loader
```

## Step 2 — the vectors (PR A)

```bash
cargo nextest run -p xtriever-dense
cargo bench -p xtriever-dense --bench scan          # the changed kernel, recorded
python3 reference/gen_026_fixtures.py               # the 004/005 goldens, recomputed from dense_format3.py
python3 reference/gen_026_fixtures.py --check-oracle  # the oracle, likewise
```

Expected: rows are 396 bytes at dimension 384, a version-1 or version-2 file is refused by name,
and the oracle reproduces every score the Rust stage produces. The benchmark is recorded whatever
it says; this feature claims size and quality, not speed.

## Step 3 — the models (PR B)

```bash
scripts/fetch-model.sh --manifest reference/models/manifest-q8.json
cargo nextest run -p xtriever-dense -p xtriever-rerank
```

Expected: both artefacts load, both fingerprints name the eight-bit file, a corrupted byte is
refused by checksum, and a re-ranker artefact without a classification head is refused by name.

## Step 4 — the quality gate (FR-009, SC-004)

```bash
# The eight-bit artefacts, by their pinned directories (`local_dir` in reference/models/manifest-q8.json
# and manifest-rerank-q8.json). After T023 the manifest-named artefact is the default and the two
# flags are redundant; until then they are what selects the eight-bit models — without them, beir
# evaluates the float weights (Copilot, PR A).
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config dense-baseline-v1 \
      --model-dir reference/models/all-MiniLM-L6-v2-q8 \
      --cache-dir target/xt-dense-cache --out target/026-dense.$d.json
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-rerank-v3 \
      --model-dir reference/models/all-MiniLM-L6-v2-q8 --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2-q8 \
      --cache-dir target/xt-dense-cache --out target/026-rerank.$d.json
done
```

`hybrid-rerank-v3` is the current re-rank rule (Feature 015) and the configuration PR A's
description reports; the same three configurations PR A ran on float weights
(`dense-baseline-v1`, `hybrid-baseline-v2`, `hybrid-rerank-v3`) are what PR B compares.

About two hours, nearly all of it embedding FiQA. Every cache must be rebuilt first, because a
different embedder invalidates every cached vector (the cache key carries the fingerprint). Compare each result with its committed
baseline: **no dataset may fall more than 0.005 below it on nDCG@10 or Recall@100**. A larger
drop stops the feature and is reported; it is never answered by moving the threshold.

## Step 5 — the artefacts (FR-010)

```bash
cargo run --release -p xtriever-cli -- wiki build --out target/xt-wiki-q8      # hours
cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki-q8/index --out target/xt-wiki-q8/expected.json
scripts/build-ios-package.sh --with-models --with-fixtures     # after T027a: stages the eight-bit artefacts
apps/python-wiki-demo/.venv/bin/wikidemo build --limit 2000 --out target/xt-wiki-slice-q8
```

Both packagers (`build-ios-package.sh --with-models`, `build-android-package.sh`) stage the
float directories by name today; T027a teaches them the eight-bit artefacts and the tokenizer
they borrow from the float directory (`tokenizer_from`). Until it lands, a packaged app ships
float weights whatever the index was built with (Copilot, PR A).

Then the demonstrations' own checks: the Python measurement against the new goldens, the Android
module's parity test against the regenerated fixture, and the iOS simulator tests.

## Step 6 — the footprint (SC-005)

```bash
apps/python-wiki-demo/.venv/bin/wikidemo measure --artefact target/xt-wiki-q8
```

Expected: parity passes against the new goldens, and the peak resident size is **below 600 MB**
for the first time — the vectors fall by about 490 MB and the weights by about 124 MB. Record it
under `runs/` beside the previous measurements, which stay for comparison.

## Step 7 — the gate (Rule 5)

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo nextest run --workspace && cargo deny check
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-linux-android
(cd python && .venv/bin/maturin build) && python/.venv/bin/pytest python/tests -q
grep -rn "$(hostname -s)\|$USER" specs/026-eight-bit-precision reference/models   # expect nothing
```

## Step 8 — documents

`docs/adr/0015-eight-bit-vectors-and-models.md` (the format and the contract, written first, not
last), `report.md` with the evaluation table and the footprint, both pull request descriptions,
and the note that arbitrary user-supplied models are the next feature.
