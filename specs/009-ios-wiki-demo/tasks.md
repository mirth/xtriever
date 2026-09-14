# Tasks: The iOS Wikipedia Demo App

**Input**: Design documents from `/specs/009-ios-wiki-demo/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/app.md](./contracts/app.md),
[quickstart.md](./quickstart.md), the 007 package (`swift/Xtriever/`), the 008 artefact
(`target/xt-wiki/`), [ADR-0010](../../docs/adr/0010-device-rss-ceiling-600mb.md)

**Tests**: **Mandatory** (Principle II, spec FR-015). Phase 2 lands the app's test target red:
it references `DemoModel`, `ChangeMark`, `Settings` and `CorpusSidecar` that do not exist, so it
fails to compile — the recorded red state (as 007's Swift suite did). Story phases contain
implementation only and end with the task that turns their tests green.

**Organization**: Setup (project, build flag, scaffold) → Red suite → US1 + US2 together as
the model (the two-phase search *is* both stories' core; the views split them) → US1 views →
US2 views → US4 (cost and cancellation — mostly model, tested in Phase 2, finished by settings
and footer) → US3 (About) → Polish (device record, report, gate). PRs: **PR 1** = Phases 1–2,
**PR 2** = Phase 3, **PR 3** = Phases 4–7, **PR 4** = Phase 8.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers, data-model sections
  and the contract — read them before implementing. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

App `apps/ios-wiki-demo/` (`project.yml`, `App/`, `App/Model/`, `App/Views/`, `Tests/`,
`README.md`); package `swift/Xtriever/` (unchanged); script `scripts/build-ios-package.sh`;
simulator id `822F3C90-5124-432B-B84A-75426A04722D`; device id
`A3C0F8DE-11F1-5D1E-AA6F-C728A4F91BB4`, team `J483F464F3`; run records
`specs/009-ios-wiki-demo/runs/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: The project that builds, the flag that generates it, the scaffold the tests
reference.

