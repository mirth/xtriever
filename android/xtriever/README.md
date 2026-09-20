# Xtriever for Android

The engine's Kotlin surface: a Gradle library module carrying the generated bindings, one
native library for 64-bit ARM, and a thin wrapper over them. It is the Android counterpart of
`swift/Xtriever` and exposes the same capabilities — nothing more (Feature 025,
`specs/025-android-kotlin-demo/`).

The module owns no retrieval logic. Every number it returns is the engine's.

## Requirements

| | |
|---|---|
| Processor | 64-bit ARM with ARMv8.2 half-precision (`fphp`, `asimdhp`) — every device from about 2018 |
| Operating system | Android 8.0 (API 26) or later |
| Emulator | an **ARM** system image; an Intel image cannot run this library |

The requirement is not negotiable at run time: the inference engine's matrix kernel is compiled
with half-precision instructions, and a processor without them dies on an illegal instruction
rather than raising an error. `DeviceSupport.check()` therefore runs before anything native and
returns a refusal naming the requirement. The reasoning is in
[ADR-0014](../../docs/adr/0014-android-half-precision-floor.md).

## Building it

```bash
export JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home
export ANDROID_SDK_ROOT="$HOME/Library/Android/sdk"
scripts/build-android-package.sh                  # the library and the bindings
scripts/build-android-package.sh --with-fixtures  # …plus the parity fixture, for the tests
```

One command produces everything generated: the native library under `src/main/jniLibs/`, the
Kotlin bindings under `src/main/kotlin/uniffi/`, and the test fixture under
`src/androidTest/assets/`. None of it is committed. Prerequisites are the native toolkit
(`sdkmanager "ndk;28.2.13676358"`) and `cargo-ndk`; the script names what is missing.

## Using it

```kotlin
when (val support = DeviceSupport.check()) {
    is DeviceSupport.Unsupported -> return showMessage(support.reason)
    DeviceSupport.Supported -> Unit
}

XtrieverIndex.open(indexDir, embedderDir, rerankerDir).use { index ->
    val response = index.search("why is the sky blue", k = 10, rerankDepth = 10, explain = true)
    response.hits.forEach { println("${it.externalId}  ${it.score}  ${it.text.lineSequence().first()}") }
}
```

The directories must be real files the operating system can memory-map. Assets inside an
application package are not: copy them into application storage first, as
`apps/android-wiki-demo` does on its first launch.

The index must be dense format version 2. An index written before Feature 024 is refused at
open with the engine's own message naming the rebuild.

## What the answers promise

Hit identifiers and their order, the lexical score bits and the fused score bits are identical
to the same index's answers on the host. The two model-computed scores — dense and re-rank —
are within 1e-3, the tolerance this project already applies across devices, because the matrix
kernels compiled for this target round their reductions differently. Measured over 80 hits:
dense within 1.3e-7, re-rank within 3.3e-6. `FixtureParityTest` enforces the rule and
`ParityCensusTest` prints the census on every run.

## Errors

The engine's errors arrive as the generated `XtrieverException` variants, unchanged and
untranslated. One Android-only detail: the binding generator emits Kotlin that does not compile
when an error variant carries a field named `message`, so the packaging script applies the fix
the compiler asks for. The effect on this surface is that `e.message` is the engine's text.

## Tests

```bash
./gradlew :android:xtriever:connectedAndroidTest     # needs a running ARM emulator or a device
```

Five tests: the goldens replayed at four re-rank depths with and without the re-ranker, the
index information, the processor requirement, and the parity census. The goldens are
`swift/Xtriever/Tests/Fixtures/expected.json`, the same file the Swift package is checked
against, minted on the host.
