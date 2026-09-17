# Tasks: The Minimal Python Demo

**Input**: Design documents from `/specs/020-minimal-python-example/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md)
(D1–D7), [data-model.md](./data-model.md), [contracts/example.md](./contracts/example.md),
[quickstart.md](./quickstart.md); on disk: the two models, `apps/python-wiki-demo/.venv`
(the wheel and pytest — `PY=apps/python-wiki-demo/.venv/bin`).

**Tests**: **Mandatory** (Principle II; spec FR-007): `tests/test_demo.py` committed red
before `demo.py`. One PR; two checkpoints the **owner commits** (the agent runs no git
command that changes state): **C1** = Phases 1–2 (red), **C2** = Phases 3–6 (green, docs,
report).

**Organization**: Setup → Foundational (conftest, the red tests) → US1 (the file) → US2 (the
oracle) → US3 (README) → Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US3 from spec.md. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`apps/python-minimal-demo/{demo.py,README.md}`, `apps/python-minimal-demo/tests/{conftest,test_demo}.py`;
`specs/020-minimal-python-example/{report.md,pr-description.md}`; one line in
`apps/python-wiki-demo/README.md`. All commands from the repository root with `unset SDKROOT`.

---

## Phase 1: Setup

- [X] T001 Confirm the inputs: `ls reference/models/all-MiniLM-L6-v2/model.safetensors reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors apps/python-wiki-demo/.venv/bin/pytest` and `apps/python-wiki-demo/.venv/bin/python -c "import xtriever; print(xtriever.__version__)"` → `0.1.0`; `git branch --show-current` → `020-minimal-python-example`; `mkdir -p apps/python-minimal-demo/tests`

---

## Phase 2: Foundational — the red tests

- [X] T002 Write `apps/python-minimal-demo/tests/conftest.py`: `REPO = Path(__file__).resolve().parents[3]`; `DEMO = REPO / "apps/python-minimal-demo/demo.py"`; `EMBEDDER` / `RERANKER` from `XTRIEVER_MODEL_DIR` / `XTRIEVER_RERANK_MODEL_DIR` with the `reference/models/…` defaults; the `models` marker registered via `pytest_configure` and the skip-with-reason `pytest_collection_modifyitems` hook (the 011/019 pattern) checking both `model.safetensors`; `load_demo()` returning the module loaded by `importlib.util.spec_from_file_location("demo", DEMO)` (so the tests need no package); `f64_bits` / `f32_bits`
- [X] T003 Write `apps/python-minimal-demo/tests/test_demo.py`: `test_line_budget` — `DEMO` has ≤ 80 lines (spec FR-005); `test_usage_exit_2` — `load_demo().main([])` returns 2 and prints `usage: demo.py "your question"` to stderr, `main(["a", "b"])` likewise, before any model loads (monkeypatch `xtriever.IndexHandle.create` to raise `AssertionError`); `test_print_hits_on_stubs` — `print_hits("fused (lexical + dense)", [stub hits])` prints `fused (lexical + dense), 2 hits` then ` 1. doc-03  score=0.0328  Honey bees` (a stub with `rerank_score=None`) and ` 2. doc-07  score=0.0301  rerank=8.6573  Tides` (with a re-rank score) — the contract's lines; `test_corpus_shape` — `DOCS` has 8–12 entries, every text has a title line, a blank line and a body, ids are unique; `test_hits_are_the_engines` (`models`) — `fused, reranked = load_demo().run("how do bees make honey")`; build the same `DOCS` directly through the package (`IndexHandle.create` in a `tempfile.mkdtemp()` with an `IndexConfig` of one `contents` field / `dense_fields=["contents"]`, `add` the same `Document`s, `commit`, `merge`) and search with `SearchOptions(k=5, rerank_depth=0)` and `(k=5, rerank_depth=10)`; the tuples `(external_id, f64_bits(score), f32_bits(rerank_score))` equal at both depths; a second `run` gives the same tuples; the temporary directories are gone afterwards (`run` cleans its own; the test cleans the direct one)
- [X] T004 Quickstart Step 1: `$PY/pytest apps/python-minimal-demo/tests -q` → every test fails (`DEMO` does not exist: the budget test on the missing file, the others on `load_demo`); record in `specs/020-minimal-python-example/report.md` ("Red checkpoint"). **⛔ Checkpoint C1 — the owner commits** the tests and the report stub

---

## Phase 3: User Story 1 — A person reads the whole recipe on one screen and runs it (Priority: P1)

**Goal**: `demo.py` ≤ 80 lines: corpus, schema, build, two searches, print, cleanup.

**Independent Test**: quickstart Step 2.

- [ ] T005 [US1] Write `apps/python-minimal-demo/demo.py` (research D1–D4; contracts/example.md): the docstring (what it is; the two inputs and `scripts/fetch-model.sh` for each; the command; what it leaves out — chunking and a real corpus → `apps/python-wiki-demo`, explanations and marks → `wikidemo search --explain`, the phone → `apps/ios-wiki-demo`); `DOCS`: ten `(id, text)` tuples, `doc-01`…`doc-10`, `"<title>\n\n<one or two sentences>"` on honey bees, the water cycle, bread, tides, the Moon, rust, sourdough, coral reefs, thunderstorms, glaciers (bread/sourdough and tides/the Moon share vocabulary); `EMBEDDER` / `RERANKER` from the two environment variables with the defaults relative to `Path(__file__).resolve().parents[2]`; `SCHEMA = xtriever.IndexConfig(fields=[xtriever.FieldDef(name="contents", kind=xtriever.FieldKind.TEXT(analyzer="standard_en"))], dense_fields=["contents"])`; `build()` → `tempfile.mkdtemp()`, `IndexHandle.create(dir, SCHEMA, str(EMBEDDER), str(RERANKER), LoadPath.MMAP)`, `add([Document(external_id=i, fields={"contents": FieldValue.TEXT(t)}) for i, t in DOCS])`, `commit()`, `merge()`, return `(handle, dir)`; `run(query)` → `build`, `search(query, SearchOptions(k=5, rerank_depth=0))`, `search(query, SearchOptions(k=5, rerank_depth=10))`, `shutil.rmtree(dir)` in a `finally`, return the two responses; `print_hits(label, hits)` → `f"{label}, {len(hits)} hits"` then per hit `f" {rank}. {id}  score={score:.4f}" + (f"  rerank={rs:.4f}" if rs is not None else "") + f"  {first line of text}"`; `main(argv)` → exactly one argument or the usage line to stderr and 2; `print(f"indexed {len(DOCS)} documents")`, blank, `print_hits("fused (lexical + dense)", …)`, blank, `print_hits("re-ranked (depth 10)", …)`, return 0; the `__main__` guard. One comment per step: `# the corpus`, `# the schema: one text field, also the dense field (Feature 013's layout)`, `# build: create → add → commit → merge`, `# search: the fused stage, then the re-ranked stage`, `# print`
- [ ] T006 [US1] `$PY/pytest apps/python-minimal-demo/tests -q -m "not models"` → green (budget, usage, printing, corpus shape); quickstart Step 2 by hand: `time $PY/python apps/python-minimal-demo/demo.py "how do bees make honey"` and `"why does the sea rise and fall"` — paste both outputs and the wall time into the report (SC-003 < 10 s; **⛔** over budget or over 80 lines is stop-and-report — trim comments or prose, never the steps); `$PY/python apps/python-minimal-demo/demo.py; echo $?` → usage, 2; `ls /tmp | grep -c tmp` unchanged before/after a run (nothing left behind)

