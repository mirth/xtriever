# Implementation Plan: The Minimal Python Demo

**Branch**: `020-minimal-python-example` | **Date**: 2026-09-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/020-minimal-python-example/spec.md`

## Summary

A self-contained demo at `apps/python-minimal-demo/`: one file, `demo.py` (≤ 80 lines), with
ten short documents written in it, that builds an index through the package — one
`contents` text field (the 013 layout), `create` in a temporary directory, `add`, `commit`,
`merge` — searches it at depth 0 and depth 10 (k = 5), prints the fused and the re-ranked
lists, and cleans up. No snapshot, no chunker, no import from the Wikipedia demo. The oracle
is a direct package call on the same documents (ids in order, score bits, both depths) plus
determinism across two runs; model-free tests cover the line budget, the usage exit and the
printing. A short README; one pointer line in the Wikipedia demo's README. Research D1–D7 in
[research.md](./research.md).

## Technical Context

**Language/Version**: Python 3.12. **Primary Dependencies**: the `xtriever` wheel and the two
pinned models — nothing else (no `tokenizers`, no `wikidemo`). **Storage**: a temporary
directory per run, removed. **Testing**: `pytest` under `apps/python-wiki-demo/.venv`; a
two-line conftest with the model-path skip. **Target Platform**: host. **Project Type**:
example script. **Performance Goals**: the whole run under ten seconds (SC-003).
**Constraints**: ≤ 80 lines; standard library only besides the wheel; no change under
`apps/python-wiki-demo`. **Scale/Scope**: `demo.py` (~70 lines), `tests/test_demo.py`
(~80), `README.md`, the spec documents.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.4.0, ratified 2026-09-10, last amended 2026-09-13). Every row below MUST be
marked **PASS**, **FAIL**, or **N/A** with a one-line justification — an empty verdict is a FAIL.

- Any **FAIL** blocks the phase. Do not proceed, and do not weaken the gate to make it pass.
- Any **N/A** MUST state why the principle cannot apply to this feature.
- A violation that is genuinely required is recorded in Complexity Tracking below and REQUIRES an
  accepted ADR in `docs/adr/`; the row stays **FAIL** until that ADR is merged.
- These are plan-time gates. They do not replace the blocking CI gates in the constitution's
  "Quality Gates (CI)" table.

### Core Principles

| # | Principle | Gate | Verdict | Justification |
|---|---|---|---|---|
| I | Reuse Before Build | Commodity components come from `tantivy`, `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR. | PASS | The demo calls the package and nothing else; it builds nothing of its own. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests first, committed failing; reference behaviour pinned to goldens at stated tolerances; ranking work reports eval deltas. | PASS | Tests first (quickstart Step 1); the oracle is a direct package call on the same documents — ids in order and every score bit at depths 0 and 10, two runs identical (D5). Nothing ranking-affecting changes. |
| III | Portability Is a Feature | Pure crates stay pure; cross-target checks pass; on-device RSS under the ceiling. | PASS | No crate change (SC-004). |
| IV | Measured, Not Asserted | Performance claims backed by benchmarks/records; reproducible; `explain()` per hit. | PASS | The only claim is SC-003's run time, measured in the report; no engine claim. |
| V | Small, Explicit Interfaces | Core traits, on-disk format, error semantics unchanged; dependencies point downward. | PASS | Nothing touched; the demo depends on the wheel, nothing depends on the demo. |
| VI | Graceful Degradation and Determinism | Degrade not error unless strict; identical results for identical inputs. | PASS | Determinism is what the test checks (two runs, the direct call); the demo passes no budget. |
| VII | Rust Hygiene | Toolchain, lints, `cargo add`, no `unwrap` in libraries, `unsafe` confined. | PASS | No Rust touched; the four commands run once to show the gate unchanged. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external item read from the pinned docs and cited. | PASS | Package items as in 019's plan (`IndexHandle.create/add/commit/merge/search`, `IndexConfig`, `FieldDef`, `FieldKind.TEXT`, `Document`, `FieldValue.TEXT`, `SearchOptions`, `LoadPath.MMAP` — `crates/xtriever-ffi/src/ffi/types.rs`, `python/README.md`); `tempfile.mkdtemp`, `shutil.rmtree` from the standard library. |
| 2 | Don't touch the contract uninvited | No `xtriever-core` trait or `deny.toml` change. | PASS | Neither touched. |
| 3 | One spec, small PRs | Scoped to this spec, under ~800 lines. | PASS | ~150 lines of code and tests plus a README and the spec documents; one PR. |
| 4 | Tests first | Acceptance tests committed failing first. | PASS | `tests/test_demo.py` red at the first checkpoint (the owner commits). |
| 5 | Full gate before done | The local gate run and deltas pasted. | PASS | Quickstart Step 4; no eval deltas because nothing ranking-affecting changed. |
| 6 | Never weaken the oracle | Stop and report on a drop. | PASS | A mismatch with the direct call or a line-budget breach is stop-and-report; bits exact, no tolerance. |
| 7 | Prefer boring code | No macros, trait gymnastics, premature generics. | PASS | Three plain functions and `sys.argv`. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-17, Claude (agent).

**Post-design gate (post-Phase 1)**: PASS — 2026-09-17, Claude (agent); no new dependency,
no package change.

## Project Structure

### Documentation (this feature)

```text
specs/020-minimal-python-example/
├── plan.md · research.md · data-model.md · quickstart.md
├── contracts/example.md
├── report.md · pr-description.md      # /speckit-implement
└── tasks.md                           # /speckit-tasks
```

### Source Code (repository root)

```text
apps/python-minimal-demo/
├── demo.py                 # the file (D1): corpus, schema, build, two searches, print, cleanup
├── README.md               # what it is, the two inputs, the command, what is left out and where it lives
└── tests/
    ├── conftest.py         # model paths from the environment; skip-with-reason
    └── test_demo.py        # line budget, usage exit, printing on stubs; [models] hits equal the direct call, two runs agree
apps/python-wiki-demo/README.md   # one pointer line
```

Not touched: `crates/`, `swift/`, `python/src/`, `apps/python-wiki-demo/wikidemo/`, any
baseline.

**Structure Decision**: a third `apps/` entry beside the two Wikipedia demos, with no package
and no manifest — a script and its README — because that is what it is meant to be copied
as. It depends on the wheel only; nothing depends on it.

## Complexity Tracking

None.
