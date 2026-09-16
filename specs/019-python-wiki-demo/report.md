# Report: The Python Wikipedia Demo

**Feature**: 019 · **Branch**: `019-python-wiki-demo` · **Status**: in progress

## Red checkpoint A (2026-09-17)

`apps/python-wiki-demo/.venv/bin/pytest apps/python-wiki-demo/tests -q` at commit A1:
**7 errors during collection** — every test file fails on import (`No module named
'wikidemo.hits'` and the like); only the package skeleton (`__init__`, `__main__`, a `cli.main`
returning 2) exists. Test files: `test_hits`, `test_record`, `test_inputs`, `test_render`,
`test_search` (models), `test_about` (models), `test_measure`.
