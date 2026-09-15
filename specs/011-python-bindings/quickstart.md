# Quickstart: validating the Python Interface and Bindings

**Feature**: `011-python-bindings` | **Date**: 2026-09-15 | **Plan**: [plan.md](./plan.md)

## Step 0 — Prerequisites

The pinned Rust toolchain; the two models (`scripts/fetch-model.sh` and
`scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json`); the fixture index
(`cargo run --release -p xtriever-ffi --example fixture_index -- swift/Xtriever/Tests/Fixtures`);
an **arm64** Python on this host (`/opt/homebrew/bin/python3.12` or `python3.13` — the
`/usr/local/bin/python3` is x86_64 and cannot load the library, research D3); `uv`.

```bash
unset SDKROOT                                                     # a shell older than the Xcode update (010 F-006)
/opt/homebrew/bin/uv venv --python 3.12 python/.venv && VIRTUAL_ENV=$PWD/python/.venv /opt/homebrew/bin/uv pip install maturin pytest
```

## Step 1 — Red checkpoint (Rule 4)

```bash
cargo nextest run -p xtriever-ffi --release --run-ignored only -j 1 -E 'binary(build)'   # tests/build.rs: fails to compile (no builder exports)
python/.venv/bin/maturin build --release -m python/pyproject.toml                            # the wheel builds — the search surface exists
VIRTUAL_ENV=$PWD/python/.venv /opt/homebrew/bin/uv pip install --force-reinstall target/wheels/xtriever-*.whl
python/.venv/bin/pytest python/tests -q                                                      # test_build: AttributeError (no create); test_search/options/threads/overhead: green by construction, stated; surface/errors: green
```

Expected at the red commit: the Rust builder suite does not compile; the Python build tests
fail on the missing `create`/`add`/`commit`; every test of the existing surface passes (the
surface is 007's — the tests exist so the feature cannot change it).

## Step 2 — Green (after the builder exports)

```bash
cargo nextest run -p xtriever-ffi                                              # model-free suite
cargo nextest run -p xtriever-ffi --release --run-ignored only -j 1            # model-backed incl. tests/build.rs
python/.venv/bin/maturin build --release -m python/pyproject.toml && VIRTUAL_ENV=$PWD/python/.venv /opt/homebrew/bin/uv pip install --force-reinstall target/wheels/xtriever-*.whl
python/.venv/bin/pytest python/tests -q                                        # all, models present
python/.venv/bin/pytest python/tests -q -m "not models"                        # the CI subset
```

Expected: 16 / 16 golden pairs bit-identical; the Python-built fixture index equals the
goldens for every query (SC-007); the exception classes one per kind; the GIL test's thread
finishes during a search (SC-005); binding overhead median ≤ 5 % of `elapsed_ms` (SC-004).

## Step 3 — A second interpreter and a clean install (SC-001)

```bash
/opt/homebrew/bin/uv venv --python 3.13 /tmp/xt313 && VIRTUAL_ENV=/tmp/xt313 /opt/homebrew/bin/uv pip install target/wheels/xtriever-*.whl pytest
PATH=/usr/bin:/bin /tmp/xt313/bin/pytest python/tests -q            # no cargo/rustc on PATH: install and run need no toolchain
```

## Step 4 — Swift untouched

```bash
scripts/build-ios-package.sh --with-models --with-fixtures && cd swift/Xtriever && xcodebuild test -scheme Xtriever -destination 'platform=iOS Simulator,id=822F3C90-5124-432B-B84A-75426A04722D' -configuration Release ARCHS=arm64 -skip-testing:XtrieverTests/DeviceMeasurementTests   # 18 / 18
git checkout -- swift/Xtriever/Tests/Fixtures/expected.json          # the regenerated provenance line only (008/010)
```

## Step 5 — CI (Linux, model-free)

Push; the `python` job builds the wheel on `ubuntu-latest`, installs it and runs
`pytest -m "not models"`; its wall time and the wheel's `manylinux` tag go into the report
(SC-006: under 10 minutes; nothing downloaded but crates and `uv`/`maturin`).

## Step 6 — Full gate (Rule 5)

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
cargo nextest run --workspace && cargo deny check
for t in aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android; do cargo check --workspace --target $t; done
scripts/check-no-stubs.sh
git diff --stat main -- crates/xtriever-core deny.toml crates/xtriever-lexical crates/xtriever-dense crates/xtriever-rerank crates/xtriever-pipeline swift/ apps/   # empty
```

No eval delta is due: no ranking crate changes (the FFI adds conversions, no computation).
