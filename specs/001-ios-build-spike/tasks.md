# Tasks: iOS Build Spike — tantivy, tokenizers, candle on Device

**Input**: Design documents from `/specs/001-ios-build-spike/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: **REQUIRED, not optional.** Principle II is NON-NEGOTIABLE and FR-013 requires the
acceptance tests to be written and committed **failing** before any implementation. Test tasks are
ordered first within every phase, and Phase 2 exists specifically to land them red.

**Gate status**: Constitution Check **PASSES** on all 14 rows against constitution **v1.1.0**. ADRs [0001](../../docs/adr/0001-pin-candle-0-9-2.md) and [0002](../../docs/adr/0002-unsafe-mmap-safetensors-measurement.md) accepted 2026-09-10; [0003](../../docs/adr/0003-uniffi-scaffolding-requires-unsafe-allow.md) and [0004](../../docs/adr/0004-ignore-rustsec-2024-0436.md) accepted 2026-09-11. Principle VII's unsafe clause was expanded to admit `xtriever-ffi` (v1.1.0), so ADR-0003 is the rule rather than a deviation.

**Organization**: Tasks are grouped by user story so each is independently implementable and testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- Exact file paths are given in every task

## Path Conventions

Per [plan.md](./plan.md) "Source Code": all spike Rust lands in `crates/xtriever-ffi/` behind a
non-default `spike` feature. The five pure crates and the other stage crates stay **untouched** —
that is what keeps the Principle III gate green, so any task that would edit them is a defect in this
plan, not a licence.

## ⚠️ Read before starting

Three constraints from Phase 0 are correctness requirements, not preferences. Getting any of them
wrong produces a *false* failure that looks like an iOS problem:

1. **Toolchain provenance** (research D14) — `/opt/homebrew/bin/cargo` shadows rustup's and ignores
   `rust-toolchain.toml`. Every cross-target verdict recorded without T001's guard is void.
2. **One writer thread** (research D5) — tantivy's multi-threaded writer does not allocate `DocId`s
   reproducibly, and ties break on `DocAddress`, so the golden ranking is not comparable otherwise.
3. **256-token truncation override** (research D6) — `tokenizer.json` bakes in 128; the reference is
   256. Skip this and Rust and Python disagree before iOS is involved.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Pin the measured dependency set and make the toolchain trap impossible to fall into.

- [X] T001 Add toolchain provenance guard at `scripts/check-toolchain.sh` that fails loudly unless `command -v cargo` resolves under `$HOME/.cargo/bin`, `cargo --version` does not contain `(Homebrew)`, and `rustup target list --installed` lists both `aarch64-apple-ios` and `aarch64-apple-ios-sim` (research D14)
- [X] T002 Add the pinned dependencies to `crates/xtriever-ffi/Cargo.toml` using `cargo add` (never hand-written versions, Principle VII): `tantivy@0.26.2 --no-default-features --features mmap,stopwords,lz4-compression,stemmer`, `tokenizers@0.23.2 --no-default-features --features fancy-regex`, `candle-core@0.9.2`, `candle-nn@0.9.2`, `candle-transformers@0.9.2`, `uniffi@0.32.1`, `thiserror` (workspace), all as `optional = true`
- [X] T003 Add a comment at each candle dependency line in `crates/xtriever-ffi/Cargo.toml` pointing to `docs/adr/0001-pin-candle-0-9-2.md`, so a future `cargo add candle-core` upgrade to 0.11.0 is caught in review (ADR-0001 follow-up action)
- [X] T004 Configure `crates/xtriever-ffi/Cargo.toml` per [contracts/ffi-surface.md](./contracts/ffi-surface.md): `[lib] crate-type = ["lib", "cdylib", "staticlib"]` (**`lib` is required** or `tests/` cannot link and fails as a compile error), `[features] default = []`, `spike = [...]`, `cli = ["uniffi/cli"]`, and `[[bin]] name = "uniffi-bindgen"` with `required-features = ["cli"]`
- [X] T005 Create `crates/xtriever-ffi/src/bin/uniffi-bindgen.rs` containing `fn main() { uniffi::uniffi_bindgen_swift() }`
- [X] T006 Commit `Cargo.lock` and verify `cargo deny check` passes — **now passes** (`advisories ok, bans ok, licenses ok, sources ok`). Required one authorized `deny.toml` change: `ignore = ["RUSTSEC-2024-0436"]` (`paste` unmaintained, via `candle-core 0.9.2 → gemm 0.19.0`), recorded in [ADR-0004](../../docs/adr/0004-ignore-rustsec-2024-0436.md). The `onig_sys` ban is untouched, so ADR-0001's tripwire still fires on a candle upgrade

- [X] T001a Add `.gitignore` entries for the Python venv, `reference/models/`, `*.safetensors`, `/build/`, and macOS/Xcode artefacts (the file was one line, `target/`, with no trailing newline)
- [X] T001b Write `docs/adr/0003-uniffi-scaffolding-requires-unsafe-allow.md` and amend ADR-0002 condition 5 — uniffi's macros emit `unsafe` at 38 sites, which `unsafe_code = "deny"` rejects; without this the crate cannot compile at all
- [X] T001c Add `cargo check -p xtriever-ffi --features spike` for both iOS triples to the `cross-check` CI job — without it CI gives zero coverage of the spike's iOS build and SC-001's verdicts silently rot

- [X] T001d Amend the constitution to **v1.1.0** — expand Principle VII's unsafe clause to admit `xtriever-ffi`'s uniffi scaffolding, per Governance (PR + ADR-0003 + human approval + MINOR bump); propagate the wording to `CLAUDE.md`
- [X] T001e Add `ignore = ["RUSTSEC-2024-0436"]` to `deny.toml` with rationale and review triggers in [ADR-0004](../../docs/adr/0004-ignore-rustsec-2024-0436.md) — one advisory id, deliberately not a blanket `unmaintained = "allow"`

**Checkpoint**: dependencies pinned and provably C-free; nothing implemented yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Land the oracles **red**. Principle II requires tests committed failing before code.

**⚠️ CRITICAL**: No user story work may begin until T017 confirms the suite fails for the right reason.

- [X] T007 Create `reference/gen_001_fixtures.py` that downloads `sentence-transformers/all-MiniLM-L6-v2` at the pinned revision `1110a243fdf4706b3f48f1d95db1a4f5529b4d41` and asserts `model.safetensors` is **exactly 90,868,376 bytes** with dtype **F32** read from the safetensors header, failing hard on mismatch (FR-016, FR-033, research D7)
- [X] T008 Extend `reference/gen_001_fixtures.py` to emit `FixtureCorpus` to `reference/fixtures/001/corpus.json` from a fixed `--seed`: **exactly 1,000** `FixtureDocument` entries, each with a unique stable `external_id` and ASCII-only `text` of **one to three sentences**; plus one `query` that **MUST match at least one document** and one `sentence` (data-model.md `FixtureCorpus`/`FixtureDocument`)
- [X] T009 Extend `reference/gen_001_fixtures.py` to emit `ExpectedTokens` to `reference/fixtures/001/tokens.json`, **overriding the tokenizer's baked-in truncation and padding to max_length 256** before encoding, and recording `input_ids`, `attention_mask` and an all-zero `token_type_ids` (research D6, data-model.md `ExpectedTokens`)
- [X] T010 Extend `reference/gen_001_fixtures.py` to emit `ExpectedEmbedding` to `reference/fixtures/001/embedding.json` using attention-mask-weighted **mean** pooling followed by **L2 normalization**, producing **384** floats, and recording `cosine_min = 0.9999` and `max_abs_diff = 1e-3` (research D8, data-model.md `ExpectedEmbedding`)
- [X] T011 Extend `reference/gen_001_fixtures.py` to emit an **independent Python BM25 transcription** to `reference/fixtures/001/bm25_reference.json` — `k1=1.2`, `b=0.75`, the Lucene IDF, and the `FIELD_NORMS_TABLE` u8 fieldnorm quantization (all public in tantivy 0.26.2) — compared ids-and-order exact with scores within `score_rel_tol = 1e-5`. **Python cannot build a tantivy index**, so `ExpectedRanking` is minted separately by a Rust example and cross-checked by this script at mint time; see deviation D-001 in `report.md` (Problem A)
- [X] T012 [P] Write failing acceptance test `crates/xtriever-ffi/tests/index_query.rs` asserting `spike_index` reports `segment_count == 1` and `documents_indexed == 1000`, and that `spike_query` returns a ranking **exactly equal** to `ranking.json` — order, `external_id`s and `score`s (FR-014, no tolerance)
- [X] T013 [P] Write failing acceptance test `crates/xtriever-ffi/tests/tokenize.rs` asserting the Rust token sequence is **exactly equal** to `tokens.json` (FR-015). This test must be capable of failing independently of the embedding test
- [X] T014 [P] Write failing acceptance test `crates/xtriever-ffi/tests/embed.rs` asserting `spike_embed` output is within `cosine >= 0.9999` **and** `max abs diff <= 1e-3` of `embedding.json`, and that the vector has length **384** (FR-015)
- [X] T015 [P] Write failing acceptance test `crates/xtriever-ffi/tests/load_paths.rs` asserting `LoadPath::Buffered` and `LoadPath::Mmapped` produce the **same embedding bit-for-bit** — this is ADR-0002 condition 4, and its failure means the mmap path is deleted, not accommodated
- [X] T016 [P] Write failing property test `crates/xtriever-ffi/tests/determinism.rs` asserting that indexing the same corpus twice with one writer thread yields identical rankings for the same query (Principle II invariant clause, Principle VI)
- [X] T017 Run `cargo nextest run -p xtriever-ffi --features spike` and confirm the suite **FAILS for missing implementation, not for a broken fixture** — commit the tests in this red state (FR-013, Rule 4)

- [X] T007a Add `scripts/setup-reference-venv.sh`, `reference/.python-version` (3.12) and hash-pinned `reference/requirements-001.txt` — the system interpreter is 3.14, which torch publishes no wheel for, and is PEP 668 externally-managed. The generator refuses to run outside the pinned venv
- [X] T011a Write `crates/xtriever-ffi/tests/bm25_parity.rs` — the Principle II cross-implementation oracle (ids/order exact, scores within `score_rel_tol = 1e-5`), distinct from `index_query.rs`'s host↔device determinism oracle (report.md D-001)
- [X] T012a Write `crates/xtriever-ffi/tests/support/mod.rs` and `tests/fixtures_valid.rs` — the latter is **not** spike-gated and must be **GREEN**, which is what makes "fails for the right reason" mechanical rather than a judgement call. It also hashes every fixture against `manifest.json`, so hand-editing a golden to make a test pass turns red and names the file (FR-028)
- [X] T012b Expose `pub mod spike` and add a crate-internal `tokenize()` seam in `spike/embed.rs` so token parity can be asserted before the embedding comparison. Not `#[uniffi::export]`ed, so FR-006's three-operation FFI cap is intact

