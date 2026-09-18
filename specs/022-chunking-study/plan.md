# Implementation Plan: The Chunking Study

**Branch**: `022-chunking-study` | **Date**: 2026-09-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/022-chunking-study/spec.md`

## Summary

A reference study script, `reference/chunking_study.py` (the 014/016 pattern, with
`reference/tests_022/`), that indexes the BEIR corpora four ways through the Python package
— `whole`, the 008 `contract` chunker, `chonky`, `chonky-bounded` — searches the test
queries at k = 300 / candidate depth 300 at re-rank depths 0 and 20, folds passages to
documents by MaxP, and scores with the 003 scorer against the test qrels. The anchor
(`whole@100`) must reproduce `hybrid-baseline-v2` / `hybrid-rerank-v3` to 1e-6 on every
query before any variant runs. SciFact and NFCorpus for all four variants; FiQA for `whole`
and the leader. The decision rule (0.005 / 0.005 / 0.005, ties to the non-neural chunker)
is a set of literals with tests. Nothing in the engine, the FFI, the formats, the baselines,
the demos or the shipped artefact changes. Research D1–D10 in [research.md](./research.md).

## Technical Context

**Language/Version**: Python 3.12 in `reference/.venv-022` (the 012 lock + `chonky==0.1.7`,
pinned — see research D1 on why not re-hashed; the `xtriever` wheel installed from the local build). **Primary
Dependencies**: `xtriever`, `chonky` / `transformers` / `torch`, `tokenizers`, `pytrec_eval`;
the repository's `gen_003_fixtures` (scoring) and `gen_008_fixtures` (the contract chunker).
**Storage**: indexes and the cached chonky splits under `target/xt-chunking-study/`
(gitignored); run files, scores, build records and the decision under the feature's
`runs/`. **Testing**: `reference/tests_022/` for the pure parts (join, the bounding and
merging, MaxP, run files, the anchor comparison, the rule); the anchor check is the
harness's oracle. **Target Platform**: host. **Project Type**: reference study.
**Performance Goals**: none claimed — the study measures quality; its cost is stated
(~3 h 10 for SciFact + NFCorpus, ~8 h in full). **Constraints**: no change under `crates/`,
`swift/`, `python/src`, `apps/`, any baseline; the study's constants literal and tested;
resumable per cell. **Scale/Scope**: one script (~450 lines), one test suite (~250), one
requirements pair, ~22 run files with scores, build records, the decision, the report.

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
| I | Reuse Before Build | Commodity components come from `tantivy`, `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR. | PASS | The study calls the package for indexing and retrieval, `chonky` for the neural split, `pytrec_eval` for scoring; the contract chunker is the existing reference implementation, imported. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests first, committed failing; reference behaviour pinned to goldens at stated tolerances; ranking work reports nDCG@10 / Recall@100 deltas on the three sets. | PASS | Tests first for every pure part; the anchor reproduces the committed baselines at 1e-6 per query before any variant runs (Rule 6 stop otherwise); the outcome *is* nDCG@10 / Recall@100 deltas on the BEIR sets, per cell, committed. |
| III | Portability Is a Feature | Pure crates stay pure; cross-target checks pass; RSS under the ceiling. | PASS | No crate change (SC-005). |
| IV | Measured, Not Asserted | Performance claims backed by benchmarks/records; reproducible: pinned models and datasets, fixed seeds; `explain()` per hit. | PASS | Models pinned (the engine's and the chonky manifest), datasets pinned by the BEIR manifest, the environment pinned; every cell a committed run file; the decision by a rule fixed in the spec with literal constants. |
| V | Small, Explicit Interfaces | Core traits, on-disk format, error semantics unchanged. | PASS | Nothing touched. |
| VI | Graceful Degradation and Determinism | Degrade not error unless strict; identical results for identical inputs. | PASS | The engine unchanged; no budgets; the same index and query give the same run (the anchor check relies on it). |
| VII | Rust Hygiene | Toolchain, lints, `cargo add`, no `unwrap` in libraries, `unsafe` confined. | PASS | No Rust touched; the four commands run once. Python pins are the 012 lock's (resolver output) plus chonky's 021 pin, never from memory. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external item read from the pinned docs and cited. | PASS | Package items as in 019/021; `SearchOptions.depth` = candidate depth (`index.rs:393`); `gen_003_fixtures.reference` / `load_qrels_tsv`, `gen_008_fixtures.chunk` / `sentences`, `rerank_study.read_run` / `write_run` / `compare_metrics` — read from the tree (D2, D5, D7). |
| 2 | Don't touch the contract uninvited | No `xtriever-core` trait or `deny.toml` change. | PASS | Neither touched. |
| 3 | One spec, small PRs | Scoped to this spec, under ~800 lines. | PASS | ~700 lines of script + tests; the run files are data (committed as records, as 014/016's were). One PR. |
| 4 | Tests first | Acceptance tests committed failing first. | PASS | Quickstart Step 1; the owner commits the red checkpoint. |
| 5 | Full gate before done | The local gate run and deltas pasted. | PASS | Quickstart Step 5; the deltas are the study's product and go into the PR. |
| 6 | Never weaken the oracle | Stop and report on a drop. | PASS | The anchor's 1e-6 tolerance and the rule's constants are literals with tests; a failed anchor stops the study. |
| 7 | Prefer boring code | No macros, trait gymnastics, premature generics. | PASS | Plain functions per subcommand, as the earlier studies. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-17, Claude (agent).

**Post-design gate (post-Phase 1)**: PASS — 2026-09-17, Claude (agent); the design adds a
pinned reference environment and nothing to the workspace.

## Project Structure

### Documentation (this feature)

```text
specs/022-chunking-study/
├── plan.md · research.md · data-model.md · quickstart.md
├── contracts/study.md
├── runs/                          # <variant>-d<depth>@<k>.<dataset>.{jsonl,json}, build-records.json
├── owner-decision.json            # /speckit-implement
├── report.md · pr-description.md
└── tasks.md                       # /speckit-tasks
```

### Source Code (repository root)

```text
reference/
├── chunking_study.py              # build | search | score | check | table | decide | all (contracts/study.md)
├── requirements-022.in / .txt     # the 012 pins + chonky, pinned (D1)
└── tests_022/
    ├── conftest.py · helpers_022.py    # sys.path, REPO, the constants
    ├── test_join.py                    # the baselines' title/text join
    ├── test_splitters.py               # contract budget and the title-fills-window case; chonky-bounded merge and re-chunk on a stub splitter and a stub cost
    ├── test_maxp.py                    # first occurrence, dedupe, truncation, short queries counted
    ├── test_runs.py                    # run/score file names and shapes; the anchor comparison on synthetic reports
    └── test_decide_022.py                  # the rule at its boundaries; two-way vs three-way scope; ties
```

Not touched: `crates/`, `swift/`, `python/src/`, `apps/`, `reference/fixtures/`, any baseline,
the shipped artefact.

**Structure Decision**: the same layout as the 014 and 016 studies — one script beside
them, one test suite that collects with theirs, records under the feature's `runs/`.

## Complexity Tracking

None.
