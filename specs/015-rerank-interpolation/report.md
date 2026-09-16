# Report: Interpolated Re-ranking as the Default

**Feature**: `015-rerank-interpolation` | **Date**: 2026-09-16 | **Status**: done — the interpolating rule is the pipeline default (ADR-0012); `hybrid-rerank-v3` equals 014's cells list for list

## Verdict

The re-rank stage now orders its head by `0.5·minmax(fused) + 0.5·minmax(cross-encoder)`,
ties by fused rank, and records that in the descriptor; replace-order stays one option away
and bit-identical to Feature 006. The harness's `hybrid-rerank-v3` reproduces Feature 014's
`lin-0.5-d20` cells on all three sets — the same 300 / 323 / 648 lists, query for query —
so the engine's rule *is* the derivation that won under 014's fixed decision rule. Against the
replace-order default it retires: +2.5 / +0.1 / +1.7 nDCG@10 points, mean 0.4768 → 0.4913, at
the same 20 cross-encoder calls per query; against no re-ranking at all: +0.6 / +0.9 / +2.2.
Existing indexes read as the new default (no rebuild); the 007 parity goldens were regenerated
with every un-re-ranked response byte-identical.

## Baselines (nDCG@10 / Recall@100; every run `--verify-run` PASS)

| dataset | hybrid-rerank-v2 (replace, 013) | **hybrid-rerank-v3** (interpolate α 0.5) | 014 `lin-0.5-d20` cell | Δ v2 → v3 | Δ vs depth 0 |
|---|---|---|---|---|---|
| scifact | 0.695430 / 0.955000 | **0.720711** / 0.955000 | 0.720711 | **+0.025280** | +0.006342 |
| nfcorpus | 0.360854 / 0.321648 | **0.362246** / 0.321648 | 0.362246 | +0.001392 | +0.008736 |
| fiqa | 0.374214 / 0.707111 | **0.390964** / 0.707111 | 0.390964 | **+0.016750** | +0.021754 |
| **mean** | 0.476833 | **0.491307** | 0.491307 | **+0.014474** | +0.012277 |

Recall@100 unchanged in every cell (the head is re-ordered within the first 100).

## Exactness

| check | result |
|---|---|
| 006 `cases` golden after regeneration | byte-identical (`git diff` shows only `interpolate_cases`); `every_ordering_case_is_exact` green |
| `every_interpolation_case_is_exact` (11 cases from the extended 006 reference) | green; combined scores within 1e-12 |
| `hybrid-rerank-v3` vs `specs/014-rerank-depth-study/runs/lin-0.5-d20.<d>.json` | `check-cell: PASS (per-query metrics, lists)` on scifact, nfcorpus, fiqa — lists equal to the derived runs list for list |
| `hybrid-rerank-v2` on SciFact after the change | 0.695430 / 0.955000, `config` unchanged — the 013 baseline; the eval index directories carry no `rerank_mode` key, so this is also the "pre-015 index + `Replace` override" check |
| `descriptor_without_rerank_mode_reads_as_interpolate_half` / `descriptor_round_trips_rerank_mode` | green; `format_version` 2 |
| `replace_override_reproduces_the_006_order` (pipeline), `rerank_mode_defaults_and_override` (FFI, models), `test_rerank_mode_override_and_info` (Python, models) | green — the pre-015 golden order `d016 d011 d031 d026 d001 …` reproduced under `Replace` |

## The 007 parity goldens (`swift/Xtriever/Tests/Fixtures/expected.json`)

`rerank_study.py golden-diff`: **`without_reranker` identical (8/8)**; `with_reranker` changed:
7 (`q0 q1 q2 q3 q5 q6 q7` — `q4`'s head kept its order); `info` gains `rerank_mode` =
`{"interpolate": {"alpha": 0.5}}`; every hit gains `rerank_combined_bits` (`null` where not
re-ranked). The fixture index under `Tests/Fixtures/index/` was rebuilt by the same run.

## Success criteria

| criterion | result |
|---|---|
| SC-001 v3 = 014's cells to 1e-6 per query | **PASS** — and list for list |
| SC-002 v2 reproduces; 006 goldens pass; earlier baselines byte-identical | **PASS** |
| SC-003 Swift and Python suites pass on the regenerated goldens; only re-ranked heads changed | Python **PASS** (33/33 incl. models); Swift: the owner's simulator run (below) |
| SC-004 info reports the mode; `Replace` on a new index reproduces the v2 order | **PASS** (FFI and Python tests) |
| SC-005 cross-encoder calls unchanged | **PASS** — depth 20; the runs' `re-ranked N pairs` lines equal 013's (6,000 / 6,460 / 12,960) |
| SC-006 PR under ~800 lines excluding regenerated goldens and baselines | **over**: ≈ 1,180 lines (≈ 640 tests, ≈ 70 reference generator, ≈ 330 production code); the PR description states a two-PR split (pipeline + oracle + ADR ≈ 700; surfaces + goldens + baselines + docs ≈ 480) for the owner to take or decline |

## Review of what the numbers say

- **F-001 — NFCorpus gains least from the switch (+0.14 points over replace)** but most over
  depth 0 (+0.87): on NFCorpus the cross-encoder's order was already good; interpolation keeps
  what it had. FiQA is the opposite (+1.7 over replace): the fused signal matters most where
  the cross-encoder is least sure.
- **F-002 — The exactness held at the first attempt**: the engine's f64 min-max and
  `(1 − α)·f + α·c` reproduced the Python derivation on every query of every set — no
  rounding difference between the two implementations at the tie-relevant precision.
- **F-003 — A pre-existing wall-clock test fails on this machine today.**
  `xtriever-ffi`'s `a_short_time_budget_yields_a_partial_rerank_without_an_error` (model-backed,
  `#[ignore]`) asserts every query under a 200 ms budget returns in < 1,000 ms; today the first
  query takes ~3.2 s (`q0: 3233 ms`) — alone, uncontended, **and on `main`'s code** (checked by
  stashing this feature's changes and re-running). The laptop was also ~2× slower per
  cross-encoder pair than in 013 (289 vs 157 ms on SciFact). Not a 015 defect and not touched
  here; the other 16 model-backed FFI tests pass, including `rerank_mode_defaults_and_override`.
  Worth a look in a follow-up: the assertion measures the machine, not the contract.

