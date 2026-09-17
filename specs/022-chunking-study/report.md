# Report: The Chunking Study

**Feature**: 022 · **Branch**: `022-chunking-study` · **Status**: in progress

## Environment (T001)

`reference/requirements-022.in` = the 012 pins + `chonky==0.1.7`. The plan said the lock would
be hashed; it is not: the 012 lock turned out to be pinned but unhashed (0 hashes — so
`scripts/setup-reference-venv.sh`'s `--require-hashes` cannot have produced `.venv-012`
either), an online `pip-compile --generate-hashes` downloaded more than 1 GB of torch wheels in
30 minutes without finishing, and `--no-index` cannot see torch. `requirements-022.txt` is
therefore the 012 lock verbatim plus `chonky==0.1.7`, with a header saying so;
`reference/.venv-022` was made with `uv venv` + `uv pip install -r` (seconds, from the cache)
and the `xtriever` wheel from the local build: xtriever 0.1.0, torch 2.14.0, transformers 5.17.0.
Research D1, the plan and the quickstart corrected to match.

## Red checkpoint (2026-09-17)

`reference/.venv-022/bin/python -m pytest reference/tests_022 -q` at checkpoint C1: **5 errors
during collection** — every test file fails on `import chunking_study` (the script does not
exist). Files: `test_join`, `test_splitters`, `test_maxp`, `test_runs`, `test_decide`, with
`helpers_022.py` (the constants, a words-cost stub, a stub splitter) and `conftest.py`.

