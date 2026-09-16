# Tasks: The Demo Re-ranks at Depth 10

**Input**: Design documents from `/specs/018-demo-rerank-depth-10/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [quickstart.md](./quickstart.md); the two models, the 007
fixture index, `target/xt-wiki/`, xcodegen, the simulator, the iPhone (owner).

**Tests**: **Mandatory** (Principle II; spec FR-006): `SettingsTests` committed red before the
constant changes. One PR; commits: **C1** = Phases 1–2 (red), **C2** = Phases 3–4 (the
setting, the copy, the simulator suite green), **C3** = Phases 5–7 (the record, docs, report, PR).

**Organization**: Setup → Red → US1 (the default and picker) → US2 (the copy) → US3 (the
device record) → US4 (docs) → Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`apps/ios-wiki-demo/App/Model/Settings.swift`, `App/Views/{SettingsView,AboutView}.swift`,
`Tests/SettingsTests.swift`, `README.md`; `specs/009-ios-wiki-demo/{spec,report}.md`;
`specs/018-demo-rerank-depth-10/{runs,report.md,pr-description.md}`.

---

## Phase 1: Setup

- [ ] T001 Confirm the inputs and the pre-state: `ls reference/models/all-MiniLM-L6-v2/model.safetensors reference/models/ms-marco-MiniLM-L-6-v2/model.safetensors target/xt-wiki/index/xtriever-pipeline.json swift/Xtriever/Tests/Fixtures/index/xtriever-pipeline.json`; `xcodegen --version`; `scripts/build-ios-package.sh --with-models --with-fixtures --demo` → PASS (the demo project regenerated); the existing demo suite green on the simulator once before any change (quickstart Step 2's command, `-skip-testing:XtrieverWikiDemoTests/DemoMeasurementTests`; the owner supplies the simulator id on the command line); `mkdir -p specs/018-demo-rerank-depth-10/runs`

---

## Phase 2: Foundational — the red test

- [ ] T002 Write `apps/ios-wiki-demo/Tests/SettingsTests.swift` (XCTest, imports the app module as the other test files do): `testDefaultsAreTheAppsChoice` — `Settings().rerankDepth == 10`, `Settings().budgetMs == 4_000`, `Settings().strict == false`, `Settings().rerankedOptions.rerankDepth == 10`, `Settings().fusedOptions.rerankDepth == 0`; `testDepthChoices` — `Settings.depths == [0, 5, 10, 20]` and `Settings.depths.contains(Settings().rerankDepth)`; `testPersistedChoiceWins` — encode `Settings(rerankDepth: 20, budgetMs: nil, strict: false)` with `JSONEncoder`, decode it, equal (a stored 20 stays 20); the Settings picker's footer text is exposed as `Settings.depthExplanation` and contains "10", "20", "0.3" and "1.4" (the numbers the UI must quote — spec FR-002)
- [ ] T003 Run quickstart Step 1 (`-only-testing:XtrieverWikiDemoTests/SettingsTests`) → the file fails to compile (`depthExplanation` does not exist) or fails on the default (20 ≠ 10) and the choices; record in `specs/018-demo-rerank-depth-10/report.md` ("Red checkpoint"). **⛔ Commit C1**: the test and the report stub

---

## Phase 3: User Story 1 — A search answers faster at the default setting (Priority: P1)

- [ ] T004 [US1] In `apps/ios-wiki-demo/App/Model/Settings.swift`: `rerankDepth: UInt32 = 10`; `static let depths: [UInt32] = [0, 5, 10, 20]`; `static let depthExplanation` (the footer text, spec FR-002: "Re-ranks the first 10 fused candidates by default — half the cross-encoder work of the engine's default 20 for −0.3 mean nDCG@10 on the BEIR sets (Feature 014); on the reference phone a re-ranked answer in 1.4 s instead of 2.3 s (Feature 017). Choose 20 for the engine's default."); the struct's doc comment cites 014 / 017 for the depth and 008 for the budget; `SettingsStore` unchanged (a persisted value wins)

---

## Phase 4: User Story 2 — The trade-off is explained where the setting is (Priority: P1)

- [ ] T005 [P] [US2] In `apps/ios-wiki-demo/App/Views/SettingsView.swift`: the depth picker gets its own `Section` with `footer: Text(Settings.depthExplanation)`; the budget picker and strict toggle keep their section and existing footer
- [ ] T006 [P] [US2] In `apps/ios-wiki-demo/App/Views/AboutView.swift` "Engine" section: `row("re-rank depth (engine default)", "\(info.info.rerankDepth)")` and, beside it, `row("re-rank depth (app default)", "\(Settings().rerankDepth)")` with a footnote `Text("The app re-ranks fewer candidates than the engine's default; see Settings.")`
- [ ] T007 [US2] `scripts/build-ios-package.sh --with-models --with-fixtures --demo` (regenerates the project so the new test file is included); quickstart Step 2 on the simulator: `SettingsTests` green and every existing demo test green; paste the test counts into the report. **⛔ Commit C2**

---

## Phase 5: User Story 3 — The app-level latency at the new default is on record (Priority: P1)

- [ ] T008 [US3] **Owner**: quickstart Step 3 — `scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki --demo`, then `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo-Measure -configuration Release -destination 'platform=iOS,id=XXXXX' -skipMacroValidation -allowProvisioningUpdates ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX -only-testing:XtrieverWikiDemoTests/DemoMeasurementTests 2>&1 | tee /tmp/018-demo-device.log` (alone in its process — 009 F-002; airplane mode; identifiers only on the command line); `scripts/extract-device-run.py /tmp/018-demo-device.log specs/018-demo-rerank-depth-10/runs/`; check the record's settings show depth 10, `latency.medianRerankedMs ≤ 0.7 × 2288`, `medianFusedMs` within 20 % of 339, `medianTotalMs ≤ 3000`, `medianFusedMs ≤ 1000`, footprint PASS (**⛔** otherwise: stop-and-report — the setting, never the figures); paste the medians and maxima beside 009's into the report

---

## Phase 6: User Story 4 — The documents follow (Priority: P2)

- [ ] T009 [P] [US4] `apps/ios-wiki-demo/README.md`: in the re-ranking paragraph (017), add that the app's default depth is 10 with the trade-off numbers and the pointer to this feature's record; `specs/009-ios-wiki-demo/spec.md` Assumptions "Default budget: 3,000 ms and re-rank depth 20 …" gains "*(re-rank depth 10 since Feature 018 — `specs/018-demo-rerank-depth-10/report.md`)*"; `specs/009-ios-wiki-demo/report.md` gains one line under its latency table pointing to the 018 record
- [ ] T010 [US4] Write `specs/018-demo-rerank-depth-10/report.md`: verdict; the setting and its evidence (014 table row, 017 harness medians); the record beside 009's (fused / re-ranked / total medians and maxima, footprint); SC-001–SC-005 with numbers; "Deliberately not done" (engine default unchanged; no parity run — 017; no budget change)

---

## Phase 7: Polish

- [ ] T011 Gate (quickstart Step 4): `git diff --stat main -- crates/ swift/ specs/003-eval-harness specs/004-dense-stage specs/005-hybrid-pipeline specs/006-rerank-stage specs/013-lexical-quality/baselines specs/014-rerank-depth-study/runs specs/015-rerank-interpolation/baselines specs/016-sparse-remeasure/runs specs/017-post-015-hygiene/runs` empty; the Rust gate unchanged (fmt, clippy, nextest, deny — run once); the demo suite green on the simulator; no identifiers in the tree (the record's `device` field is the model name)
- [ ] T012 Write `specs/018-demo-rerank-depth-10/pr-description.md` (the setting in one line, the two measurements that chose it, the record's medians beside 009's, what is unchanged, the attribution line). **⛔ Commit C3**; the owner pushes and merges

---

## Dependencies & Execution Order

T001 → T002 → T003 (C1) → T004 → (T005 ‖ T006) → T007 (C2) → T008 (owner) → (T009 ‖ T010)
→ T011 → T012 (C3).

### User story completion order

US1 → US2 → US3 → US4.

### Parallel opportunities

T005 ‖ T006 (two views); T009 while the owner runs T008; T010 after it.

## Implementation Strategy

**MVP** = Phases 1–4: the default, the copy, the suite green. **Rule 6 stop-points**: T003
(red), T008 (a record failing 009's figures or SC-002 — report, do not move the figures),
T011 (any gate failure).
