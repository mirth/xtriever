# Contract: the demo app's screens and model

**Feature**: `009-ios-wiki-demo` | the app exposes no API; this is what a person and the tests can rely on

## Model (`DemoModel`, `@MainActor`, `ObservableObject`)

```swift
@Published var preparation: Preparation          // data-model "Preparation"
@Published var settings: Settings                // persisted
@Published var search: SearchState?              // the current or last search
let measurementQueries: [MeasurementQuery]?      // from the bundle, for the device test

func start() async                               // idle → … → ready | failed; idempotent while in flight
func submit(_ query: String)                     // cancels the previous search's Task; empty → .empty
func cancel()                                    // cancels the current search's Task
```

Guarantees:

- `submit` never blocks the main actor; both engine calls run through the package's queue.
- A cancelled search never mutates `search` after cancellation (its id is stale).
- With `settings.rerankDepth == 0` only the fused search runs; `phase` goes `.fusing → .done`.
- Engine errors surface as `phase = .failed(error.message)` — the engine's message, never a
  reworded one; `strict` budget exhaustion is such an error, non-strict is degradation in the
  report.
- `preparation == .failed(.missingResource(name, flag))` names the missing file and the
  `build-ios-package.sh` flag that stages it.

## Screens

| Screen | Shows | From |
|---|---|---|
| Preparation | the current step; on `.ready` the open / warm timings; on `.failed` the message and remedy | `preparation` |
| Search (root) | search field (submit only), stage label ("fused" / "re-ranked" / "re-ranking…"), the hit list with rank, title, passage excerpt, change mark, article link; footer with the stage report and the app's timings/footprint; "no hits" state; a dropped-hits line | `search` |
| Hit detail | title, "passage *ordinal* of the article", full passage, article link, the seven features (name, value or "not seen by this stage"), fused score, re-rank score | `DisplayedHit` |
| Settings | re-rank depth (0 / 5 / 20), time budget (none / 500 … 8,000 ms), strict | `settings` |
| About | edition, snapshot date, articles / selected / passages, corpus identity, embedder and re-ranker identities, format version, candidate depth / rrf_k / re-rank depth, this session's open, embedder, re-ranker and warm-up times; the attribution text verbatim and the licence link | `ReadyInfo` |

Every number on the Search and About screens is an engine value or the app's own wall time
around an engine call; nothing is computed from hit contents except the change marks.

## Resources the app expects (staged by `scripts/build-ios-package.sh`)

`XtrieverData/models/{embedder,reranker}` (`--with-models`); `XtrieverData/wikipedia/{index,
ATTRIBUTION.txt, queries.json, wiki-build.json}` (`--with-wiki`); for the app's own simulator
tests `XtrieverData/fixtures/{index, expected.json}` (`--with-fixtures`). With only the fixture
bundled the app runs against it and About says so.

## Build

`scripts/build-ios-package.sh [--with-models] [--with-fixtures] [--with-wiki] --demo` →
`apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj` (gitignored). Schemes: `XtrieverWikiDemo`
(run: Release; test: Debug on the simulator) and `XtrieverWikiDemo-Measure` (test: Release,
device, `DemoMeasurementTests` only).
