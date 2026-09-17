# Tasks: The Chunking Study

**Input**: Design documents from `/specs/022-chunking-study/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md)
(D1–D10), [data-model.md](./data-model.md), [contracts/study.md](./contracts/study.md),
[quickstart.md](./quickstart.md); on disk: the three BEIR sets under `reference/datasets/beir/`,
the two engine models, the chonky model (`reference/models/chonky_distilbert_base_uncased_1`),
the wheel under `target/wheels/`, `reference/.venv-012` (for pip-tools), `uv`.

**Tests**: **Mandatory** (Principle II; spec FR-009): `reference/tests_022/` committed red
before `reference/chunking_study.py`. One PR; the **owner commits** at the checkpoints
(the agent runs no git command that changes state): **C1** = Phases 1–2 (environment, red
tests); **C2** = Phase 3–4 (the script green on the pure parts, the anchor PASS on SciFact
and NFCorpus); **C3** = Phase 5 (the SciFact/NFCorpus cells, the provisional decision);
**C4** = Phases 6–8 (FiQA, the final decision, the report, the PR).

**Organization**: Setup → Foundational (red tests) → US1 (the anchor) → US2 (the four
variants on SciFact/NFCorpus) → US4 (the provisional decision) → US3 (FiQA) → US4 (final) →
Polish. Long runs go to the background with a progress monitor (`PY=reference/.venv-022/bin/python`;
`unset SDKROOT`; every command from the repository root).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`reference/chunking_study.py`, `reference/requirements-022.{in,txt}`, `reference/tests_022/*.py`;
`specs/022-chunking-study/{runs/,owner-decision.json,report.md,pr-description.md}`;
scratch under `target/xt-chunking-study/` (gitignored).

---

## Phase 1: Setup

- [X] T001 Write `reference/requirements-022.in` (research D1): the 012 pins verbatim (`torch==2.14.0`, `transformers==5.17.0`, `tokenizers==0.23.2`, `safetensors==0.8.0`, `numpy==2.5.3`, `pytrec_eval==0.5`, `huggingface-hub==1.31.0`, `pytest`, `pip-tools`) plus `chonky==0.1.7`, with a header comment (what the study needs each for; the wheel is installed separately); `reference/.venv-012/bin/pip-compile --generate-hashes --output-file=reference/requirements-022.txt reference/requirements-022.in`; `scripts/setup-reference-venv.sh 022` → PASS; `VIRTUAL_ENV=reference/.venv-022 uv pip install target/wheels/xtriever-*.whl`; `$PY -c "import xtriever, chonky, pytrec_eval, torch, transformers; print(xtriever.__version__, torch.__version__, transformers.__version__)"` → `0.1.0 2.14.0 5.17.0`; confirm `reference/.venv-*` is gitignored; `mkdir -p specs/022-chunking-study/runs`
- [X] T002 Confirm the inputs: `ls reference/datasets/beir/{scifact,nfcorpus,fiqa}/{corpus.jsonl,queries.jsonl,qrels/test.tsv} reference/models/chonky_distilbert_base_uncased_1/model.safetensors reference/models/all-MiniLM-L6-v2/model.safetensors reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors specs/013-lexical-quality/baselines/hybrid-baseline-v2.scifact.json specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.scifact.json`; `jq '.scored_queries, .mean_ndcg_10, .mean_recall_100' specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.{scifact,nfcorpus,fiqa}.json` → 300 / 0.7207… / 0.955, 323 / 0.3622…, 648 / 0.3909…; `grep -n "xt-chunking-study\|^target" .gitignore` (the `target/` rule covers the scratch)

---

## Phase 2: Foundational — the red tests

- [X] T003 Write `reference/tests_022/helpers_022.py` (`REPO`, `sys.path` insertion for `reference/`, the expected constants `WINDOW = 256`, `MIN_POSITIONS = 16`, `MEAN_GAIN = MAX_DROP = RECALL_DROP = 0.005`, `K_STUDY = 300`, `K_ANCHOR = 100`, a `words_cost(unit)` stub (= words + 2 → cost words) and a `StubSplitter(pieces_by_text)`), and `reference/tests_022/conftest.py` importing it (no test imports `conftest` by name — the 017 rule)
- [X] T004 [P] Write `reference/tests_022/test_join.py`: `join_title_text("T", "x") == "T x"`, `("", "x") == "x"`, `("T", "") == "T"`, `("", "") == ""`; `passage_contents(title, chunk)` is the same join
- [X] T005 [P] Write `reference/tests_022/test_splitters.py` (research D5; spec FR-004): `split_contract(doc, cost, title_positions)` — a two-paragraph text with a budget that fits one paragraph gives two chunks (byte-for-byte the paragraphs), a title with `title_positions >= 256` returns `[text]` and sets `title_fills_window`; `bound_and_merge(chunks, cost, title_positions)` on stubs: (a) a chunk with `cost < 16 − 2` positions merges into its predecessor joined by `"\n"`, the first such into its successor; (b) a chunk over the budget is re-chunked by the contract chunker (the pieces concatenate back modulo the joiner and each fits); (c) a remainder under 16 merges into its predecessor only when the result fits, otherwise stays and is counted; `budget = 256 − title_positions`; `chonky_passages(text, splitter)` returns non-empty stripped pieces and raises on a non-partition; the constants `WINDOW == 256`, `MIN_POSITIONS == 16` as module literals of `chunking_study`
- [X] T006 [P] Write `reference/tests_022/test_maxp.py` (research D6): `maxp(hits, parent_of)` on stub hits `[a#1, b#0, a#0, c#2, b#3]` → `[a, b, c]`; truncation at 100 (150 distinct → 100); a query with 40 distinct documents → 40 and `short == True`; `whole` hits (no chunk) map to their external id
- [X] T007 [P] Write `reference/tests_022/test_runs.py` (research D7, contracts/study.md): `cell_name("contract", 20, 300, "scifact") == "contract-d20@300.scifact"`; `run_path` / `score_path` under `RUNS_DIR`; `write_run` / `read_run` round-trip in the 014 JSONL shape; `check_anchor(got, want)` on synthetic reports — identical → `None`; one query's `ndcg_10` off by 1e-5 → a message naming the query; a differing query set → a message; `short_queries` counted into the score file
- [X] T008 [P] Write `reference/tests_022/test_decide_022.py` (spec US4; research D9): synthetic score rows for `whole` and three chunkers over `scifact`/`nfcorpus`(/`fiqa`): a chunker at exactly `whole + 0.005` mean with no dataset below by more than 0.005 and recall within 0.005 → `recommended`; at `+ 0.0049` → not; one dataset at `−0.0051` → not; recall at `−0.0051` → not; two chunkers tied → `best_chunker == "contract"` (the non-neural); a chunker with only two datasets → `scope == "two-way"` and its mean over those two, `whole`'s mean over the same two; the constants are the module's literals; the decision dict has the keys of data-model "Decision"
- [X] T009 Quickstart Step 1: `$PY -m pytest reference/tests_022 -q` → every file errors on import (`chunking_study` does not exist); record in `specs/022-chunking-study/report.md` ("Red checkpoint"). **⛔ Checkpoint C1 — the owner commits** the requirements pair, the tests, the report stub

---

## Phase 3: User Story 1 — The study harness reproduces the baselines (Priority: P1)

**Goal**: `chunking_study.py`'s pure parts green; `whole@100` reproduces v2/v3 on SciFact and NFCorpus (FiQA in Phase 6).

**Independent Test**: `check --dataset scifact` and `--dataset nfcorpus` → PASS.

- [X] T010 [US1] Write `reference/chunking_study.py` (contracts/study.md; research D1–D9): module docstring (the study, the variants, the rule, the file names, the cost); constants (`WINDOW`, `MIN_POSITIONS`, `MEAN_GAIN`, `MAX_DROP`, `RECALL_DROP`, `K_STUDY = 300`, `K_ANCHOR = 100`, `DEPTHS = (0, 20)`, `DATASETS`, `VARIANTS = ("whole", "contract", "chonky", "chonky-bounded")`, `RUNS_DIR`, `STUDY_DIR = REPO/"target/xt-chunking-study"`, model dirs from the 019 environment variables with the `reference/models/…` defaults); `sys.path` imports of `gen_003_fixtures as ref003`, `gen_008_fixtures as ref008`, `rerank_study as rs` (`read_run`, `write_run`, `compare_metrics`, `mean_of`); `join_title_text`, `passage_contents`; `load_corpus(dataset)`, `load_queries(dataset)` (only the ids in the test qrels), `qrels_for`; `Cost(embedder_dir)` (the 021 `Window`: `token_count`, `cost = token_count − 2`); `split_contract(text, cost, title_positions)` → `ref008.chunk(text, 256 − title_positions, cost)` texts, `[text]` + a flag when `title_positions >= 256`; `chonky_passages(text, splitter)` (021's partition check; `ParagraphSplitter(model_id=<dir>, device="cpu")` imported lazily); `bound_and_merge(chunks, cost, title_positions)` per D5 (fragments merged, over-window re-chunked via `ref008.chunk`, remainder rule) returning `(passages, under_16_kept)`; `chonky_cache(dataset)` (write/read `STUDY_DIR/chonky.<dataset>.jsonl`); `passages_for(variant, doc, cost, splitter)` → `list[str]` + counters; `build(variant, dataset, limit=None)` → `IndexHandle.create(STUDY_DIR/f"{variant}.{dataset}[.limitN]"/"index", IndexConfig(fields=[FieldDef("contents", TEXT("standard_en"))], dense_fields=["contents"]), …, MMAP)`, `Document(external_id=doc_id (whole) or f"{doc_id}#{i}", fields={"contents": TEXT(passage_contents(title, passage))}, chunk=None (whole) or ChunkInfo(parent=doc_id, ordinal=i))`, batches of 4,096, `commit`, `merge`, `build.json` (data-model "Build record": `documents`, `passages`, `over_window`, `under_16`, `title_fills_window`, `split_s`, `embed_s`, `passages_per_doc_median`), skipped when `build.json` exists; `maxp(hits)` → document ids by first occurrence via `hit.chunk.parent` or the external id, truncated to 100, and `short` when fewer; `search(variant, dataset, depth, k)` → for each test query `SearchOptions(k=k, depth=k, rerank_depth=depth)` → `maxp` → `write_run` to `runs/<cell>.jsonl` (skipped when present), a `short_queries` count kept beside; `score(dataset, variant=None)` → `ref003.reference(qrels, read_run)` + `short_queries` → `runs/<cell>.json`; `check(dataset)` → `rs.compare_metrics` of `whole-d0@100` vs `hybrid-baseline-v2.<dataset>.json` and `whole-d20@100` vs `hybrid-rerank-v3.<dataset>.json` → `PASS` or the first difference (exit 1); `table()` → markdown: per dataset, rows per variant at `@300` with fused / re-ranked nDCG@10 and Recall@100 and deltas vs `whole@300`, plus the `whole@100` row and the build record columns (passages, over-window, under-16, short queries); `decide(rows)` per D9 → dict, `cmd_decide` prints and writes `owner-decision.json` when asked; `all(dataset, variants)` → build whole → search @100 both depths → score → check (⛔ stop on FAIL) → build the chunked variants → search @300 both depths for every variant → score; argparse per the contract, exit codes 0/1/2
- [X] T011 [US1] `$PY -m pytest reference/tests_022 -q` → green; `$PY -m pytest reference/tests_012 reference/tests_014 reference/tests_016 reference/tests_022 -q` → one collection, green (012's model-backed skips as before); smoke: `$PY reference/chunking_study.py build --variant chonky-bounded --dataset scifact --limit 50` → an index under `target/xt-chunking-study/chonky-bounded.scifact.limit50/` with `build.json` (`over_window == 0`), in under two minutes; `rm -rf target/xt-chunking-study/*.limit50`
- [X] T012 [US1] The anchor, SciFact (quickstart Step 3; ~20 min, background + monitor): `build --variant whole --dataset scifact`; `search --variant whole --dataset scifact --depth 0 --k 100`; `… --depth 20 --k 100`; `score --dataset scifact`; `check --dataset scifact` → **PASS** (**⛔** any difference is stop-and-report: the harness, never the baselines or the tolerance); paste the two means into the report
- [X] T013 [US1] The anchor, NFCorpus (~15 min, the same four commands with `--dataset nfcorpus`) → **PASS**. **⛔ Checkpoint C2 — the owner commits** the script, the tests green, the four anchor run/score files, the report so far

---

## Phase 4: User Story 2 — Four ways of chunking, scored the same way (Priority: P1)

**Goal**: the twelve chunked cells plus `whole@300` on SciFact and NFCorpus.

**Independent Test**: sixteen `@300` run files with scores under `runs/`; one table.

- [X] T014 [US2] SciFact (`~1 h 20` after the anchor; background + monitor on the build/search progress lines): `search --variant whole --dataset scifact --depth 0 --k 300` and `--depth 20 --k 300`; `build --variant contract --dataset scifact`, `build --variant chonky --dataset scifact` (the split cached), `build --variant chonky-bounded --dataset scifact`; the six `search … --k 300` cells; `score --dataset scifact` (**⛔** a non-partition from chonky or a `chonky-bounded` build with `over_window > 0` is stop-and-report — the bounding, never the count); paste each `build.json` into the report
- [X] T015 [US2] NFCorpus, the same (~1 h 15); `$PY reference/chunking_study.py table` → the SciFact and NFCorpus tables into the report, with the `whole@100` vs `whole@300` difference stated
- [X] T016 [US2] Inspect `chonky-bounded`'s build records: `over_window == 0` on both sets; `under_16` (kept remainders) stated — expected near zero; the share of short queries (fewer than 100 documents) per cell into the report (spec edge case)

---

## Phase 5: User Story 4 — The decision by the rule (provisional, two-way) (Priority: P1)

- [X] T017 [US4] `$PY reference/chunking_study.py decide` → the two-way verdict for each chunker vs `whole@300` (SciFact + NFCorpus): means, per-dataset deltas, recall delta, `recommended`, `best_chunker`; paste into the report as **provisional (two-way)**; name the leader for FiQA (or "none leads — FiQA runs `whole` only"). **⛔ Checkpoint C3 — the owner commits** the sixteen cells, the build records and the report

---

## Phase 6: User Story 3 — FiQA for the anchor and the winner (Priority: P2)

- [ ] T018 [US3] FiQA `whole` (~2 h 10, background + monitor): `build --variant whole --dataset fiqa`; `search … --k 100` at depths 0 and 20; `score --dataset fiqa`; `check --dataset fiqa` → **PASS** (**⛔** otherwise); then `search … --k 300` at both depths; `score`
- [ ] T019 [US3] FiQA for the leader named in T017 (~2 h 35): `build --variant <leader> --dataset fiqa`; the two `@300` cells; `score --dataset fiqa`; `table` → the FiQA table into the report; if no chunker led, record that FiQA ran `whole` only and why

---

## Phase 7: User Story 4 — The decision by the rule (final) (Priority: P1)

- [ ] T020 [US4] `$PY reference/chunking_study.py decide --owner-decision specs/022-chunking-study/owner-decision.json` → the final verdict (three-way for `whole` and the leader, two-way for the others — labelled); `runs/build-records.json` assembled from every `build.json`; the report's decision section states what follows (the demos' recipe and the docs in a later feature; the shipped Wikipedia index untouched; a chonky win over `contract` → a separate feature for a pre-split input to the Rust build) — the study itself changes none of them

---

## Phase 8: Polish

- [ ] T021 Gate (quickstart Step 5): `git diff --stat main -- crates/ swift/ python/src apps/ specs/*/baselines` empty; `cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo deny check` unchanged; the four reference suites in one collection green; `grep -rn "$(hostname -s)\|$USER" specs/022-chunking-study reference/chunking_study.py reference/tests_022 reference/requirements-022.*` → nothing; `git status --short` shows only the intended files (`target/xt-chunking-study` and `reference/.venv-022` absent — ignored)
- [ ] T022 Write `specs/022-chunking-study/report.md` (verdict; the red checkpoint; the anchor check per dataset; the build records; the tables with deltas; the `@100`/`@300` difference; the short-query shares; the decision with the numbers and its scope; the cost actually spent per block; "Deliberately not done": no engine or demo change, no fallback beyond the two fixes, no CI) and `specs/022-chunking-study/pr-description.md` (the question, the method in four lines, the table's headline numbers, the verdict, the cost, the attribution line). **⛔ Checkpoint C4 — the owner commits**, pushes and opens the PR

---

## Dependencies & Execution Order

T001 → T002 → T003 → (T004 ‖ T005 ‖ T006 ‖ T007 ‖ T008) → T009 (C1, owner) → T010 → T011 →
T012 → T013 (C2, owner) → T014 → T015 → T016 → T017 (C3, owner) → T018 → T019 → T020 →
T021 → T022 (C4, owner).

### User story completion order

US1 (the anchor) → US2 (the cells) → US4 provisional → US3 (FiQA) → US4 final.

### Parallel opportunities

The five test files (T004–T008); the SciFact and NFCorpus blocks (T014, T015) run one after
the other in the background while the report is drafted; T019 can be skipped by the rule.

## Implementation Strategy

**MVP** = Phases 1–4 (the harness reproducing the baselines, then the sixteen SciFact /
NFCorpus cells — enough for a two-way verdict). **Rule 6 stop-points**: T009 (red), T012 /
T013 / T018 (an anchor difference), T014 (a non-partition or a breached bound), T021 (any
gate failure). Never the tolerance, never the baselines, never the rule's constants.
