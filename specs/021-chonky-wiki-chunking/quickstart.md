# Quickstart: Chonky Chunking for the Wikipedia Demo Build

From the repository root, `unset SDKROOT`; `PY=apps/python-wiki-demo/.venv/bin`.

## Step 0 — the model and the environment

```bash
scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json      # PASS, six files verified
cd apps/python-wiki-demo && uv pip install --python .venv/bin/python -e ".[test]" && cd ../..   # chonky, transformers, torch
$PY/python -c "import chonky, torch, transformers; print(chonky.__name__, torch.__version__, transformers.__version__)"
```

## Step 1 — red

```bash
$PY/pytest apps/python-wiki-demo/tests/test_chunking.py apps/python-wiki-demo/tests/test_build.py apps/python-wiki-demo/tests/test_record.py apps/python-wiki-demo/tests/test_inputs.py -q
```

Expected: `test_chunking` fails on import (`Splitter`, `Window` do not exist); the build,
record and inputs tests fail on the chunker block / the new input.

## Step 2 — green

```bash
$PY/pytest apps/python-wiki-demo/tests -q            # all green; the chonky-backed ones skip with the reason if the model is absent
```

## Step 3 — the slice

```bash
time $PY/wikidemo build --limit 2000 --out target/xt-wiki-slice-chonky
$PY/wikidemo about --artefact target/xt-wiki-slice-chonky | sed -n '3,8p'
$PY/wikidemo search --artefact target/xt-wiki-slice-chonky --snippet 80 -k 3 "April"
XTRIEVER_CHONKY_MODEL_DIR=/nonexistent $PY/wikidemo build --limit 1 --out target/never; echo "exit $?"   # missing input, 1
cp target/xt-wiki-slice-chonky/wiki-build.json specs/021-chonky-wiki-chunking/runs/slice-chonky-<machine>-<stamp>.json
```

Expected: `passages over the embedder window` around 10 %; the split under a minute, the
build under 30 minutes; `about` shows the chonky chunker block; `search` returns April's
passages.

## Step 4 — gate

```bash
git diff --stat main -- crates/ swift/ python/src apps/python-minimal-demo reference/fixtures specs/*/baselines   # empty
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo deny check                                # unchanged
$PY/pytest apps/python-wiki-demo/tests -q; $PY/pytest apps/python-minimal-demo/tests -q                          # green
wc -l apps/python-wiki-demo/wikidemo/chunking.py                                                                 # < 80
grep -rn "$(hostname -s)\|$USER" specs/021-chonky-wiki-chunking apps/python-wiki-demo --exclude-dir=.venv       # nothing
```

## Step 5 — docs

`apps/python-wiki-demo/README.md` (the recipe around chonky; the over-window trade-off with
the numbers; the Rust-build claim removed with the reason); `specs/021-…/report.md`,
`pr-description.md`.
