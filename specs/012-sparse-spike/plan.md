# Implementation Plan: Sparse Expansion Spike

**Branch**: `012-sparse-spike` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/012-sparse-spike/spec.md`

## Summary

A Python spike, in the 001 mould, that decides on numbers whether learned sparse expansions
belong in the inverted index: two inference-free document encoders (OpenSearch doc-v2 /
doc-v3-distill, Apache-2.0, pinned by revision and hash), encoded over the three BEIR sets and
cached; the expansions scored as a dot product, as BM25 over an expansion field (alone and as
a boosted field beside text BM25), and RRF-fused with the engine's own exported lexical and
dense runs; everything scored by the 003 reference scorer, which must first reproduce the
engine's committed baselines; quantisation loss at three scales; throughput, non-zeros,
truncation and a Wikipedia projection; and a go / no-go by the rule the spec fixed in
advance. No Rust changes; nothing ships.

## Technical Context

**Language/Version**: Python 3.12 (arm64), `reference/.venv-012`; no Rust changes (the
harness is only *run*, with its existing `--export-run`).

**Primary Dependencies**: `torch 2.14.0`, `transformers 5.17.0`, `tokenizers 0.23.2`,
`safetensors 0.8.0`, `numpy 2.5.3` (the 004 pins, which resolve on this host), `pytrec_eval
0.5` (003), `scipy` and `snowballstemmer` (new, pinned by `pip-compile` into
`requirements-012.txt`), `huggingface-hub` (pinning only).

**Storage**: `target/xt-sparse-cache/` (encodings), `target/xt-sparse-runs/` (runs, reports);
committed: two manifests, `runs/summary.json`, `runs/costs-*.json`, the report.

**Testing**: the 003 scorer's convention probe; SC-001 (engine runs reproduce baselines to
1e-6) as a hard precondition; a doc-side recipe check against the model card's own example
(the card's sample sentence → the same top terms and weights within 1e-4); a unit check that
`bm25x` on a hand-made 3-document CSR matches a by-hand BM25.

**Target Platform**: this host only (macOS arm64, MPS or CPU); nothing in CI.

**Project Type**: research spike — scripts + report.

**Performance Goals**: SC-006 — under 3 hours per model for the three corpora including
encoding, and a second run reuses every encoding.

**Constraints**: models never committed; non-commercial models never run; the engine's
parameters only (BM25 k1/b, RRF k = 60, depth 100); nothing under `crates/` changes; CI
untouched (standing rule).

**Scale/Scope**: one script (~500 lines), two manifests, `requirements-012.{in,txt}`, the
report, two committed JSON summaries. Corpora: 5,183 + 3,633 + 57,638 documents; 300 + 323 +
648 judged queries.

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | The spike reuses published models, `transformers`, `pytrec_eval`, `scipy`; its small NumPy BM25 exists only to measure a variant (it is the *measurement* Principle I asks for before any engine decision), and the 003 scorer is imported, not re-implemented. No engine component is built. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | The oracle is the 003 reference scorer over the pinned BEIR sets; SC-001 (the engine's exported runs reproduce the committed baselines to 1e-6) is the spike's red-then-green test and runs before any variant; the encoder recipe is checked against the model card's own example. The spike *produces* the deltas 013 will be held to; it changes no ranking code. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | **N/A** | No crate is touched; nothing runs on a device. The spike's *reason* for the inference-free family is this principle (no query-time model on the phone) and the report records the query side's requirement (tokenizer + IDF table) for 013's memory case. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | Models pinned by revision hash and file sha256; datasets by the 003 manifest; every run is a file with a report and a name; throughput, non-zeros and truncation measured; the Wikipedia index size is a projection and is labelled as one with its method (D6). No `criterion` (no engine code). |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **N/A** | Nothing in the engine changes. The spike's report is the input to 013's ADRs (a format bump for the expansion field and the stage list amendment). |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | The spike's runs are deterministic (ties by ascending doc id, stable sorts, no sampling); its cache is keyed by model identity so a different revision can never be read as the same encoding. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **N/A** | No Rust is written. The Python pins follow the reference convention (`requirements-012.in` → `pip-compile`d `.txt`), versions taken from the resolver, never typed from memory. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | The model cards' recipes quoted in research D1 (activation, pooling, special-token zeroing, `idf.json` query weighting, inner-product score); `gen_003_fixtures.reference` / `run_scores` / `load_qrels_tsv` / `probe` (`reference/gen_003_fixtures.py:43–120, 197`); the harness export shape (`crates/xtriever-eval/src/run.rs:163`) and flags (`examples/beir.rs:10–13`); `scripts/fetch-model.sh:53–70` (generic file list). Python library calls (`AutoModelForMaskedLM`, `scipy.sparse.csr_matrix`, `pytrec_eval.RelevanceEvaluator`) are used exactly as the 003/004 references already use them. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Nothing under `crates/` or `deny.toml` changes (FR-012); the gate checks the diff. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | One branch; ~500 lines of script + manifests + report; one PR. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | The first tasks commit the scorer probe, the SC-001 export check and the recipe/BM25 unit checks (red: the script's subcommands do not exist), before the encoder and variants. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | The reduced gate (quickstart Step 4) plus the full tables in the report and PR; the eval *numbers* are the deliverable. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | The decision rule is fixed in the spec before the runs; a gap to the model card's numbers is a finding to explain, not a reason to change the recipe until it matches; SC-001's 1e-6 is the 003 tolerance. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | One script, subcommands, NumPy/SciPy arrays, JSON files; the 003 scorer imported. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-15, agent (III, V, VII N/A: no engine code).

**Post-design gate (post-Phase 1)**: PASS — 2026-09-15, agent (design in research D1–D7, data-model, contract; no row changed).

## Project Structure

### Documentation (this feature)

```text
specs/012-sparse-spike/
├── plan.md              # This file
├── research.md          # D1 models and recipes, D2 oracle, D3 environment, D4 storage, D5 variants, D6 costs, D7 not done
├── data-model.md        # manifests, sparse vectors, runs, reports, variants, costs, summary, decision
├── quickstart.md        # pins → SC-001 → SciFact smoke → the rest → summary → gate
├── contracts/spike-cli.md
├── runs/                # summary.json, costs-<model-key>.json (committed)
├── report.md            # the tables and the verdict
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
reference/
├── sparse_spike.py                  # pin | encode | export | score | all | summary (contract)
├── requirements-012.in / .txt       # torch, transformers, tokenizers, safetensors, numpy (004 pins); pytrec_eval (003); scipy, snowballstemmer, huggingface-hub
└── models/
    ├── manifest-sparse-doc-v2.json  # pinned revision + 4 file hashes (committed; weights git-ignored)
    └── manifest-sparse-doc-v3.json

target/xt-sparse-cache/<model-key>/<dataset>/docs-NNNNN.npz, queries.npz     # git-ignored
target/xt-sparse-runs/<dataset>/*.jsonl, *.json                                # git-ignored
```

**Structure Decision**: everything under `reference/` beside the earlier generators, in their
conventions (own venv, pinned requirements, manifests fetched by the existing script);
nothing under `crates/`, `swift/`, `apps/`, `python/`, `.github/`.

## Complexity Tracking

No violations.
