# Research: The Android Kotlin Wikipedia Demo (Feature 025)

Phase 0. Every decision below was either exercised on this machine on 2026-09-20, before the
plan, or follows an existing feature's settled pattern. Where something was measured, the
measurement is quoted.

## D1 — Where the Android code lives

**Decision**: a library module at `android/xtriever/` and an application at
`apps/android-wiki-demo/`, outside the Cargo workspace.

**Rationale**: it is the layout the iOS side already uses — `swift/Xtriever` is the package,
`apps/ios-wiki-demo` the demonstration — so the two platforms read the same way and the demo
directory keeps meaning "a thing a person can run". Keeping both outside the workspace keeps
`cargo` unaware of them, which matters because the workspace lints and the gate apply to Rust
only.

**Alternatives considered**: one Gradle project containing both, rejected because the module
must be usable by another project without dragging a demonstration along; putting the module
under `crates/`, rejected because it is not a crate.

## D2 — The native build: `cargo-ndk`, one architecture, half precision on

**Decision**: `cargo-ndk 4.1.2` drives the native toolkit `28.2.13676358` for `arm64-v8a`
only, with `RUSTFLAGS="-C target-feature=+fp16"`.

**Rationale**: measured. Without the flag the build stops at `gemm-f16` with eleven copies of
`error: instruction requires: fullfp16` — the inference engine's half-precision kernel writes
inline assembly (`fmul v0.8h, v1.8h, v2.8h`) that the base 64-bit ARM profile rejects. With
the flag the whole workspace links: a 260 MB unstripped library in the development profile and
a 9.3 MB one in release, both reported by `file` as `ELF 64-bit LSB shared object, ARM
aarch64`. Toolkit release 28 aligns loadable segments at 16 KB (`0x4000`), which Android 15
requires. One architecture covers physical devices and the emulator images that run on this
host; a 32-bit or Intel variant would double the build for no audience here.

**Alternatives considered**: configuring the linker by hand in `.cargo/config.toml`, rejected
because `cargo-ndk` already encodes the per-architecture triples and the output layout;
shipping a second library without half precision for older processors, rejected by the owner
on 2026-09-20 (spec FR-016).

## D3 — Bindings: the generator already in the repository

**Decision**: generate Kotlin with the existing `uniffi-bindgen` binary in `crates/xtriever-ffi`
behind its `cli` feature, through the generic path it already dispatches to:
`generate --library <the Android library> --language kotlin --out-dir <dir>`.

**Rationale**: verified — it produced a 4,249-line Kotlin file from the Android library with no
change to the binary, carrying the whole surface including `denseCompactDeadShare`, the field
added a day earlier. The binary's `main` already routes a first argument of `generate` to
`uniffi::uniffi_bindgen_main()`, which is what Python uses; only Swift takes the special path.

**Alternatives considered**: installing the upstream `uniffi-bindgen` command separately,
rejected because a second copy could drift from the pinned `uniffi 0.32.1` in the lockfile.

## D4 — The symbol-stripping trap

**Decision**: build the module's library with a Cargo profile that inherits `release` but
strips only debug information, mirroring the existing `[profile.wheel]`.

**Rationale**: measured. The binding metadata lives in the symbol table; the release profile
sets `strip = true`, and the stripped Android library reports **0** `UNIFFI_META` symbols
against the development build's **30**, so the generator cannot read it. The repository already
documents this for Linux and already solved it once for the Python wheel — Android is an ELF
platform and hits the same wall for the same reason.

**Alternatives considered**: generating the bindings from the host library instead, rejected
because the bindings would then be produced from a build whose target is not the one shipped;
keeping the full symbol table, rejected as unnecessary weight in the application package.

## D5 — The hardware floor and how the failure is made legible

**Decision**: require the ARMv8.2 half-precision features, and check for them in Kotlin before
any engine call — read `/proc/cpuinfo` and require `fphp` and `asimdhp` — refusing with a named
error. Record the floor in `docs/adr/0014-android-half-precision-floor.md`.

**Rationale**: an unsupported processor does not fail politely. The instruction is decoded at
run time and the process dies with an illegal-instruction signal, which no `try`/`catch` can
turn into a message. The only way to give a person a sentence instead of a crash is to look
before leaping. The check is cheap and runs once at startup. The emulator prepared for this
feature reports `fphp asimdhp` among its processor features, so the floor does not block the
emulator record.

