# Tasks: Chonky as an Optional Chunker for the Wikipedia Demo Build

**Input**: Design documents from `/specs/023-optional-chonky-chunker/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md)
(D1–D9), [data-model.md](./data-model.md), [contracts/build.md](./contracts/build.md),
[quickstart.md](./quickstart.md); on disk: the two engine models, the chonky model, the
snapshot, the Rust slice `target/xt-wiki-slice-rs` (identity `20949fb4…`), the demo venv
(`PY=apps/python-wiki-demo/.venv/bin`); the 019 files at commit `802cf72` (read with
`git show 802cf72:<path>` — read-only).

**Tests**: **Mandatory** (Principle II; spec FR-008): the new / rearranged tests committed
red before the code. One PR; two checkpoints the **owner commits** (the agent runs no git
command that changes state): **C1** = Phases 1–2 (red), **C2** = Phases 3–7 (green, the
records, docs, report).

**Organization**: Setup (the extra, the venv) → Foundational (the red tests) → US1 (the
contract chunker back as the default, parity with the Rust slice) → US2 (the chonky option)
→ US3 (the extra and its refusal) → US4 (README) → Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`apps/python-wiki-demo/{pyproject.toml,README.md}`;
`apps/python-wiki-demo/wikidemo/{chunking,contract,chonky_chunker,build,cli,inputs,record}.py`;
`apps/python-wiki-demo/tests/{conftest,test_chunking,test_contract,test_chonky,test_build,test_cli,test_inputs,test_record}.py`;
`specs/023-optional-chonky-chunker/{runs/,report.md,pr-description.md}`. All commands from
the repository root with `unset SDKROOT`.

---

## Phase 1: Setup

- [ ] T001 `apps/python-wiki-demo/pyproject.toml` (research D6): move `"chonky==0.1.7"`, `"transformers==5.17.0"`, `"torch==2.14.0"` from `dependencies` to `[project.optional-dependencies] chonky = [...]` with the 021 comment moved along ("the `--chunker chonky` splitter and what it runs on; the resolver's versions on 2026-09-17"); `dependencies` keeps `xtriever>=0.1.0` and `tokenizers==0.23.2` with its comment rewritten ("the embedder's tokenizer: prices the contract chunker's units and counts passages against the window — the version `crates/xtriever-dense/Cargo.toml` pins"); `markers` gains `"chonky: needs the chonky extra and its pinned model on disk (skipped otherwise)"`; `cd apps/python-wiki-demo && uv pip install --python .venv/bin/python -e ".[test,chonky]"` → resolves with nothing new downloaded; `.venv/bin/python -c "import chonky, torch, tokenizers"`; `mkdir -p specs/023-optional-chonky-chunker/runs`

---

## Phase 2: Foundational — the red tests

- [ ] T002 `apps/python-wiki-demo/tests/conftest.py` (research D6): `missing_for_models()` no longer lists `CHONKY / "model.safetensors"` (the `models` marker = the two engine models + the 007 fixture); new `missing_for_chonky()` → the install command `uv pip install --python apps/python-wiki-demo/.venv/bin/python -e 'apps/python-wiki-demo[chonky]'` when `importlib.util.find_spec("chonky") is None`, else the fetch command when `CHONKY / "model.safetensors"` is absent, else `None`; `pytest_collection_modifyitems` adds a skip with that reason to `chonky`-marked items; docstring updated (two markers)
- [ ] T003 [P] Rewrite `apps/python-wiki-demo/tests/test_chunking.py` for the shared module (research D2): `WINDOW == 256`; `passage_text("T", "b") == "T\n\nb"`; `CHUNKERS == ("contract", "chonky")` and `DEFAULT_CHUNKER == "contract"`; `make_chunker("contract", paths, window)` (a `SimpleNamespace(chonky=Path("/nonexistent"))` for paths, a stub window with `token_count = words + 2`) returns an object with `documents_for`, `block == record.CONTRACT_CHUNKER`, `label == "008 contract (256 - token_count(title))"`, and — with `monkeypatch.setitem(sys.modules, "chonky", None)` — succeeds, proving the contract path never imports the extra; `make_chunker("whole", …)` raises `BuildError` naming the two choices; `Window(EMBEDDER).token_count("a b")` is an int > 2 (`models`)
- [ ] T004 [P] Write `apps/python-wiki-demo/tests/test_contract.py` from `git show 802cf72:apps/python-wiki-demo/tests/test_chunking.py` (research D3): the docstring; imports `from wikidemo.chunking import WINDOW, BuildError, passage_text` and `from wikidemo.contract import ContractChunker, chunk, words`; `test_set_a_byte_identical` (48 cases, cost = words) and `test_set_b_byte_identical` (nine articles, `unit_costs`, priced > 0) verbatim; the `_Pricer` stub becomes a stub window (`token_count = words + 2`, an optional title override) passed to `ContractChunker(window)`; `test_title_that_fills_the_window_is_an_error` (`documents_for` → `BuildError` with "article 7" and "256"); `test_documents_for_shapes` (one passage, `"42#0"`, the fields, `ChunkInfo(parent="42", ordinal=0, byte_start=0, byte_end=len(text.encode()))`, and the returned positions list has one int > 0); `test_documents_for_splits_at_the_budget` (`["42#0", "42#1"]`, `byte_start == len("One two three four.\n\n")`); `test_over_window_is_zero_by_construction`: the two-paragraph article with the small-budget window → every returned position count ≤ `WINDOW` (the stub counts words + 2, the budget is words, so `positions == cost + title + 2 ≤ 256`)
- [ ] T005 [P] Write `apps/python-wiki-demo/tests/test_chonky.py` from the current `tests/test_chunking.py` (research D4): the docstring (Feature 021's chunker behind `--chunker chonky`); `from wikidemo.chonky_chunker import ChonkyChunker, Splitter`; the stub splitter tests verbatim (`test_chunks_are_character_ranges_from_the_splitter`, `test_partition_is_enforced`, `test_documents_for_shapes_and_offsets`, `test_over_window_is_counted_not_cut`, `test_empty_text_yields_no_documents`) with `documents_for(article, splitter, window)` → `ChonkyChunker.__new__`-built instances holding `_splitter` and `_window` (or a small `_chunker(pieces, window)` helper), and `block == record.CHONKY_CHUNKER`, `label.startswith("chonky (mirth/chonky_distilbert_base_uncased_1, revision 01d8aae")`; `test_real_splitter_partitions_articles` marked `@pytest.mark.chonky` (and `models` for the embedder window) — `ChonkyChunker(CHONKY, Window(EMBEDDER))` on the snapshot's first article as now
- [ ] T006 [P] Update `apps/python-wiki-demo/tests/test_build.py`: import `CONTRACT_CHUNKER, CHONKY_CHUNKER` instead of `CHUNKER`; the module fixture `built` builds with the default and asserts `sidecar["chunker"] == CONTRACT_CHUNKER == {"version": 1, "budget": "256 - token_count(title)", "cost": "MiniLmEmbedder::token_count(unit) - 2"}`, `counts["passages_over_window"] == 0`, stdout has `chunker: 008 contract (256 - token_count(title))` and `passages over the embedder window: 0 (0.0 %)`; `test_identity_matches_the_record_helper` uses `CONTRACT_CHUNKER`; `test_built_index_searches`'s `about` assertion → `chunker: 008 contract (`; new `test_default_build_never_imports_the_extra`: `monkeypatch.setitem(sys.modules, "chonky", None)` (and `"transformers"`, `"torch"` likewise — only if not already imported in this session, else skip those two) around a `--limit 3` default build into a fresh dir → exit 0; new `test_chonky_build_without_the_extra_is_refused`: `monkeypatch.setitem(sys.modules, "chonky", None)`, `--chunker chonky --limit 1` with all inputs present (`--chonky` pointing at `CHONKY`, which may be absent — then the expected message is the missing-model one, assert either way in under a second) → exit 1, stderr starts `wikidemo: --chunker chonky needs the demo's chonky extra` and contains `-e 'apps/python-wiki-demo[chonky]'`, nothing at `<out>`; new `test_chonky_build_produces_the_chonky_block` marked `chonky`: `_build(..., ["--limit", "3", "--chunker", "chonky"])` → exit 0, `sidecar["chunker"] == CHONKY_CHUNKER`, stdout has `chunker: chonky (mirth/chonky_distilbert_base_uncased_1, revision 01d8aae`, `passages_over_window` an int in `[0, passages]`
- [ ] T007 [P] Update `apps/python-wiki-demo/tests/test_cli.py` and `tests/test_inputs.py` (research D7): `test_the_four_subcommands_are_offered` also parses `["build", "--out", "x", "--chunker", "whole"]` → exit 2 with `invalid choice` in stderr, and `build_parser().parse_args(["build", "--out", "x"]).chunker == "contract"`; `needs_for(build default) == ["embedder", "reranker", "snapshot", "manifest"]` and with `--chunker chonky` the same plus `"chonky"`; `test_build_refuses_a_missing_splitter_model_before_loading_torch` adds `--chunker chonky` to its argv (unchanged otherwise); `test_inputs.py`: `first_missing(q, ["chonky"])` unchanged (the input still exists; only `needs_for` decides when it is asked for)
- [ ] T008 [P] Update `apps/python-wiki-demo/tests/test_record.py`: `SHIPPED_CHUNKER` literal replaced by `from wikidemo.record import CONTRACT_CHUNKER, CHONKY_CHUNKER`; `test_identity_of_the_shipped_corpus` uses `CONTRACT_CHUNKER` (the shipped identity still reproduces — this is the parity's root); `test_chunker_block_is_chonky` → `test_the_two_chunker_blocks`: `CHONKY_CHUNKER == {"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1", "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5"}`, `corpus_identity(…, CHONKY_CHUNKER, …) != SHIPPED_IDENTITY`, and `record.CHUNKER` no longer exists (`not hasattr(record, "CHUNKER")`)
- [ ] T009 Quickstart Step 1: `$PY/pytest apps/python-wiki-demo/tests -q 2>&1 | tail -15` → `test_contract`, `test_chonky` error on import; `test_chunking` fails on `make_chunker` / `CHUNKERS`; `test_cli`, `test_record`, `test_build` fail as listed; the untouched files (`test_about`, `test_hits`, `test_measure`, `test_render`, `test_rules`, `test_search`) still pass; record the counts in `specs/023-optional-chonky-chunker/report.md` ("Red checkpoint"). **⛔ Checkpoint C1 — the owner commits** the pyproject, the tests, the report stub

---

## Phase 3: User Story 1 — The default build is the shipped recipe (Priority: P1)

**Goal**: the 008 contract chunker restored as the default; a default slice is the Rust slice.

**Independent Test**: the fixture replay (T004) green; quickstart Step 3 parity PASS.

- [ ] T010 [US1] `apps/python-wiki-demo/wikidemo/record.py`: `CHUNKER` → two constants with their comments — `CONTRACT_CHUNKER = {"version": 1, "budget": "256 - token_count(title)", "cost": "MiniLmEmbedder::token_count(unit) - 2"}` (the shipped `corpus.json`'s literal strings, the block the Rust build writes — so a contract build shares the Rust build's identity) and `CHONKY_CHUNKER = {"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1", "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5"}` (Feature 021's, pinned in `reference/models/manifest-chonky.json`)
- [ ] T011 [US1] Rewrite `apps/python-wiki-demo/wikidemo/chunking.py` as the shared module (research D2): docstring (the two chunkers, the flag, the default, where each lives, the over-window rule); `WINDOW = 256`; `BuildError`; `Window` (as now); `passage_text`; `CHUNKERS = ("contract", "chonky")`; `DEFAULT_CHUNKER = "contract"`; `def make_chunker(name, paths, window)`: `if name == "contract": from .contract import ContractChunker; return ContractChunker(window)`; `if name == "chonky": from .chonky_chunker import ChonkyChunker; return ChonkyChunker(paths.chonky, window)`; else `raise BuildError(f"unknown chunker {name!r}; choose one of {', '.join(CHUNKERS)}")` — imports inside the branches so the contract path never touches the chonky module
- [ ] T012 [US1] Write `apps/python-wiki-demo/wikidemo/contract.py` from `git show 802cf72:apps/python-wiki-demo/wikidemo/chunking.py` (research D3): the module docstring adapted (the 008 contract's reference implementation carried unchanged from Feature 019 — the passages the Rust build and the shipped index have; the fixture replay pins it; now the default behind `--chunker contract`); `from .chunking import WINDOW, BuildError, passage_text`; `WHITESPACE`, `BLANK_LINE_INNER`, `TERMINATORS`, `is_ws`, `trim`, `collapse`, `paragraphs`, `sentences`, `words`, `byte_len`, `fragments`, `chunk` **verbatim** (`diff <(git show 802cf72:… | sed -n '/^# ---.*the contract/,/^# ---.*pricing/p') <(sed -n …) contract.py` → only the marker lines differ); `class ContractChunker`: `__init__(self, window)` stores `self._window = window`, `self._costs = {}`, `self.block = CONTRACT_CHUNKER` (from `.record`), `self.label = "008 contract (256 - token_count(title))"`; `cost(self, unit)` memoised `window.token_count(unit) - 2` (019's `Pricer.cost`); `documents_for(self, article) -> tuple[list[xtriever.Document], list[int]]`: 019's body (`title_positions = window.token_count(title)`; `>= WINDOW` → the `BuildError` with the article id and the window; `budget = WINDOW - title_positions`; `chunk(body, budget, self.cost)` → documents as before) plus `positions.append(self._window.token_count(passage))` per passage
- [ ] T013 [US1] `apps/python-wiki-demo/wikidemo/build.py` and `cli.py` (contracts/build.md; research D7): `cli.py` — `build` gains `--chunker` with `choices=CHUNKERS`, `default=DEFAULT_CHUNKER`, help `"how articles are cut into passages: contract — the Feature 008 contract chunker, the shipped index's recipe (default); chonky — the chonky neural splitter (needs the chonky extra and its model)"`; the `build` parser help → "…verify, exclude, split (the 008 contract chunker, or chonky with --chunker chonky), add, commit, merge"; `--chonky`'s help adds "(used with --chunker chonky)"; `needs_for("build")` → the four inputs plus `"chonky"` if `args.chunker == "chonky"`; `build.py` — `run_build` passes `args.chunker`; `build(paths, out, limit, chunker_name=DEFAULT_CHUNKER)`: `window = Window(paths.embedder)` then `chunker = make_chunker(chunker_name, paths, window)` timed into `phases["load_splitter"]`; the loop calls `chunker.documents_for(article)`; the sidecar and identity use `chunker.block`; the completion line prints `f"chunker: {chunker.label}"`; the docstring's recipe line → "split each with the chosen chunker — the 008 contract chunker by default, chonky with `--chunker chonky` (Feature 023)", and the closing paragraph → a default build's passages are the Rust build's, so a demo-built slice is the shipped recipe (`measure --against` checks it); with chonky it is not; `$PY/pytest apps/python-wiki-demo/tests -q --deselect apps/python-wiki-demo/tests/test_chonky.py --ignore apps/python-wiki-demo/tests/test_chonky.py` → green except the chonky-marked build test (module missing until T015)
- [ ] T014 [US1] Quickstart Step 3 (**⛔** a parity FAIL, a passage count other than 8,529 or a build over 30 minutes is stop-and-report — never a tolerance change): `time $PY/wikidemo build --limit 2000 --out target/xt-wiki-slice-py-023 2>/tmp/023-build.err` (monitor for `wrote`/`Error` only); `$PY/wikidemo measure --artefact target/xt-wiki-slice-py-023 --against target/xt-wiki-slice-rs --out specs/023-optional-chonky-chunker/runs/parity-$(sysctl -n hw.model)-<UTC stamp>-mmap-threadsdefault.json` → `parity: PASS`, `against.verdict PASS`, identity `20949fb4…` equal, exit 0; `cp target/xt-wiki-slice-py-023/wiki-build.json specs/023-optional-chonky-chunker/runs/slice-contract-$(sysctl -n hw.model)-<UTC stamp>.json`; `grep -n "$(hostname -s)\|$USER" specs/023-optional-chonky-chunker/runs/*` → nothing; note the counts, phases and the verdict for the report

---

## Phase 4: User Story 2 — `--chunker chonky` splits as Feature 021 did (Priority: P1)

**Goal**: 021's chunker behind the flag, unchanged in behaviour.

**Independent Test**: `test_chonky.py` green; quickstart Step 4's 200-article build.

- [ ] T015 [US2] Write `apps/python-wiki-demo/wikidemo/chonky_chunker.py` from the current `chunking.py` (research D4): docstring (021's text, now behind `--chunker chonky`; the extra; why the module is not named `chonky`); `from .chunking import BuildError, passage_text`; `from .record import CHONKY_CHUNKER`; `INSTALL = "uv pip install --python apps/python-wiki-demo/.venv/bin/python -e 'apps/python-wiki-demo[chonky]'"`; `def ensure_extra()`: `importlib.util.find_spec("chonky") is None` → `raise BuildError(f"--chunker chonky needs the demo's chonky extra; install it with: {INSTALL}")`; `class Splitter` verbatim; `class ChonkyChunker`: `__init__(self, model_dir, window)` → `ensure_extra()`, `self._splitter = Splitter(model_dir)`, `self._window = window`, `self.block = CHONKY_CHUNKER`, `self.label = f"chonky ({CHONKY_CHUNKER['model']}, revision {CHONKY_CHUNKER['revision'][:7]}…)"`; `documents_for(self, article)` = the current function body over `self._splitter` / `self._window`
- [ ] T016 [US2] `$PY/pytest apps/python-wiki-demo/tests/test_chonky.py apps/python-wiki-demo/tests/test_build.py apps/python-wiki-demo/tests/test_chunking.py -q` → green including the `chonky`-marked tests (the extra and the model are on disk; **⛔** a partition failure is stop-and-report)
- [ ] T017 [US2] Quickstart Step 4: `time $PY/wikidemo build --chunker chonky --limit 200 --out target/xt-wiki-slice-chonky-200` → `chunker: chonky (…)`, over-window count > 0; `$PY/wikidemo about --artefact target/xt-wiki-slice-chonky-200 | sed -n '3,8p'`; `cp …/wiki-build.json specs/023-optional-chonky-chunker/runs/slice-chonky-200-$(sysctl -n hw.model)-<UTC stamp>.json`; `XTRIEVER_CHONKY_MODEL_DIR=/nonexistent $PY/wikidemo build --chunker chonky --limit 1 --out target/never; echo $?` → 1 with the fetch command in under a second; `$PY/wikidemo build --chunker whole --limit 1 --out target/never; echo $?` → 2; `$PY/wikidemo measure --artefact target/xt-wiki-slice-chonky-200 --against target/xt-wiki-slice-rs` → exit 1 on the identity line (the designed FAIL; no record written into `runs/` — omit `--out` or point it at the scratchpad)

---

## Phase 5: User Story 3 — Chonky is an install-time extra (Priority: P2)

**Goal**: a chonky build without the extra is refused before any input is opened; a default build never needs it.

**Independent Test**: T006's two new tests; the no-extra venv below.

- [ ] T018 [US3] `apps/python-wiki-demo/wikidemo/build.py`: `run_build` calls `ensure_extra()` (imported from `.chonky_chunker` only when `args.chunker == "chonky"` — the import itself is light: the module imports no torch at module level) **before** `build()` so the refusal precedes `verify_snapshot`; then the real check in a venv without the extra, in the scratchpad: `uv venv "$SCRATCH/no-extra" --python 3.12 && uv pip install --python "$SCRATCH/no-extra/bin/python" target/wheels/xtriever-*.whl -e apps/python-wiki-demo` (no `[chonky]`; confirm `torch` is not installed), then `"$SCRATCH/no-extra/bin/wikidemo" build --chunker chonky --limit 1 --out target/never; echo $?` → exit 1 with the install command, and `"$SCRATCH/no-extra/bin/wikidemo" build --limit 3 --out "$SCRATCH/default-3"` → exit 0, `chunker: 008 contract`; `$PY/pytest apps/python-wiki-demo/tests -q` → all green; note both results for the report (the venv is disposable)

---

## Phase 6: User Story 4 — The documentation describes both (Priority: P3)

- [ ] T019 [P] [US4] `apps/python-wiki-demo/README.md` (research D9): the inputs table's chonky row → "the chonky splitter model (for `build --chunker chonky`)"; "Install": the base install line, then `-e ".[test,chonky]"` "for `--chunker chonky`", and the dependencies sentence → the wheel and `tokenizers==0.23.2` are the demo's own; the chonky extra (`chonky`, `transformers`, `torch`) only for the option; "In run order": the chonky fetch line commented as optional, the build comment `# a slice: the first 2,000 articles, ~17 min, the shipped recipe`; "Build an index": step 3 split into **default** (the 008 contract chunker — paragraphs → sentences → words → fragments priced by the embedder's tokenizer, `contract.py`; the same passages as the Rust build) and **`--chunker chonky`** (`chonky_chunker.py`: the 021 paragraph, the install and fetch commands, the over-window numbers 9.2 % / median 82 / p90 239 from the 021 slice, "not the shipped index"); step 6's sidecar sentence → "whose chunker block names the chunker (the contract's strings or chonky's model and revision)"; the "Not the shipped index" paragraph → "**The same index as the Rust build.** A default build cuts articles exactly as `xtriever wiki build` does, so a demo-built slice has the Rust slice's corpus identity and `measure --against target/xt-wiki-slice-rs` is PASS (`specs/023-optional-chonky-chunker/runs/parity-…`); a `--chunker chonky` build is a different index — its identity says so — and that comparison (correctly) fails."; "Measure": one line on `--against` for slice parity
- [ ] T020 [P] [US4] `build --help`, the `build.py` and `chunking.py` docstrings re-read against contracts/build.md; `grep -n "Feature 021\|chonky" apps/python-wiki-demo/wikidemo/*.py` → every mention is either the option or the label; `$PY/wikidemo build --help | grep -A2 chunker`

---

## Phase 7: Polish

- [ ] T021 Gate (quickstart Step 5): `git diff --stat main -- crates/ swift/ python/src apps/python-minimal-demo reference/ specs/*/baselines` empty; `cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo deny check` unchanged; `$PY/pytest apps/python-wiki-demo/tests -q` green (counts into the report); `$PY/pytest apps/python-wiki-demo/tests -q -m "not models"` green; `$PY/pytest apps/python-minimal-demo/tests -q` → 6 passed; `git diff --stat main | tail -1` (the line count; the verbatim share: `diff <(git show 802cf72:apps/python-wiki-demo/wikidemo/chunking.py) apps/python-wiki-demo/wikidemo/contract.py | grep -c '^[<>]'`); `grep -rn "$(hostname -s)\|$USER" specs/023-optional-chonky-chunker apps/python-wiki-demo --exclude-dir=.venv` → nothing; `git status --short` shows only the intended files
- [ ] T022 Write `specs/023-optional-chonky-chunker/report.md` (verdict; the red checkpoint; the default slice's numbers beside 019's and the Rust slice's — 8,529 passages, the identity, the parity verdict, build time; the 200-article chonky slice's counts; the no-extra venv's two results; SC-001–SC-004; "Deliberately not done": no whole-article chunker, no fallback, the full chonky slice not rebuilt and why, the minimal demo untouched) and `specs/023-optional-chonky-chunker/pr-description.md` (what changed in five lines, the parity line, the dependency move, the restored-verbatim note with the diff command, the attribution line). **⛔ Checkpoint C2 — the owner commits**, pushes and opens the PR

---

## Dependencies & Execution Order

T001 → T002 → (T003 ‖ T004 ‖ T005 ‖ T006 ‖ T007 ‖ T008) → T009 (C1, owner) → T010 → T011 →
T012 → T013 → T014 → T015 → T016 → T017 → T018 → (T019 ‖ T020) → T021 → T022 (C2, owner).

### User story completion order

US1 (the default and its parity) → US2 (the option) → US3 (the extra's refusal — needs
`chonky_chunker.ensure_extra` from US2) → US4.

### Parallel opportunities

The six test files (T003–T008); T019 ‖ T020; T015–T017 and T019 can proceed while T014's
slice build runs (~17 min); T018's no-extra venv can be created during T014 too.

## Implementation Strategy

**MVP** = Phases 1–3 (the contract chunker back as the default, parity PASS against the
Rust slice). **Rule 6 stop-points**: T009 (red), T014 (parity FAIL, wrong passage count,
over budget), T016 (a partition failure), T021 (any gate failure). Never the fixture
replay, never the parity rule, never a silent fallback between chunkers.
