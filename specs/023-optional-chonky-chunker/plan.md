# Implementation Plan: Chonky as an Optional Chunker for the Wikipedia Demo Build

**Branch**: `023-optional-chonky-chunker` | **Date**: 2026-09-18 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/023-optional-chonky-chunker/spec.md`

## Summary

`wikidemo build` gains `--chunker {contract,chonky}` (default `contract`). The contract
chunker is the Feature 019 copy of the 008 reference implementation, restored verbatim from
commit `802cf72` into its own module with its fixture-replay tests; the chonky path is
Feature 021's code moved into its own module behind the flag. Both present one interface
(`documents_for(article) -> (documents, positions)`, a sidecar `block`, a `label`), so
`build.py` reads as before with the chunker chosen once. `chonky`, `transformers` and
`torch` move to an optional extra (`.[chonky]`); a chonky build without it is refused with
the install command before the snapshot is opened, and the chonky model directory is an
input only for a chonky build. The 2,000-article slice is rebuilt with the default and
checked against the Rust slice on disk (`measure --against`, PASS expected — the identity
`20949fb4…`); a 200-article chonky slice is recorded. Research D1–D9 in
[research.md](./research.md).

## Technical Context

**Language/Version**: Python 3.12 (the demo's venv). **Primary Dependencies**: the
`xtriever` wheel, `tokenizers==0.23.2` (mandatory: the contract pricer and the position
count); extra `chonky`: `chonky==0.1.7`, `transformers==5.17.0`, `torch==2.14.0` (the 021
pins, unchanged). **Storage**: demo-built artefacts under `target/`; the Rust slice
`target/xt-wiki-slice-rs` (on disk, 24 MB, identity `20949fb4…`). **Testing**: the 019/021
suite plus a `chonky` marker (skipped with the reason when the extra or its model is
absent); the 008 fixture replay (48 + 9 cases). **Target Platform**: host, CPU.
**Project Type**: demo application. **Performance Goals**: the default slice build under
30 minutes (019 measured ~17 min); the refusal without the extra under a second (SC-003).
**Constraints**: no change under `crates/`, `swift/`, `python/src`, the minimal demo,
`reference/`, baselines, the shipped artefact; a default build never imports the extra.
**Scale/Scope**: ~8 demo files, 2 new modules (one restored), 2 test files rearranged,
README, two records; ~850 changed lines of which ~330 are the verbatim restoration (Rule 3
below).

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
| I | Reuse Before Build | Commodity components come from `tantivy`, `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR. | PASS | Nothing new is built: the contract chunker is the 008 reference implementation restored from history (the contract the shipped index is cut by), chonky stays the library call. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests first, committed failing; reference behaviour pinned to goldens at stated tolerances; ranking work reports eval deltas. | PASS | Tests first (quickstart Step 1). The contract path is pinned to the 008 goldens byte-for-byte (`chunk_a.json`, `chunk_b.json`) and to the Rust slice by `measure --against` (identity, documents, order at every depth); the chonky path keeps 021's exact partition oracle. No engine change, no eval deltas. |
| III | Portability Is a Feature | Pure crates stay pure; cross-target checks pass; on-device RSS under the ceiling. | PASS | No crate change (SC-004). |
| IV | Measured, Not Asserted | Performance claims backed by benchmarks/records; reproducible: model revisions pinned by SHA. | PASS | The parity verdict and both build records are committed under `runs/` (D8); the chonky model stays pinned by the 021 manifest; dependency pins unchanged. |
| V | Small, Explicit Interfaces | Core traits, on-disk format, error semantics unchanged. | PASS | Nothing touched; the sidecar's chunker block is one of the two shapes `about` already reads. |
| VI | Graceful Degradation and Determinism | Degrade not error unless strict; identical results for identical inputs. | PASS | The engine unchanged; both chunkers are deterministic; a missing extra or model is a hard, early error, never a silent fallback to the other chunker (D5). |
| VII | Rust Hygiene | Toolchain, lints, `cargo add`, no `unwrap` in libraries, `unsafe` confined. | PASS | No Rust touched; the four commands run once. Python pins are the 021 ones, moved, not rewritten. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external item read from the pinned docs and cited. | PASS | `importlib.util.find_spec` (stdlib, D5, behaviour verified); `argparse` `choices`/`default`; `chonky.ParagraphSplitter` as cited in 021 D1; `tokenizers.Tokenizer.from_file` as in 019 D5; `xtriever.Document`/`ChunkInfo` unchanged. |
| 2 | Don't touch the contract uninvited | No `xtriever-core` trait or `deny.toml` change. | PASS | Neither touched. |
| 3 | One spec, small PRs | Scoped to this spec, under ~800 lines. | PASS | One PR of ~850 changed lines, ~330 of them the verbatim restoration of `802cf72`'s chunker and its tests (reviewable by `git diff 802cf72 -- <old path>` against the new module: empty apart from the module header). If the reviewer prefers, the split is PR A = modules, option, tests, pyproject; PR B = README and records. |
| 4 | Tests first | Acceptance tests committed failing first. | PASS | Quickstart Step 1; the owner commits the red checkpoint. |
| 5 | Full gate before done | The local gate run and deltas pasted. | PASS | Quickstart Step 5; no eval deltas (nothing ranking-affecting in the engine); the parity verdict is the PR's number. |
| 6 | Never weaken the oracle | Stop and report on a drop. | PASS | A fixture-replay mismatch, a parity FAIL against the Rust slice, or a default build that imports the extra is stop-and-report. |
| 7 | Prefer boring code | No macros, trait gymnastics, premature generics. | PASS | Two plain classes with the same three attributes, chosen by a dict lookup on the flag; no base class, no registry. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-18, the agent.