**Checkpoint**: oracles committed failing. Implementation may now begin.

---

## Phase 3: User Story 1 — The three stacks cross-compile with no C/C++ dependencies (Priority: P1) 🎯 MVP

**Goal**: Establish, per crate per target, that the pinned stack builds for both iOS triples with no C/C++ build dependency reachable from the pure crates.

**Independent test**: Runnable on a macOS host with **no iPhone and no provisioning profile**. Delivers the feature-set matrix on its own.

- [X] T018 [US1] Create `crates/xtriever-ffi/src/spike/error.rs` defining `SpikeError` as a `thiserror` enum with the five fielded variants in [contracts/ffi-surface.md](./contracts/ffi-surface.md) (`IndexIo`, `QueryParse`, `Model`, `Tokenize`, `Inference`) and `#[derive(uniffi::Error)]` — no `unwrap`/`expect`/`panic!`/`todo!` anywhere (Principle VII)
- [X] T019 [US1] Create `crates/xtriever-ffi/src/lib.rs` with `uniffi::setup_scaffolding!()` and the module wiring, placing every `#[cfg(feature = "spike")]` **before** its `#[uniffi::export]` attribute, never inside the block (research risk R3)
- [X] T020 [US1] Run `scripts/check-toolchain.sh`, then `cargo check -p xtriever-ffi --features spike` for `aarch64-apple-ios` and `aarch64-apple-ios-sim`, recording a **separate verdict per crate per target** — 8 minimum (FR-001, SC-001)
- [X] T021 [US1] Verify per target that `cargo tree -p xtriever-ffi --features spike -e normal,build --target <triple>` matches none of `onig|zstd|-sys v|cc v[0-9]`; note `esaxx-rs` **with no features** is expected and is not a violation, and the decisive signal is the absence of `cc` (FR-003, research D2)
- [X] T022 [US1] Verify no C dependency is reachable from `xtriever-core`, `xtriever-analysis`, `xtriever-pipeline`, `xtriever-ltr` or `xtriever-eval` via `cargo tree -p <crate> -e normal,build` (FR-003, Principle III)
- [X] T023 [US1] Record the `FeatureSetMatrix` into `specs/001-ios-build-spike/report.md`, including for each disabled feature the native dependency it would have pulled — at minimum tantivy's `columnar-zstd-compression → zstd-sys` (FR-002, data-model.md `FeatureSetMatrix`)
- [X] T024 [US1] Record resolved versions of all six dependencies in `specs/001-ios-build-spike/report.md`, taken from `Cargo.lock` rather than from `Cargo.toml` requirements (FR-004)
- [X] T025 [US1] Build the real static library — `cargo build -p xtriever-ffi --features spike --release --target aarch64-apple-ios` — and confirm with `lipo -info` that it is a genuine `arm64` iOS slice. `cargo check` does no codegen or linking, so US1 is **not** complete without this
- [X] T026 [US1] *(condition did not fire)* — all 8 crate/target verdicts passed and the release staticlib linked, so there was no FAILED verdict to record. Closed as N/A rather than left open. Nothing was vendored, patched or forked (FR-005)

