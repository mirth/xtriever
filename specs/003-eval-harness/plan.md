# Implementation Plan: The Evaluation Harness

**Branch**: `003-eval-harness` | **Date**: 2026-09-12 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-eval-harness/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Implement `xtriever-eval`: a `std`-only library that loads the three BEIR datasets from a
hash-verified local cache, drives any `LexicalIndex` through a named evaluation configuration,
scores the retriever's ordered results with nDCG@10 and Recall@100 under exactly the conventions
BEIR's `pytrec_eval` wrapper uses, and produces a committed JSON report plus deltas. An example
binary (`beir`) wires it to `TantivyIndex` through a dev-dependency, records the lexical stage's
baseline on all three datasets against commit `94ddbe6`, and a new blocking CI job runs the SciFact
smoke on every change to a ranking crate. Every number this plan depends on — archive hashes, file
counts, `pytrec_eval`'s tie and zero rules, the published BM25 figures and their exact source — was
measured or read on 2026-09-12 and is recorded in [research.md](./research.md).

## Technical Context

**Language/Version**: Rust, edition 2024, toolchain 1.91.1

**Primary Dependencies**: library — `xtriever-core`, `serde`, `serde_json`, `sha2`; dev/example —
`xtriever-lexical` (constructs `TantivyIndex`), `anyhow`, `tempfile`. All via `cargo add`.
Python oracle — `pytrec_eval 0.5`, `numpy 2.5.3` in `reference/.venv-003` (research D9).

**Storage**: datasets in a git-ignored cache `reference/datasets/beir/` (23 MB for all three);
indexes in a temp directory per run; reports as committed JSON under
`specs/003-eval-harness/baselines/`.

**Testing**: `cargo nextest`; offline suite (metric goldens from `reference/fixtures/003/`,
manifest verification against synthetic files, delta/smoke logic, config → schema mapping) runs in
CI with no network; dataset-backed tests are `#[ignore]` and run where the cache exists;
property tests for the metric layer (permutation of irrelevant docs below the cutoff leaves nDCG@k
unchanged; recall is monotone in k; mean is order-independent).

**Target Platform**: host for everything; `cargo check` on the three mobile targets for the
library (Principle III); the example is host-only.

**Project Type**: library crate + example binary + shell script + CI job.

**Performance Goals**: none claimed. SC-008 *records* SciFact end-to-end time and treats > 1 min as
a finding. FiQA memory and index size are *observations* (FR-018), measured by `/usr/bin/time -l`
and `du`, not by a `criterion` bench.

**Constraints**: library `std`-only, no C/C++, no async, no threads of its own, no `Instant` in
library code; `xtriever-core`, `xtriever-lexical` and `deny.toml` untouched (FR-027);
dependencies downward (`eval → core`; example `eval → lexical → core`).

**Scale/Scope**: one crate (`xtriever-eval`), one example, one script, one CI job, one Python
generator, ~11 golden cases, three baseline reports. Four PRs.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.1.0, ratified 2026-09-10, amended 2026-09-11).
Every row below MUST be marked **PASS**, **FAIL**, or **N/A** with a one-line justification — an
empty verdict is a FAIL.

- Any **FAIL** blocks the phase. Do not proceed, and do not weaken the gate to make it pass.
- Any **N/A** MUST state why the principle cannot apply to this feature.
- A violation that is genuinely required is recorded in Complexity Tracking below and REQUIRES an
  accepted ADR in `docs/adr/`; the row stays **FAIL** until that ADR is merged.
- These are plan-time gates. They do not replace the blocking CI gates in the constitution's
  "Quality Gates (CI)" table.

### Core Principles

