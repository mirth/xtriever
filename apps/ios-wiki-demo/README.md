# Xtriever Wikipedia demo (iOS)

A SwiftUI app that searches all of Simple English Wikipedia on the phone, offline, through the
Xtriever pipeline — lexical → dense → fusion → re-rank — and shows the pipeline working: the
fused list first, then the re-ranked order with what moved, each hit's explanation, the engine's
stage report, and the corpus's identity and licence attribution. Feature 009
(`specs/009-ios-wiki-demo/`). The app owns no retrieval logic; every number on screen is the
engine's or a wall clock around an engine call.

## Build

```bash
scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki --demo
open apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj      # run on a device (Release); ~1.3 GB of resources
```

`--with-wiki` needs the 008 artefact at `target/xt-wiki/` (`xtriever wiki build …`, minutes from
the cache once built). Without it the app runs against the 40-document 007 fixture
(`--with-fixtures`) and says so in About. The `.xcodeproj` is generated and gitignored.

## Tests

```bash
cd apps/ios-wiki-demo && xcodebuild test -project XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo \
  -destination 'platform=iOS Simulator,id=<simulator>' -configuration Debug ARCHS=arm64
```

The app's tests drive its model against the fixture index and its goldens on the simulator —
no 1.3 GB bundle needed. `DemoMeasurementTests` runs only on a device with the Wikipedia index
staged (scheme `XtrieverWikiDemo-Measure`, `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia` exported).