**Checkpoint**: US1 independently complete. The most valuable negative result, if there is one, now exists in writing.

---

## Phase 4: User Story 2 — The three operations are callable from Swift (Priority: P2)

**Goal**: Prove the three operations' data crosses the language boundary and that Rust errors arrive as catchable Swift errors.

**Independent test**: Entirely in the iOS simulator. No physical device required.

- [X] T027 [P] [US2] Implement `spike_index` in `crates/xtriever-ffi/src/spike/index.rs`: `Index::create_in_dir`, text field with `IndexRecordOption::WithFreqsAndPositions`, id field `STRING | STORED`, and **`Index::writer_with_num_threads(1, budget)`** with `budget` at or above tantivy's 15 MB per-thread minimum — never `Index::writer` (research D5). Add documents in list order, then commit
- [X] T028 [P] [US2] Implement `spike_query` in `crates/xtriever-ffi/src/spike/query.rs`: `QueryParser::for_index`, `TopDocs::with_limit(k).order_by_score()`, resolving each stored `external_id` via `searcher.doc::<TantivyDocument>(addr)`, and returning `RankedHit { external_id, score, segment_ord, doc_id }`. An **empty result is a `QueryParse` error, not a pass** (spec edge case)
- [X] T029 [US2] Implement the safe weight-loading path in `crates/xtriever-ffi/src/spike/embed.rs` using `from_buffered_safetensors(data, DTYPE, &Device::Cpu)` with `VarBuilder` root prefix `""`, asserting the 90,868,376-byte size and recorded SHA-256 before loading (FR-016, ADR-0002 condition 1 — this path is primary)
- [X] T030 [US2] Implement tokenization in `crates/xtriever-ffi/src/spike/embed.rs`, **overriding truncation and padding to 256 tokens** after `Tokenizer::from_file` and passing an all-zero `token_type_ids` tensor, which candle 0.9.2's `BertModel::forward(&input_ids, &token_type_ids, Some(&attention_mask))` requires positionally (research D6, D8)
- [X] T031 [US2] Implement mean pooling weighted by the attention mask followed by L2 normalization in `crates/xtriever-ffi/src/spike/embed.rs`, returning **384** floats, and set `CANDLE_NUM_THREADS=1` so the rayon pool does not size itself to the device core count (research D8, D15)
- [X] T032 [US2] Implement the `Mmapped` branch in `crates/xtriever-ffi/src/spike/embed.rs` as a single `unsafe { VarBuilder::from_mmaped_safetensors(&[path], DTYPE, &Device::Cpu) }` — first argument is a **slice** — under an `#[allow(unsafe_code)]` scoped to that one item, preceded by a `// SAFETY:` comment stating that the mapped file is a read-only resource in the signed app bundle and is neither modified nor truncated for the `VarBuilder`'s lifetime (ADR-0002 conditions 2 and 3)
- [X] T033 [US2] Run `cargo nextest run -p xtriever-ffi --features spike` on the host and confirm T012–T016 now **pass**, including T015's bit-for-bit agreement between the two load paths (ADR-0002 condition 4)
- [X] T034 [US2] Generate the Swift bindings with the three `cargo run -p xtriever-ffi --features cli --bin uniffi-bindgen` invocations in [quickstart.md](./quickstart.md) Step 3 (`--swift-sources`, `--headers`, then `--modulemap --module-name xtriever_ffiFFI --modulemap-filename module.modulemap` — **not** `--xcframework`, which emits a framework module a static-library slice cannot satisfy; see F-006)
- [X] T035 [US2] Assemble the XCFramework across device and simulator slices with `xcodebuild -create-xcframework`. This step is **not** documented in the UniFFI guide (research risk R2) — if it costs materially more than expected, record a `Finding` rather than absorbing it silently
- [X] T036 [US2] Swift harness at `harness/ios/XtrieverSpike/` — a SwiftPM package with an XCTest target rather than a three-button app; rationale in `harness/ios/README.md`. **4/4 green on the iOS 26.5 simulator**: indexing 1,000 documents in one segment, a 10-hit descending ranking, and a 384-dim L2-normalized embedding all cross the boundary
- [X] T037 [US2] Typed-error propagation verified on the simulator: a missing model directory, an unopened index, and a query matching nothing each arrive as a **caught** `SpikeError` — no process abort (FR-008)

