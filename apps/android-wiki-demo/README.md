# Xtriever Wikipedia demo (Android)

The third demo of the engine over the same corpus as the iOS app (`apps/ios-wiki-demo`) and
the Python command line (`apps/python-wiki-demo`): a Compose application that searches
Wikipedia on the phone, offline, through the `android/xtriever` module, and shows the pipeline
working — the fused (lexical + dense) list first, then the re-ranked order with what moved per
hit, each hit's eight explained features under the engine's names, the engine's stage report,
and the corpus's identity and licence attribution. Feature 025
(`specs/025-android-kotlin-demo/`).

The app owns no retrieval logic: every number on screen is the engine's or a wall clock around
one engine call. Its only arithmetic is the change marks, which compare two lists the engine
produced.

## Requirements

A 64-bit ARM processor with ARMv8.2 half precision (every device from about 2018) and Android
8.0 or later; on a Mac, an **ARM** emulator image. The reasoning is in
[ADR-0014](../../docs/adr/0014-android-half-precision-floor.md); an unsupported processor is
refused with a sentence rather than a crash.

## Inputs

| Input | Default | Produced by |
|---|---|---|
| the library module | `android/xtriever` | `scripts/build-android-package.sh` |
| both pinned models | `reference/models/…-q8` | `scripts/fetch-model.sh --manifest reference/models/manifest-q8.json`, then `--manifest reference/models/manifest-rerank-q8.json` |
| the corpus slice | `target/xt-wiki-slice-py` | `apps/python-wiki-demo/.venv/bin/wikidemo build --limit 2000 --out target/xt-wiki-slice-py` |
| the host goldens for that slice | `target/xt-wiki-slice-py/expected.json` | `cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki-slice-py/index --out target/xt-wiki-slice-py/expected.json` |

## Build and run

```bash
export JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home
export ANDROID_SDK_ROOT="$HOME/Library/Android/sdk"
scripts/build-android-package.sh --with-models --with-corpus --with-fixtures
./gradlew :apps:android-wiki-demo:installDebug
"$ANDROID_SDK_ROOT/platform-tools/adb" shell am start -n dev.xtriever.demo/.MainActivity
```

The package carries 52 MB of model weights and a 15 MB corpus (the eight-bit artefacts and
format-3 rows of Feature 026; 174 MB and 24 MB before), so it installs over a cable,
not from a store. On first launch the app copies both out of the package into its own storage —
the engine memory-maps them, and an asset inside a package is not a file the system can map —
showing progress while it does. A launch interrupted mid-copy re-extracts on the next start
rather than opening a half-written index.

## What the screens show

**Search.** The fused list appears first, then the re-ranked order with a mark per hit — `↑n`,
`↓n`, `=` or `new` — and the fused hits that fell out of the head. Tapping *explain* opens the
hit: the passage, the article link, and the eight features under the engine's names, with
"not seen by this stage" where a stage did not retrieve it.

**Stage report.** Candidate counts per stage, degradation and its reason, the re-rank counts,
the time-limit flag, and the wall clocks around each engine call.

**About.** The corpus identity, snapshot and counts from the index's own sidecar; the engine's
document count, format version, model fingerprints, candidate depth, fusion constant, the
recorded re-rank mode and the recorded dense compaction share; this session's timings; and the
attribution verbatim.

**Settings.** The re-rank depth — 0, 5, 10 or 20, default 10 as in the other two demos
(Feature 018) — remembered across restarts.

## Tests

```bash
./gradlew :apps:android-wiki-demo:connectedAndroidTest     # needs a running ARM emulator
```

Twenty-six tests against the 40-document fixture and the bundled corpus: the two lists and
their marks, the fused answer published before the re-ranked one, a stale search never
overwriting a newer one, the title, passage and link rules, an empty query, a spent budget both
degrading and strict, preparation and its recovery, a refusal when storage is short, settings
persistence, and About's facts against what the engine and the sidecar report — including the
recorded re-rank mode and dense compaction share. The measurement test is the twenty-seventh
and is run on its own (below).

## The measured run

```bash
./gradlew :apps:android-wiki-demo:installDebug :apps:android-wiki-demo:installDebugAndroidTest
adb shell am instrument -w -e class dev.xtriever.demo.MeasurementTest \
    dev.xtriever.demo.test/androidx.test.runner.AndroidJUnitRunner
adb exec-out run-as dev.xtriever.demo cat files/measurement.json
```

The twenty measurement queries at re-rank depths 0, 5, 10 and 20, compared with the host's
answers for the same corpus. Run it this way rather than through `connectedAndroidTest`, which
uninstalls the app afterwards and takes the record with it.

The committed record is under `specs/025-android-kotlin-demo/runs/`: **parity PASS**, 800 hits
compared over twenty queries at four depths, identifiers, order, lexical bits and fused bits
all identical, dense scores within 1.2e-7 and re-rank scores within 5.3e-6. Latency and memory in that record describe an
**emulator**, not a phone, and must not be read beside the iPhone's numbers.