| # | Principle | Gate | Verdict | Justification |
|---|---|---|---|---|
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | No index, graph or runtime is built. The retriever under test is Feature 002's `TantivyIndex`, unchanged. nDCG/Recall are ~40 lines each and are *exactly* the kind of thing the constitution puts in the innovation budget ("evaluation"); a Rust trec_eval binding does not exist and a Python dependency in a `std`-only crate is not an option. The metrics are verified against `pytrec_eval` rather than trusted (II). |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | Tests-first: PR 1 is goldens + red suite. The reference is `pytrec_eval 0.5` behind BEIR's own wrapper (research D3), pinned in `requirements-003`, with 11 golden cases covering every FR-002 clause and a `--verify-run` cross-check of the *real* baseline (D4). Tolerance 1e-6 absolute (spec Assumptions). **This feature is the instrument the eval clause names**; it changes no ranking, so the delta line reads "establishes the baseline — ADR-0006 condition 2 discharged". The three named invariants are Feature 002's; this feature property-tests its own (metric permutation/monotonicity/order independence). |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 300 MB for a 100k-chunk index including loaded models). | **PASS** | `xtriever-eval` is one of the five named pure crates and the library graph is `core + serde + serde_json + sha2` — no threads, no async, no `Instant` (timing is done by the shell). The example binary's `tantivy` dependency is a **dev-dependency** and never enters the library graph (D6; quickstart Step 8 greps for it). Downloading is `curl` in a script. **RSS clause**: FR-018 *observes* FiQA memory on the host as the first data point on the deferred curve; no on-device configuration is claimed. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | This feature exists to make Principle IV true for ranking. No performance claim is made; SC-008 records a time. Reproducibility: datasets pinned by measured SHA-256 (archive and per-file, D1), oracle package pinned, published figures pinned to arXiv 2104.08663 Tables 2 and 9 with the text extracted (D5), mean computed in fixed order (D7). **`explain()` N/A**: no stage chain and no `explain` in the core trait; the per-query metric values in the report are the eval-side analogue. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | Core untouched (FR-027; quickstart Step 8 diff must be empty). The harness holds the external-id map itself and hands the backend dense `DocId(0..n)` in corpus order (D5, FR-014) — the pattern Principle V prescribes for the pipeline, exercised here first. Dependencies: library `eval → core`; example `eval → lexical → core`; nothing points upward. Synchronous throughout. The harness API is explicitly not yet stable (spec Assumptions); the stable contracts are the report format and the commands. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | Determinism: the retriever's order is Feature 002's (deterministic incl. the k-boundary); the harness never re-sorts it (D4); per-query values are summed in query-id order (FR-004); two runs must be byte-identical (SC-004). Hard errors, never silent: hash mismatch stops the load (FR-007). **Degradation N/A**: no ML stage. **Fingerprint N/A**: the harness opens no persistent index; each run builds a fresh one. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. `unsafe` only in `xtriever-dense` SIMD kernels, each block preceded by `// SAFETY:` and tested against the safe path. All public items documented (`missing_docs` on). | **PASS** | Deps via `cargo add` (D10); `sha2` already in the workspace. Library errors: a local `thiserror` enum; `anyhow` only in the example. No `unsafe`. `deny.toml` unchanged. One backend fact worth stating: the qrels files are **CRLF with a header** (D2) — a parser that forgets `\r` produces a grade of `"1\r"`; the loader strips it and the count assertions (FR-009) would catch a regression. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | The external dependencies here are data and a reference implementation, not a crate API, and both were **measured**: archives downloaded and hashed, files counted and inspected (D1, D2); `pytrec_eval 0.5` probed with a synthetic run for every convention the spec cares about (D3); BEIR's `evaluate()` read from `beir/retrieval/evaluation.py` (identical-id pop, mean over returned keys, 5-decimal rounding); the published BM25 figures read from the paper's extracted text, Tables 2 and 9, with the BM25 setup paragraph quoted (D5). The spec's "provisional" numbers turned out correct and are now pinned. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither is touched; nor is `xtriever-lexical` (FR-027). |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Four PRs (below), each under ~800 hand-written lines; the baseline JSONs carry per-query values (up to 648 rows) and are data. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | PR 1: generator, goldens, manifest, script, scaffold, every offline test red; dataset-backed tests `#[ignore]` but present. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 8 is the gate; the PR text carries the baseline table and the "establishes the baseline" line. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | Pre-committed stops: a `pytrec_eval` convention probe that disagrees with D3 makes the generator refuse (quickstart Step 2); an FR-020 band miss is a finding, never a wider band; a `--verify-run` disagreement on the real baseline stops the baseline from being committed. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | Plain functions over `&[&str]` and `BTreeMap`s; the runner takes `&dyn LexicalIndex`; downloading is a shell script; memory is measured by `time`. |

**Initial gate (pre-Phase 0)**: **PASS** — 2026-09-12, evaluated during `/speckit-plan`. No ADR
required: this feature changes no contract and no ranking.

**Post-design gate (post-Phase 1)**: **PASS** — 2026-09-12. Phase 1 introduced no violation. The
one design choice worth flagging under V is that the harness owns the external-id map (D5); it is
the pattern the constitution prescribes and will be lifted into `xtriever-pipeline` later, so it
is written as plain data (`Vec<String>`) rather than a type the pipeline would have to inherit.