- [X] T028a [US2] Add `crates/xtriever-ffi/examples/gen_ranking.rs` and `--emit-ranking` to the generator: mint `ranking.json` from a real host tantivy run, cross-checked against the independent Python BM25 before anything is written (report.md D-001). Worst relative score difference **8.3e-08** against a 1e-5 tolerance

- [X] T034a Add `scripts/build-ios-harness.sh` — staticlibs for both slices, bindings, and XCFramework assembly in one reproducible command (FR-031 makes the harness durable, which means reproducible rather than committed: the XCFramework is ~262 MB)

**Checkpoint**: US2 independently complete. Packaging and boundary proven without a device.

---

## Phase 5: User Story 3 — The numbers are measured on a physical iPhone (Priority: P3)

**Goal**: Record installed size, peak memory footprint and per-operation wall time on real hardware, and reach a verdict against the 300 MB ceiling.

**Independent test**: Install the US2 app on one physical iPhone and run the three operations.

- [X] T038 [P] [US3] Implement footprint sampling in `harness/ios/XtrieverSpike/Measure.swift` via `task_info(mach_task_self_, TASK_VM_INFO, …)` reading `phys_footprint`, checking the returned count against `TASK_VM_INFO_REV1_COUNT` first since REV0 predates the field (research D9)
- [X] T039 [US3] Peak footprint — **`proc_pid_rusage` is not reachable from Swift on iOS**, confirming research risk R1. Rather than `dlsym` an undeclared symbol, the peak comes from `task_vm_info.ledger_phys_footprint_peak` cross-checked against sampling, and `peakMethod` records which produced the number. Note it is a process-**lifetime** high-water mark, so per-operation rows are cumulative and only the run maximum is a verdict input
- [X] T040 [P] [US3] Implement wall-time measurement in `harness/ios/XtrieverSpike/Measure.swift` via `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)` — not `ContinuousClock`, which keeps counting while the system sleeps (research D11)
- [X] T041 [P] [US3] Capture `os_proc_available_memory()` before and after each operation in `harness/ios/XtrieverSpike/Measure.swift` so the device's **own** limit can be derived and reported **separately from** the 300 MB constitutional ceiling — the two must never be conflated (research D9, data-model.md `DeviceRun`)
- [X] T042 [US3] Record `device_model`, `ios_version`, `build_config`, `thermal_state` from `ProcessInfo.thermalState`, and `candle_num_threads` (**MUST be 1**) into each `DeviceRun` in `harness/ios/XtrieverSpike/` (FR-019, data-model.md `DeviceRun`)
- [X] T043 [US3] Configure the harness scheme for **Release**, "Debug executable" **off**, code coverage and all runtime sanitizers **off**, per Apple's performance-test guidance (research D11, FR-019)
- [X] T044 [US3] Run the harness on a physical iPhone: index 1,000 documents, run the query, and embed the sentence, recording wall time and footprint for **index, model load, query and embed separately** (FR-017)
- [X] T045 [US3] Run model load **twice**, once per `LoadPath`, recording both footprints — this comparison is the entire justification for ADR-0002 (FR-017, ADR-0002)
- [X] T046 [US3] Verify the on-device oracles in this order: token sequence exact → `segment_count == 1` → ranking exact → embedding within tolerance. Checking tokens first is what stops a tokenizer disagreement being misfiled as an iOS embedding failure (FR-014, FR-015, research D6)
- [X] T047 [US3] Compare peak footprint against the 300 MB ceiling and record the verdict **with the measured value**, not a rounded or best-case number (FR-020, SC-004)
- [X] T048 [US3] Repeat the entire device run at least once more on the same commit and device, recording all runs, so the reproducibility band is measured rather than assumed (FR-022, SC-007)
- [X] T049 [US3] Installed size — **untested by decision (2026-09-11)**. Apple's App Thinning Size Report needs a distribution provisioning profile that a free Apple ID cannot mint (F-010). The component breakdown FR-018 asks for *was* measured (95.8 MB unsigned `.app`; 86.7 MB model, 8.43 MB executable, 97.7% of code from the Rust staticlib) and is recorded as such rather than presented as Apple's figure. Reopen when a paid account exists — ~15 minutes
- [X] T050 [US3] Attribute size to components with a link map (`LD_GENERATE_MAP_FILE`), separating the Rust staticlib from the 90,868,376 bytes of bundled weights, which are a bundle resource rather than part of the executable (FR-018, data-model.md `BinarySizeReport`) — link map attributes **97.7%** of executable symbol bytes to the Rust staticlib (6.61 MiB); weights are a bundle resource, not executable. Note the 131 MB `.a` dead-strips to 6.6 MiB

