# Quickstart: The Android Kotlin Wikipedia Demo (Feature 025)

How to prove this feature works, in the order the tasks build it. Every command runs from the
repository root unless stated. Nothing here needs a physical phone.

## Prerequisites

Installed and verified on 2026-09-20:

| Input | Where | Producer |
|---|---|---|
| Java Development Kit 21 | `/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home` | `brew install openjdk@21` |
| Android SDK, platform tools, emulator | `~/Library/Android/sdk` | Android Studio, then the SDK manager |
| Native toolkit 28.2.13676358 | inside the SDK | `sdkmanager "ndk;28.2.13676358"` |
| ARM system image, API 36 | inside the SDK | `sdkmanager "system-images;android-36;google_apis;arm64-v8a"` |
| Virtual device `xtriever-arm64` | `~/.android/avd` | `avdmanager create avd -n xtriever-arm64 -k … -d pixel_7` |
| `cargo-ndk` 4.1.2 | `~/.cargo/bin` | `cargo install cargo-ndk` |
| The corpus slice on format 2 | `target/xt-wiki-slice-py` | `wikidemo build --limit 2000 --out …` |
| Both pinned models | `reference/models/…` | `scripts/fetch-model.sh` |

The build names its Java explicitly; do not rely on the shell's:

```bash
export JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home
export ANDROID_SDK_ROOT="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/28.2.13676358"
```

## Step 0 — the emulator

```bash
"$ANDROID_SDK_ROOT/emulator/emulator" -avd xtriever-arm64 -no-window -no-audio -no-snapshot &
"$ANDROID_SDK_ROOT/platform-tools/adb" wait-for-device shell getprop sys.boot_completed   # → 1, about 75 s
"$ANDROID_SDK_ROOT/platform-tools/adb" shell grep -m1 Features /proc/cpuinfo              # must list fphp and asimdhp
```

The second check is not ceremony: without those two processor features every test below dies
on an illegal instruction rather than failing (research D5).

## Step 1 — red (Rule 4)

The parity test exists before the module does, and fails for the right reason — the module is
not there yet, not because the goldens are wrong.

```bash
./gradlew :android:xtriever:connectedAndroidTest    # expect failure: no module / no library
```

## Step 2 — the module (PR A)

```bash
scripts/build-android-package.sh --with-fixtures
./gradlew :android:xtriever:connectedAndroidTest
```

Expected: the native library is built for `arm64-v8a`, the Kotlin bindings are generated from
that library, the 40-document fixture index and its goldens are staged, and the parity suite
passes against `swift/Xtriever/Tests/Fixtures/expected.json` by the FR-004 rule — identifiers,
order, lexical bits and fused bits identical, dense and re-rank scores within 1e-3 (SC-001).
The goldens carry one re-rank depth per query, which is the depth they are replayed at; depth 0
is checked against the no-re-ranker goldens.

Two checks worth running once by hand, because both have bitten already:

```bash
"$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-nm" \
    android/xtriever/src/main/jniLibs/arm64-v8a/libxtriever_ffi.so | grep -c UNIFFI_META   # must be > 0
"$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-readelf" -l \
    android/xtriever/src/main/jniLibs/arm64-v8a/libxtriever_ffi.so | awk '/LOAD/ {print $NF}' | sort -u   # only 0x4000
```

## Step 3 — the application (PR B)

```bash
scripts/build-android-package.sh --with-models --with-corpus --with-fixtures
./gradlew :apps:android-wiki-demo:installDebug
"$ANDROID_SDK_ROOT/platform-tools/adb" shell am start -n <the application>/.MainActivity
```

Expected on first launch: a progress display while the assets are extracted, then a search
screen within 60 seconds (SC-005). Type a question; the fused list appears first, then the
re-ranked order with a mark per hit; opening a hit shows the eight explained features; the
stage report shows the engine's counts and elapsed milliseconds (SC-002).

With the network off — the emulator's airplane mode — the same question gives the same answer.

## Step 4 — the demonstration's own tests

```bash
./gradlew :apps:android-wiki-demo:connectedAndroidTest
```

Covers what the iOS demo's tests cover: the two lists and their marks, an empty query, a second
question cancelling the first, settings persistence across a restart, About's fields against
what the engine and the sidecar report, and preparation recovering from an interrupted
extraction.

## Step 5 — the record (US4, FR-017)

Run the twenty measurement queries on the emulator and write the record under
`specs/025-android-kotlin-demo/runs/`, in the shape `contracts/records.md` fixes. It must carry
`emulated: true` and the sentence bounding what it claims. Compare its answers with the host's
for the same corpus and queries by the same rule as SC-001 — identifiers, order, lexical and
fused bits identical, model scores within 1e-3 (SC-003). Peak memory is recorded
with the 600 MB phone ceiling quoted beside it, for comparison only (SC-004).

## Step 6 — the gate (Rule 5)

The workspace gate is unchanged and must still pass, because this feature touches no crate
logic:

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo nextest run --workspace && cargo deny check
cargo check --workspace --target aarch64-linux-android
git diff --stat main -- crates/ | tail -1        # expect no stage-crate logic; Cargo.toml profile only
grep -rn "$(hostname -s)\|$USER" specs/025-android-kotlin-demo android apps/android-wiki-demo   # expect nothing
```

## Step 7 — documents

`report.md` with the verdict, the record and what was deliberately not done; `pr-description-a.md`
and `pr-description-b.md`; `docs/adr/0014-android-half-precision-floor.md`; the demo's README;
and a note in the module's documentation naming the processor requirement.