---

## Phase 4: User Story 2 — The file's output is the engine's (Priority: P1)

- [ ] T007 [US2] `$PY/pytest apps/python-minimal-demo/tests -q` → `test_hits_are_the_engines` green: ids in order and score bits equal the direct package call at depths 0 and 10, two runs identical (**⛔** a mismatch is stop-and-report — the script, never the test); paste the counts into the report

---

## Phase 5: User Story 3 — A README that fits the file (Priority: P2)

- [ ] T008 [P] [US3] Write `apps/python-minimal-demo/README.md` (spec US3): what it is (the smallest demo of the pipeline — ten documents, build, fused then re-ranked); the inputs (a Python with the wheel: `cd python && .venv/bin/maturin build --release`, `uv pip install target/wheels/xtriever-*.whl`; the two models: `scripts/fetch-model.sh`, `scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json`; the two environment variables); the command and the two suggested queries with what each shows (the lexical and dense stages disagreeing, the cross-encoder settling it); what is left out and which demo has it (chunking and a real corpus → `apps/python-wiki-demo`; explanations, marks, the stage report → `wikidemo search --explain`; the phone → `apps/ios-wiki-demo`); the tests command (`apps/python-wiki-demo/.venv/bin/pytest apps/python-minimal-demo/tests -q`)
- [ ] T009 [P] [US3] `apps/python-wiki-demo/README.md`: one line near the top pointing to `apps/python-minimal-demo/` as the smallest demo (ten documents, no snapshot) for readers who want the recipe before the measured one

---

## Phase 6: Polish

- [ ] T010 Gate (quickstart Step 4): `git diff --stat main -- crates/ swift/ python/src apps/python-wiki-demo/wikidemo specs/*/baselines` empty; `cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo deny check` unchanged; `$PY/pytest apps/python-wiki-demo/tests -q` → 70 passed (untouched); `$PY/pytest apps/python-minimal-demo/tests -q` → 5 passed; `wc -l apps/python-minimal-demo/demo.py` ≤ 80; no identifiers in any new file; `git status --short` shows only the intended files
- [ ] T011 Write `specs/020-minimal-python-example/report.md` (verdict; the red checkpoint; the two outputs and the wall time; the oracle test's result; SC-001–SC-004; "Deliberately not done": no chunker, no snapshot, no output directory, no CI job) and `specs/020-minimal-python-example/pr-description.md` (what it is in three lines, the line count, the oracle, the run time, the attribution line). **⛔ Checkpoint C2 — the owner commits**, pushes and opens the PR

---

## Dependencies & Execution Order

T001 → T002 → T003 → T004 (C1, owner) → T005 → T006 → T007 → (T008 ‖ T009) → T010 → T011 (C2, owner).

### User story completion order

US1 → US2 → US3.

### Parallel opportunities

T008 ‖ T009 (two READMEs); T008 can be drafted while T007's model-backed test runs.

## Implementation Strategy

**MVP** = Phases 1–3 (the file, model-free green, the two runs by hand). **Rule 6
stop-points**: T004 (red), T006 (over 80 lines or over ten seconds), T007 (a mismatch with
the direct call), T010 (any gate failure). Never the test, the budget or the bits.