**Checkpoint**: US3 complete. The spike's commissioned numbers now exist.

---

## Phase 6: User Story 4 — Every failure survives as a finding (Priority: P1)

**Goal**: Convert whatever happened — including total failure — into something the next person can act on without re-running the spike.

**Independent test**: Inspect the committed artifacts against the set of attempted items. Testable **even if US1–US3 all fail**.

> **Priority note**: US4 is P1 alongside US1, but its close-out tasks necessarily read the other
> stories' outcomes. T051 therefore runs in **Phase 2** in practice — create the report skeleton
> early so verdicts are recorded as they happen rather than reconstructed at the end, which is when
> they get lost.

- [X] T051 [US4] Create `specs/001-ios-build-spike/report.md` with every item in FR-001–FR-022 pre-listed and marked `untested`, so that a verdict is recorded as each is attempted rather than reconstructed afterwards (FR-024, SC-008). **Do this during Phase 2**
- [X] T052 [US4] For every item that did not pass, write a `Finding` in `specs/001-ios-build-spike/report.md` giving the item, observed behaviour, evidence, and **smallest reproduction found** — a finding without a reproduction is incomplete (FR-025, data-model.md `Finding`)
- [X] T053 [US4] Four ADRs written and linked from their findings: 0001 (candle pin), 0002 (unsafe mmap), 0003 (uniffi scaffolding -> constitution v1.1.0), 0004 (RUSTSEC ignore). F-009 and F-010 remain open findings without ADRs because neither constrains a design decision — both are follow-up work, not decisions
- [X] T054 [US4] Disclose any workaround applied to obtain a pass as a deviation **with its cost** in `specs/001-ios-build-spike/report.md`, never as an unqualified pass (FR-027)
- [X] T055 [US4] State explicitly in `specs/001-ios-build-spike/report.md` that 1,000 documents is **1%** of the 100,000-chunk configuration the 300 MB ceiling is written against, and make no claim about the ceiling holding at 100k (FR-021)
- [X] T056 [US4] State explicitly in `specs/001-ios-build-spike/report.md` that no performance budget was set and that these numbers are the baseline later specs will set budgets against (FR-023)
- [X] T057 [US4] Record Android and `wasm32-unknown-unknown` as **untested** in `specs/001-ios-build-spike/report.md`, never inferred from the iOS result; note the measured wasm32 blocker is `getrandom` needing the `wasm_js` backend (FR-030, research D13)