**Alternatives considered**: declaring a required feature in the application manifest, rejected
because the platform has no manifest flag for this processor extension; catching the signal,
rejected as unserious.

## D6 — How the models and the index reach the device

**Decision**: ship both model directories and the corpus slice inside the application package
as uncompressed assets, and copy them into the application's private files directory on first
launch, each file written to a temporary name and renamed, with a completion marker written
last.

**Rationale**: the engine memory-maps the vectors and both model weight files. An asset inside
the package is not a file the operating system can map — it lives inside the package archive —
so the bytes must exist as real files before anything opens them. Writing then renaming, with
the marker last, makes an interrupted first launch recoverable: on the next start either the
marker is present and everything is complete, or the extraction runs again (FR-011). The owner
chose bundling over a manual push on 2026-09-20.

**Alternatives considered**: downloading the weights on first run, rejected because the demo
must work offline and the retrieval path may not touch the network; a manual push over the
cable, rejected by the owner as a worse first impression.

## D7 — The corpus

**Decision**: the 2,000-article slice — 8,529 passages, 24 MB, corpus identity
`20949fb44303f590…`, rebuilt on dense format 2 on 2026-09-19 — with its sidecar and
attribution file, staged by the script from `target/xt-wiki-slice-py`.

**Rationale**: it is a complete artefact in the same shape as the shipped one, so About has a
real corpus identity, real counts and the real attribution to show, and a person searching it
gets Wikipedia rather than a toy. The full artefact is 1 GB, which no package should carry; it
can still be pushed by hand for a comparison run.

## D8 — The application's shape

**Decision**: Compose, four screens, mirroring the iOS demo: search, hit detail, stage report,
About, plus settings. The change marks, the title and passage split, and the article link
follow the Feature 008 convention exactly as the Swift and Python demos do.

**Rationale**: three demonstrations of one engine should show the same thing, and the rules
they share are already pinned by tests on two platforms. The application owns no retrieval
logic, so every number on screen is the engine's or a wall clock around one engine call
(FR-012).

## D9 — Threads

**Decision**: record the thread count the engine's pool reports rather than setting it.

**Rationale**: the records on the other two platforms name the effective count and its source,
and a device's core count is not the laptop's. Nothing in the demo should pin it.

## D10 — What the tests are, and where they run

**Decision**: instrumented tests on the emulator. PR A's is the oracle: open the 40-document
fixture index staged by the script and compare against `swift/Xtriever/Tests/Fixtures/expected.json`
by the FR-004 split — identifiers, order, lexical bits and fused bits exact; dense and re-rank
scores within 1e-3. The goldens carry one depth per query (5), which is what they can be
replayed at; depth 0 is covered by requiring an attached re-ranker at depth 0 to reproduce the
no-re-ranker goldens. PR B's tests mirror the iOS demo's model tests — marks, settings
persistence, preparation recovery, empty query, cancellation.

**Rationale**: the goldens are committed, cross-platform and already the oracle for Swift;
reusing them makes "Android agrees with the host" a one-file comparison rather than a new
fixture. Everything stays local: no continuous-integration job may download a 2.8 GB toolkit or
boot an emulator (FR-015).

## D11 — Versions on the Gradle side

**Decision**: Gradle, the Android Gradle Plugin, Kotlin and Compose versions are taken from a
project generated by the tooling at implementation time and pinned in a version catalogue;
none is written from memory. Java Development Kit 21 is used explicitly, by path.

**Rationale**: Principle VII's rule against remembered version numbers applies to every
ecosystem, not only to Cargo. The bundled runtime in Android Studio is version 25, which the
plugin may not accept, so the build names the one it wants instead of inheriting the shell's.

## D12 — What is deliberately not built

No physical-device run (spec FR-017, owner's decision — the record is an emulator record). No
store packaging: 174 MB of weights exceeds what a listing allows, and there is no store
audience. No index building on the phone; that is what the Python demo demonstrates. No
publishing of the module to a package registry — it is consumed by path. No second native
library for unsupported processors. No continuous-integration change.
