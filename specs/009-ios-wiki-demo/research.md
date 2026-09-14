# Research: The iOS Wikipedia Demo App

**Feature**: `009-ios-wiki-demo` | **Date**: 2026-09-14 | **Spec**: [spec.md](./spec.md)

Everything below is checked against the 007 package as it exists (`swift/Xtriever/Sources`),
the 008 run records, and the iOS 16 SwiftUI surface the package's platform floor allows.

## D1. Project layout: the harness app's shape, one level deeper

**Decision**: `apps/ios-wiki-demo/` holds `project.yml` (xcodegen), `App/` (the SwiftUI
sources), and `Tests/` (an app-hosted XCTest target). `packages: Xtriever: path:
../../swift/Xtriever`; the same arm64-only settings block and the same "derived, gitignored
`.xcodeproj`" rule as `swift/XtrieverHarnessApp/project.yml`. `scripts/build-ios-package.sh`
gains `--demo`, which runs `xcodegen generate` there exactly as `--app` does for the harness.
Bundle id `dev.xtriever.wikidemo`; deployment target iOS 16 (the package's floor).

**Rationale**: the harness project already encodes every trap (arm64 slices, Release test
config, hostless tests on device); the demo inherits them by copying the shape, not by a
second discovery. The package's resources (`XtrieverData/…`) ride into the app bundle as the
package's resource bundle, as they do for the harness — no extra copy step.

## D2. Two searches per query: fused first, then re-ranked

**Decision**: a submitted query runs `search(q, SearchOptions(k: 10, rerankDepth: 0, explain:
true))` and shows it labelled *fused*; then, if not cancelled, `search(q, SearchOptions(k: 10,
rerankDepth: settings.depth, maxTimeMs: settings.budget, strict: settings.strict, explain:
true))` and replaces the list labelled *re-ranked*, with change marks (D3). Both responses are
kept; the stage report shown is the second's (the first's is shown while it is the only one).

**Rationale**: the pipeline returns one response per call; there is no intermediate delivery
(007 D2: synchronous Rust, cooperative cancellation). Showing the fused stage costs one extra
lexical + dense pass — 0.35 s median on the device (008 run 2, depth 0) — and buys the
progression the owner asked to see. The second search redoes the fused stages (the pipeline
has no "re-rank this list" entry point; adding one is a pipeline change out of scope) —
a known cost, stated in the report.

**Cancellation**: the app's search is a `Task`; a new submission cancels it. The package's
`search` throws `CancellationError` after the engine finishes (007 FR-008), so the fused
result of a cancelled search is dropped and its re-ranked search is **not enqueued** (the
`Task` checks cancellation between the two calls). Stale work bounded to one engine call.