**Checkpoint**: the spike is closeable — successfully or not.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T058 Run the full local gate from [quickstart.md](./quickstart.md) Step 5 — `fmt`, `clippy`, `nextest`, `deny check`, and `cargo check` for all four targets — and paste the results into the PR body (Rule 5)
- [X] T059 [P] Confirm `git diff --stat` shows **zero** changes to `crates/xtriever-core/`, `crates/xtriever-analysis/`, `crates/xtriever-pipeline/`, `crates/xtriever-ltr/`, `crates/xtriever-eval/` and `deny.toml` (FR-009, Rule 2, Principle III/V gate rows)
- [X] T060 [P] Confirm the only **hand-written** `unsafe` in the diff is the single ADR-0002 block in `crates/xtriever-ffi/src/spike/embed.rs`, with its `// SAFETY:` comment and item-scoped `#[allow(unsafe_code)]` (ADR-0002 conditions 2, 3, 5)
- [X] T061 [P] Confirm all public items in `crates/xtriever-ffi/` are documented — `missing_docs` is on and CI runs with `-D warnings` (Principle VII)
- [X] T062 **Decision: KEEP the mmap path.** T045 measured a **39.6x** footprint reduction (101.1 MB -> 2.56 MB, ~99 MB saved), identical across both device runs, with both load paths producing bit-identical embeddings. ADR-0002's condition for deletion was "no material footprint benefit"; the benefit is decisive, so the `unsafe` block stays and the ADR is vindicated
- [X] T063 Ensure `specs/001-ios-build-spike/report.md` is readable by a reviewer who does not own an iPhone, with no measurement taken on trust (SC-010)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)** → no dependencies
- **Phase 2 (Foundational)** → depends on Phase 1. **BLOCKS everything else** (Principle II)
- **Phase 3 (US1)** → depends on Phase 2
- **Phase 4 (US2)** → depends on US1 (needs the staticlib from T025)
- **Phase 5 (US3)** → depends on US2 (needs the app from T036)
- **Phase 6 (US4)** → T051 runs during Phase 2; T052–T057 close out whatever completed
- **Phase 7 (Polish)** → depends on everything attempted

