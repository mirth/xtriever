# Research: The Minimal Python Demo

Rewritten after the owner's redirection (self-contained, `apps/python-minimal-demo/`).

## D1 — Shape of the file

**Decision**: a docstring (what it is, the two inputs, the command, what it leaves out and
which demo does it), the corpus as a list of `(id, text)` tuples, `SCHEMA` (one `contents`
text field, the dense field — Feature 013's layout), then three functions — `build(docs)`
(create in a `tempfile.mkdtemp()`, add, commit, merge; returns the handle and the directory),
`print_hits(label, hits)`, `main(argv)` (one positional argument, the two searches, the
cleanup with `shutil.rmtree`) — and the `__main__` guard. Functions so the tests can load the
file by path (`importlib.util.spec_from_file_location`) and call `main([])` / `print_hits`
on stubs. Budget ≤ 80 lines, counted by a test.

## D2 — The corpus

**Decision**: ten short prose documents on distinct everyday topics (honey bees, the water
cycle, bread, tides, the Moon, rust, sourdough, coral reefs, thunderstorms, glaciers), each
`"<title>\n\n<one or two sentences>"`. Two pairs share vocabulary across topics (bread /
sourdough; tides / the Moon) so a query such as "how do bees make honey" or "why does the
sea rise and fall" shows the lexical and dense stages disagreeing and the cross-encoder
settling it. Plain ids `doc-01` … `doc-10`. Passage-sized by construction: no chunker.

## D3 — Model paths

**Decision**: `XTRIEVER_MODEL_DIR` / `XTRIEVER_RERANK_MODEL_DIR` with the defaults
`reference/models/all-MiniLM-L6-v2` and `reference/models/ms-marco-MiniLM-L-6-v2` relative
to the repository root (`Path(__file__).resolve().parents[2]`) — the names the other demos and
the package's tests use. A missing directory fails with the package's own `Model` error;
the script adds no input checks (spec edge case).

## D4 — The two searches and the printing

**Decision**: `SearchOptions(k=5, rerank_depth=0)` then `SearchOptions(k=5,
rerank_depth=10)`; both lists printed as `label, n hits` then ` r. id  score=…  [rerank=…]
title line`. No explain, no marks, no timings — each named in the docstring with the demo
that has it (019's `search --explain`, the marks; 009 on the phone).

## D5 — The oracle

**Decision**: `tests/test_demo.py::test_hits_are_the_engines` (models): build the same
documents directly through the package into another temporary directory with the same
`IndexConfig`, search with the same two options, and compare with the script's `main`
output parsed back (ids in order) and with the script's own `search` results (score bits):
`main` returns the two responses as well as printing — simplest: `run(query) -> (fused,
reranked)` returns the responses, `main` prints them. The test compares `run`'s hits with
the direct call's on `(external_id, f64 bits of score, f32 bits of rerank_score)` at both
depths, and runs `run` twice for determinism. Model-free: `test_line_budget`,
`test_usage_exit_2`, `test_print_hits_on_stubs`.

## D6 — Environment for the tests

**Decision**: run under `apps/python-wiki-demo/.venv` (the wheel and pytest are there); a
tiny `conftest.py` with the model-path skip on the 011/019 pattern. The demo directory
carries no `pyproject.toml`: it is one script, and the README's install line is "a Python
with the wheel installed".

## D7 — Not done

No output-directory argument, no `--depth`/`-k`, no chunker, no snapshot, no records, no
parity against a second build (the oracle is the direct call), no CI job (standing rule).
