# Tasks: Python Interface and Bindings

**Input**: Design documents from `/specs/011-python-bindings/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/python.md](./contracts/python.md),
[quickstart.md](./quickstart.md); the two models under `reference/models/`; the fixture index
at `swift/Xtriever/Tests/Fixtures/index` (rebuilt by the `fixture_index` example); the 007
goldens `swift/Xtriever/Tests/Fixtures/expected.json`; an arm64 Python (`/opt/homebrew/bin/python3.12`)
and `uv` (`/opt/homebrew/bin/uv`).

**Tests**: **Mandatory** (Principle II; spec FR-011). Two PRs, each tests-first (plan Rule 3
row): **PR A** — the package over the existing surface; its Python suite is committed with the
package skeleton, the search/options/threads/overhead tests green by construction (the surface
is 007's — they exist so the feature cannot change it, stated in the files), the build tests
red on the missing `create`. **PR B** — the builder exports; the Rust builder suite is committed
first, not compiling, then the exports turn it and the Python build tests green.

**Organization**: Setup → PR A red (package, suite) → US1 + US2 (the package built and
proven; one phase — the tests *are* US2 and the package *is* US1) → US3 (wheel on a clean
interpreter, CI) → PR B red (builder suite) → US4 (builder exports) → Polish. Commits: **A1**
= Phases 1–2, **A2** = Phases 3–4, **B1** = Phase 5, **B2** = Phases 6–7.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers, data-model sections
  and the contract — read them before implementing. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

Python package `python/` (`pyproject.toml`, `README.md`, `src/xtriever/__init__.py`,
`tests/`, `.venv/` git-ignored); FFI crate `crates/xtriever-ffi/` (`src/bin/uniffi-bindgen.rs`,
`src/ffi/types.rs`, `src/ffi/mod.rs`, `src/index.rs`, `tests/`); CI `.github/workflows/ci.yml`;
wheels under `target/wheels/` (git-ignored). Never a device or team identifier in any file.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: The environment, the bindgen dispatch, the package skeleton that builds.

- [ ] T001 Create the Python environment: `unset SDKROOT; /opt/homebrew/bin/uv venv --python 3.12 python/.venv && VIRTUAL_ENV=$PWD/python/.venv /opt/homebrew/bin/uv pip install "maturin>=1.15,<2" pytest`; add `python/.venv/` to `.gitignore` (with a comment naming the feature); confirm `python/.venv/bin/python -c "import platform; print(platform.machine())"` prints `arm64` (research D3)
- [ ] T002 Make the bindgen bin dispatch in `crates/xtriever-ffi/src/bin/uniffi-bindgen.rs`: if the first argument is `generate` call `uniffi::uniffi_bindgen_main()`, else `uniffi::uniffi_bindgen_swift()` (research D2; uniffi-0.32.1 `src/cli/mod.rs:8,15`); update the file's doc comment with both invocations (the Swift one unchanged, the Python one `cargo run -p xtriever-ffi --features cli --bin uniffi-bindgen -- generate --library target/release/libxtriever_ffi.dylib --language python --out-dir <dir>`); `cargo build -p xtriever-ffi --features cli --bin uniffi-bindgen` and `scripts/build-ios-package.sh --with-models --with-fixtures` still PASS (then `git checkout -- swift/Xtriever/Tests/Fixtures/expected.json` for the provenance line)
- [ ] T003 [P] Write `python/pyproject.toml` (contract §4; research D2): `[build-system] requires = ["maturin>=1.15,<2"], build-backend = "maturin"`; `[project] name = "xtriever"`, `dynamic = ["version"]`, `requires-python = ">=3.9"`, `description`, `readme = "README.md"`, `license` (the workspace's), classifiers for 3.9–3.13, macOS and Linux; `[tool.maturin] manifest-path = "../crates/xtriever-ffi/Cargo.toml"`, `bindings = "uniffi"`, `python-source = "src"`, `module-name = "xtriever._ffi"`; `[tool.pytest.ini_options] markers = ["models: needs the pinned models and the fixture index on disk"]`, `testpaths = ["tests"]`
- [ ] T004 [P] Write `python/src/xtriever/__init__.py`: a module docstring (what the package is, where the engine lives, that the module is generated); `from ._ffi.xtriever_ffi import (IndexHandle, SearchOptions, SearchResponse, Hit, HitExplain, ChunkInfo, StageReport, RerankReport, Degradation, DegradeReason, IndexInfo, LoadPath, XtrieverError)` (the builder names are added in PR B); `__version__` read from `importlib.metadata.version("xtriever")`; an explicit `__all__` of exactly those names plus `__version__`. No logic (FR-014)
- [ ] T005 Build and install the skeleton: `python/.venv/bin/maturin build --release -m python/pyproject.toml` → `target/wheels/xtriever-0.1.0-py3-none-macosx_11_0_arm64.whl`; `VIRTUAL_ENV=$PWD/python/.venv /opt/homebrew/bin/uv pip install --force-reinstall target/wheels/xtriever-*.whl`; `python/.venv/bin/python -c "import xtriever; print(xtriever.__version__, xtriever.__all__)"` prints `0.1.0` and the names; `unzip -l` shows exactly `xtriever/__init__.py`, `xtriever/_ffi/__init__.py`, `xtriever/_ffi/xtriever_ffi.py`, `xtriever/_ffi/libxtriever_ffi.dylib` plus `dist-info`

---

## Phase 2: Foundational — PR A's red suite

**Purpose**: Every Python test of the spec, committed before anything else; the build tests
red, the rest green by construction and saying so. **⛔ Commit A1 at the end of this phase.**

- [ ] T006 Write `python/tests/conftest.py`: `REPO` (three parents up from the file), `FIXTURE_INDEX = REPO/"swift/Xtriever/Tests/Fixtures/index"`, `EMBEDDER = REPO/"reference/models/all-MiniLM-L6-v2"` (override `XTRIEVER_MODEL_DIR`), `RERANKER = REPO/"reference/models/ms-marco-MiniLM-L-6-v2"` (override `XTRIEVER_RERANK_MODEL_DIR`), `GOLDENS = REPO/"swift/Xtriever/Tests/Fixtures/expected.json"`, `FIXTURE_DOCS = REPO/"reference/fixtures/005/hybrid.json"`; a `pytest_collection_modifyitems` hook that skips `models`-marked tests with a reason naming the missing path when the embedder's `model.safetensors` or the fixture index's `xtriever-pipeline.json` is absent; fixtures `handle` (open with re-ranker, `LoadPath.MMAP`, module scope) and `handle_fused` (no re-ranker); helpers `f64_bits(x) -> "%016x"`, `f32_bits(x) -> "%08x" | None`
- [ ] T007 [P] Write `python/tests/test_surface.py` (model-free): `import xtriever` works; `xtriever.__version__` equals the version in `crates/xtriever-ffi/Cargo.toml`'s workspace (read `Cargo.toml` at the repo root: `[workspace.package] version`); `xtriever.__all__` equals the contract's name list (§1 — search names in PR A, builder names added in PR B by editing the expected list); every nested class of `XtrieverError` named in contract §2 exists and `issubclass(cls, XtrieverError)`; `SearchOptions(k=10)` has `depth is None`, `rerank_depth is None`, `max_time_ms is None`, `max_items is None`, `strict is False`, `explain is False`; `LoadPath.MMAP` and `LoadPath.BUFFERED` exist
- [ ] T008 [P] Write `python/tests/test_errors.py`: model-free — `IndexHandle.open(str(tmp_path), str(tmp_path/"no-model"), None, LoadPath.MMAP)` raises `XtrieverError.Model` whose `str()` mentions `config.json`; and `pytest.raises(XtrieverError)` catches it (the base class). `models`-marked — a missing index directory with the real embedder → `XtrieverError.Corrupt` mentioning `xtriever-pipeline.json`; a directory holding a `xtriever-pipeline.json` with `"format_version": 99` copied from the fixture with the version edited → `Corrupt` mentioning `format version`; `search("x", SearchOptions(k=5, max_time_ms=1, strict=True))` → `BudgetExhausted`; a builder-only case placeholder is added in PR B (T023)
- [ ] T009 [P] Write `python/tests/test_search.py` (`models`; docstring: green at the red commit by construction — the surface is 007's): for every query in the goldens and both handles (`without_reranker` ↔ `handle_fused`, `with_reranker` ↔ `handle`), `search(q["text"], SearchOptions(k=q["k"], rerank_depth=q["rerank_depth"], explain=True))`; assert the hit list equals the golden's on `(external_id, f64_bits(score), f32_bits(rerank_score), f32_bits(explain.bm25_score), f32_bits(explain.dense_score), explain.rerank_rank)` in order, and the stage report's `lexical_candidates`, `dense_candidates`, `degraded is None` ↔ `stages.degraded == false`, `rerank.candidates/scored` where the golden has `rerank`; count 16 pairs; `info()` — `documents == 40`, `format_version == 2`, `embedder_fingerprint` equals the goldens' `info.embedder_fingerprint`, `reranker_model_id` present on `handle` and `None` on `handle_fused`
- [ ] T010 [P] Write `python/tests/test_options.py` (`models`): strict vs non-strict at `max_time_ms=1` — non-strict returns hits and `stages.degraded` or `stages.rerank.skipped` names a budget reason (`DegradeReason.BUDGET_EXCEEDED`), strict raises `BudgetExhausted`; `rerank_depth=0` → `stages.rerank is None or stages.rerank.scored == 0` and no hit has `rerank_score`; `explain=False` → every `hit.explain is None`; `k=0` → `hits == []`; `depth=1` → `stages.lexical_candidates <= 1`; a query of 2,000 words is accepted (the engine truncates); a non-ASCII query returns without error
- [ ] T011 [P] Write `python/tests/test_threads.py` (`models`, SC-005): start a thread that performs 20 × `sum(range(100_000))` and records its wall time; call `handle.search` (re-ranked, depth 20) on the main thread; join; assert the thread's wall time < 25 % of the search's wall time (the lock was released) and both completed; a second test with two threads each searching 5 times on the same handle — every call returns 10 hits, none raises
- [ ] T012 [P] Write `python/tests/test_overhead.py` (`models`, SC-004): warm up once; for each golden query, time `handle.search` with `time.perf_counter_ns()` and take `response.elapsed_ms`; overhead ratio = `(wall_ms − elapsed_ms) / elapsed_ms`; assert the **median** over the 8 queries × 3 repetitions ≤ 0.05 and print the ratios
- [ ] T013 [P] Write `python/tests/test_build.py` (`models`, SC-007; **red until PR B**): from `FIXTURE_DOCS` build `IndexConfig(fields=[FieldDef(name, kind, indexed, stored, boost) for each schema field with FieldKind.TEXT(analyzer=…) / FieldKind.KEYWORD() …], dense_fields=…)` (`candidate_depth`, `rrf_k`, `rerank_depth` defaults), `IndexHandle.create(str(tmp_path/"idx"), config, EMBEDDER, RERANKER, LoadPath.MMAP)`, `add([Document(external_id, fields={name: FieldValue.TEXT(v["Text"]) | FieldValue.KEYWORD(v["Keyword"]) …}, chunk=ChunkInfo(...) | None)])` in fixture order, `commit()`; then for every golden query both with and without the re-ranker (open a second handle without) the hits equal the goldens exactly as in `test_search.py` (SC-007); `info().documents == 40`; `contains("d001")`; a second test: `add` a replaced `d001` with a new text then `contains` still true and a search for the new text does not find it until `commit()`; `delete(["d001"])` → `contains` true until `commit()`, false after; reopen → same; a third test: `add_embedded` with a wrong-width vector → `XtrieverError.DimensionMismatch`, mismatched lengths → `Schema`; a fourth: `create` in a non-empty directory → `Corrupt`; a dense field not in the schema → `Schema`
- [ ] T014 Run the red checkpoint (quickstart Step 1): `python/.venv/bin/pytest python/tests -q` → `test_build.py` fails on `AttributeError: … has no attribute 'create'` / missing `IndexConfig`, everything else passes (with the models) — record the counts in `specs/011-python-bindings/report.md` under "Red checkpoint (A1)"; `python/.venv/bin/pytest python/tests -q -m "not models"` → surface + the model-free error test pass. **⛔ Commit A1**: `.gitignore`, the bin, `python/` (no `.venv`, no wheel), the report stub

---

## Phase 3: User Story 1 + 2 — Search from Python, provably the engine's (Priority: P1)

**Goal**: The package as built in Phase 1 with its README; the suite green for the existing
surface (it already is — this phase documents and verifies, and adds nothing to the engine).

**Independent Test**: quickstart Step 2's Python lines with the models present.

- [ ] T015 [US1] Write `python/README.md` (written for a Python developer): install from a wheel (the build command, `pip install target/wheels/xtriever-*.whl`), the models (`scripts/fetch-model.sh` twice), a first search (open, `SearchOptions(k=5, explain=True)`, reading hits / explanation / stage report), errors (`XtrieverError` and its kinds), threads (the lock is released; one handle serialises), platforms (macOS arm64, Linux x86_64; the x86_64-Python-on-Apple-silicon `OSError`, research D3), the build section left as "PR B" until T027 fills it
- [ ] T016 [US2] Verify the goldens end to end after A1: `python/.venv/bin/pytest python/tests -q -m models -k "search or options or threads or overhead or errors"` all green; paste the overhead ratios and the thread timing into the report

---

## Phase 4: User Story 3 — Installation is one step (Priority: P2)

**Goal**: A wheel that installs on a clean interpreter without a toolchain; a Linux CI job that
builds it and runs the model-free subset.

**Independent Test**: quickstart Steps 3 and 5.

- [ ] T017 [US3] Clean-interpreter install (SC-001): `/opt/homebrew/bin/uv venv --python 3.13 /tmp/xt313 && VIRTUAL_ENV=/tmp/xt313 /opt/homebrew/bin/uv pip install target/wheels/xtriever-*.whl pytest`; `PATH=/usr/bin:/bin /tmp/xt313/bin/pytest python/tests -q` (no `cargo` on PATH) → same results as T016; record Python 3.13's result in the report
- [ ] T018 [US3] Add the `python` job to `.github/workflows/ci.yml` (research D6): `runs-on: ubuntu-latest`; `permissions: contents: read, pull-requests: read`; `dorny/paths-filter@v3` with `python: ['crates/xtriever-ffi/**', 'python/**', 'Cargo.lock', '.github/workflows/ci.yml']`; when true: `rustup toolchain install`, `Swatinem/rust-cache@v2` (`key: python-wheel`), `astral-sh/setup-uv@v5` (pinned major), `uv venv .venv && uv pip install "maturin>=1.15,<2" pytest`, `.venv/bin/maturin build --release -m python/pyproject.toml`, `uv pip install target/wheels/xtriever-*.whl`, `.venv/bin/pytest python/tests -q -m "not models"`, and `ls -la target/wheels` (the `manylinux` tag in the log); a comment block stating: model-free by the standing rule, no dataset, no macOS job, the 10-minute budget (SC-006). **⛔ Commit A2** (Phases 3–4); the owner pushes; the job's wall time and wheel tag are read from the first run and recorded in the report (over 10 minutes is stop-and-report, not a threshold to move)

---

## Phase 5: Foundational — PR B's red suite (the builder)

**Purpose**: The Rust builder suite against the FFI, committed not compiling. **⛔ Commit B1
at the end of this phase.**

- [ ] T019 Write `crates/xtriever-ffi/tests/build.rs` (model-backed, `#[ignore]`, release; uses `tests/support` for the model dirs and `fixture_docs()`): (a) `fixture_built_through_the_wire_equals_the_goldens` — `IndexHandle::create(dir, IndexConfig { fields: <the 005 schema mapped to wire FieldDef>, dense_fields, candidate_depth: 100, rrf_k: 60, rerank_depth: 20 }, embedder_dir, Some(reranker_dir), LoadPath::Mmap)`, `add(<the 40 fixture docs as wire Documents with FieldValue and ChunkInfo>)`, `commit()`, then for every golden query with/without re-ranker the hits equal `swift/Xtriever/Tests/Fixtures/expected.json` bit for bit (reuse `parity.rs`'s comparison shape); (b) `staged_changes_are_invisible_until_commit` — replace, delete, `contains` before/after `commit`, reopen; (c) `refusals_are_the_engines` — `create` in a non-empty dir → `Corrupt`; a `dense_fields` entry not in `fields` → `Schema`; a `Keyword` dense field → `Schema`; `add_embedded` with a 3-wide vector → `DimensionMismatch`; `add_embedded` with 2 docs and 1 vector → `Schema`; `add` on a handle opened on a `0o555` directory (unix) → `Io` mentioning `read-only`; an empty `external_id` → `Schema`; (d) `merge_leaves_one_segment_and_identical_hits` — `merge()` then the same hits as (a). Does not compile: no `create`, `IndexConfig`, … (the recorded red state)
- [ ] T020 Add the builder names to the Python surface expectations: `python/tests/test_surface.py`'s expected `__all__` gains `IndexConfig`, `FieldDef`, `FieldKind`, `FieldValue`, `Document`; `python/tests/test_errors.py` gains the `models`-marked `create` refusals (`Corrupt` non-empty dir, `Schema` bad dense field). `pytest -m "not models"` now fails on the names (red). **⛔ Commit B1**: `tests/build.rs`, the two Python test edits