### User Story Dependencies

```
US1 (P1) ──► US2 (P2) ──► US3 (P3)
  │            │            │
  └────────────┴────────────┴──► US4 (P1, continuous)
```

The chain is real, not conservative sequencing: US2 needs a compiled staticlib and US3 needs an
installable app. US4 is the exception — it attaches to whatever exists and is the reason a spike that
fails at US1 still produces value.

### Within Each User Story

Tests (Phase 2) precede all implementation. Inside a story, models/errors precede services, which
precede the FFI surface, which precedes the Swift harness.

### Parallel Opportunities

- **Phase 2**: T012–T016 are five independent test files — fully parallel
- **Phase 4**: T027 (`index.rs`) and T028 (`query.rs`) are different files — parallel. T029–T032 all
  edit `embed.rs` and **must be sequential**
- **Phase 5**: T038–T041 are independent measurement primitives in `Measure.swift` — parallel if
  split by function, sequential if one person edits the file
- **Phase 7**: T059–T061 are independent verification passes — parallel

---

## Parallel Example: Phase 2 (Foundational)

```bash
# The five acceptance/property tests are independent files — write them together:
T012  crates/xtriever-ffi/tests/index_query.rs
T013  crates/xtriever-ffi/tests/tokenize.rs
T014  crates/xtriever-ffi/tests/embed.rs
T015  crates/xtriever-ffi/tests/load_paths.rs
T016  crates/xtriever-ffi/tests/determinism.rs

# Then, sequentially, confirm they fail for the right reason:
T017  cargo nextest run -p xtriever-ffi --features spike   # expect FAIL, not error
```

