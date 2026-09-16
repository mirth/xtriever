# Tasks: Interpolated Re-ranking as the Default

**Input**: Design documents from `/specs/015-rerank-interpolation/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/rerank-mode.md](./contracts/rerank-mode.md),
[quickstart.md](./quickstart.md); the BEIR sets, the 004 vector cache, the re-rank index
directories `target/xt-rerank-index-v2/<d>`, the two models, `reference/.venv-012`,
`python/.venv` (maturin, pytest), 014's cells and (where present) derived runs.

**Tests**: **Mandatory** (Principle II; spec FR-007). Phase 2 commits the oracles and tests
red; Phase 3 turns the pipeline green; Phases 4–5 the surfaces, goldens and baselines; Phase 6
the record. One PR; commits: **C1** = Phases 1–2 (red), **C2** = Phases 3–4 (pipeline, eval,
FFI/Python, regenerated goldens), **C3** = Phases 5–7 (baselines, ADR, docs, PR).

**Organization**: Setup → Red → US1 (the rule, the harness v3) → US2 (descriptor, options,
explain, FFI/Python, goldens) → US3 (ADR, docs, report) → Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US3 from spec.md. Design facts are research D-numbers, data-model rows and
  the contract. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`crates/xtriever-pipeline/{src,tests}`, `crates/xtriever-ffi/{src,tests,examples}`,
`crates/xtriever-eval/{src,examples,tests}`, `reference/gen_006_fixtures.py`,
`reference/rerank_study.py`, `reference/fixtures/006/`, `swift/Xtriever/Tests/`, `python/`,
`docs/adr/`, `specs/015-rerank-interpolation/{baselines,report.md,pr-description.md}`.

---

## Phase 1: Setup

- [ ] T001 Confirm the inputs and the pre-state: `ls target/xt-dense-cache/{scifact,nfcorpus,fiqa}/index.bin target/xt-rerank-index-v2/{scifact,nfcorpus,fiqa} reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors specs/014-rerank-depth-study/runs/lin-0.5-d20.{scifact,nfcorpus,fiqa}.json python/.venv/bin/maturin`; note whether `target/xt-rerank-study/<d>/lin-0.5-d20.jsonl` still exist (list-level checks); `git show main:swift/Xtriever/Tests/Fixtures/expected.json | jq '.queries | length'` = 8; `cargo nextest run -p xtriever-pipeline` green before any change; `mkdir -p specs/015-rerank-interpolation/baselines`

---

## Phase 2: Foundational — the oracles and the red tests

**Purpose**: The interpolation golden from the 006 reference and every test, failing.
**⛔ Commit C1 at the end of this phase.**

- [ ] T002 In `reference/gen_006_fixtures.py`: add `minmax(values)` (all zeros for a constant column or a single value), `order_interpolated(fused_ids, fused_scores, scores, d, k, alpha)` (research D1: `f = minmax(fused_scores of the scored head)`, `c = minmax(scores)`, `combined = (1 − alpha)·f + alpha·c` in that operation order, head sorted by `(−combined, fused position)`, tail in fused order, cut at `k`, returns `[[id, combined | None], …]`), `INTERPOLATE_CASES` (names: `both-present`, `single`, `constant-ce`, `constant-fused`, `ties-by-fused-rank`, `head-shorter-than-d`, `k-below-head`, `alpha-zero-is-fused`, `alpha-one-is-ce`, `partial-scores` — each with `fused: [[id, fused_score]…]`, `scores`, `d`, `k`, `alpha`), `gen_pipeline_order()` writing both `cases` (unchanged) and `interpolate_cases` with `expected`; a `--order-only` flag that writes `pipeline_order.json` and refreshes `manifest.json` without touching `rerank.json` (no torch needed); run `reference/.venv-012/bin/python reference/gen_006_fixtures.py --order-only`; `git diff reference/fixtures/006/pipeline_order.json` shows only the new key (the `cases` array byte-identical — ⛔ otherwise)
- [ ] T003 [P] Pipeline tests: `crates/xtriever-pipeline/tests/rerank_golden.rs` + `every_interpolation_case_is_exact` (reads `interpolate_cases`, calls `xtriever_pipeline::order_interpolated(&fused, &scores, k, alpha)`, compares ids and `combined` to the golden — f64 bit-equal or 1e-12); `tests/rerank_prop.rs` + properties for `order_interpolated`: output ≤ k, no duplicates, the tail is the fused order minus the head, α 0 gives the fused order of the head, α 1 gives `(−s_c, fused position)` order, a constant `s_c` column keeps the fused order; `tests/persist.rs` + `descriptor_round_trips_rerank_mode` (write → read → equal; the JSON key `"rerank_mode"` follows `"rerank_depth"`) and `descriptor_without_rerank_mode_reads_as_interpolate_half` (a v2 descriptor string without the key parses to `Interpolate { alpha: 0.5 }`); `tests/rerank.rs` + `search_orders_head_by_combined_score_by_default`, `replace_override_reproduces_the_006_order`, `invalid_alpha_is_a_schema_error_at_create_and_at_search` (α 1.5, −0.1, NaN); `tests/explain.rs` + `rerank_combined_is_reported_and_features_has_eight` (`features()[7].0 == "rerank.combined"`, `NaN` for un-re-ranked hits, `Some` only under `Interpolate`)
- [ ] T004 [P] Eval tests: `crates/xtriever-eval/tests/rerank_run.rs` + `hybrid_rerank_v3_is_v2_hybrid_at_depth_20_interpolated` (`RerankConfig::hybrid_rerank_v3()`: name `"hybrid-rerank-v3"`, `hybrid == HybridConfig::hybrid_baseline_v2()`, `rerank_depth == 20`, `mode == RerankMode::Interpolate { alpha: 0.5 }`; v1 and v2 have `mode == RerankMode::Replace`; the serialized configuration carries `mode`)
- [ ] T005 [P] FFI tests: `crates/xtriever-ffi/tests/options.rs` + `rerank_mode_defaults_and_override` (`IndexConfig.rerank_mode == None` by default; `IndexInfo.rerank_mode == RerankMode::Interpolate { alpha: 0.5 }` for the fixture index; a search with `SearchOptions { rerank_mode: Some(RerankMode::Replace), .. }` yields the pre-015 golden order for query 1 of `expected.json` as committed on `main` — embed the expected id list in the test; an α of 2.0 → `XtrieverError::Schema`); `tests/parity.rs` + explain feature names have eight entries ending `rerank.combined`
- [ ] T006 [P] Python tests: `python/tests/test_surface.py` + `xtriever.RerankMode.REPLACE()` / `.INTERPOLATE(alpha=0.5)` exist, `IndexConfig(...).rerank_mode is None`, `SearchOptions(k=1).rerank_mode is None`; `python/tests/test_options.py` (models) + `test_rerank_mode_override_and_info` (info reports `RerankMode.INTERPOLATE(alpha=0.5)`; `rerank_mode=xtriever.RerankMode.REPLACE()` reproduces the main-branch golden order for the first query; alpha 2.0 raises `XtrieverError.Schema`); `python/tests/test_search.py` feature-name assertion gains `"rerank.combined"`; Swift: `swift/Xtriever/Tests/XtrieverTests/InfoTests.swift` asserts `i.rerankMode == expected.info.rerankMode` (add `rerankMode` to `GoldenInfo` in `Support.swift`), `ParityTests.swift` feature list gains `"rerank.combined"`
- [ ] T007 Run `cargo nextest run -p xtriever-pipeline -p xtriever-eval -p xtriever-ffi` → does not compile (no `RerankMode`, `order_interpolated`, `rerank_combined`, `hybrid_rerank_v3`); `python/.venv/bin/pytest python/tests -q -m "not models"` → `AttributeError: RerankMode` / failing; record both in `specs/015-rerank-interpolation/report.md` ("Red checkpoint"). **⛔ Commit C1**: the generator, the regenerated `pipeline_order.json` + `manifest.json`, every test file, the report stub

---

## Phase 3: User Story 1 — The engine orders the head by the combined score (Priority: P1)

**Goal**: the rule in the pipeline, the default, the harness's `hybrid-rerank-v3`.

**Independent Test**: the golden and property tests green; `hybrid-rerank-v3` on SciFact
equal per query to 014's `lin-0.5-d20.scifact.json` (the three-set run is Phase 5).

- [ ] T008 [US1] In `crates/xtriever-pipeline/src/rerank.rs`: `pub enum RerankMode { Replace, Interpolate { alpha: f64 } }` (`Debug, Clone, Copy, PartialEq, Serialize, Deserialize`, `#[serde(rename_all = "snake_case")]`, `Default` = `Interpolate { alpha: 0.5 }`, `pub fn validate(&self) -> Result<()>` → `Error::Schema("rerank alpha must be within [0, 1], got …")` for non-finite or out-of-range α); `pub fn order_interpolated(fused, scores, k, alpha) -> Vec<(DocId, f64, Option<f32>, Option<f64>)>` exactly per research D1 (min-max in f64 with `(v − lo) / (hi − lo)`, zeros when `hi == lo`; `(1.0 − alpha) * f + alpha * c`; `sort_by` on `(−combined, fused position)` using `partial_cmp` with the `Equal` fallback — the values are finite by construction); `pub(crate) fn order_head(mode, fused, scores, k)` returning the four-tuple for both modes (`Replace` → `order_reranked` with `None` combined); module docs updated; re-export `RerankMode`, `order_interpolated` from `src/lib.rs`
- [ ] T009 [US1] In `crates/xtriever-pipeline/src/types.rs`: `HybridConfig.rerank_mode: RerankMode` (set in `new` to the default; `Debug` line in `index.rs:67`), `SearchOptions.rerank_mode: Option<RerankMode>` (+ `Debug`), `HitExplain.rerank_combined: Option<f64>` (+ `features()` → 8 with `pub const RERANK_COMBINED: &str = "rerank.combined"`), `Response::hits` doc for both orders; `src/descriptor.rs`: `rerank_mode: RerankMode` after `rerank_depth` with `#[serde(default)]` and a doc comment "Feature 015: absent in indexes written before it → the default (ADR-0012)"; `src/index.rs`: `validate` calls `rerank_mode.validate()`, config ↔ descriptor mapping both ways (`:135`, `:253`); `src/search.rs`: effective mode = `opts.rerank_mode.unwrap_or(self.config.rerank_mode)` validated when overridden, `order_head` in `rerank()` (`:308–315`), `rerank_combined` filled beside `rerank_score` / `rerank_rank` (`:200–212`); `cargo nextest run -p xtriever-pipeline` green (T003 included, the 006 `cases` still exact); `cargo clippy -p xtriever-pipeline --all-targets -- -D warnings`
- [ ] T010 [US1] In `crates/xtriever-eval/src/run.rs`: `pub enum RerankMode { Replace, Interpolate { alpha: f64 } }` (eval's own; serde, `PartialEq`), `RerankConfig.mode`, `hybrid_rerank_v1/v2` → `Replace`, `hybrid_rerank_v3()` per data-model (doc comment citing 014's cells); `examples/beir.rs`: dispatch `"hybrid-rerank-v3"`, map the eval mode to `xtriever_pipeline::RerankMode` in the `SearchOptions` of `evaluate_hybrid` (`:557` — `rerank_mode: rerank.map(|r| to_pipeline_mode(r.mode))`; the un-re-ranked explain search keeps `rerank_depth: Some(0)`), update the "known:" list and the usage comment; `cargo nextest run -p xtriever-eval` green (T004); `cargo clippy -p xtriever-eval --all-targets -- -D warnings`
- [ ] T011 [US1] SciFact first (≈ 15 min): `beir run --dataset scifact --config hybrid-rerank-v3 --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index-v2/scifact --rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2 --out specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.scifact.json --export-run target/xt-rr3-run.scifact.jsonl` → nDCG@10 0.720…; add to `reference/rerank_study.py` the `check-cell` subcommand (`--dataset --report --cell [--run --derived]`: `compare_metrics` per query to 1e-6 between the harness report and the 014 cell, `compare_runs` when both runs are given; PASS/FAIL, exit 1) and `golden-diff` (`--old --new`: `without_reranker` responses identical count, `with_reranker` changed count, `info` keys added); `pytest reference/tests_014` still green (add one test for `check-cell` on a synthetic pair); run `check-cell` on SciFact → PASS. **⛔** A mismatch is a defect in the engine's rule: stop, compare the first differing query's explain against `rerank_study.order_lin` on the same inputs, fix the rule — never the cell

