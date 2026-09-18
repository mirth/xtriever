# Quickstart: Chonky as an Optional Chunker for the Wikipedia Demo Build

From the repository root, `unset SDKROOT`; `PY=apps/python-wiki-demo/.venv/bin`.

## Step 0 — the environment

```bash
cd apps/python-wiki-demo && uv pip install --python .venv/bin/python -e ".[test,chonky]" && cd ../..   # the extra (already in the venv from 021)
ls target/xt-wiki-slice-rs/index/xtriever-pipeline.json reference/models/chonky_distilbert_base_uncased_1/model.safetensors
```

If the Rust slice is missing: `specs/019-python-wiki-demo/quickstart.md` Step 5 (~20 min).

## Step 1 — red

```bash
$PY/pytest apps/python-wiki-demo/tests -q -x --co -q | tail -3     # collection: test_contract.py / test_chonky.py import errors
$PY/pytest apps/python-wiki-demo/tests/test_chunking.py apps/python-wiki-demo/tests/test_contract.py apps/python-wiki-demo/tests/test_chonky.py apps/python-wiki-demo/tests/test_cli.py apps/python-wiki-demo/tests/test_inputs.py apps/python-wiki-demo/tests/test_record.py apps/python-wiki-demo/tests/test_build.py -q
```

Expected: `test_contract` / `test_chonky` fail on import (no such modules); `test_chunking`
fails on `make_chunker`; `test_cli` on `--chunker` / `needs_for`; `test_record` on the two
blocks; `test_build` on the contract block and the poisoned-import build.

## Step 2 — green

```bash
$PY/pytest apps/python-wiki-demo/tests -q            # all green; `chonky`-marked tests skip with the reason if the extra or model is absent
$PY/pytest apps/python-wiki-demo/tests -q -m "not models"
```

## Step 3 — the default slice and its parity (US1)

```bash
time $PY/wikidemo build --limit 2000 --out target/xt-wiki-slice-py-023                 # ~17 min; chunker: 008 contract; 8,529 passages; 0 over window
$PY/wikidemo measure --artefact target/xt-wiki-slice-py-023 --against target/xt-wiki-slice-rs \
  --out specs/023-optional-chonky-chunker/runs/parity-<machine>-<stamp>.json            # parity: PASS
cp target/xt-wiki-slice-py-023/wiki-build.json specs/023-optional-chonky-chunker/runs/slice-contract-<machine>-<stamp>.json
```

Expected: identity `20949fb44303f59039ae21d3aef74ca20ea8f050277676ed5d7e2623dd966133`,
counts equal, documents equal, order identical at every depth, exit 0.

## Step 4 — the option (US2, US3)

```bash
time $PY/wikidemo build --chunker chonky --limit 200 --out target/xt-wiki-slice-chonky-200  # chunker: chonky; over-window > 0
$PY/wikidemo about --artefact target/xt-wiki-slice-chonky-200 | sed -n '3,8p'
cp target/xt-wiki-slice-chonky-200/wiki-build.json specs/023-optional-chonky-chunker/runs/slice-chonky-200-<machine>-<stamp>.json
XTRIEVER_CHONKY_MODEL_DIR=/nonexistent $PY/wikidemo build --chunker chonky --limit 1 --out target/never; echo "exit $?"   # missing model, 1
$PY/wikidemo build --chunker whole --limit 1 --out target/never; echo "exit $?"                                             # usage error, 2
```

The refusal without the extra is the test `test_chonky_build_without_the_extra_is_refused`
(in-process, `sys.modules["chonky"] = None`); to see it for real:
`uv venv /tmp/no-extra --python 3.12 && uv pip install --python /tmp/no-extra/bin/python target/wheels/xtriever-*.whl -e apps/python-wiki-demo && /tmp/no-extra/bin/wikidemo build --chunker chonky --limit 1 --out target/never` → exit 1 with the install command.

## Step 5 — gate

```bash
git diff --stat main -- crates/ swift/ python/src apps/python-minimal-demo reference/ specs/*/baselines   # empty
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo deny check                     # unchanged
$PY/pytest apps/python-wiki-demo/tests -q; $PY/pytest apps/python-minimal-demo/tests -q                  # green
git diff --stat main                                                                                      # ~850 lines; the split if asked
grep -rn "$(hostname -s)\|$USER" specs/023-optional-chonky-chunker apps/python-wiki-demo --exclude-dir=.venv   # nothing
```

## Step 6 — docs

`apps/python-wiki-demo/README.md` (research D9); `specs/023-…/report.md`, `pr-description.md`.
