# Implementation Plan: The Re-rank Stage

**Branch**: `006-rerank-stage` | **Date**: 2026-09-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/006-rerank-stage/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Implement `xtriever-core`'s `Reranker` in `xtriever-rerank` with the pinned
`cross-encoder/ms-marco-MiniLM-L-6-v2` (revision `233902d2…`, weights `sha256:821d1aa6…`,
measured research D2) on candle 0.9.2: the encoder is candle-transformers' `BertModel`, the
classification head (CLS → pooler → tanh → linear) is composed from two `candle_nn::Linear`
layers because candle 0.9.2 ships no such head (D1). Each query–passage pair is scored alone at
its own length under `longest_first` truncation at 512 (D4/D5); `rerank` scores in input order
and stops at the item or time limit, returning `None` for the rest (D6). The pipeline gains a
passage text store (`passages.bin`, on-demand reads, pipeline format **v2**, ADR-0008 — every
hybrid index has it, user decision Q2 = A) and step 9 of `search`: the first *d* (default 20)
fused candidates are re-scored under the remaining budget; scored candidates come first by
re-rank score, every unscored candidate follows in fused order (Q1 = A); a re-ranker failure
degrades per stage, and explanations carry `rerank.score` / `rerank.rank` (D8). The harness
gains `hybrid-rerank-v1`, and the three datasets are measured as the first delta against the
guarded `hybrid-baseline-v1` (D10). Two governance items, both decided by the owner on
2026-09-13: the re-ranker offers the dense stage's two load paths, which required amending the
constitution to **v1.3.0** so the same read-only mapping block may live in `xtriever-rerank`
(**ADR-0009**, D3); and the pipeline's format version is bumped rather than made
backward-compatible (**ADR-0008**, D7).

## Technical Context

**Language/Version**: Rust, edition 2024, toolchain 1.91.1

**Primary Dependencies**: `xtriever-rerank` — `candle-core`, `candle-nn`, `candle-transformers`
**= 0.9.2** (ADR-0001 pin), `tokenizers 0.23.2` (`fancy-regex`), `serde`, `serde_json`, `sha2`,
`xtriever-core`, `memmap2` optional behind `mmap`; all already in `Cargo.lock`, added with
`cargo add`; dev: `serde_json`, `tempfile` (D3, D12). `xtriever-pipeline` — no new
dependency (takes `Box<dyn Reranker>`). `xtriever-eval` — dev-dependency on `xtriever-rerank`
for the example; library graph unchanged. Python — the 004 pins in `reference/.venv-004`.

**Storage**: model files under `reference/models/ms-marco-MiniLM-L-6-v2/` (git-ignored, fetched
by `scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json`, pinned by size and
SHA-256); pipeline directory gains `passages.bin` (data-model), format version 2; baselines under
`specs/006-rerank-stage/baselines/`.

**Testing**: `cargo nextest`; offline: fixture hashes, pins = manifest, the budget loop over a
stub scorer, the pipeline's ordering/degradation/explain/store suites with stub re-rankers over
the 005 fixture index, harness config and report round trips; model-backed (`#[ignore]`): load
verification, goldens (1e-3 + exact order + tokenization parity), determinism (three
arrangements, two thread counts cross-process), time budget, buffered-vs-mapped parity
(`--features mmap`), full pipeline round trip; ranking
via `xtriever-eval` on the three datasets with `--verify-run` and `--verify-rerank` (D13).

**Target Platform**: host for everything that runs; `cargo check` on the three mobile targets
(FR-025); wasm32 best-effort (unchanged failure point).

**Project Type**: one library crate implemented (`xtriever-rerank`), one extended
(`xtriever-pipeline`), harness + example additions, Python generator, ADR.

**Performance Goals**: none claimed. SC-010 records FiQA per-query and per-pair re-rank time,
pair count, peak RSS, directory size with the store, and the fresh-process model load per load
path (D11).
Planning estimate 50–100 ms per pair, 1–2 s per query at depth 20, ~40 min for the three
baselines at `RAYON_NUM_THREADS=4` (D9).

**Constraints**: `xtriever-rerank` may read `Instant` (leaf crate) and stays free of C/C++ in
every feature set; its one `unsafe` block is behind the non-default `mmap` feature (ADR-0009);
`xtriever-pipeline` stays pure — no clock, no `unsafe`, no
dependency on the re-rank crate; `xtriever-core`, `xtriever-lexical`, `xtriever-dense`, the 003
metric/dataset layers and `deny.toml` untouched (FR-022, SC-009); CI stays SciFact-only and
lexical (standing rule).

