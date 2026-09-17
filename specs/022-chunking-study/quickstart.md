# Quickstart: The Chunking Study

From the repository root, `unset SDKROOT`; `PY=reference/.venv-022/bin/python`.

## Step 0 — environment

```bash
reference/.venv-012/bin/pip-compile --generate-hashes --output-file=reference/requirements-022.txt reference/requirements-022.in
scripts/setup-reference-venv.sh 022
VIRTUAL_ENV=reference/.venv-022 uv pip install target/wheels/xtriever-*.whl      # the local wheel (unhashed)
$PY -c "import xtriever, chonky, pytrec_eval, torch; print(xtriever.__version__, torch.__version__)"
ls reference/models/chonky_distilbert_base_uncased_1/model.safetensors reference/datasets/beir/scifact/corpus.jsonl
```

## Step 1 — red

```bash
$PY -m pytest reference/tests_022 -q          # fails: chunking_study does not exist
```

## Step 2 — green on the pure parts, then a smoke build

```bash
$PY -m pytest reference/tests_022 -q
$PY reference/chunking_study.py build --variant chonky-bounded --dataset scifact --limit 50   # seconds; not a cell
```

## Step 3 — the anchor (⛔ stop on any difference)

```bash
$PY reference/chunking_study.py build --variant whole --dataset scifact       # ~8 min
$PY reference/chunking_study.py search --variant whole --dataset scifact --depth 0 --k 100
$PY reference/chunking_study.py search --variant whole --dataset scifact --depth 20 --k 100   # ~8 min
$PY reference/chunking_study.py score --dataset scifact && $PY reference/chunking_study.py check --dataset scifact   # PASS
# the same for nfcorpus (~15 min); fiqa later (~2 h)
```

## Step 4 — the cells (background, monitored)

```bash
$PY reference/chunking_study.py all --dataset scifact     # ~1 h 40 in all, resumable
$PY reference/chunking_study.py all --dataset nfcorpus    # ~1 h 30
$PY reference/chunking_study.py table
$PY reference/chunking_study.py decide                     # provisional, two-way
$PY reference/chunking_study.py all --dataset fiqa --variant whole,<winner>   # ~2 h 10 + ~2 h 35
$PY reference/chunking_study.py decide --owner-decision specs/022-chunking-study/owner-decision.json
```

## Step 5 — gate

```bash
git diff --stat main -- crates/ swift/ python/src apps/ specs/*/baselines     # empty
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo deny check   # unchanged
$PY -m pytest reference/tests_012 reference/tests_014 reference/tests_016 reference/tests_022 -q   # one collection
grep -rn "$(hostname -s)\|$USER" specs/022-chunking-study reference/chunking_study.py reference/tests_022   # nothing
```
