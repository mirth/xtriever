# Report: The Minimal Python Demo

**Feature**: 020 · **Branch**: `020-minimal-python-example` · **Status**: done

## Verdict

`apps/python-minimal-demo/demo.py` — **79 lines**, ten documents written in it, one
`contents` field, `create` → `add` → `commit` → `merge` in a temporary directory, a search at
depth 0 and at depth 10, both lists printed, the directory removed. It needs the wheel and
the two models and nothing else; it imports nothing from the Wikipedia demo. Its hits are
the engine's — ids in order and identical score bits against a direct package call at
both depths — and a second run gives the same. A run takes **1.8 s** on this laptop.

## Red checkpoint (2026-09-17)

`apps/python-wiki-demo/.venv/bin/pytest apps/python-minimal-demo/tests -q` at checkpoint C1:
**5 failed** — `test_line_budget` on the missing file, the other four on `load_demo`
(`FileNotFoundError: demo.py`). Nothing under `apps/python-minimal-demo/` but the tests.

## US1 — the file (green)

The first draft was **87 lines** (the budget test failed, as it should); the docstring was
tightened and the blank lines between definitions reduced to one — the steps, the comments
per step and the corpus are unchanged. Final: 79.

`python apps/python-minimal-demo/demo.py "how do bees make honey"` (1.8 s wall):

```
indexed 10 documents

fused (lexical + dense), 5 hits
 1. doc-01  score=0.0328  Honey bees
 2. doc-05  score=0.0161  The Moon
 3. doc-03  score=0.0159  Bread
 4. doc-06  score=0.0156  Rust
 5. doc-04  score=0.0154  Tides

re-ranked (depth 10), 5 hits
 1. doc-01  score=0.0328  rerank=7.4850  Honey bees
 2. doc-05  score=0.0161  rerank=-11.2395  The Moon
 3. doc-03  score=0.0159  rerank=-11.0999  Bread
 4. doc-06  score=0.0156  rerank=-11.0485  Rust
 5. doc-04  score=0.0154  rerank=-11.2089  Tides
```

`"why does the sea rise and fall"`: fused *Tides, The water cycle, Bread, The Moon, Coral
reefs*; re-ranked *Tides (8.66), The water cycle (−1.77), Thunderstorms (−9.84), Bread,
Coral reefs* — the cross-encoder pulls *Thunderstorms* into the head and drops *The Moon*
(the README explains the two lists on this query). No argument → the usage line, exit 2.
`/tmp` holds no `xtriever-minimal-*` directory before or after a run.

## US2 — the oracle (green)

`test_hits_are_the_engines` (models): the script's `run` vs the same documents built
directly through the package with the same `IndexConfig` — `(external_id, f64 bits of
score, f32 bits of rerank_score)` equal for all five hits at depth 0 and at depth 10; a
second `run` identical. Model-free: the 80-line budget, the usage exit (with
`IndexHandle.create` patched to raise — no model loads on a usage error), the contract's
hit lines on stubs, the corpus's shape (ten unique ids, title / blank / body).

## Success criteria

| SC | Result |
|---|---|
| SC-001 | 79 lines ≤ 80 (tested) — **met** |
| SC-002 | ids in order and score bits equal the direct call at depths 0 and 10; two runs identical (tested) — **met** |
| SC-003 | one command, 1.8 s end to end — **met** |
| SC-004 | `git diff --stat main -- crates/ swift/ python/src apps/python-wiki-demo/wikidemo specs/*/baselines` empty — **met** |

## Gate

`cargo fmt --check`, `clippy`, `deny` unchanged; the Wikipedia demo suite 70 passed
(untouched); this suite 5 passed; no identifiers in the new files.

## Deliberately not done

- No chunker, no snapshot, no output directory, no flags, no explanations or marks — each
  named in the docstring and the README with the demo that has it.
- No `pyproject.toml`: one script and its README; the tests run under the Wikipedia demo's
  environment.
- No CI job (standing rule: nothing model-backed in CI).