## Project Structure

### Documentation (this feature)

```text
specs/003-eval-harness/
├── plan.md              # This file
├── research.md          # Phase 0 — D1–D10, measured pins, R1–R6
├── data-model.md        # Phase 1 — dataset, evaluation and fixture entities
├── quickstart.md        # Phase 1 — validation contract
├── contracts/
│   └── eval-harness.md  # Phase 1 — library surface, `beir` command, report format
├── baselines/           # written during implementation: lexical-baseline-v1.{scifact,nfcorpus,fiqa}.json
├── checklists/requirements.md
├── report.md            # written during implementation: baseline table, band verdicts, observations, findings
└── tasks.md             # /speckit-tasks — NOT created here
```

### Source Code (repository root)

```text
crates/xtriever-eval/
├── Cargo.toml                    # + serde, serde_json, sha2; dev: xtriever-lexical, anyhow, tempfile
├── src/
│   ├── lib.rs                    # crate docs; pub mod dataset, metrics, run, report; error
│   ├── error.rs                  # thiserror enum: Manifest, HashMismatch, Parse, Io, Core
│   ├── dataset.rs                # Manifest, hash verification, Corpus/QuerySet/Qrels loaders (CRLF, header)
│   ├── metrics.rs                # ndcg_at, recall_at over ordered ids (D3 rules)
│   ├── run.rs                    # EvalConfig, build() → Schema + Documents + IdMap, execute() over &dyn LexicalIndex, identical-id drop
│   └── report.rs                 # score(), EvalReport (stable JSON), delta(), smoke()
├── examples/
│   └── beir.rs                   # verify | run | delta | smoke — builds TantivyIndex (dev-dep)
└── tests/
    ├── support/mod.rs            # fixture loading; synthetic dataset writer for manifest tests
    ├── fixtures_valid.rs         # manifest hashes of reference/fixtures/003
    ├── metrics.rs                # every golden case exact to 1e-6 (US1)
    ├── metrics_prop.rs           # permutation / monotonicity / order-independence properties
    ├── dataset.rs                # synthetic files: CRLF qrels, header, hash mismatch names both hashes, counts (US2 offline)
    ├── dataset_real.rs           # #[ignore]: real cache — counts, integrity, FiQA empty titles, 55 id collisions (US2)
    ├── run.rs                    # config → schema, empty-title omission, id map round trip, identical-id drop (US3)
    ├── report.rs                 # stable JSON key order, delta table, ADR trigger, smoke pass/fail (US4)
    └── baseline_real.rs          # #[ignore]: SciFact end-to-end twice ⇒ identical reports; FR-020 band (US3)

reference/
├── requirements-003.{in,txt}     # pytrec_eval==0.5, numpy==2.5.3
├── gen_003_fixtures.py           # convention probe + 11 goldens + --verify-run
├── fixtures/003/{metrics.json,manifest.json}
└── datasets/
    ├── beir-manifest.json        # committed pins (D1)
    └── beir/                     # git-ignored cache

scripts/fetch-beir.sh             # curl + shasum + unzip, idempotent
.github/workflows/ci.yml          # + eval-smoke job (D8)
.gitignore                        # + reference/datasets/beir/
```

**Structure Decision**: one pure crate whose library depends on `xtriever-core` only; the only
place that names `TantivyIndex` is the example binary, through a dev-dependency, so
`cargo tree -p xtriever-eval -e normal` stays free of `tantivy`. Five small modules along the
spec's seams (dataset, metrics, run, report) plus an error type.

### PR split (Rule 3)

| PR | contents | ends at |
|---|---|---|
| 1 | `requirements-003`, generator + goldens + fixture manifest, `beir-manifest.json`, `fetch-beir.sh`, `.gitignore`, crate scaffold (`NotImplemented`), **all tests** (offline red, dataset ones `#[ignore]`) | red checkpoint |
| 2 | `error.rs`, `dataset.rs`, `metrics.rs` | US1 + US2 offline green; `dataset_real` green where the cache exists |
| 3 | `run.rs`, `report.rs`, `examples/beir.rs`; the three baseline reports; `--verify-run` cross-check; FR-020 verdicts; FiQA observations; `report.md` | US3 + US4 green |
| 4 | CI `eval-smoke` job; `check-no-stubs.sh` extended to `xtriever-eval`; docs | gate restored |

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

None — no row is FAIL or requires an ADR.