**Alternatives**: one search at full depth (no progression); three searches for three side-by-
side lists (3× cost, rejected in the spec's assumptions); streaming from Rust (no such API).

## D3. Change marks: a pure function of two ranked lists

**Decision**: `ChangeMark.compute(fused: [Hit], reranked: [Hit]) -> [ChangeMark]` keyed by
`externalId`: for each hit in `reranked`, `.new` if absent from `fused`, `.up(n)` / `.down(n)`
/ `.same` by rank difference; hits of `fused` absent from `reranked` are listed as `.dropped`
in a footer ("3 fused hits fell out of the top 10"). Pure, unit-tested with hand-written cases
and against the 007 fixture goldens (`withoutReranker` vs `withReranker` per query).

## D4. Warm-up during preparation, with a fixed query

**Decision**: preparation is `loadingModels → openingIndex → warming → ready`. After open the
app runs one depth-0 search of a fixed string (`"warm up"`) and discards the result, with the
screen saying "warming the index (first search pages the vectors in)". FR-012 permits this and
says to say so.

**Rationale**: 008 run 1's first query took 2,008 ms vs 476–560 ms afterwards — the mapped
661 MB paging in cold. Paying it during a labelled preparation step is honest and keeps the
first *real* search at the measured latency. On the simulator with the 40-document fixture
this is instantaneous and harmless.

## D5. Default settings: depth 20, budget 4,000 ms, strict off; thread count = process default

**Decision**: `Settings(rerankDepth: 20, budgetMs: 4_000, strict: false)`; depth choices 0 / 5
/ 20; budget choices none / 500 / 1,000 / 2,000 / 4,000 / 8,000 ms. The budget applies to the
re-ranked search (the fused search runs unbudgeted: 0.35 s median, 0.48 s max on device).

**Rationale**: 008 run 2 (default threads) depth-20 latencies: median 2,285 ms, max 2,738 ms.
A 3,000 ms budget would cut the tail on a warm phone and more on a `fair`-thermal one; 4,000
never cut a measured query and still bounds the wait. SC-001's "re-ranked within 3 s" is
measured as the **median** over the 20 measurement queries after warm-up (0.35 + 2.3 ≈ 2.65 s
median; the max ≈ 3.2 s is reported beside it, not hidden).

**Threads**: the process default. The FFI has no thread knob (007/008 wire unchanged) and iOS
ignores `LSEnvironment`; candle sizes its pool from the core count at first use. 008 measured
97 ms per pair at the default vs 194 at one thread — the default is the better number and the
one a person gets. 004 F-005 (no gain beyond ~2–4 cores; 8 slower than 4 on the laptop) means
six threads on a 2P+4E phone *may* waste effort; the device run records what it gets, and a
thread option on the FFI is a later feature if the numbers say so. Owner decision Q2.

## D6. The model: one `@MainActor` `ObservableObject`, no view logic in views

**Decision**: `DemoModel: ObservableObject` (iOS 16 — `@Observable` needs 17) owning
`preparation: Preparation`, `settings: Settings`, `search: SearchState?` and the
`XtrieverIndex`. Views render state; every transition is a method on the model; the model is
what the tests drive (FR-015). `SearchState` holds `query`, `fused: SearchResponse?`,
`reranked: SearchResponse?`, `marks`, `phase: .fusing | .reranking | .done | .failed(message)`,
`fusedMs`, `rerankedMs`, `footprintBytes`.

**Rationale**: the spec's "the app owns no retrieval logic" is kept checkable: the model calls
`XtrieverIndex.open/search` and `Measure.snapshot()` and computes nothing but change marks and
labels. Boring SwiftUI (Rule 7).

## D7. Views (iOS 16 SwiftUI, cited)

- `NavigationStack` with a `List` of hits; `.searchable(text:placement:prompt:)` +
  `.onSubmit(of: .search)` for submit-only search (Q1 = A); `.autocorrectionDisabled()`.
- Hit row: title, passage (line-limited, expands on tap to `HitDetailView`), rank, change mark
  as an SF Symbol (`arrow.up`, `arrow.down`, `sparkles` for new, `minus` for same) with the
  delta; `Link(destination: hit.wikipediaURL!)` opens Safari (FR-003's one outbound action).
- `HitDetailView`: full passage, "passage *n* of the article", the seven features from
  `HitExplain.features()` (007) with `.nan` rendered "not seen by this stage", the URL.
- `StageReportView`: a footer/sheet rendering `StageReport` verbatim: lexical/dense candidates,
  `degraded` (reason), `rerank` (candidates/scored/skipped reason), `timeLimitIgnored`,
  `elapsedMs`; plus the app's own two wall times and `Measure.snapshot().footprintBytes`.
- `SettingsView`: `Picker`s for depth and budget, `Toggle` for strict.
- `AboutView`: `IndexInfo` fields, the corpus sidecar, `ATTRIBUTION.txt` text with the licence
  `Link`.
- `PreparationView`: the state and the engine's timings; on failure the engine's message and
  the missing-resource guidance (edge case 1).

## D8. Reading the corpus sidecar and attribution in the app

**Decision**: the app decodes `XtrieverData/wikipedia/index/corpus.json` (008 data-model
"Corpus identity": `schema_version`, `corpus_identity`, `snapshot.{edition, snapshot_date}`,
`counts.{articles, selected, passages}`) with a small `Codable` struct and reads
`ATTRIBUTION.txt` as text, both via `HarnessResources` paths (`wikipediaIndexDirectory`,
`wikipediaAttribution` — added in 008). Counts and identities that the engine also reports
(`documents`, fingerprints, `formatVersion`) come from `IndexInfo`, and the About test asserts
the sidecar's `counts.passages == info.documents`.

## D9. Tests: the app's model against the 007 fixture on the simulator; one device record

**Decision**: `apps/ios-wiki-demo/Tests/` (app-hosted XCTest, **Debug** config on the
simulator — `@testable import` is fine in Debug and these are not measurements):
- `ChangeMarkTests` — pure cases + fixture goldens.
- `DemoModelTests` — with the fixture index and both models bundled: preparation reaches
  `.ready` with timings; a submitted fixture query yields `fused` equal to the goldens'
  `withoutReranker` and then `reranked` equal to `withReranker` (SC-003); an empty query yields
  the "no hits" state, not an error; a second submission cancels the first (only the second's
  results land, 20 iterations — SC-006); settings depth 0 gives a report with `rerank == nil`
  and a 1 ms strict budget gives a thrown, displayed engine error; main-thread cadence during a
  search ≥ 90 % (the 007 method — SC-002).
- `AboutTests` — sidecar/attribution equality against the staged files (when the Wikipedia
  resources are bundled; skips otherwise; runs on the device build).
- `DemoMeasurementTests` — device only: drives `DemoModel` over the 20 measurement queries,
  records fused/re-ranked wall times, footprint samples and the 600 MB verdict in the 007/008
  record shape (`feature: "009-ios-wiki-demo"`, plus `fusedMs`/`rerankedMs` per query), emitted
  between the same `XTRIEVER_DEVICE_RUN_BEGIN/END` markers so `scripts/extract-device-run.py`
  works unchanged. Committed under `specs/009-ios-wiki-demo/runs/`.

**Rationale**: FR-015 — the app's tests need no 1.3 GB bundle; the engine is already proven;
what the app adds (state, sequencing, marks, cancellation, display fidelity) is what is tested.

## D10. Staging and building

`scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki --demo` stages
everything and generates `apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj`. The simulator test
run needs only `--with-models --with-fixtures` (the app then shows "Wikipedia index not bundled"
on the simulator — edge case 1 exercised for free). The device build takes the full staging.
`.gitignore` gains `apps/ios-wiki-demo/*.xcodeproj`.

## D11. What is deliberately not done

- No thread knob, no streaming search, no "re-rank this list" entry point — all engine changes.
- No as-you-type search (Q1 = A).
- No persistence of settings beyond `@AppStorage` defaults; no history; no sharing.
- No App Store packaging; sideload.
- CI builds nothing for the app (standing rule); the simulator tests are a local gate step.
