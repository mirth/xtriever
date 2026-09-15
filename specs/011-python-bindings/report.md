# Report: Python Interface and Bindings

**Feature**: `011-python-bindings` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Red checkpoint (A1)

`python/.venv/bin/pytest python/tests -q` against the wheel built from the existing surface:
**23 passed, 4 failed** — the four `test_build.py` tests on `AttributeError: type object
'IndexHandle' has no attribute 'create'` (the builder is PR B). The 16 golden pairs, the
options, the thread and overhead tests are green by construction (the surface is 007's; the
files say so). `-m "not models"`: 8 passed (surface 6, errors 2).

## After A1 (US1/US2/US3 on the host)

- Model-backed search suite (`-k "search or options or threads or overhead or errors"`): 15 / 15;
  binding overhead **median 0.36 %**, max 0.66 % of the engine's `elapsed_ms` (24 samples,
  SC-004 ≤ 5 %); a Python thread's 20 × `sum(range(100_000))` finished during a re-ranked
  search (SC-005).
- Clean interpreter (SC-001): `uv venv --python 3.13` (CPython 3.13.11, arm64), the wheel
  installed with `pytest` only, run under `env -i PATH=/usr/bin:/bin` (no `cargo`): the same
  23 passed / 4 red-by-design as on 3.12.