- [ ] T001 Write `apps/ios-wiki-demo/project.yml` from `swift/XtrieverHarnessApp/project.yml` (research D1): `name: XtrieverWikiDemo`, `bundleIdPrefix: dev.xtriever`, iOS 16.0, the arm64-only `settings` block and its comment verbatim, `packages: Xtriever: path: ../../swift/Xtriever`; targets `XtrieverWikiDemo` (application, `sources: [App]`, `PRODUCT_BUNDLE_IDENTIFIER: dev.xtriever.wikidemo`, generated Info.plist with scene manifest + launch screen, `SWIFT_VERSION: "5.9"`) and `XtrieverWikiDemoTests` (bundle.unit-test, `sources: [Tests]`, depends on the app target and the package, `dev.xtriever.wikidemo.tests`); schemes `XtrieverWikiDemo` (build app + tests; **test config Debug**; run Release) and `XtrieverWikiDemo-Measure` (test config Release, `XtrieverWikiDemoTests` only — the device measurement); a header comment saying the `.xcodeproj` is derived and gitignored
- [ ] T002 [P] `scripts/build-ios-package.sh`: add `--demo` next to `--app` — `xcodegen generate` in `apps/ios-wiki-demo`, the same "INCOMPLETE if xcodegen is missing" exit 2 path, usage comment updated; add `apps/ios-wiki-demo/*.xcodeproj` to `.gitignore` with a one-line comment
- [ ] T003 [P] Scaffold the app: `apps/ios-wiki-demo/App/WikiDemoApp.swift` (`@main`, a `WindowGroup` showing `Text("not implemented")` — the app's red state), `apps/ios-wiki-demo/README.md` (what it is, how it is built and run, what it needs staged — contracts/app.md "Resources" and "Build"), empty `App/Model/` and `App/Views/` directories with a `.gitkeep`
- [ ] T004 Run `scripts/build-ios-package.sh --with-models --with-fixtures --demo` and `xcodebuild build -project apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo -destination 'platform=iOS Simulator,id=822F3C90-…' -configuration Debug ARCHS=arm64`: the scaffold builds and links the package

---

## Phase 2: Red Suite (Rule 4 — committed failing)

**Purpose**: The app's oracles before its code. The test target references types that do not
exist; it fails to compile, and that is recorded. **Checkpoint: PR 1.**

- [ ] T005 [P] Write `apps/ios-wiki-demo/Tests/Support.swift`: `@testable import XtrieverWikiDemo`; helpers to locate the bundled fixture (`HarnessResources.fixtureIsBundled`, `fixtureExpected`, decode 007's `Expected` shape — copy the decoding structs from `swift/Xtriever/Tests/XtrieverTests/Support.swift`, do not import the package's test target), `skipUnlessFixture()`, `skipUnlessWikipedia()`, a `waitUntil(_:timeout:)` polling helper for `@Published` state, and `assertHitsEqualGoldens(_ hits: [Hit], _ golden: …)` comparing ids, order and score bits exactly (SC-003)
- [ ] T006 [P] Write `apps/ios-wiki-demo/Tests/ChangeMarkTests.swift` (data-model "ChangeMark"): hand cases — identical lists → all `.same`, no dropped; one hit moved from rank 5 to 1 → `.up(4)`, the displaced ones `.down(1)`; a hit only in `reranked` → `.new`; a hit only in `fused` → in `dropped`; empty lists; and the fixture goldens: for every fixture query, `compute(fused: withoutReranker.hits, reranked: withReranker.hits)` has `marks.keys == Set(reranked ids)` and `dropped == fused ids − reranked ids` (needs no engine — goldens only). Red: no `ChangeMark`
- [ ] T007 [P] Write `apps/ios-wiki-demo/Tests/DemoModelTests.swift` (needs the fixture + models bundled; `@MainActor`): **ready** — `await model.start()` → `.ready(info)` with `info.openMs > 0`, `info.info.documents == 40`, `info.corpus == nil` (fixture), `warmMs != nil`; **fused then re-ranked** — `submit(q.text)` with default settings; wait for `phase == .fusing → fused != nil`, assert `fused.hits` == goldens `withoutReranker` (SC-003, exact); wait for `.done`, assert `reranked.hits` == `withReranker`, `marks.count == reranked.hits.count`; **empty** — `submit("   ")` → `phase == .empty`, engine not called (the previous `fused` cleared); **depth 0** — `settings.rerankDepth = 0`, submit → `.done` with `reranked == nil` and `fused.stages.rerank == nil`; **strict budget error** — `settings.strict = true; budgetMs = 1` → `.failed(message)` with a non-empty engine message, and the model stays usable (a later submit succeeds); **non-strict budget degrades** — `strict = false; budgetMs = 1` → `.done` with `reranked.stages.degraded != nil || reranked.stages.rerank?.skipped != nil`; **cancellation** (SC-006) — 20 iterations of `submit(a); submit(b)`: after `.done`, `search.query == b`, `fused/reranked` hits equal b's goldens, never a's; **cadence** (SC-002, the 007 method) — a 50 ms main-actor timer during one full submit keeps ≥ 90 % of its ticks; **timings** — `fusedMs`, `rerankedMs`, `footprintBytes` are set after `.done`. Red: no `DemoModel`
- [ ] T008 [P] Write `apps/ios-wiki-demo/Tests/AboutTests.swift`: `skipUnlessWikipedia()`; after `start()`, `info.corpus` decodes with `counts.passages == info.info.documents` and `corpus_identity` of 64 hex chars, `info.attribution` equals the bytes of `HarnessResources.wikipediaAttribution` decoded as UTF-8 (SC-004), and `snapshot.edition == "simple"`. Red: no `CorpusSidecar`
- [ ] T009 [P] Write `apps/ios-wiki-demo/Tests/DemoMeasurementTests.swift` (device-only; `skipUnlessWikipedia()`; skips on the simulator with a note as 007's does): drives `DemoModel` — `start()`, then for each of the 20 measurement queries (`HarnessResources.wikipediaQueries`) `submit` and wait for `.done`, recording `fusedMs`, `rerankedMs`, the responses' `elapsedMs`, hit counts, re-rank candidates/scored, `Measure.snapshot()` after each; footprint peak = max(sampled, ledger) with the 007 rules (valid readings, Release, not simulator); medians and maxima per stage; the record per data-model "Device run record" (`schemaVersion: 2`, `feature: "009-ios-wiki-demo"`, `corpus: "wikipedia"`, `ceilingBytes: 600 * 1_000_000`), emitted between `XTRIEVER_DEVICE_RUN_BEGIN/END` and as an `XCTAttachment`; `XCTFail` on a FAIL verdict. Red: no `DemoModel`
- [ ] T010 Red checkpoint: `xcodebuild test … -scheme XtrieverWikiDemo -configuration Debug` fails at compile time on the missing model types (record the first error); the app target still builds. Commit as **PR 1**

---

## Phase 3: User Stories 1 + 2 — the model (Priority: P1)

**Goal**: Everything the app *does* — preparation with warm-up, the two-phase search with
cooperative cancellation, change marks, settings — as one `@MainActor` `ObservableObject`, with
no view yet. This is the MVP: the model is what the tests prove.

**Independent Test**: `ChangeMarkTests` and `DemoModelTests` green on the simulator against the
fixture (quickstart Step 2).

- [ ] T011 [P] [US2] Implement `apps/ios-wiki-demo/App/Model/ChangeMark.swift`: `enum ChangeMark: Equatable { case new, same, up(Int), down(Int) }` and `static func compute(fused: [Hit], reranked: [Hit]) -> (marks: [String: ChangeMark], dropped: [Hit])` keyed by `externalId`, O(n) with a dictionary of fused ranks; `up(n)` = fused rank − re-ranked rank when positive; T006 green
- [ ] T012 [P] [US1] Implement `apps/ios-wiki-demo/App/Model/Settings.swift`: `struct Settings { rerankDepth: UInt32 (0 / 5 / 20, default 20); budgetMs: UInt64? (nil / 500 / 1_000 / 2_000 / 4_000 / 8_000, default 4_000); strict: Bool (default false) }` with `static let depths`, `static let budgets` for the pickers, `fusedOptions` = `SearchOptions(k: 10, rerankDepth: 0, explain: true)` and `rerankedOptions` = `SearchOptions(k: 10, rerankDepth: rerankDepth, maxTimeMs: budgetMs, strict: strict, explain: true)`; an `@AppStorage`-backed store (`SettingsStore`) the model reads (research D5)
- [ ] T013 [P] [US1] Implement `apps/ios-wiki-demo/App/Model/Preparation.swift` (data-model "Preparation"): `enum Preparation { idle, loadingModels, warming, ready(ReadyInfo), failed(PreparationFailure) }`, `struct ReadyInfo { info: IndexInfo; openMs: UInt64; warmMs: UInt64?; corpus: CorpusSidecar?; attribution: String?; indexName: String /* "Simple English Wikipedia" | "007 fixture (40 documents)" */ }`, `enum PreparationFailure { missingResource(name: String, stagingFlag: String), engine(message: String) }` with a `remedy` string
- [ ] T014 [P] [US1] Implement `apps/ios-wiki-demo/App/Model/CorpusSidecar.swift`: `Codable` with `schema_version`, `corpus_identity`, `snapshot { edition, snapshot_date }`, `counts { articles, selected, passages }` (snake_case keys via `CodingKeys`), `static func load(from indexDir: URL) throws -> CorpusSidecar` reading `corpus.json`; and `SearchState.swift` (data-model "SearchState"): `struct SearchState { id: UUID; query: String; phase: Phase; fused: SearchResponse?; reranked: SearchResponse?; marks: [String: ChangeMark]; dropped: [Hit]; fusedMs: UInt64?; rerankedMs: UInt64?; footprintBytes: UInt64? }`, `enum Phase: Equatable { fusing, reranking, done, empty, failed(String) }`
- [ ] T015 [US1] Implement `apps/ios-wiki-demo/App/Model/DemoModel.swift` per contracts/app.md: `@MainActor final class DemoModel: ObservableObject` with `@Published preparation`, `@Published settings`, `@Published search`, `private var index: XtrieverIndex?`, `private var task: Task<Void, Never>?`; `start()` — idempotent while in flight; resolves resources: Wikipedia (`HarnessResources.wikipediaIsBundled`) else fixture (`fixtureIsBundled`) else `.failed(.missingResource("XtrieverData/wikipedia/index", "--with-wiki"))`; models missing → `.failed(.missingResource("XtrieverData/models", "--with-models"))`; `.loadingModels` → `XtrieverIndex.open(indexDir:embedderDir:rerankerDir:loadPath: .mmap)` timed → `.warming` → one `search("warm up", fusedOptions)` timed, result discarded (D4) → `.ready(ReadyInfo)` with the sidecar and attribution when Wikipedia; engine errors → `.failed(.engine(error.message))`; `submit(_:)` — trims; cancels `task`; empty → `search = SearchState(phase: .empty)`; else a new `SearchState(id:)` in `.fusing` and a `Task` that: measures `fused = try await index.search(q, settings.fusedOptions)`, `try Task.checkCancellation()`, publishes only if `search?.id == id`; if `rerankDepth == 0` → `.done`; else `.reranking`, second search with `rerankedOptions`, marks via `ChangeMark.compute`, `footprintBytes = Measure.snapshot().footprintBytes`, `.done`; `CancellationError` → ignored (stale); `XtrieverError` → `.failed(error.message)` (only if still current); `cancel()`; T007 green — ⛔ a goldens mismatch in the fused or re-ranked hits is a report (the app must not be transforming hits)
- [ ] T016 [US1] Run `ChangeMarkTests` + `DemoModelTests` on the simulator (quickstart Step 2, Debug). Commit **PR 2**

---

## Phase 4: User Story 1 — the person searches and gets passages (views) (Priority: P1)

**Goal**: The preparation gate, the search screen, the hit rows, the detail screen.

**Independent Test**: On the simulator against the fixture: prepare → search a fixture query →
a list with titles, passages, ranks → tap → detail with the article link (the fixture's hits
have no title line, so the fallback path shows). On the device: the Wikipedia flow.

- [ ] T017 [P] [US1] Implement `apps/ios-wiki-demo/App/Model/DisplayedHit.swift` (data-model): `init(rank:hit:mark:)` deriving `title`/`passage` from `hit.titleAndPassage` (fallback: `externalId` / `hit.text`), `url = hit.wikipediaURL`, `ordinal = hit.chunk?.ordinal`, `features = hit.explain?.features() ?? []`, `score`, `rerankScore`; `static func list(from response: SearchResponse, marks:) -> [DisplayedHit]`
- [ ] T018 [P] [US1] Implement `apps/ios-wiki-demo/App/Views/PreparationView.swift`: one view over `Preparation` — a `ProgressView` with the step's label ("opening index and models", "warming the index — the first search pages the vectors in"), on `.ready` the timings line, on `.failed` the message and `remedy` with a Retry button calling `start()`
- [ ] T019 [US1] Implement `apps/ios-wiki-demo/App/Views/SearchView.swift` + `HitRow.swift`: `NavigationStack`; `.searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Ask Simple English Wikipedia")` with `.onSubmit(of: .search) { model.submit(query) }` and `.autocorrectionDisabled()` (D7); a stage label under the field ("fused", "re-ranking…", "re-ranked", "no hits for …", the engine's message on `.failed`); `List` of `HitRow`s (rank, title, 3-line passage excerpt, the mark as an SF Symbol + delta, `NavigationLink` to detail); a dropped-hits line; toolbar buttons to Settings and About (sheets); the search field disabled until `.ready`
- [ ] T020 [US1] Implement `apps/ios-wiki-demo/App/Views/HitDetailView.swift`: title, "passage *n* of the article" (ordinal + 1), the full passage (selectable text), `Link("Open on Wikipedia", destination:)` when `url != nil` (FR-003's one outbound action), the features list (name → value, `.nan` → "not seen by this stage"), fused score and re-rank score
- [ ] T021 [US1] Implement `apps/ios-wiki-demo/App/Views/RootView.swift` and wire `WikiDemoApp.swift`: `@StateObject var model = DemoModel()`; `RootView` shows `PreparationView` until `.ready`, then `SearchView`; `.task { await model.start() }`; on `scenePhase` returning to `.active` with `preparation == .failed(.engine)` offer retry (edge case 3 — re-open rather than assume). Build and run on the simulator with the fixture; walk US1 scenarios 1, 2, 4, 5 by hand and note anything the tests do not cover

---

## Phase 5: User Story 2 — the person sees the pipeline work (views) (Priority: P1)

**Goal**: The stage report on screen and the fused → re-ranked transition made visible.

**Independent Test**: Submit; the label reads "fused" then "re-ranked"; marks appear; the footer
shows the engine's counts, degradation, re-rank report, time-limit flag and elapsed.

- [ ] T022 [US2] Implement `apps/ios-wiki-demo/App/Views/StageReportView.swift`: renders a `StageReport` verbatim — "lexical candidates N", "dense candidates N" or "dense skipped: <reason>" from `degraded`, "re-rank: scored S of C" or "re-rank skipped: <reason>", "time limit ignored" when set, "engine N ms"; below it the app's own `fusedMs` / `rerankedMs` and `footprintBytes` in MB; shown as the list's footer section in `SearchView` for the current response (the re-ranked one when present, else the fused)
- [ ] T023 [US2] `SearchView.swift`: animate the list transition (`.animation(.default, value: search?.reranked != nil)`) and show the mark column only once `reranked` exists; the stage label states which stage's list is on screen; verify by hand on the simulator that a fixture query with a visible reorder shows arrows

---

## Phase 6: User Story 4 — honest about cost, never hangs (Priority: P2)

**Goal**: Settings, the budget's effect visible, cancellation proven by use.

**Independent Test**: Set budget 500 ms, submit; the report shows the re-rank cut short and the
list arrives; submit twice quickly; only the second's results show; the field stays usable.

- [ ] T024 [US4] Implement `apps/ios-wiki-demo/App/Views/SettingsView.swift`: `Picker("Re-rank depth")` over `Settings.depths`, `Picker("Time budget")` over `Settings.budgets` ("none" for nil), `Toggle("Strict mode")` with a footnote ("a cut stage becomes an error instead of a degraded result"); bound to the model's `SettingsStore`
- [ ] T025 [US4] `SearchView.swift`: a "first search after opening is the warm-up" note is **not** needed (the model warms during preparation — say so in the preparation label instead, FR-012); confirm by hand: submit, immediately submit another — the first's list never appears; set strict + 1 ms → the engine's error text in the label, field still usable

---

## Phase 7: User Story 3 — what is being searched, on what terms (Priority: P2)

**Goal**: The About screen from the engine and the shipped files.

**Independent Test**: `AboutTests` green on the device build; About shows every item of
contracts/app.md "About".

- [ ] T026 [US3] Implement `apps/ios-wiki-demo/App/Views/AboutView.swift` over `ReadyInfo`: sections — Corpus (`indexName`, edition, snapshot date, articles / selected / passages from the sidecar, corpus identity — monospaced, or "fixture index — no corpus sidecar"), Engine (`info.documents`, `formatVersion`, `embedderFingerprint`, `rerankerModelId ?? "none"`, `candidateDepth`, `rerankDepth`, `rrfK`), This session (open, embedder load, re-ranker load, warm-up ms), Attribution (the text verbatim, `Link` to the licence URL parsed from it or the constant `https://creativecommons.org/licenses/by-sa/4.0/`); "retrieval quality of this corpus is unmeasured" as one plain line (spec Assumptions)
- [ ] T027 [US3] `AboutTests` on the simulator are skipped (fixture); run them on the device build later in T029 — note that here. Commit **PR 3** (Phases 4–7)

---

## Phase 8: Polish & Cross-Cutting Concerns

- [ ] T028 Stage everything and build for the device: `scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki --demo`; install and run the app on the phone from Xcode (Release) in airplane mode; walk US1–US4 by hand: prepare (watch the warm-up), search "why is the sky blue", see fused → re-ranked with marks, open a hit, open the article in Safari (leaves airplane mode aside — the link is the one outbound action), Settings (depth 0, budget 500 ms, strict), About; note thread count observed (`Measure` cannot see it — record the phone's core count and 008's per-pair number as the reference)
- [ ] T029 Device measurement (manual, recorded; quickstart Step 3): `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test … -scheme XtrieverWikiDemo-Measure … -only-testing:XtrieverWikiDemoTests/DemoMeasurementTests` (env var before the command — 008 F-005) plus `AboutTests`; extract to `specs/009-ios-wiki-demo/runs/<device>-<timestamp>-demo.json`. ⛔ Footprint over 600 MB, median total over 3 s, or a cadence miss → stop and report
- [ ] T030 [P] Write `specs/009-ios-wiki-demo/report.md`: verdict; the simulator suite; the device record (footprint verdict, median/max fused, re-ranked and total, per-pair derived, vs 008's numbers); the cost of the fused-first design (the extra fused pass, measured); the by-hand walk of the four stories; thread count as observed/assumed; findings; "no eval delta due"
- [ ] T031 [P] Write `specs/009-ios-wiki-demo/pr-description.md`: summary, four-PR split with line counts, the device table, "nothing under `crates/` or `swift/Xtriever/Sources/` changed" (`git diff --stat main` pasted), CI unchanged (standing rule), what is deliberately not done (D11)
- [ ] T032 Run the gate (quickstart Step 4): the Rust gate unchanged; `git diff --stat main -- crates/ swift/Xtriever/Sources/ .github/` empty; the package's simulator suite still 18 / 18; the app's simulator suite green; paste into the report and PR description. Commit **PR 4**. ⛔ Any failure — stop and report

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T004; T002 ‖ T003 (both before T004)
- **Red suite (Phase 2)**: T005 first (helpers), then T006 ‖ T007 ‖ T008 ‖ T009; T010 last → **PR 1**
- **Model (Phase 3)**: T011 ‖ T012 ‖ T013 ‖ T014 → T015 → T016 → **PR 2**
- **US1 views (Phase 4)**: T017 ‖ T018 → T019 → T020 → T021
- **US2 views (Phase 5)**: T022 → T023 (after T019)
- **US4 (Phase 6)**: T024 → T025 (after T019)
- **US3 (Phase 7)**: T026 → T027 → **PR 3**
- **Polish (Phase 8)**: T028 → T029 → T030 ‖ T031 → T032 → **PR 4**

### Rule 6 stop-points (⛔)

T015 (a goldens mismatch in the model's hits); T029 (footprint over 600 MB, median total over
3 s, cadence under 90 %); T032 (any gate failure). The response is a report, never a looser
bound, a raised budget, or a relaxed golden.

### Parallel Opportunities

- Phase 1: T002 ‖ T003
- Phase 2: T006 ‖ T007 ‖ T008 ‖ T009 (four files)
- Phase 3: T011 ‖ T012 ‖ T013 ‖ T014 (four files)
- Phase 4: T017 ‖ T018
- Phase 8: T030 ‖ T031

---

## Parallel Example: Phase 3 model

```text
after PR 1:
  T011 ChangeMark   T012 Settings   T013 Preparation   T014 CorpusSidecar + SearchState
then T015 DemoModel (uses all four) → T016 green on the simulator → PR 2
```

## Implementation Strategy

1. **PR 1** (Phases 1–2): a project that builds, a red test target that does not compile.
2. **PR 2** (Phase 3) — **MVP**: the model, green against the fixture goldens: preparation,
   fused-then-re-ranked, cancellation, marks, settings. The app still shows nothing, but
   everything it will show is proven.
3. **PR 3** (Phases 4–7): the screens — search, detail, stage report, settings, About.
4. **PR 4** (Phase 8): the device walk, the measurement record against 600 MB and SC-001, the
   report, the gate.
