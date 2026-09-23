# Xtriever Wikipedia demo (iOS)

A SwiftUI app that searches all of Simple English Wikipedia on the phone, offline, through the
Xtriever pipeline — lexical → dense → fusion → re-rank — and shows the pipeline working: the
fused list first, then the re-ranked order with what moved, each hit's explanation, the engine's
stage report, and the corpus's identity and licence attribution. Feature 009
(`specs/009-ios-wiki-demo/`). The app owns no retrieval logic; every number on screen is the
engine's or a wall clock around an engine call.

**Re-ranking order.** Since Feature 015 (ADR-0012) the engine orders the re-ranked head by
`0.5 · minmax(fused score) + 0.5 · minmax(cross-encoder score)` — the cross-encoder informs the
fused order rather than replacing it (+1.45 mean nDCG@10 over the previous replace-order rule on
the BEIR sets, at the same 20 cross-encoder calls). The shipped Wikipedia index adopts this
default on upgrade with no rebuild (its descriptor predates the mode and reads as the default).
The previous order is one option away — `SearchOptions(k: 10, rerankMode: .replace)` — and
`hit.explain?.rerankCombined` is the score a re-ranked hit was ordered by. Numbers:
`specs/015-rerank-interpolation/report.md`.

**The other demos.** The same corpus, engine and measurement queries from a Python command
line — `apps/python-wiki-demo/` (Feature 019) — and on Android through Kotlin bindings,
`apps/android-wiki-demo/` (Feature 025), whose record is an emulator record and says so: `wikidemo search`, `about`, `build` (the index
from the raw snapshot through the package alone) and `measure` (the laptop against the same
host goldens this app's parity check uses; its record beside this app's under
`specs/019-python-wiki-demo/runs/`).

**Re-rank depth.** The app re-ranks the first **10** fused candidates by default (Feature 018);
the engine's own default is 20. Feature 014 measured depth 10 at −0.3 mean nDCG@10 on the BEIR
sets for half the cross-encoder calls, and Feature 017 measured the reference phone at 1.4 s
instead of 2.3 s per re-ranked search; the app-level record is under
`specs/018-demo-rerank-depth-10/runs/`. Settings offers 0 / 5 / 10 / 20; a choice persists.

## Build

```bash
scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki --demo
open apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj      # run on a device (Release); ~640 MB of resources
```

`--with-wiki` needs the 008 artefact at `target/xt-wiki/` (`xtriever wiki build …`, minutes from
the cache once built); `--with-wiki-dev` stages a `--limit` build from `target/xt-wiki-dev/`
for simulator work. Without either the app runs against the 40-document 007 fixture
(`--with-fixtures`) and says so in About. The `.xcodeproj` is generated and gitignored.

Since Feature 024 the dense vectors live in `index/dense/manifest.bin` with one row file per
generation (`vectors.<generation>.bin`) instead of `index/dense/index.bin`, and since Feature
026 every row is eight-bit (dense format 3): the Wikipedia artefact is 585 MB where it was
1,076 MB, and the two models' files 51.5 MB where they were 182.9 MB. An index staged from an artefact
built before either feature is refused at open naming both versions, with the rebuild
instruction; rebuild it (`xtriever wiki build …`) rather than re-staging. About shows the
compaction share the index recorded, unset on every shipped artefact — those are built with
one commit and a merge.

## Tests

```bash
cd apps/ios-wiki-demo && xcodebuild test -project XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo \
  -destination 'platform=iOS Simulator,id=<simulator>' -configuration Debug ARCHS=arm64
```

The app's tests drive its model against the fixture index and its goldens on the simulator —
no 640 MB bundle needed. `DemoMeasurementTests` runs only on a device with the Wikipedia index
staged (scheme `XtrieverWikiDemo-Measure`, `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia` exported).