---

## Implementation Strategy

### PR split (Agent Operating Rule 3 — under ~800 hand-written lines each)

| PR | Phases | Contents | Reviewable without a device? |
|---|---|---|---|
| **1** | 1, 2, 3 + T051 | Pinned deps, fixture generator, failing tests, build matrix, report skeleton | **Yes** |
| **2** | 4 | uniffi surface, XCFramework, simulator harness | **Yes** |
| **3** | 5, 6, 7 | Device measurement, findings, close-out | No — needs the iPhone |

Generated fixture data under `reference/fixtures/001/` is committed as artifacts produced by a
committed script and counted separately from hand-written source: the 1,000-document corpus is data,
not reviewable code. `model.safetensors` is 87.1 MiB and **must not** be committed to git — it is
downloaded by T007 at the pinned revision and verified by size and hash.

### MVP scope

**PR 1 alone (Phases 1–3) is the MVP.** It answers the project's central portability bet, needs no
iPhone and no Apple developer account, and produces the spike's highest-value finding if it fails.
Phase 0 already measured `cargo check` passing on both triples, so the residual risk it retires is
codegen and linking (T025), not dependency resolution.

### Incremental delivery

1. PR 1 → the build matrix exists; the bet is confirmed or dead.
2. PR 2 → packaging and the FFI boundary are proven, still without hardware.
3. PR 3 → the numbers, the verdict, and the close-out.

Stopping after PR 1 or PR 2 is a legitimate outcome, provided Phase 6 records what was left
`untested` rather than leaving it looking like a pass (FR-024, spec US4 scenario 4).

---

## Notes

- **Never weaken an oracle to get a pass.** If a metric fails or a target does not build, stop and
  report (FR-028, Rule 6). Shrinking the corpus after an out-of-memory kill and re-reporting as a
  pass is called out explicitly in the spec as forbidden.
- **`cargo deny` reporting `onig_sys` means candle was upgraded**, not that `deny.toml` needs an
  exception. Revisit ADR-0001 (Rule 2).
- **Do not enable candle's `ug` feature** on 0.9.2 — the `not(target_os = "ios")` guard against App
  Store rejection `ITMS-90755` was added for 0.11.0 and is absent here (research risk R9).
- The binding surface is **provisional** (FR-031). Do not extend it for convenience, and do not
  defend its shape in review as an API decision. The durable deliverables are the report and the
  harness.
