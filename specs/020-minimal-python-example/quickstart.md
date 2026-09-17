# Quickstart: The Minimal Python Demo

From the repository root, `unset SDKROOT`; `PY=apps/python-wiki-demo/.venv/bin` (has the
wheel and pytest).

## Step 1 — red

```bash
$PY/pytest apps/python-minimal-demo/tests -q      # fails: demo.py does not exist
```

## Step 2 — by hand

```bash
time $PY/python apps/python-minimal-demo/demo.py "how do bees make honey"
$PY/python apps/python-minimal-demo/demo.py "why does the sea rise and fall"
$PY/python apps/python-minimal-demo/demo.py; echo "exit $?"          # usage, 2
wc -l apps/python-minimal-demo/demo.py                                # ≤ 80
```

Expected: `indexed 10 documents`, the fused list, the re-ranked list; under ten seconds.

## Step 3 — the tests

```bash
$PY/pytest apps/python-minimal-demo/tests -q         # 5 model-free + 1 model-backed
```

## Step 4 — gate

```bash
git diff --stat main -- crates/ swift/ python/src apps/python-wiki-demo specs/*/baselines   # empty
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo deny check        # unchanged
$PY/pytest apps/python-wiki-demo/tests -q                                                    # 70 passed, untouched
```

## Step 5 — docs

`apps/python-minimal-demo/README.md`; `specs/020-…/report.md`, `pr-description.md`; one
pointer line in `apps/python-wiki-demo/README.md`.
