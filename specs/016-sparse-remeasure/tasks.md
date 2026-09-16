# Tasks: Sparse Stage Re-measurement

**Input**: Design documents from `/specs/016-sparse-remeasure/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/remeasure-cli.md](./contracts/remeasure-cli.md),
[quickstart.md](./quickstart.md); on disk: `target/xt-rerank-study/<d>/explain-d50.jsonl`,
`target/xt-sparse-runs/<d>/dot@opensearch-neural-sparse-encoding-doc-v3-distill@babf71f3.jsonl`,
`target/xt-rr3-run.<d>.jsonl`, the BEIR sets, `reference/models/ms-marco-MiniLM-L-6-v2`,
`reference/.venv-012`.

**Tests**: **Mandatory** (Principle II; spec FR-008). Phase 2 commits `reference/tests_016/`
failing; Phase 3 turns it green with the two reproduction checks on the real anchors. One PR;
commits: **C1** = Phases 1–2 (red), **C2** = Phases 3–4 (the script, the cells), **C3** = Phases 5–6.

**Organization**: Setup → Red → US1 (fusion + plain cells + anchor check) → US2 (re-ranking +
rr cells + anchor check) → US3 (table, decision, report) → Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US3 from spec.md. Design facts are research D-numbers, data-model rows and
  the contract. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`reference/sparse_remeasure.py`; `reference/tests_016/`; runs under
`target/xt-sparse-remeasure/<d>/` (git-ignored); cells, table and decision under
`specs/016-sparse-remeasure/runs/`; the report and PR description beside them.

---

## Phase 1: Setup

- [X] T001 Confirm the inputs: `ls target/xt-rerank-study/{scifact,nfcorpus,fiqa}/explain-d50.jsonl target/xt-sparse-runs/{scifact,nfcorpus,fiqa}/dot@opensearch-neural-sparse-encoding-doc-v3-distill@babf71f3.jsonl target/xt-rr3-run.{scifact,nfcorpus,fiqa}.jsonl reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors`; every explain line has `fused_scores` and 50 `rerank` entries (`python -c` one-liner over the three files: min/max `len(rerank)`); the 013 `target/xt-lex2-run.<d>.jsonl` lists equal the explain's `lexical` lists (one-off check, result noted for research D1); `reference/.venv-012/bin/python -c "import torch, transformers"`; `mkdir -p specs/016-sparse-remeasure/runs target/xt-sparse-remeasure/{scifact,nfcorpus,fiqa}`

---

## Phase 2: Foundational — the red tests

**Purpose**: `reference/tests_016/` importing a module that does not exist. **⛔ Commit C1.**

- [X] T002 [P] Write `reference/tests_016/conftest.py` (the 014 shape: `sys.path` insertion; fixtures — `positions` for ids `d0…d9` = index, two synthetic candidate lists `lex` / `dense` of 6 ids with ranks, a `dot` list of 6 ids sharing 3 with them, a synthetic explain line with `fused`, `fused_scores`, `rerank` scores for its first 4 fused ids, a `qrels` dict, a `StubScorer` whose `score(query, passage)` returns a deterministic value from the doc id) and `reference/tests_016/test_fuse.py`: `fuse([lex, dense], positions)` gives the hand-computed RRF sums (`1/61 + 1/62` for an id at ranks 1 and 2, …) and order; an id in one list only carries one term; a tie (two ids with the same sum) is broken by **corpus position, not id string** (choose ids `d10`-like vs `d9` so string order and position order disagree); three lists add a third term; output ≤ 100; the returned pairs carry `(id, fused_score)`
- [X] T003 [P] Write `reference/tests_016/test_scores.py`: `head_scores(fused, explain_line, reference_cache, scorer)` — for the first 20 fused ids: covered pairs take the explain `rerank` score and `source == "engine"`, uncovered take the scorer's value, `source == "reference"`, and are written to the cache; a second call reads the cache and calls the scorer zero times; `rerank_fused(fused, scores)` orders by `rerank_study.order_lin` at α 0.5, depth 20, and its tail equals the fused order minus the head; the reference share = uncovered / head pairs
- [X] T004 [P] Write `reference/tests_016/test_decide.py`: the FR-007 rule on the `lex2+dense+dot-rr` row — mean 0.4963 with no dataset more than 0.005 below the v3 anchors qualifies, 0.4962 does not, a drop of 0.0051 on one dataset disqualifies; `qualifies == False` yields the statement `"012's GO withdrawn"` and the `reopen` list is non-empty either way; the anchors are read from the 015 baselines' `mean_ndcg_10` (pass a dict in the test)
- [X] T005 [P] Write `reference/tests_016/test_end_to_end.py`: from the conftest lists → `fuse` → `head_scores` with the stub → `rerank_fused` → `write_run` / `read_run` (reuse `rerank_study`'s) round trip; Recall@100 of the re-ranked run equals the plain run's under the `qrels` fixture via `gen_003_fixtures.reference`; the anchor variant (`lex + dense`, no dot) re-ranked with only covered scores equals `order_lin` applied directly to the explain line (the FR-003 shape on synthetic data)
- [X] T006 Run `reference/.venv-012/bin/python -m pytest reference/tests_016 -q` → `ModuleNotFoundError: No module named 'sparse_remeasure'`; record it in `specs/016-sparse-remeasure/report.md` ("Red checkpoint"). **⛔ Commit C1**: the tests and the report stub

---

## Phase 3: User Story 1 — The un-re-ranked fusion on the current lists (Priority: P1)

**Goal**: `fuse`, the four plain runs and cells per dataset, the `hybrid-baseline-v2` reproduction.

**Independent Test**: `check` PASS on the plain half; the three plain variants' cells with deltas.

- [X] T007 [US1] Write `reference/sparse_remeasure.py` — module header (the study, the inputs, the rule), constants (paths from the contract, `RRF_K = 60`, `DEPTH = 100`, `RERANK_DEPTH = 20`, `ALPHA = 0.5`, anchors), `load_explain(dataset)` (refuses a file lacking `fused_scores` or with fewer than 50 `rerank` entries on any query with ≥ 50 fused), `load_dot_run(dataset)`, `positions = rerank_study.corpus_positions`, `fuse(lists, positions, k=RRF_K, depth=DEPTH)` (research D2: term `1/(k + rank)`, 1-based rank, summed per id, sorted by `(−sum, position)`, cut at `depth`, returns `[(id, score)]`), `VARIANTS = {"lex2+dense": ("lexical","dense"), "lex2+dense+dot": ("lexical","dense","dot"), "dense+dot": ("dense","dot"), "lex2+dot": ("lexical","dot")}`, `cmd_fuse` writing `target/xt-sparse-remeasure/<d>/<variant>-plain.jsonl` (ids) and `<variant>-plain.fused.json` (ids with scores, for the re-rank step); `pytest reference/tests_016/test_fuse.py` green
- [X] T008 [US1] Add `cmd_score` (the 003 scorer; `probe()` first; cells `specs/016-sparse-remeasure/runs/<variant>-plain.<d>.json` in the data-model shape) and the plain half of `cmd_check`: `lex2+dense-plain` list equals the explain's `fused` per query (exact) and its cell equals `specs/013-lexical-quality/baselines/hybrid-baseline-v2.<d>.json` per query to 1e-6 (`rerank_study.compare_runs` / `compare_metrics`); run `fuse`, `score`, `check` for the three datasets → **`check <d>: PASS`** (**⛔** otherwise: the fusion code, not the anchor); paste the plain cells into the report with deltas against `hybrid-baseline-v2` and 012's `rrf-lex+dense+dot` (0.7139 / 0.3508 / 0.3881)

---

## Phase 4: User Story 2 — The re-ranked fusion against the current default (Priority: P1)

**Goal**: `head_scores` with the reference for uncovered pairs, the four rr runs and cells, the `hybrid-rerank-v3` reproduction, the agreement figures.

**Independent Test**: `check` PASS on the rr half with 0 reference-scored pairs for the anchor variant; the rr cells with counts and agreement.

- [X] T009 [US2] Add to `reference/sparse_remeasure.py`: `Passages(dataset)` (corpus id → `title + " " + text`, empty side omitted — the pipeline's passage), `ReferenceScores(dataset, model_dir)` (the 006 `Reference` loaded lazily on first uncovered pair; the cache `target/xt-sparse-remeasure/<d>/reference-scores.jsonl` `{query_id, doc_id, score}` read at start and appended per new pair), `head_scores(fused, explain_line, ref, query_text)` (research D4: engine score when `(query, id)` is in the explain `rerank` map else reference; returns `[(id, score, source)]` for the first `RERANK_DEPTH` fused ids), `rerank_fused(fused, head)` (build `rerank_study` head dicts `{id, pos, r_f, s_f, s_c}` and call `order_lin(head, rest, 100, ALPHA)`), `agreement(dataset, ref, n=200)` (the first 200 covered pairs in query order re-scored by the reference: `max_abs_diff`, `mean_abs_diff`), `cmd_rerank` (per variant: runs `<variant>-rr.jsonl`, per-dataset counts printed: head pairs, engine-covered, reference-scored, share) and the rr half of `cmd_score` (cells gain `reference_scored_pairs`, `reference_share`, `agreement`); `pytest reference/tests_016 -q` fully green
- [X] T010 [US2] Add the rr half of `cmd_check`: `lex2+dense-rr` equals `target/xt-rr3-run.<d>.jsonl` list for list and `specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.<d>.json` per query to 1e-6 **with 0 reference-scored pairs**; every `rr` cell's Recall@100 equals its `plain` cell's; run `rerank`, `score`, `check` for the three datasets (the reference scoring is the wall time) → **`check <d>: PASS`** (**⛔** otherwise: the re-rank code or the score sourcing, never the anchor); paste the rr cells, the counts/shares and the agreement figures into the report with deltas against `hybrid-rerank-v3`

---

## Phase 5: User Story 3 — The decision is recorded (Priority: P1)

- [X] T011 [US3] Add `cmd_table` (`runs/table.json`, `runs/table.md`: rows variant × plain/rr, per-dataset nDCG@10 with Δ against the matching anchor — plain vs `hybrid-baseline-v2`, rr vs `hybrid-rerank-v3` — the mean, the reference share for rr rows; anchor rows and 012's three-way row included) and `cmd_decide` (research D6: the `lex2+dense+dot-rr` row; `qualifies` iff mean ≥ 0.4913 + 0.005 and every dataset ≥ v3 − 0.005; `runs/decision.json` with `rule`, `row`, `qualifies`, `statement`, `reopen` — the reopening conditions: a corpus with heavy vocabulary mismatch (FiQA-shaped) where a dense-free configuration is wanted, a cheaper encoder or one runnable in Rust, a measured need at the head that the cross-encoder does not cover); run both
- [X] T012 [US3] Write `specs/016-sparse-remeasure/report.md`: verdict; the table; the reproduction checks (both anchors, per query and lists, 0 reference pairs on the anchor); the reference's contribution (counts, shares, agreement per dataset) and what it bounds; the decision (rule verbatim, the row's cells, outcome); findings (the pairings — where `dense+dot` or `lex2+dot` beats the three-way, and what that says; how much of 012's +1.6 the 013/015 changes absorbed; Recall@100 of the three-way fusion vs v2 — the candidate-set effect separate from the ordering effect); "Deliberately not done" (research D8); pointers written into `specs/012-sparse-spike/report.md` ("GO status after 016: …" under its verdict) and `specs/015-rerank-interpolation/report.md` F-005-style note if that report has one

---

## Phase 6: Polish

- [X] T013 Gate (quickstart Step 4): `git diff --stat main -- crates/ specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage specs/013-lexical-quality/baselines specs/014-rerank-depth-study/runs specs/015-rerank-interpolation/baselines` empty; the Rust gate unchanged (fmt, clippy, nextest, deny — nothing under `crates/` changed, run once to confirm); `pytest reference/tests_016 -q` and `pytest reference/tests_014 -q` green (separate runs — `tests_014` imports its `conftest` by module name, which a combined collection shadows); no identifiers in the tree; nothing under `target/` staged
- [X] T014 Write `specs/016-sparse-remeasure/pr-description.md` (the question, the method in three lines, the table, the reference's share and agreement, the decision under the fixed rule and what it means for the roadmap, what is unchanged, the attribution line). **⛔ Commit C3**; the owner pushes and merges

---

## Dependencies & Execution Order

T001 → (T002 ‖ T003 ‖ T004 ‖ T005) → T006 (C1) → T007 → T008 → T009 → T010 (C2) → T011 →
T012 → T013 → T014 (C3).

### User story completion order

US1 → US2 (US2 re-ranks US1's fused runs) → US3.

### Parallel opportunities

T002 ‖ T003 ‖ T004 ‖ T005 (four test files); T011's table printer can be written while T010's
reference scoring runs.

## Implementation Strategy

**MVP** = Phases 1–3: the fusion with its exact reproduction of `hybrid-baseline-v2` and the
plain three-way cells — already enough to see whether the sparse list still changes the
candidate set. **Rule 6 stop-points**: T006 (red), T008 and T010 (an anchor not reproduced —
fix the code, never the anchor), T013 (any gate failure). The decision rule is fixed (spec
FR-007) and is not edited after the numbers exist.
