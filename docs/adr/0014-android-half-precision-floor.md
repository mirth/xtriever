# ADR-0014: Android requires ARMv8.2 half-precision floating point

- **Status**: Accepted — 2026-09-20 (the repository owner accepted the hardware floor rather than a fallback build, and accepted the resulting model-score tolerance)
- **Date**: 2026-09-20
- **Deciders**: mirth (repository owner), 2026-09-20
- **Spec**: [025-android-kotlin-demo](../../specs/025-android-kotlin-demo/spec.md) FR-002, FR-004, FR-016
- **Blocks**: Principle III row in [plan.md](../../specs/025-android-kotlin-demo/plan.md)

## Context

Principle III lists `aarch64-linux-android` among the targets the engine must support, and
continuous integration has type-checked it on every push since Feature 001. Type-checking is
not linking. The first attempt to produce a shared library for Android, on 2026-09-20 with
native toolkit 28.2.13676358, stopped here:

```
error: instruction requires: fullfp16
   --> gemm-f16-0.19.0/src/…
1952 |  "fmul {0:v}.8h, {1:v}.8h, {2:v}.8h",
```

Eleven times, all in `gemm-f16`. That crate is reached through `candle-core 0.9.2`, the
inference engine Principle I names and [ADR-0001](0001-pin-candle-0.9.2.md) pins; the same
dependency edge [ADR-0004](0004-ignore-rustsec-2024-0436.md) already records. Its
half-precision kernel writes inline assembly using ARMv8.2 FP16 instructions, which are
**optional** in the base 64-bit ARM profile the Android target assumes, so the assembler
refuses them. Building with `-C target-feature=+fp16` compiles and links the whole workspace.

Apple's targets do not hit this: every processor Apple ships supports the extension, so the
feature is on by default there. Android's target has to assume less.

## Decision

**The Android library is built with `-C target-feature=+fp16` and therefore requires a
processor with ARMv8.2 half-precision support** (`fphp` and `asimdhp` in `/proc/cpuinfo`).

1. **Devices without it are refused before the library is touched.** `DeviceSupport.check()`
   reads the processor's advertised features and returns a named refusal; nothing native runs
   until it returns supported. This is not defensive decoration: an unsupported processor does
   not throw, it dies on an illegal-instruction signal that no `catch` can reach, so the check
   must happen *before* the first call, not around it.
2. **No second library and no runtime fallback.** A build without the feature does not compile
   at all, so a fallback would mean patching or forking the matrix kernel.
3. **Model-computed scores carry a tolerance across targets.** Measured on the emulator over
   80 hits per mode: hit identifiers, their order, every BM25 bit and every fused bit identical
   to the host goldens; 67 of 80 dense scores differing by at most 1.3e-7 and 32 of 80 re-rank
   scores by at most 3.3e-6. The kernels compiled for this target round their reductions
   differently. Spec FR-004 therefore holds Android to the cross-device rule Features 009 and
   019 already use for the iPhone — exact identifiers, order, lexical and fused bits; model
   scores within 1e-3 — rather than to bit-identity.

## Consequences

**What this excludes.** 64-bit ARM processors without the optional FP16 extension: the
Cortex-A53, A57 and A72 generation, so devices from roughly 2014 to 2017. Everything from the
Cortex-A55 and A75 generation onward carries it, which is effectively every phone sold since
2018. 32-bit ARM and Intel Android images are out of scope for other reasons
(research D2), and an Intel emulator image cannot run this library.

**What it costs.** A person on an excluded device sees a sentence naming the requirement
instead of a search screen. The alternative was no Android support at all.

**What it does not change.** No other platform moves: the flag applies to the Android build
only, the engine's source is untouched, and the Swift and Python surfaces are unaffected.
The ranking on Android is bit-identical to the host's — only the two model stages' score
values move, and by far less than any tolerance the project applies.

**When to revisit.** If `candle` drops `gemm`, if `gemm-f16` gains a portable fallback, or if
Google's toolchain raises its baseline to ARMv8.2, this floor can be re-examined and possibly
removed. The same events would also let [ADR-0004](0004-ignore-rustsec-2024-0436.md)'s
advisory ignore go.
