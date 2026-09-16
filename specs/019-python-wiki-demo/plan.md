# Implementation Plan: The Python Wikipedia Demo

**Branch**: `019-python-wiki-demo` | **Date**: 2026-09-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/019-python-wiki-demo/spec.md`

## Summary

A command-line program at `apps/python-wiki-demo/` over the Feature 011 package: `wikidemo
search` prints the fused list, then the re-ranked list with change marks, the eight
explained features on request, the engine's stage report, the wall times and the peak
resident size; `wikidemo about` prints the corpus identity, counts, model identities and
the attribution verbatim; `wikidemo build --limit N --out DIR` builds an index from the raw
snapshot with the 008 recipe (manifest rules, the contract chunker priced by the pinned
tokenizer, `add` → `commit` → `merge`, the 008 sidecars); `wikidemo measure` checks the
shipped index against the phone's host goldens and writes a latency/footprint record, or
with `--against` checks a demo-built slice against the Rust build of the same slice. No
Rust, Swift, format, package-wire or baseline change; the demo's own logic is tested
against the 007 fixture, its goldens and the 008 chunker fixtures. Research decisions
D1–D18 in [research.md](./research.md).

## Technical Context

**Language/Version**: Python 3.12 (the demo's own venv, `uv venv --python 3.12`); the
engine unchanged (Rust, edition 2024, pinned toolchain).

**Primary Dependencies**: `xtriever` (the 011 wheel, local), `tokenizers==0.23.2` (the
version `crates/xtriever-dense/Cargo.toml` pins — for the build's unit pricing only),
`pytest>=8` (test extra); standard library for everything else (`argparse`, `json`,
`hashlib`, `resource`, `time`, `platform`, `statistics`, `shutil`, `struct`).

**Storage**: index directories under `target/` (the shipped artefact `target/xt-wiki`; demo
builds at `--out`); records under `specs/019-python-wiki-demo/runs/`.

**Testing**: `pytest` with a `models` marker and skip-with-reason (the 011 pattern): model-free
tests replay `reference/fixtures/008/chunk_{a,b}.json`, the CLI's eleven URL cases, the
iOS change-mark rule, the identity hash, the rules, the inputs and the renderer;
model-backed tests run `search` / `about` / `build` against the 40-document 007 fixture and
its goldens (`swift/Xtriever/Tests/Fixtures/`). Committed failing first.

**Target Platform**: host only (macOS arm64 where measured; Linux x86_64 by construction of
the wheel). No CI job (research D18; standing rule: nothing model-backed in CI).

**Project Type**: application (`apps/`), a console script.

**Performance Goals**: SC-001 — after warm-up on this laptop, fused list ≤ 1 s and re-ranked
list ≤ 3 s at depth 10 (measured before the spec: ~250 ms / ~0.8–1.0 s); SC-005 — a
2,000-article slice builds under 30 min (≈ 93 ms per passage, 008); SC-006 — first search
within 5 s of the command, a missing input reported within 1 s.

**Constraints**: no retrieval logic in the demo; every printed number the engine's or a
wall clock around one call; no network; no identifiers in records; no use of the Rust
embedding cache; the 008 schema (title 2.0 + text) so the Rust build is the oracle.

**Scale/Scope**: ~10 Python modules (~900 lines), ~9 test files (~550 lines), README,
two records, docs; two PRs (Rule 3, below).

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | PASS | Nothing retrieval-shaped is built: the demo calls the package; unit pricing uses the `tokenizers` library at the pinned version; the chunker is the 008 contract's reference implementation, fixture-tested (D5); argparse is the standard library. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | PASS | Tests first (quickstart Step 1, red on import); the chunker against `reference/fixtures/008/chunk_{a,b}.json` byte for byte; search output against the 007 goldens (bits); the shipped index against the host goldens (the device rule, 1e-3 / bit-exact lexical); the slice against the Rust build (ids, order, bits). Nothing ranking-affecting changes — no eval deltas arise; the gate is run unchanged. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only … `cargo check` passes on host, iOS, iOS-sim, Android (wasm32 best-effort). On-device RSS stays under the spec's ceiling … | PASS | No crate changes (`git diff --stat main -- crates/` empty, SC-007); the cross-target checks are unaffected. The laptop's peak resident size is recorded against the 600 MB figure for comparison, not claimed for the device (D12). |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec … Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | PASS | The demo's latency and footprint claims come from a committed record over the pinned queries, models and snapshot (`measure`); no engine performance claim is made, so no new `criterion` bench. `--explain` prints the eight features the engine reports. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits, the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR … | PASS | No trait, format, error or package-wire change; the demo reads the 008 sidecar and writes it in the same shape (a file 008's ADR-free data model already defines; D9). Dependencies point outward: `apps/` → the package. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results … | PASS | `--budget-ms` shows the engine's degradation as a report line, `--strict` surfaces its error (quickstart Step 3); determinism is what the host-goldens and slice checks verify. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned, `Cargo.lock` committed, dependencies added with `cargo add` … `cargo fmt --check`, `cargo clippy`, `cargo nextest run`, `cargo deny check` all pass … | PASS | No Rust touched; the four commands are run once to show the gate unchanged. The demo's Python dependencies are declared in its own manifest at versions read from the tree (`tokenizers==0.23.2` from `xtriever-dense/Cargo.toml`), never from memory. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | PASS | Package items from `crates/xtriever-ffi/src/ffi/types.rs` (`SearchOptions`, `Hit`, `ChunkInfo`, `HitExplain`, `StageReport`, `RerankReport`, `Degradation`, `IndexInfo`, `IndexConfig`, `FieldDef`, `FieldKind`, `FieldValue`, `Document`, `RerankMode`, `LoadPath`) and `python/README.md` (`IndexHandle.open/create/add/commit/merge/info/search`); `tokenizers` calls as `reference/gen_008_fixtures.py` makes them at the same pinned version; the Swift/Rust items ported are cited in research D3, D4, D6, D7, D9, D13. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | PASS | Neither is touched. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | PASS | Two PRs on this branch, both from this spec: **PR A** (search, about, the hit/mark/URL helpers, inputs, rendering, identity/record helpers, `measure` against the host goldens, tests, README, the host record — ~750 lines code+tests, plus the spec documents) and **PR B** (build: rules, chunker, the build command, `measure --against`, the slice record, the build tests, README's build section, the 009/root README pointers — ~600 lines). |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | PASS | Each PR opens with its tests red on import (quickstart Step 1). |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | PASS | Quickstart Step 6; the record's medians and the parity counts go into the PR; no eval deltas exist because nothing ranking-affecting changed, and the PR says so. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | PASS | A parity `FAIL` (host or slice), a chunker fixture mismatch or a fixture-golden mismatch stops the feature; tolerances are the device test's (1e-3) and bit-exactness where the device requires it. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | PASS | Plain functions and dataclasses; argparse; one module per concern; the chunker copied rather than abstracted; no plugin/registry/ABC. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-17, Claude (agent), all rows PASS.

**Post-design gate (post-Phase 1)**: PASS — 2026-09-17, Claude (agent); the design added
no crate, format or wire change; the only new third-party dependency is the demo's own
pinned `tokenizers`.

## Project Structure

### Documentation (this feature)

```text
specs/019-python-wiki-demo/
├── plan.md              # This file
├── research.md          # D1–D18
├── data-model.md        # inputs, search, build, about, measure entities
├── quickstart.md        # Steps 0–7
├── contracts/
│   ├── cli.md           # the four subcommands, flags, output, exit codes
│   └── records.md       # corpus.json / ATTRIBUTION.txt / wiki-build.json / the two run records
├── runs/                # the host measurement record, the slice parity record
├── report.md            # written by /speckit-implement
├── pr-description.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
apps/python-wiki-demo/
├── pyproject.toml            # name xtriever-wiki-demo; deps xtriever, tokenizers==0.23.2; [test] pytest; script wikidemo
├── README.md
├── .gitignore                # .venv/
├── wikidemo/
│   ├── __init__.py           # __version__, DEFAULT_DEPTH = 10, DEPTHS = (0, 5, 10, 20)
│   ├── __main__.py           # python -m wikidemo
│   ├── cli.py                # argparse: search | about | build | measure → the four run functions; exit codes
│   ├── inputs.py             # repo root, Paths (flag → env → default), missing-input checks and messages, snapshot verification
│   ├── hits.py               # title_and_passage, wikipedia_url, marks, features (eight names), DisplayedHit
│   ├── render.py             # the text layout of contracts/cli.md (open line, lists, report, wall/footprint, about)
│   ├── search.py             # open the artefact; the fused call, the re-ranked call; wall times; ru_maxrss
│   ├── about.py              # info() + corpus.json + ATTRIBUTION.txt → the About lines
│   ├── record.py             # canonical JSON, corpus identity, attribution text, RFC 3339 now, machine name
│   ├── measure.py            # goldens (file or live --against), the comparison (device rule), medians, the record
│   ├── rules.py              # PR B: the manifest's exclusion rules, first match wins, Rust names
│   ├── chunking.py           # PR B: the 008 contract chunker (from reference/gen_008_fixtures.py) + tokenizer pricing
│   └── build.py              # PR B: verify snapshot → read/exclude → chunk → add (4,096) → commit → merge → sidecars → rename
└── tests/
    ├── conftest.py           # REPO, fixture paths, models marker + skip-with-reason, bits helpers (the 011 pattern)
    ├── test_hits.py          # split, the eleven URL cases, marks (the iOS rule), features/"not seen"
    ├── test_record.py        # canonical json, identity == ea0fc78c… for the shipped basis, attribution text, no hostname
    ├── test_inputs.py        # defaults, overrides, missing-input messages name the producer
    ├── test_render.py        # the layout lines, snippet, degradation/skip lines, empty result
    ├── test_search.py        # [models] fused/re-ranked lists equal the 007 goldens' ids and bits; marks; exit codes
    ├── test_about.py         # [models] fields equal info()/sidecar; attribution verbatim
    ├── test_measure.py       # comparison rule on synthetic truths (incomplete, tolerance, order); [models] fixture goldens PASS
    ├── test_rules.py         # PR B: the three rules, char-window semantics, first-match counting
    ├── test_chunking.py      # PR B: chunk_a (48) and chunk_b (9 articles via unit_costs) byte-identical; title ≥ 256 error
    └── test_build.py         # PR B: [models] a 3-article JSONL with its own manifest → index, sidecar, attribution, search
```

Not touched: `crates/`, `swift/`, `python/src/`, `reference/`, any baseline. Touched
docs: `README.md` (root), `apps/ios-wiki-demo/README.md` (one pointer line),
`specs/009-ios-wiki-demo/{spec,report}.md` (pointer to the second demo).

**Structure Decision**: an application under `apps/` beside the iOS demo, consuming the
package as an installed dependency — the same relationship `apps/ios-wiki-demo` has to
`swift/Xtriever`. Dependencies point outward only (`apps/` → wheel); nothing in the
workspace depends on the demo.

## Complexity Tracking

None — no principle is violated and no ADR is required.
