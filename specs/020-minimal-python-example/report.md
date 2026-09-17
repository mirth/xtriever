# Report: The Minimal Python Demo

**Feature**: 020 · **Branch**: `020-minimal-python-example` · **Status**: in progress

## Red checkpoint (2026-09-17)

`apps/python-wiki-demo/.venv/bin/pytest apps/python-minimal-demo/tests -q` at checkpoint C1:
**5 failed** — `test_line_budget` on the missing file, the other four on `load_demo`
(`FileNotFoundError: demo.py`). Nothing under `apps/python-minimal-demo/` but the tests.
