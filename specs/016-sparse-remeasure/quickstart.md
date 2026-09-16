# Quickstart: validating the Sparse Stage Re-measurement

**Feature**: `016-sparse-remeasure` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## Step 1 — Red (Rule 4)

```bash
reference/.venv-012/bin/python -m pytest reference/tests_016 -q      # ModuleNotFoundError: sparse_remeasure
```

## Step 2 — The measurement (minutes, plus the reference scoring of uncovered pairs)

```bash
P=reference/.venv-012/bin/python
for d in scifact nfcorpus fiqa; do $P reference/sparse_remeasure.py all --dataset $d; done
$P reference/sparse_remeasure.py table && $P reference/sparse_remeasure.py decide
```

Expected: `check <d>: PASS` on every dataset — `lex2+dense-plain` equals `hybrid-baseline-v2`
per query and the explain's fused order; `lex2+dense-rr` equals `hybrid-rerank-v3` per query
and `target/xt-rr3-run.<d>.jsonl` list for list, with 0 reference-scored pairs; every `rr`
cell's Recall@100 equals its `plain` cell's. **⛔** A mismatch is a defect in the fusion or
re-rank code — never in the anchor. `rerank` prints the reference-scored share and the
agreement figures per dataset; `decide` prints the rule, the row and the outcome.

## Step 3 — Record

The table and decision into `report.md`; a pointer in `specs/012-sparse-spike/report.md`
(the GO's status) and in `specs/015-rerank-interpolation/report.md` F-005-style note if any.

## Step 4 — Gate (Rule 5, reduced: nothing under `crates/` changes)

```bash
git diff --stat main -- crates/ specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage specs/013-lexical-quality/baselines specs/014-rerank-depth-study/runs specs/015-rerank-interpolation/baselines   # empty
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo nextest run --workspace && cargo deny check   # unchanged
reference/.venv-012/bin/python -m pytest reference/tests_016 -q && reference/.venv-012/bin/python -m pytest reference/tests_014 -q   # separate runs: tests_014 imports from its own `conftest` module by name
```
