# Tasks: Re-rank Depth Study

**Input**: Design documents from `/specs/014-rerank-depth-study/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/study-cli.md](./contracts/study-cli.md),
[quickstart.md](./quickstart.md); the BEIR sets, the 004 vector cache, the re-rank model,
the 013 `hybrid-rerank-v2` baselines and exported runs (`target/xt-rr2-run.<d>.jsonl` — if
absent, T001 regenerates them), `reference/.venv-012`.

**Tests**: **Mandatory** (Principle II; spec FR-010). Phase 2 commits `reference/tests_014/`
failing (no `rerank_study` module); Phase 3 turns it green. One PR; commits: **C1** = Phases
1–2 (red), **C2** = Phases 3–4 (the flag, the script, the derivation checks), **C3** = Phases 5–7.

**Organization**: Setup → Red → US1 (the flag, the depth-50 runs, the replace derivation
and its checks) → US2 (the interpolation variants) → US3 (table, decision, report) → Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US3 from spec.md. Design facts are research D-numbers, data-model rows and
  the contract. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`crates/xtriever-eval/examples/beir.rs`; `reference/rerank_study.py`; `reference/tests_014/`;
runs under `target/xt-rerank-study/<d>/` (git-ignored); cells, table, decision and end-to-end
reports under `specs/014-rerank-depth-study/runs/`.

---

## Phase 1: Setup

- [X] T001 Confirm the inputs: `ls target/xt-dense-cache/{scifact,nfcorpus,fiqa}/index.bin target/xt-rerank-index-v2/{scifact,nfcorpus,fiqa} reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors reference/.venv-012/bin/pytest`; the 013 exported depth-20 runs `target/xt-rr2-run.{scifact,nfcorpus,fiqa}.jsonl` exist (else regenerate each with `beir run --config hybrid-rerank-v2 … --export-run`, ~50 min total, and confirm `--verify-run` PASS against `specs/013-lexical-quality/baselines/hybrid-rerank-v2.<d>.json`); `mkdir -p specs/014-rerank-depth-study/runs target/xt-rerank-study/{scifact,nfcorpus,fiqa}`; `hybrid-rerank-v2` on SciFact still reproduces its baseline (quickstart Step 4's last two lines)

---

## Phase 2: Foundational — the red tests

**Purpose**: `reference/tests_014/` importing a module that does not exist. **⛔ Commit C1.**

- [X] T002 [P] Write `reference/tests_014/conftest.py` (the 012 shape: `sys.path` insertion of `reference/`, a fixture building a small explain file — 3 queries, 8 candidates each with `fused`, `fused_scores`, `rerank` for the first 6, ids mapping to corpus positions via a fixture `corpus_ids` list — and a fixture with qrels) and `reference/tests_014/test_order.py`: `replace` at depths 5 / 10 / 20 / 50 over the fixture equals `gen_006_fixtures.order_reranked(fused, scores, d, k)` (ids as corpus positions), for every case in `gen_006_fixtures.ORDER_CASES`; a depth larger than the scored head uses only the scored entries; a depth of 0 returns the fused list; ties by ascending corpus position
- [X] T003 [P] Write `reference/tests_014/test_variants.py`: `rrf` — a hand-computed 4-candidate head (fused ranks 1–4, cross-encoder order 3, 1, 4, 2) gives the expected order under `1/(60+r_f) + 1/(60+r_c)`, tie by position; `lin` — for α = 0.5 on a head with fused scores `[0.03, 0.02, 0.01]` and cross-encoder scores `[−1, 3, 1]` the min-max normalisation and order are the hand-computed ones; α = 0 reproduces the fused order and α = 1 the cross-encoder order; a single-candidate head and a constant cross-encoder column keep the fused order (`minmax` of a constant column is all zeros); the rest of the list after the head is the fused order minus the head; every variant's output is ≤ `k` ids and contains no duplicates; `recall_100` of every variant equals depth 0's on the fixture (the set of the first 100 is unchanged)
- [X] T004 [P] Write `reference/tests_014/test_table.py`: `decide(rows)` under research D7 — a row with mean 0.4818 and no drop qualifies, 0.4817 does not, a drop of 0.0051 on one dataset disqualifies, ties by the smaller depth, no qualifying row → `winner is None` and the statement "no configuration qualifies; the default stays"; `derive` refuses an explain line without `fused_scores` (the contract's message); a JSONL run round-trips through `write_run` / `read_run`
- [X] T005 Run `reference/.venv-012/bin/python -m pytest reference/tests_014 -q` → `ModuleNotFoundError: No module named 'rerank_study'` (the red state); record it in `specs/014-rerank-depth-study/report.md` ("Red checkpoint"). **⛔ Commit C1**: the tests and the report stub

---

## Phase 3: User Story 1 — The depth sweep is measured (Priority: P1)

**Goal**: the `--rerank-depth` flag and the `fused_scores` key; the depth-50 runs and the
depth-5 check; the replace-order derivation at every depth, scored and verified.

**Independent Test**: quickstart Steps 2–3 for `replace` only — the depth-20 cells equal the
013 baselines to 1e-6; depth 5 on SciFact equals the end-to-end run list for list.

- [X] T006 [US1] In `crates/xtriever-eval/examples/beir.rs`: parse `--rerank-depth N` (`usize`); with a non-re-rank configuration bail `"--rerank-depth applies only to a re-rank configuration"`; with `Config::Rerank(cfg)` set `cfg.rerank_depth = N` and, when `N != RerankConfig::hybrid_rerank_v2().rerank_depth` (or v1's), set `cfg.name = format!("{}@d{N}", cfg.name)` before `validate()` (so `1 ≤ N ≤ k` still applies and the report's `config` carries the depth; contract); add `"fused_scores": fused.iter().map(|(s, _, _)| *s)` to the explain line beside `"fused"` (research D3; existing keys and order unchanged); update the usage doc comment; `cargo clippy -p xtriever-eval --all-targets -- -D warnings` clean; `beir run --config hybrid-rerank-v2 --rerank-depth 20 --dataset scifact …` reproduces the 013 SciFact baseline with `config == "hybrid-rerank-v2"` (no suffix at the constructor's depth)
- [X] T007 [US1] Run quickstart Step 2: depth 50 on scifact, nfcorpus, fiqa with `--out specs/014-rerank-depth-study/runs/e2e-d50.<d>.json --export-run target/xt-rerank-study/<d>/e2e-d50.jsonl --export-explain target/xt-rerank-study/<d>/explain-d50.jsonl` (`RAYON_NUM_THREADS=4`, ≈ 2 h), each `--verify-run` PASS; depth 5 on scifact → `runs/e2e-d5.scifact.json` + `target/xt-rerank-study/scifact/e2e-d5.jsonl`, verified; note each run's `re-ranked N pairs` line (50 or the candidate count per query) in the report
- [X] T008 [US1] Write `reference/rerank_study.py` (contract): `read_explain(path)` (refuses a line without `fused_scores`), `corpus_positions(dataset)` (external id → index from `corpus.jsonl` order), `head(line, d)`, `order_replace(head, rest, k)` (research D4 — `(−s_c, position)` then the fused rest, cut at `k = 100`), `derive(explain, variants, depths)` → runs, `write_run` / `read_run` (the harness JSONL shape), `score_run(dataset, run)` via `gen_003_fixtures.reference` (import as 012 did; `probe()` first), `check(dataset, runs, baseline_run, baseline_report, e2e_run, e2e_depth)` (list-for-list and per-query-metric comparisons, first mismatch printed, non-zero exit), the `derive | score | check | all` subcommands with required flags per the contract; `pytest reference/tests_014/test_order.py test_table.py::test_round_trip test_table.py::test_refuses_old_explain` green
- [X] T009 [US1] Run `rerank_study.py all` for `replace` on the three sets (quickstart Step 3, `--e2e-run … --e2e-depth 5` on scifact): cells `runs/replace-d{5,10,20,50}.<d>.json` written; `check` PASS on every dataset (derived `replace-d20` = `target/xt-rr2-run.<d>.jsonl` and the 013 report per query; `replace-d5` = the end-to-end SciFact run; `replace-d50` = the explain's own `hits`); `recall_100` of every cell = `hybrid-baseline-v2.<d>.json`'s. **⛔** Any mismatch is stop-and-report (research D2's fallback: run 5 / 10 / 20 end to end). **⛔ Commit C2** after Phase 4

---

## Phase 4: User Story 2 — The interpolation variant is measured (Priority: P1)

**Goal**: `rrf` and `lin-{0.25,0.5,0.75}` derived from the same explain files, scored.

**Independent Test**: the variant cells on the three sets, Recall@100 equal to depth 0, the
`test_variants.py` suite green.

- [X] T010 [US2] In `reference/rerank_study.py`: `order_rrf(head, rest, k)` (`1/(60 + r_f) + 1/(60 + r_c)`, `r_c` the 1-based rank by `(−s_c, position)`, order by `(−score, position)`), `minmax(values)` (all zeros for a constant column or a single value), `order_lin(head, rest, k, alpha)` (`s = (1 − α)·f + α·c`, order by `(−s, r_f)` — ties fall back to the fused order), variants registered as `rrf`, `lin-0.25`, `lin-0.5`, `lin-0.75`; `pytest reference/tests_014 -q` fully green
- [X] T011 [US2] Re-run `rerank_study.py all` on the three sets (seconds): cells `runs/{rrf,lin-0.25,lin-0.5,lin-0.75}-d{5,10,20,50}.<d>.json`; `recall_100` equal to depth 0 in every cell (**⛔** a difference is a defect); paste the per-dataset nDCG@10 of every variant × depth into the report. **⛔ Commit C2** (the flag, the script, the tests green, the cells and end-to-end reports under `runs/`)

---

## Phase 5: User Story 3 — The decision is recorded (Priority: P1)

- [X] T012 [US3] In `reference/rerank_study.py`: `table(runs_dir)` → `runs/table.json` and `runs/table.md` (rows = variant × depth plus `depth0` from `specs/013-lexical-quality/baselines/hybrid-baseline-v2.<d>.json`, columns per dataset `ndcg_10`, `recall_100`, `delta_vs_depth0`, `delta_vs_default` (default = `replace-d20`), `mean`, `calls_per_query = d`); `decide(rows)` under research D7 (`mean ≥ 0.4768 + 0.005` and no dataset `> 0.005` below depth 0; highest qualifying mean, ties by smaller `d`; none → the default stays) → `runs/decision.json` with `rule`, `qualifying`, `winner`, `statement`; the `table` and `decide` subcommands; run both
- [X] T013 [US3] Write `specs/014-rerank-depth-study/report.md`: verdict; the full table (from `runs/table.md`); the derivation checks (SC-001, SC-003 with the exact commands and PASS lines); the decision section (the rule verbatim, the qualifying set, the winner or "none", its cells, its cost in cross-encoder calls, the follow-up — the default constant in `crates/xtriever-pipeline/src/descriptor.rs:88`, the FFI default, the 007 / 011 goldens to re-check — or "nothing changes"); findings (what the sweep says about *where* the SciFact loss lives — head depth vs. replace-vs-inform; NFCorpus / FiQA behaviour across depths); "Deliberately not done" (research D9); the 013 F-001 / ADR-0011 cross-reference

---

## Phase 6: Polish

- [X] T014 Gate (quickstart Step 4): fmt; clippy workspace; `cargo nextest run --workspace`; deny; iOS / iOS-sim / Android checks; `pytest reference/tests_014 -q`; `git diff --stat main -- crates/ ':!crates/xtriever-eval' specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage specs/013-lexical-quality/baselines` empty (SC-005, FR-008); `hybrid-rerank-v2` without the flag reproduces its baseline (`config` unchanged); no device / team identifiers in the tree; `.gitignore` covers `target/` (nothing under `target/xt-rerank-study/` staged)
- [X] T015 Write `specs/014-rerank-depth-study/pr-description.md` (the question, the method in three lines — one depth-50 run per set, exact offline derivation, two end-to-end checks — the table, the decision under the fixed rule, what stays unchanged, the follow-up, the attribution line) and update `specs/013-lexical-quality/report.md` F-001 with "studied in 014: <decision>". **⛔ Commit C3**; the owner pushes and merges

---

## Dependencies & Execution Order

T001 → (T002 ‖ T003 ‖ T004) → T005 (C1) → T006 → T007 (the 2 h runs; T008 can be written
while they run) → T009 → T010 → T011 (C2) → T012 → T013 → T014 → T015 (C3).

### User story completion order

US1 → US2 → US3 (US2 derives from US1's explain files; US3 tabulates both).

### Parallel opportunities

T002 ‖ T003 ‖ T004 (three test files); T008 while T007 runs; T013's prose while T012's
table is produced.

## Implementation Strategy

**MVP** = Phases 1–3: the depth sweep for `replace` with its two exactness checks — if depth
alone recovers SciFact the decision could already be read. **Rule 6 stop-points**: T005
(red), T009 (derivation mismatch), T011 (a Recall@100 that differs), T014 (any gate failure
or a baseline that no longer reproduces). The decision rule is fixed (spec FR-006) and is not
edited after the numbers exist.