**Post-design gate (post-Phase 1)**: PASS — 2026-09-18, the agent (design in research D1–D9; no new violations).

## Project Structure

### Documentation (this feature)

```text
specs/023-optional-chonky-chunker/
├── plan.md · research.md · data-model.md · quickstart.md
├── contracts/build.md
├── runs/                              # the default slice's build record + parity verdict; the 200-article chonky record
├── report.md · pr-description.md      # /speckit-implement
└── tasks.md                           # /speckit-tasks
```

### Source Code (repository root)

```text
apps/python-wiki-demo/
├── pyproject.toml                               # chonky/transformers/torch → [project.optional-dependencies] chonky; marker `chonky` (D6)
├── wikidemo/
│   ├── chunking.py                              # shared: WINDOW, BuildError, Window, passage_text; CHUNKERS, make_chunker (D2)
│   ├── contract.py                              # new (restored from 802cf72): the 008 reference chunker, Pricer, ContractChunker (D3)
│   ├── chonky_chunker.py                        # new (from 021's chunking.py): Splitter, ChonkyChunker, ensure_extra (D4, D5)
│   ├── build.py                                 # build(paths, out, limit, chunker); the chunker line via its label
│   ├── cli.py                                   # --chunker on build; needs_for per chunker (D7)
│   ├── record.py                                # CONTRACT_CHUNKER, CHONKY_CHUNKER (the two blocks)
│   └── about.py                                 # unchanged (chunker_label already covers both)
├── tests/
│   ├── conftest.py                              # `chonky` marker: skip with the reason when the extra or the model is absent (D6)
│   ├── test_chunking.py                         # the shared bits and the chooser
│   ├── test_contract.py                         # new (restored 019 tests): set A/B byte-identical, shaping, budget split
│   ├── test_chonky.py                           # new (021's chunking tests, `chonky` marker where the model is needed)
│   ├── test_build.py                            # both chunkers; the poisoned-import default build; the refusal
│   ├── test_cli.py · test_inputs.py · test_record.py   # the option, the per-chunker inputs, the two blocks
└── README.md                                    # the default recipe with the parity claim back; the chonky option
```

Not touched: `crates/`, `swift/`, `python/src/`, `apps/python-minimal-demo/`, `reference/`,
any baseline, the shipped artefact, `reference/models/manifest-chonky.json`.

**Structure Decision**: one module per chunker, each ending in a small class with the same
three members (`documents_for`, `block`, `label`), chosen by name in `chunking.make_chunker`;
`build.py` keeps its recipe shape. The contract module is the 019 file minus the members
that moved to `chunking.py`, so the restoration is diffable against history.

## Complexity Tracking

None.