---

## Phase 4: User Story 2 — Recorded, selectable, reported; the surfaces and goldens (Priority: P1)

**Goal**: the FFI and Python surfaces; regenerated 007 goldens; the suites green.

**Independent Test**: T005 / T006 green; `expected.json` regenerated with `without_reranker`
byte-identical; the Python suite (models) green; the Swift suite green on the simulator.

- [ ] T012 [US2] In `crates/xtriever-ffi/src/ffi/types.rs`: `#[derive(uniffi::Enum)] pub enum RerankMode { Replace, Interpolate { alpha: f64 } }` (doc: the default, the override, the reproducibility path); `IndexConfig.rerank_mode: Option<RerankMode>` `#[uniffi(default = None)]`, `SearchOptions.rerank_mode: Option<RerankMode>` `#[uniffi(default = None)]`, `IndexInfo.rerank_mode: RerankMode`, `HitExplain.rerank_combined: Option<f64>`; `src/index.rs`: `From` mappings both ways (`:216`, `:335`, `:374` and the explain mapping), `None` at build → the pipeline default; `src/ffi/mod.rs` exports; `cargo nextest run -p xtriever-ffi` green (T005; the parity goldens are regenerated in T013 — until then `parity.rs` may fail on `with_reranker` heads only, stated); `cargo clippy -p xtriever-ffi --all-targets -- -D warnings`
- [ ] T013 [US2] Regenerate the 007 goldens: in `crates/xtriever-ffi/examples/fixture_index.rs` add `"rerank_mode"` to `info_json` and `"rerank_combined"` to the explain JSON of `golden_response`; `cargo run --release -p xtriever-ffi --example fixture_index -- --out swift/Xtriever/Tests/Fixtures` (the fixture index is rebuilt too); `reference/.venv-012/bin/python reference/rerank_study.py golden-diff --old <(git show main:swift/Xtriever/Tests/Fixtures/expected.json) --new swift/Xtriever/Tests/Fixtures/expected.json` → `without_reranker identical (8/8)` (**⛔** otherwise), N `with_reranker` changed, `info` gains `rerank_mode`; paste the summary into the report; `cargo nextest run -p xtriever-ffi` fully green
- [ ] T014 [US2] Python: `python/src/xtriever/__init__.py` re-exports `RerankMode`; `python/README.md` search section (the default mode and α, `SearchOptions(rerank_mode=xtriever.RerankMode.REPLACE())` for the pre-015 order, the +1.45 mean points in one sentence, `explain.rerank_combined`); `(cd python && .venv/bin/maturin build) && uv pip install --force-reinstall target/wheels/xtriever-*.whl && python/.venv/bin/pytest python/tests -q` → all green including `models`
- [ ] T015 [US2] Swift: `scripts/build-ios-package.sh`; the owner runs `xcodebuild test -scheme Xtriever -destination 'platform=iOS Simulator,id=XXXXX' -configuration Release ARCHS=arm64 -skip-testing:XtrieverTests/DeviceMeasurementTests` (the simulator id only on the command line) → green with the regenerated goldens; the count of tests pasted into the report. **⛔ Commit C2** (Phases 3–4: pipeline, eval, FFI, Python, Swift tests, `expected.json`, the SciFact v3 baseline, `rerank_study.py`)

