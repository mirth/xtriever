# Report: Python Interface and Bindings

**Feature**: `011-python-bindings` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Red checkpoint (A1)

`python/.venv/bin/pytest python/tests -q` against the wheel built from the existing surface:
**23 passed, 4 failed** — the four `test_build.py` tests on `AttributeError: type object
'IndexHandle' has no attribute 'create'` (the builder is PR B). The 16 golden pairs, the
options, the thread and overhead tests are green by construction (the surface is 007's; the
files say so). `-m "not models"`: 8 passed (surface 6, errors 2).
