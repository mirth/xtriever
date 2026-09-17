# Implementation Plan: Chonky Chunking for the Wikipedia Demo Build

**Branch**: `021-chonky-wiki-chunking` | **Date**: 2026-09-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/021-chonky-wiki-chunking/spec.md`

## Summary

`wikidemo build` splits each article with `chonky`'s `ParagraphSplitter` (a
`DistilBertForTokenClassification` that returns the article as contiguous slices at
predicted paragraph breaks), loaded from a pinned local model directory fetched by the
existing `scripts/fetch-model.sh` from a new `reference/models/manifest-chonky.json`. The
demo's 250-line copy of the 008 contract chunker and its fixture replays are deleted; the
new `chunking.py` is the splitter call, the byte-offset provenance and the over-window
count (< 80 lines). Over-window chunks are embedded from their head and counted; the
sidecar's chunker block names chonky and the revision, so the corpus identity changes and
the Rust-build oracle is retired. The 2,000-article slice is rebuilt and recorded.
Research D1–D10 in [research.md](./research.md).

## Technical Context

**Language/Version**: Python 3.12 (the demo's venv). **Primary Dependencies**:
`chonky==0.1.7` (+ `transformers==5.17.0`, `torch==2.14.0`), `tokenizers==0.23.2` (kept for
the over-window count), the `xtriever` wheel. **Storage**: the demo-built artefacts under
`target/`; the model under `reference/models/chonky_distilbert_base_uncased_1` (gitignored).
**Testing**: the 019 suite, `test_chunking.py` rewritten (stub splitter for the offset
arithmetic; the real splitter under `models`), `test_build.py` through chonky.
**Target Platform**: host, CPU. **Project Type**: demo application. **Performance Goals**:
the split under one minute and the build under 30 minutes for the 2,000-article slice
(SC-002; measured ~60k chars/s for the split). **Constraints**: no change under `crates/`,
`swift/`, `python/src`, the minimal demo, the fixtures, the shipped artefact; the model
loaded from disk only. **Scale/Scope**: ~5 files changed, one rewritten, one manifest,
one record, ~400 lines net negative.

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
| I | Reuse Before Build | Commodity components come from `tantivy`, `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR. | PASS | This feature is the principle applied: a hand-carried chunker is replaced by a library; the model runs on `transformers` / `torch`. Nothing is built by hand. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests first, committed failing; reference behaviour pinned to goldens at stated tolerances; ranking work reports eval deltas. | PASS | Tests first (quickstart Step 1). The oracle for the split is the partition property (concatenation equals the text, byte ranges slice back) — exact, asserted in tests and enforced at build time; the fixture replay goes with the chunker it pinned. Nothing in the engine changes; the shipped artefact and its host-goldens check are untouched, so no eval deltas arise. |
| III | Portability Is a Feature | Pure crates stay pure; cross-target checks pass; on-device RSS under the ceiling. | PASS | No crate change (SC-004). |
| IV | Measured, Not Asserted | Performance claims backed by benchmarks/records; reproducible: model revisions pinned by SHA. | PASS | The model is pinned by revision and per-file sha256 in a manifest and fetched by the existing script (D2); the split's speed and the over-window share are measured and recorded (D9). |
| V | Small, Explicit Interfaces | Core traits, on-disk format, error semantics unchanged. | PASS | Nothing touched; the sidecar (a demo/008 file, not an engine format) changes its chunker block — a JSON object whose shape `about` already reads generically. |
| VI | Graceful Degradation and Determinism | Degrade not error unless strict; identical results for identical inputs. | PASS | The engine unchanged. The splitter is deterministic on CPU for a given model and input (`torch` inference, no sampling); a non-partition result is a hard error, never silent. |
| VII | Rust Hygiene | Toolchain, lints, `cargo add`, no `unwrap` in libraries, `unsafe` confined. | PASS | No Rust touched; the four commands run once. Python dependencies are pinned at versions read from the resolver (D7), never from memory. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external item read from the pinned docs and cited. | PASS | `chonky.ParagraphSplitter(model_id, device)` and `__call__` read from `src/chonky/__init__.py` 0.1.7 (D1); `transformers` `from_pretrained` on a local directory is what the library calls; package items as in 019. |
| 2 | Don't touch the contract uninvited | No `xtriever-core` trait or `deny.toml` change. | PASS | Neither touched. |
| 3 | One spec, small PRs | Scoped to this spec, under ~800 lines. | PASS | One PR: the rewritten chunker (−250 + 80), the build/inputs/about/record edits, the manifest, tests, README, one record — well under. |
| 4 | Tests first | Acceptance tests committed failing first. | PASS | Quickstart Step 1; the owner commits the red checkpoint. |
| 5 | Full gate before done | The local gate run and deltas pasted. | PASS | Quickstart Step 4; no eval deltas (nothing ranking-affecting in the engine). |
| 6 | Never weaken the oracle | Stop and report on a drop. | PASS | A non-partition, a missing-model path that loads from the hub, or a slice build over budget is stop-and-report; the partition check is exact. |
| 7 | Prefer boring code | No macros, trait gymnastics, premature generics. | PASS | Two small classes and one function; the library does the work. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-17, Claude (agent).

**Post-design gate (post-Phase 1)**: PASS — 2026-09-17, Claude (agent); the only new
dependencies are the demo's own (chonky, transformers, torch), pinned; nothing under the
workspace changes.

## Project Structure

### Documentation (this feature)

```text
specs/021-chonky-wiki-chunking/
├── plan.md · research.md · data-model.md · quickstart.md
├── contracts/build.md
├── runs/                              # the chonky slice's build record
├── report.md · pr-description.md      # /speckit-implement
└── tasks.md                           # /speckit-tasks
```

### Source Code (repository root)

```text
reference/models/manifest-chonky.json            # new: the pinned splitter model (D2)
apps/python-wiki-demo/
├── pyproject.toml                               # + chonky, transformers, torch (D7)
├── wikidemo/
│   ├── chunking.py                              # rewritten: Splitter, Window, documents_for, passage_text, BuildError (D3)
│   ├── build.py                                 # Splitter + Window instead of Pricer; over-window count; chunking stats in the record
│   ├── inputs.py                                # the chonky input (D6)
│   ├── about.py                                 # chunker line, over-window line
│   ├── record.py                                # CHUNKER = the chonky block (D5)
│   └── cli.py                                   # --chonky on build; needs_for
├── tests/
│   ├── conftest.py                              # CHONKY path in the models skip
│   ├── test_chunking.py                         # rewritten (D8)
│   ├── test_build.py · test_record.py · test_inputs.py · test_cli.py   # updated
└── README.md                                    # the recipe around chonky
```

Not touched: `crates/`, `swift/`, `python/src/`, `apps/python-minimal-demo/`,
`reference/fixtures/`, `reference/gen_008_fixtures.py`, any baseline, the shipped artefact.

**Structure Decision**: the same module layout as 019 — the chunker module keeps its name
and its two entry points (`documents_for`, `passage_text`) so `build.py`'s recipe reads as
before with the splitter in place of the pricer.

## Complexity Tracking

None.