---

## Phase 5: User Story 1 (continued) — The three-set baselines

- [ ] T016 [US1] Run `hybrid-rerank-v3` on nfcorpus and fiqa (quickstart Step 4, ≈ 35 min) with `--export-run`; `--verify-run` PASS each; `check-cell` against `specs/014-rerank-depth-study/runs/lin-0.5-d20.<d>.json` (and `--derived target/xt-rerank-study/<d>/lin-0.5-d20.jsonl` where present) → PASS; `beir compare specs/013-lexical-quality/baselines/hybrid-rerank-v2.<d>.json specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.<d>.json` for the three deltas; `hybrid-rerank-v2` on SciFact re-run → 0.695430 / 0.955000 (`config` unchanged) (**⛔** otherwise); the eval index directories were written before 015 and carry no `rerank_mode` key, so the v2 run above *is* the "index without the key + `Replace` override" check — state it in the report

---

## Phase 6: User Story 3 — Documentation and the record (Priority: P2)

- [ ] T017 [P] [US3] Write `docs/adr/0012-interpolated-rerank-default.md` (Accepted; deciders: the owner, Q1 = A; context: 013 F-001 / ADR-0011 and 014's table; decision: the rule, α 0.5, depth 20, the descriptor key with the missing-key semantics, format version unchanged, replace-order kept under a mode; consequences: existing indexes change their head order on upgrade, reproducibility through the override, the regenerated 007 goldens, `hybrid-rerank-v3` as the re-rank baseline from here on; revisit when the re-ranker is re-pinned or α is tuned); add an "Outcome" note to `docs/adr/0011-rerank-v2-scifact-regression.md`
- [ ] T018 [P] [US3] Docs: `crates/xtriever-pipeline/src/lib.rs` crate docs (the two rules, the default, the override, the descriptor key), `crates/xtriever-eval/src/lib.rs` (v3), `specs/014-rerank-depth-study/report.md` decision section + "landed in 015", `specs/013-lexical-quality/report.md` F-001 + "resolved by 015"
- [ ] T019 [US3] Write `specs/015-rerank-interpolation/report.md`: verdict; the baselines table (v2 → v3 per dataset with 014's cells beside them, all equal); the exactness checks (per query, lists where available); the goldens' diff summary; SC-001–SC-006 with numbers; findings; "Deliberately not done" (research D10)

---

## Phase 7: Polish

- [ ] T020 Gate (quickstart Step 5): fmt; clippy workspace (host + `x86_64-pc-windows-msvc`); `cargo nextest run --workspace`; deny; iOS / iOS-sim / Android checks; `pytest reference/tests_014`; the wheel + `pytest python/tests`; `git diff --stat main -- specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage specs/013-lexical-quality/baselines specs/014-rerank-depth-study/runs` empty; `git diff main -- reference/fixtures/006/pipeline_order.json` adds only `interpolate_cases`; no-stubs; no identifiers in the tree; changed-line count excluding `expected.json`, `pipeline_order.json`, `manifest.json` and `baselines/` under ~800 (or the split stated)
- [ ] T021 Write `specs/015-rerank-interpolation/pr-description.md` (the rule in two lines, the numbers v2 → v3 with 014's cells, what a caller sees — default, override, explain — the descriptor semantics and ADR-0012, the goldens' diff summary, what stays unchanged, the gate, the attribution line). **⛔ Commit C3**; the owner pushes and merges

---

## Dependencies & Execution Order

T001 → T002 → (T003 ‖ T004 ‖ T005 ‖ T006) → T007 (C1) → T008 → T009 → T010 → T011 → T012 →
T013 → T014 → T015 (C2) → T016 → (T017 ‖ T018) → T019 → T020 → T021 (C3).

### User story completion order

US1 (rule + SciFact) → US2 (surfaces, goldens) → US1 (the remaining baselines, which only
need the pipeline) → US3.

### Parallel opportunities

T003 ‖ T004 ‖ T005 ‖ T006 (four crates / packages); T017 ‖ T018; T016's 35-minute runs can
start as soon as T010 is green and run while T012–T015 proceed.

## Implementation Strategy

**MVP** = Phases 1–3: the rule, the default, the harness, SciFact equal to 014's cell.
**Rule 6 stop-points**: T002 (the 006 `cases` array changes), T007 (red), T011 (a cell
mismatch — fix the rule, never the cell), T013 (a `without_reranker` golden changes), T016
(a v2 baseline that no longer reproduces), T020 (any gate failure).
