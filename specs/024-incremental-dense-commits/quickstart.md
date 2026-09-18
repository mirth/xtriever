# Quickstart: Incremental Dense Commits

From the repository root, `unset SDKROOT`.

## Step 0 — mint the oracle from version 1 (before any format change)

```bash
cargo test -p xtriever-dense --test index_oracle -- --ignored mint     # writes tests/support/v1_oracle.json (scripted sequences → results, bit-exact)
```

(The minting test is written first and run once on the unchanged crate; the file is
committed with the red tests — Rule 4.)

## Step 1 — red (PR A)

```bash
cargo nextest run -p xtriever-dense 2>&1 | tail -20
```

Expected: `index_append`, `index_compact`, `index_crash`, `index_oracle` fail to compile
against the old API (`compact`, `stats`, `set_compaction_threshold`) or fail on the file
names (`manifest.bin`, `vectors.0.bin`); `index_errors`'s version test fails on the message.

## Step 2 — green (PR A)

```bash
cargo nextest run -p xtriever-dense                                     # all green, including index_prop (1,000 random sequences)
cargo test -p xtriever-dense --features mmap --test index_persist       # the mapped paths
cargo bench -p xtriever-dense --bench scan -- --save-baseline v2        # scan v1-shaped vs v2; the 10-row commit
cargo run --release -p xtriever-ffi --example fixture_index -- swift/Xtriever/Tests/Fixtures
git diff --stat swift/Xtriever/Tests/Fixtures/expected.json             # empty: the goldens reproduce bit for bit
cargo nextest run --workspace                                            # the FFI / pipeline suites over the regenerated fixture
```

Expected: scan within 5 % of the version-1-shaped scan; the 10-row commit under 100 KB and
50 ms (vs ~150 MB for the rewrite); `expected.json` unchanged.

## Step 3 — the pipeline and the knob (PR B)

```bash
cargo nextest run -p xtriever-pipeline -p xtriever-ffi                  # merge → compact; the threshold; the descriptor round-trip
(cd python && .venv/bin/maturin build --release) && uv pip install --python python/.venv/bin/python --force-reinstall target/wheels/xtriever-*.whl && python/.venv/bin/pytest python/tests -q
```

## Step 4 — the Wikipedia artefact

```bash
# convert (throwaway, not shipped) or rebuild; then the proofs:
apps/python-wiki-demo/.venv/bin/wikidemo measure --out specs/024-incremental-dense-commits/runs/measure-<machine>-<stamp>.json   # parity PASS, 800/800 bits; peak RSS recorded
cargo run --release -p xtriever-eval -- --dataset scifact …                                                                       # hybrid-baseline-v2.scifact reproduced to 1e-6 (the exact command from specs/013's quickstart)
```

## Step 5 — crash safety, by hand

```bash
cargo nextest run -p xtriever-dense --test index_crash                  # every truncation point of a commit and a compact reopens to the previous state
```

## Step 6 — gate

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo nextest run --workspace && cargo deny check
cargo check --workspace --target aarch64-apple-ios && cargo check --workspace --target aarch64-apple-ios-sim && cargo check --workspace --target aarch64-linux-android
git diff --stat main -- crates/xtriever-core deny.toml apps/ specs/*/baselines            # empty
grep -rn "$(hostname -s)\|$USER" specs/024-incremental-dense-commits docs/adr/0013-*.md   # nothing
```