---

## Phase 6: User Story 4 — Build an index from Python (Priority: P1)

**Goal**: The wire types and the seven exports (research D4, data-model "New wire types" and
"New operations"); Python re-exports; suites green.

**Independent Test**: quickstart Step 2 — `tests/build.rs` and `test_build.py` green.

- [ ] T021 [US4] Add the wire types to `crates/xtriever-ffi/src/ffi/types.rs` (doc comments on every item; `missing_docs`): `#[derive(uniffi::Enum)] FieldKind { Text { analyzer: String }, Keyword, U64, I64, F64, Bool, DateMillis }`; `#[derive(uniffi::Record)] FieldDef { name: String, kind: FieldKind, #[uniffi(default = true)] indexed: bool, #[uniffi(default = false)] stored: bool, #[uniffi(default = 1.0)] boost: f32 }`; `#[derive(uniffi::Record)] IndexConfig { fields: Vec<FieldDef>, dense_fields: Vec<String>, #[uniffi(default = 100)] candidate_depth: u32, #[uniffi(default = 60)] rrf_k: u32, #[uniffi(default = 20)] rerank_depth: u32 }`; `#[derive(uniffi::Enum)] FieldValue { Text(String), Keyword(String), U64(u64), I64(i64), F64(f64), Bool(bool), DateMillis(i64) }`; `#[derive(uniffi::Record)] Document { external_id: String, fields: HashMap<String, FieldValue>, #[uniffi(default = None)] chunk: Option<ChunkInfo> }`; `cargo build -p xtriever-ffi` clean
- [ ] T022 [US4] Add the conversions and write operations to `crates/xtriever-ffi/src/index.rs` (`#![deny(unsafe_code)]` stays): `impl From<FieldKind> for xtriever_core::FieldKind` (`Text { analyzer }` → `Text(AnalyzerId(analyzer))`), `From<FieldDef> for xtriever_core::FieldDef`, `From<IndexConfig> for HybridConfig` (`candidate_depth`/`rerank_depth` as `usize`; `Schema { fields }`), `From<FieldValue> for xtriever_core::Value`, `From<Document> for SourceDocument` (fields into `BTreeMap<FieldName, Value>`; wire `ChunkInfo` → core `ChunkInfo` with `byte_range = byte_start.zip(byte_end)`); `pub(crate) fn create(index_dir, config, embedder_dir, reranker_dir, load_path) -> Result<Inner, XtrieverError>` — load the embedder (timed), `HybridIndex::create(dir, config.into(), Box::new(embedder))`, attach the re-ranker as `open` does (factor the model loading shared with `open` into one helper); `pub(crate) fn add(inner, docs)`, `add_embedded(inner, docs, vectors)` (length mismatch → `XtrieverError::Schema` with "N documents but M vectors"), `delete(inner, ids)`, `commit`, `merge`, `contains` — each taking the `Mutex` guard as `search` does (poisoned → the existing `poisoned()` error)
- [ ] T023 [US4] Add the exports to `crates/xtriever-ffi/src/ffi/mod.rs` on `impl IndexHandle` under `#[uniffi::export]`: `#[uniffi::constructor] create(...)`, `add`, `add_embedded`, `delete`, `commit`, `merge`, `contains` with doc comments stating the engine's semantics (committed view until commit; unknown deletes ignored; read-only handle → `Io`); `cargo clippy -p xtriever-ffi --all-targets -- -D warnings` clean; `cargo nextest run -p xtriever-ffi` (model-free) green; `cargo nextest run -p xtriever-ffi --release --run-ignored only -j 1` green including `tests/build.rs`. **⛔** Any golden mismatch in (a)/(d) is stop-and-report
- [ ] T024 [US4] Update `python/src/xtriever/__init__.py`: import and re-export `IndexConfig`, `FieldDef`, `FieldKind`, `FieldValue`, `Document`; `__all__` accordingly (matches T020's expectation)
- [ ] T025 [US4] Rebuild and install the wheel (T005's commands) and run the whole Python suite: `python/.venv/bin/pytest python/tests -q` → all green including `test_build.py` (SC-007) and the new error cases; `-m "not models"` green; paste counts into the report
- [ ] T026 [US4] Swift untouched (quickstart Step 4): `unset SDKROOT; scripts/build-ios-package.sh --with-models --with-fixtures` PASS; `cd swift/Xtriever && xcodebuild test -scheme Xtriever -destination 'platform=iOS Simulator,id=822F3C90-5124-432B-B84A-75426A04722D' -configuration Release ARCHS=arm64 -skip-testing:XtrieverTests/DeviceMeasurementTests` → 18 / 18; `git checkout -- swift/Xtriever/Tests/Fixtures/expected.json`; `git diff --stat main -- swift/ apps/` empty
- [ ] T027 [US4] Fill the README's build section in `python/README.md`: schema, documents, `create` / `add` / `commit`, replace and delete, `add_embedded` for pre-computed vectors, reopening an index and adding to it, the refusals; keep it to what the tests exercise

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T028 [P] Update `crates/xtriever-ffi/src/lib.rs`'s crate docs: a "Feature 011" section — the second foreign surface (Python, generated the same way), the builder exports, the bindgen dispatch; and `CLAUDE.md`'s gate list gains one line for the Python suite (`python/.venv/bin/pytest python/tests -q`, models present) as a local obligation, CI model-free
- [ ] T029 Full gate (quickstart Step 6): fmt; clippy host + `x86_64-pc-windows-msvc`; `cargo nextest run --workspace`; deny; iOS / iOS-sim / Android checks; no-stubs; `git diff --stat main -- crates/xtriever-core deny.toml crates/xtriever-lexical crates/xtriever-dense crates/xtriever-rerank crates/xtriever-pipeline swift/ apps/` empty; grep the tree for device/team identifiers (none)
- [ ] T030 Write `specs/011-python-bindings/report.md` (verdict; what was built; the golden table; SC-001–SC-007 with numbers — the overhead ratios, the thread timing, the CI time and wheel tag, the 3.12/3.13 results; findings incl. the x86_64 Python (D3), the bindgen dispatch, anything the runs showed; gate; "Deliberately not done": async facade, model downloader, PyPI, Windows) and `specs/011-python-bindings/pr-description.md` (written for the reviewer; PR A and PR B contents and line counts; the new exports and their semantics; what Swift sees; the CI job; the attribution lines). **⛔ Commit B2** (Phases 6–7); the owner merges

---

## Dependencies & Execution Order

- **Phase 1 → 2 → A1**: T001 → T002 → (T003 ‖ T004) → T005 → T006 → (T007–T013 in parallel, all different files) → T014.
- **Phase 3–4 → A2**: T015 ‖ T016 → T017 → T018.
- **Phase 5 → B1**: T019 ‖ T020 (after A2).
- **Phase 6**: T021 → T022 → T023 → T024 → T025 → T026 ‖ T027.
- **Phase 7 → B2**: T028 any time after T023; T029 → T030.

### User story completion order

US1 + US2 (Phases 1–3, one increment: the package and its proof) → US3 (wheel on a clean
interpreter, CI) → US4 (the builder; its Python tests were written in Phase 2 and go green
here).

### Parallel opportunities

T003 ‖ T004; T007–T013 (seven test files); T015 ‖ T016; T019 ‖ T020; T026 ‖ T027; T028 ‖ T029's
prerequisites.

## Implementation Strategy

**MVP** = PR A (Phases 1–4): a Python developer can install the wheel and search an index with
the engine's exact results, on both platforms, with CI proving the build. PR B adds building.

**Rule 6 stop-points**: T014/T020 (the red commits must be red where stated), T016 (any golden
mismatch), T017 (an install or import failure), T018 (CI over 10 minutes or a download it
should not make), T023/T025 (SC-007 mismatch or a refusal of the wrong kind), T026 (any
change in the Swift suite), T029 (any gate failure). At each: stop, report, do not adjust.
