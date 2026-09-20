# Report: The Android Kotlin Wikipedia Demo (Feature 025)

**Feature**: 025 · **Branch**: `025-android-kotlin-demo` · **Status**: done — the engine runs
on Android through Kotlin, and the demonstration searches Wikipedia on a device

## Verdict

The third target in the constitution's portability rule stopped being a compile check. Two
pull requests: a reusable library module proved against the committed fixture goldens on an
ARM emulator, and a Compose demonstration over the 2,000-article Wikipedia slice whose answers
match the host's for all twenty measurement queries at four re-rank depths.

No crate changed. `git diff main -- crates/ deny.toml` is empty across both pull requests; the
only workspace file touched is `Cargo.toml`, which gained a build profile.

## PR A — the module, its build, and the oracle (2026-09-20)

**Red checkpoint**: `FixtureParityTest` was written first and run against nothing — the module
had no build file, so the instrumentation task did not exist.

**Landed**: `android/xtriever/`, a Gradle library module carrying the generated Kotlin bindings,
one native library for `arm64-v8a` and a thin wrapper mirroring the Swift package's;
`scripts/build-android-package.sh`, which produces all of it from source;
[ADR-0014](../../docs/adr/0014-android-half-precision-floor.md); six instrumented tests.

**Three findings, all from doing it rather than reading about it**:

1. **The build does not link without half precision.** `gemm-f16`, reached through the pinned
   inference engine, writes inline assembly using ARMv8.2 FP16 instructions that the base
   64-bit ARM profile rejects: eleven copies of `error: instruction requires: fullfp16`. With
   `-C target-feature=+fp16` the whole workspace links. That is a hardware floor, so it is an
   architecture decision record, and unsupported processors are refused before anything native
   runs — there the failure is an illegal instruction, not an exception.
2. **The release profile strips the binding metadata.** uniffi keeps its metadata in the symbol
   table; the stripped library reported 0 `UNIFFI_META` symbols against 30 unstripped, so the
   generator could not read it. `[profile.android]` keeps symbols, exactly as `[profile.wheel]`
   does for the Python wheel, for the same reason.
3. **The binding generator emits Kotlin that does not compile** when an error variant carries a
   field named `message`, and seven of ours do: it declares the constructor property and
   overrides `Throwable.message` in the same class. Renaming the fields would break the Swift
   and Python surfaces, so the packaging script applies the fix the compiler asks for and stops
   the build if a future generator stops producing that shape.

**One requirement revised, with evidence.** FR-004 demanded every score bit. Measured on the
emulator: identifiers, order, BM25 bits and fused bits identical; 67 of 80 dense scores
differing by at most 1.3e-7 and 32 of 80 re-rank scores by at most 3.3e-6, because the matrix
kernels compiled for this target round their reductions differently. **Owner's decision
(2026-09-20)**: adopt the cross-device rule Features 009 and 019 already apply to the iPhone —
exact identifiers, order, lexical and fused bits; model scores within 1e-3. A census test
prints the measured gaps on every run, so drift cannot hide inside the tolerance.

**Review (Copilot, nine comments — all applied)**: the 16 KB alignment assertion read only the
first loadable segment; the index information was read lazily and would have called a destroyed
handle after `close`; the search wrapper hid two options and silently wrapped negative values;
the test's documentation still claimed bit-exactness; and five documents claimed coverage "at
depths 0, 5, 10 and 20" when every golden query in the 40-document fixture carries depth 5. The
claim was corrected everywhere and the coverage that *was* available added: with the re-ranker
attached at depth 0 the stage must not run, so the answer must equal the no-re-ranker goldens.

## PR B — the demonstration (2026-09-20)

**Red checkpoint**: the mark and text tests, then the model tests, all run before the
application existed.

**Landed**: `apps/android-wiki-demo/`, a Compose application with search, hit detail, stage
report, About and settings; first-run extraction of the bundled models and corpus with progress
and recovery; 23 instrumented tests; the measured record.

**The measured run** (`runs/sdk_gphone64_arm64-…-mmap-threads4.json`), twenty queries at
re-rank depths 0, 5, 10 and 20 against host goldens for the same corpus:

| | |
|---|---|
| parity | **PASS**, 800 hits compared |
| identifiers, order, lexical bits, fused bits | identical |
| dense scores | within 1.2e-7 |
| re-rank scores | within 5.3e-6 |
| median latency, depth 0 / 5 / 10 / 20 | 248 / 1,215 / 2,190 / 4,061 ms |
| peak resident | 476 MB, against the project's 600 MB phone ceiling |

Those latencies are an **emulator's**, on four cores of a laptop, and the record says so in its
own `claims` field. They are not a phone result and must not be set beside the iPhone's.

**Two build facts worth remembering.** Compose 2026.09 requires compiling against API 37, so
both modules do. And `kotlinx-coroutines-test` must match the coroutines core the lifecycle
libraries resolve — a newer test artefact fails at run time with `NoSuchMethodError`.

## Success criteria

| | Verdict |
|---|---|
| SC-001 fixture goldens reproduced by the FR-004 rule | **met** — six tests, at the goldens' depth and at depth 0 |
| SC-002 offline search with the fused list first | **met** — no permission in the manifest; the app never reaches the network |
| SC-003 measurement parity against the host | **met** — PASS, 800 hits |
| SC-004 peak memory recorded | **met** — 476 MB recorded, ceiling quoted for comparison |
| SC-005 first launch searchable within 60 s | **met** — preparation tests and the measured run |
| SC-006 one command from a clean checkout | **met** — `scripts/build-android-package.sh` plus one Gradle install |
| SC-007 no baseline changes | **met** — no crate diff against `main` |

## Deliberately not done

No physical-device run: the record is an emulator record by the owner's decision, and a device
run is left to a later feature. No store packaging — 174 MB of weights exceeds what a listing
allows. No index building on the phone; that is what the Python demonstration shows. No
publication of the module to a package registry. No second native library for processors
without half precision. No continuous-integration change: the existing Android compile check
stays, and nothing downloads a 2.8 GB toolkit or boots an emulator in CI.

Per-depth goldens for the 40-document fixture would widen PR A's oracle beyond depth 5, and
would change the fixture the Swift tests share. Noted, not done.