**Scale/Scope**: `xtriever-rerank` ~500 lines; pipeline +~450 (store ~180, search step 9 ~150,
types/descriptor/index ~120); harness +~200; example +~150; generator ~350; tests ~1,100; ADR,
manifest, fixtures, three baselines. **Over Rule 3's ~800 lines — four PRs** (see Rule 3 row).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.3.0, ratified 2026-09-10, last amended 2026-09-13).
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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | The encoder is candle-transformers' `BertModel`, tokenization is `tokenizers`, the head is two `candle_nn::Linear` layers and a `tanh` wired as the reference defines (D1) — model wiring, not a tensor runtime, exactly the 004 mean-pooling precedent. Nothing commodity is built; the passage store and ordering rule are pipeline orchestration, the constitution's named innovation budget. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | PR 1 is goldens + red suites. Scores are pinned to the HF sequence-classification pipeline under the 004 torch/transformers pins at 1e-3 with exact per-query order and per-pair tokenization parity, the generator refusing gaps below 10× tolerance (D14); the ordering rule has its own Python oracle (`pipeline_order.json`) and a real-data check (`--verify-rerank`). Ranking: three `hybrid-rerank-v1` baselines, `--verify-run`-checked, `compare` tables against `hybrid-baseline-v1` and the SC-008 verdict — the first delta against a guarded number. Properties: `rerank` output length and prefix shape; the response ordering invariant (scored prefix sorted, suffix in fused order, set-equal). |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 300 MB for a 100k-chunk index including loaded models). | **PASS** | `xtriever-rerank` is a leaf crate: pure-Rust candle/tokenizers (the dense crate's exact graph, `memmap2` optional), no C/C++ in any feature, `Instant` permitted there by the constitution's own text and used only in `rerank` (D6). `xtriever-pipeline` reads no clock — it hands the re-ranker the *remaining* time from its caller's source (D8) — and adds no dependency; the text store is read per hit, never held whole, so Q2 = A costs disk not RSS (D7). Three mobile targets checked (FR-025). **RSS clause**: FiQA numbers recorded (SC-010); no on-device claim. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | No performance claim; observations with method (D11). Reproducibility: model pinned by revision and three file hashes asserted at load; the identity string names every score-changing input (D2); datasets and the 004 cache pinned as before; `RAYON_NUM_THREADS` recorded. `explain()` gains `rerank.score` (the core's name) and `rerank.rank`, absent as `None`/`NaN` where the stage did not score a hit (D8). |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | Core untouched (FR-022); `Reranker` implemented as declared; no new `Error` variant (contract "Error mapping"). **The pipeline's on-disk format changes** (a new file, a descriptor field, version 1 → 2) — the sanctioned path is taken: **[ADR-0008](../../docs/adr/0008-pipeline-format-v2-passage-store.md)** (accepted by the owner on 2026-09-13; lands with the PR that bumps the version) records the decision and the rejected alternatives (D7). The re-ranker sees `DocId`s and text only. Direction: `core ← {lexical, dense, rerank}`; `pipeline` names no re-rank type (`Box<dyn Reranker>`); only the harness example depends on `xtriever-rerank` (D12). Synchronous throughout. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | **Partial degradation implemented for the first time**: a spent budget yields "not scored" per passage, and the pipeline keeps every candidate in a defined order (Q1 = A, D8); a re-ranker error or a budget spent at check point C degrades to the fused list in the default mode and errors in strict; degradation is per stage (a degraded dense stage still re-ranks). Determinism: one pair per forward at its own shape (D5), `(rerank_score DESC, DocId ASC)`, tests across reopening and thread counts. Format version 2 refused/accepted at open naming both versions; a torn store is `Corrupt` by count (D7). |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | Dependencies at the versions already locked, via `cargo add`; core `Error` for library errors; `anyhow` only in the example; **one `unsafe` block in `xtriever-rerank`**, `bytes::map_readonly`, admitted by constitution **v1.3.0** and [ADR-0009](../../docs/adr/0009-unsafe-readonly-mmap-in-rerank.md) — item-scoped `#[allow]`, `// SAFETY:` with the real invariant, behind the non-default `mmap` feature, tested bit-for-bit against the buffered path (D3); the default feature set compiles no `unsafe`; zero `unsafe` in `xtriever-pipeline`; `missing_docs` on; `deny.toml` unchanged. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | candle-transformers `BertModel::{load, forward}`, `Config`, `HiddenAct`; candle-nn `linear`, `Linear::forward`; candle-core `narrow`, `tanh`, `squeeze`, `to_scalar`; tokenizers `encode`, `EncodeInput::Dual` via `From<(I1, I2)>`, `TruncationParams`, `TruncationStrategy::LongestFirst`, `Encoding::get_*` — all cited by `crate-version/path:line` in research D1/D4; workspace items (`Reranker`, `Passage`, `Budget`, `features::RERANK_SCORE`, the pipeline's search and index functions, harness runner) by `crate/path:line`. The absence of a classification head in candle 0.9.2 was verified by reading the file, not assumed. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither is touched. The two places a core change would have helped — a `LexicalIndex` document-fetch method (avoiding the text store) and a `rerank.rank` feature name in core — are explicitly not done: the store is the pipeline's, the rank name is the pipeline's (D7, D8). |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Branch `006-rerank-stage`. **PR 1** manifest + fetch script, generator + goldens, `xtriever-rerank` scaffold (`NotImplemented`), red suites in all three crates, ADR-0008 (~1,000 incl. tests and fixtures); **PR 2** the cross-encoder — US1, US2 green (~500); **PR 3** passage store, format v2, search step 9, explain — US3, US4 green (~600); **PR 4** harness config, example, `model-memory --model rerank`, baselines, report — US5, US6 (~450). |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | PR 1 ends at the red checkpoint (quickstart Step 2) on `NotImplemented` scaffolds; `check-no-stubs.sh` extended to the new crate; model-backed suites separated by `#[ignore]` so the offline suite is red for the right reasons (FR-024). |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 7 plus the containment greps (exactly one `unsafe` in the re-rank crate's `bytes.rs`, none in the pipeline, no clock in the pipeline, no `xtriever-rerank` in the pipeline graph, FR-022 diff empty); the PR carries three baselines, three `compare` tables against `hybrid-baseline-v1`, the SC-008 verdict and the SC-010 observations. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | SC-008 (re-ranked nDCG@10 ≥ fused on ≥ 2 of 3) is ⛔ stop-and-report by the spec's own words; the 1e-3 tolerance is fixed and the generator enforces a 10× gap so exact order is fair; a thread-count bit-identity failure is a finding, not a widened tolerance (D5); Recall@100 must be byte-equal, checked not assumed. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | One concrete type per crate, a twelve-line head, a for-loop budget, a `BTreeMap` of pending texts and a whole-file rewrite, an `Option<Box<dyn Reranker>>` on the handle, one ordering function; no store abstraction, no async, no generics over the re-ranker. |

**Initial gate (pre-Phase 0)**: **PASS** on all 14 rows — 2026-09-13, Claude (agent). Two rows
passed only through governance the owner exercised the same day: Principle VII by the v1.3.0
amendment + ADR-0009 (the draft's FR-003 would have failed v1.2.0; the buffered-only
alternative was offered and declined, D3), and Principle V by ADR-0008 for the pipeline format
bump (the sanctioned path; both accepted the same day).

**Post-design gate (post-Phase 1)**: **PASS** on all 14 rows — 2026-09-13, Claude. Design
added the on-demand passage store (a second pipeline-owned file, versioned), the `max(k, d)`
candidate list, `RerankReport` on the stage report, the two optional report keys and the
`LoadPath`/`mmap` mirror of the dense crate; none touches a row. `/speckit-tasks` may run.

## Project Structure

### Documentation (this feature)

```text
specs/006-rerank-stage/
├── plan.md              # This file
├── research.md          # Phase 0 — D1–D14 + risks
├── data-model.md        # Phase 1 — pins, cross-encoder, goldens, passage store, format v2, options, response, harness
├── quickstart.md        # Phase 1 — Steps 0–8
├── contracts/
│   └── rerank-stage.md
├── baselines/           # PR 4 — hybrid-rerank-v1.{scifact,nfcorpus,fiqa}.json
├── checklists/requirements.md
├── tasks.md             # /speckit-tasks
└── report.md            # closing report
docs/adr/0008-pipeline-format-v2-passage-store.md   # Accepted by the owner 2026-09-13 (format v2, no migration)
docs/adr/0009-unsafe-readonly-mmap-in-rerank.md     # Accepted by the owner 2026-09-13 (constitution v1.3.0)
.specify/memory/constitution.md                     # v1.3.0 — Principle VII names xtriever-rerank; CLAUDE.md + plan template updated
```

### Source Code (repository root)

```text
crates/xtriever-rerank/
├── Cargo.toml                 # candle-{core,nn,transformers} = 0.9.2 (ADR-0001 comment), tokenizers 0.23.2 (fancy-regex), serde, serde_json, sha2, xtriever-core, memmap2 optional; [features] default = [], mmap = ["dep:memmap2"]
├── src/
│   ├── lib.rs                 # crate docs; pub use MiniLmCrossEncoder; pub mod model; LoadPath { Buffered, #[cfg(mmap)] Mmap }
│   ├── error.rs               # model_err() → core Error::Model { model: MODEL_NAME, .. }
│   ├── model.rs               # PINNED, MODEL_NAME, MODEL_ID (concat! of the same literals), verify_files
│   ├── bytes.rs               # Bytes { Owned, #[cfg(mmap)] Mapped }; read(path, LoadPath); map_readonly — the crate's one unsafe block (ADR-0009)
│   ├── scorer.rs              # MiniLmCrossEncoder: load(dir, LoadPath) (verify → config → tokenizer → header → weights via bytes::read → encoder + pooler + classifier), encode (single/pair rule), score, load_path, tokenize_for_test
│   └── budget.rs              # rerank_with(scorer: &dyn Fn(&str,&str)->Result<f32>, query, passages, budget) — the loop, testable without the model; the Reranker impl calls it
└── tests/
    ├── support/mod.rs         # fixtures dir, model dir, golden loaders (float_roundtrip)
    ├── fixtures_valid.rs      # manifest hashes of reference/fixtures/006/*
    ├── model_pins.rs          # PINNED = manifest-rerank.json; MODEL_ID = goldens' model_id and names every input
    ├── budget.rs              # stub scorer: item limits {0,1,half,all}, zero time limit, no budget, output length, prefix shape, first-error aborts
    ├── model_load.rs          # #[ignore]: tampered size / hash / config / header refused naming file and both values
    ├── score_golden.rs        # #[ignore]: tokenization parity, |Δ| ≤ 1e-3, per-query order exact, edge cases present (SC-001)
    ├── score_determinism.rs   # #[ignore]: three arrangements in-process; RAYON 1 vs 4 in a child process (SC-002)
    ├── budget_time.rs         # #[ignore]: 40 max-length passages under 100 ms ⇒ ≥ 1 and < 40 scored (SC-003)
    └── load_paths.rs          # #[ignore], cfg(feature = "mmap"): buffered vs mapped bit-identical over the goldens (ADR-0009 condition 3)

crates/xtriever-pipeline/
├── src/
│   ├── lib.rs                 # docs + FORMAT_VERSION = 2; pub use RerankReport, RERANK_RANK
│   ├── types.rs               # + HybridConfig.rerank_depth; SearchOptions.rerank_depth; HybridHit.{rerank_score,text}; HitExplain.{rerank_score,rerank_rank}; features() × 7; StageReport.rerank; RerankReport
│   ├── descriptor.rs          # + rerank_depth; version 2 message says "rebuild the index"
│   ├── passages.rs            # PassageStore: create/open (magic, header, offsets), read(id) on demand, stage/commit (tmp + rename)
│   ├── index.rs               # + passages field, reranker field, set_reranker/reranker, staging in stage_one/delete, commit order, fifth count
│   ├── search.rs              # + fused list to max(k, d); step 9: check point C, passages, remaining budget, validation, ordering, RerankReport; text on every hit
│   └── rerank.rs              # order_reranked(fused, scores, k) — the pure ordering rule, public for tests
└── tests/
    ├── support/mod.rs         # + TableReranker (scores by id, n cut-off), FailingReranker, WrongLengthReranker, NanReranker, CapturingReranker
    ├── rerank_golden.rs       # pipeline_order.json exact
    ├── rerank_prop.rs         # proptest: scored prefix sorted (score DESC, id ASC), suffix = fused minus scored, set-equal, len ≤ k
    ├── rerank.rs              # US3: depth d < len, m < d, none, d > k, d < k, depth 0 / no re-ranker ⇒ 005-equal, dense degraded + rerank runs, determinism
    ├── passages.rs            # store round trip; replace/delete; reopen; v1 directory refused naming 1 and 2; torn store by count; empty passage
    ├── degrade.rs             # + failing / wrong-length / NaN re-rankers in both modes; check point C; remaining time captured; time limit without clock
    ├── explain.rs             # + rerank fields exactly where scored; features() names; explain never changes hits
    └── model_roundtrip.rs     # + #[ignore]: real cross-encoder attached to the real embedder's index

crates/xtriever-eval/
├── src/run.rs                 # + RerankConfig, hybrid_rerank_v1
├── src/report.rs              # + StageInfo.{reranker_model_id, rerank_depth}; Observations.{rerank_ms, rerank_pairs, rerank_model_bytes_buffered}
├── examples/beir.rs           # + --config hybrid-rerank-v1, --rerank-model-dir, --load-path for both models, TimedReranker, model-memory --model rerank --load-path P, "rerank" in --export-explain
└── tests/rerank_run.rs        # config validation; report round trip with/without new keys; compare hybrid → rerank

reference/
├── models/manifest-rerank.json
├── gen_006_fixtures.py        # rerank.json, pipeline_order.json, manifest.json; --verify-scores, --verify-rerank
└── fixtures/006/{rerank,pipeline_order,manifest}.json

scripts/fetch-model.sh         # + --manifest FILE (destination from the manifest's local_dir)
.gitignore                     # + !reference/models/manifest-rerank.json (reference/models/* is ignored; the model files stay ignored)
scripts/check-no-stubs.sh      # + xtriever-rerank
.github/workflows/ci.yml       # eval-smoke path filter + crates/xtriever-rerank/** (SciFact lexical smoke only; no model)
```

**Structure Decision**: `xtriever-rerank` is implemented in place (the placeholder crate
exists) and depends on core only; `xtriever-pipeline` depends on core and the two 00x stage
crates as before and takes the re-ranker as `Box<dyn Reranker>`; only the harness example
names `MiniLmCrossEncoder`. Direction: `core ← {lexical, dense, rerank} ← pipeline ← example`.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| *(none)* | — | — | — | — |

The pipeline format bump is not a violation: Principle V permits an on-disk change with human
review plus an ADR, and ADR-0008 is that ADR.

## Decisions this plan takes that the spec left open (recorded for the human)

1. **Both load paths in `xtriever-rerank`, by constitution amendment** (D3) — the draft FR-003
   asked for them; v1.2.0 confined `unsafe` to `xtriever-dense`. The agent recommended
   buffered-only; **the owner chose the amendment** (v1.3.0, ADR-0009) for symmetry between the
   two model-loading crates. Decided 2026-09-13.
2. **Pipeline format version 2 with no migration** (D7, ADR-0008) — a version-1 directory is
   refused, not upgraded; the only such directories are throwaway.
3. **The re-ranker is attached to the handle, not stored in the directory** (D8) — any
   re-ranker can be attached to any index; the eval report records which one.
4. **`rerank_depth` lives in `HybridConfig`/descriptor (default 20) and is overridable per
   call** (D8) — the same shape as `candidate_depth`.
5. **The fused list is built to `max(k, d)`** (D8) — so re-ranking can promote candidates from
   just below `k`.
6. **`HybridHit.score` keeps the fused meaning; the re-rank score is a separate field** (D8) —
   nothing is overwritten; the LTR feature gets both.
7. **A partial re-rank is not a degradation and is never an error, even in strict mode** (D8) —
   it is the contract working; strict mode only turns *stage failures* and *spent budgets at the
   check point* into errors.
8. **A wrong-length or non-finite result is `Error::Model` in every mode** (D8) — a defect,
   not a stage failure.
9. **A time limit without a time source is not enforced by the re-ranker either** (D8) — the
   005 `time_limit_ignored` semantics extend to the new stage rather than contradict it.
10. **The empty-passage tokenization follows the reference's quirk** (single-sequence encoding,
    D4) rather than the pair template — FR-006 says the reference's score comes back.
11. **The passage store reads on demand and rewrites whole at commit** (D7) — RSS over commit
    speed, measured on FiQA.
12. **`tokenizer_config.json` is not pinned** (D2) — nothing reads it; its two values are
    asserted from files that are.