## Review round 1 (Copilot, six comments, all taken)

1. The wiki demo's golden generator (`xtriever-cli/src/wiki/expected.rs`) omitted
   `rerank_combined_bits` per hit — added, so the device goldens carry the score that orders
   the head, as `fixture_index.rs` does.
2. The same file wrote the mode as a Rust `Debug` string — now the descriptor's JSON shape
   (`"replace"` / `{"interpolate": {"alpha": …}}`), identical to `fixture_index.rs`.
3. The harness serialised the mode in `RerankConfig` but the *report* never embedded the
   configuration, so the committed v3 baselines carried no mode. `StageInfo` gains
   `rerank_mode` (omitted when absent — the 003–014 baselines stay byte-identical, the
   round-trip test checks); the three v3 baselines were re-run so they are tool-produced.
4. A descriptor with `alpha: 2.0` opened (only `create` validated). `open` now validates the
   reconstructed mode → `Error::Corrupt("descriptor: rerank alpha …")`; test
   `a_descriptor_with_an_invalid_alpha_is_corrupt_at_open`.
5. `golden-diff` passed on any `with_reranker` change, so a tail regression would have passed
   as "heads changed". It now compares each response's tail after `rerank_depth` (ids and
   bits, on the old shape) and fails on a difference: **tails identical 8/8** — SC-003 is
   now verified, not asserted.
6. The Swift `features()` doc said seven names; now eight.

## Deliberately not done (research D10)

No conditional re-ranking policy; no other cross-encoder; no per-dataset α; no Wikipedia
rebuild or device record (memory and calls unchanged — the demo gains the new order on
upgrade); no LTR retraining.


## Red checkpoint (Rule 4, T007)

`cargo nextest run -p xtriever-pipeline -p xtriever-eval -p xtriever-ffi` does not compile (the
first errors per crate; each crate stops at its first missing item):

```text
error[E0432]: unresolved import `xtriever_pipeline::order_interpolated`      (rerank_golden.rs, rerank_prop.rs)
error[E0432]: unresolved import `xtriever_eval::run::RerankMode`
error[E0599]: no function or associated item named `hybrid_rerank_v3` found for struct `RerankConfig`
error[E0609]: no field `mode` on type `RerankConfig`
error[E0432]: unresolved import `xtriever_ffi::RerankMode`                    (options.rs, parity.rs)
```

`python/.venv/bin/pytest python/tests -q -m "not models"`: 3 failed — `test_search_options_defaults`,
`test_builder_defaults` (`AttributeError: rerank_mode`), `test_rerank_mode_surface`
(`AttributeError: RerankMode`).

The oracle was regenerated first: `reference/gen_006_fixtures.py --order-only` wrote
`interpolate_cases` (11 cases) into `reference/fixtures/006/pipeline_order.json` with the
`cases` array byte-identical (checked against `main`), and refreshed `manifest.json`.

Test files: `crates/xtriever-pipeline/tests/{rerank_golden,rerank_prop,persist,rerank,explain}.rs`,
`crates/xtriever-eval/tests/rerank_run.rs`, `crates/xtriever-ffi/tests/{options,parity}.rs`,
`python/tests/{test_surface,test_options,test_search}.py`,
`swift/Xtriever/Tests/XtrieverTests/{Support,InfoTests,ParityTests}.swift` (+ the eighth feature
name in `swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift`'s `features()`).

## Gate (Rule 5)

fmt ✓ · clippy workspace 0 warnings ✓ · nextest workspace 278/278 ✓ · deny ✓ · iOS / iOS-sim /
Android checks ✓ · `pytest reference/tests_014` 57/57 ✓ · wheel + `pytest python/tests` 33/33
(models) ✓ · FFI model-backed suite (`--run-ignored all`) 16/17 — the one failure is F-003, pre-existing on `main` · Swift suite on the
simulator — the owner's run · every earlier baseline and the 014 cells byte-identical
(`git diff --stat main -- specs/00{3,4,5,6}-* specs/013-*/baselines specs/014-*/runs` empty) ✓ ·
006 `cases` unchanged ✓ · no-stubs ✓ · no device / team identifiers ✓.

One consequence outside the eval crate: `crates/xtriever-cli/src/wiki/expected.rs` (the wiki
demo's golden generator) needed the new `rerank_mode: None` field and now records the mode in
its `info` — the 008 index's local goldens follow the engine's default.
