# Contract: the Android library module (Feature 025)

What `android/xtriever/` promises to a project that depends on it, and what the packaging
script promises to the module. Nothing here is new engine behaviour; it is the existing
foreign-function surface with a platform's obligations written down.

## What the module contains

| Item | Origin | Committed? |
|---|---|---|
| the generated Kotlin bindings | `uniffi-bindgen … generate --library … --language kotlin` | no |
| `libxtriever_ffi.so` for `arm64-v8a` | `cargo-ndk`, profile that keeps symbols | no |
| the wrapper and the device check | hand-written, in the module's source | yes |
| the parity test and its fixture index | test source committed, fixture staged by the script | test yes, fixture no |

A consumer never sees the generated file in version control; one documented command produces
it (FR-003).

## Device requirements

- Processor: 64-bit ARM with the half-precision features `fphp` and `asimdhp` (ARMv8.2).
- Operating system: Android 8.0 (API 26) or later.
- The module refuses to load on anything else **before** the native library is touched, with an
  error naming the requirement. It never allows an unsupported processor to reach the engine,
  because the failure there is an illegal instruction, not an exception (research D5).

## What the caller must provide

An index directory and two model directories that exist as real files the operating system can
map — not assets inside the package. The application's private files directory is the expected
home. The index must be dense format version 2; a version-1 directory is refused by the engine
with its own message naming the rebuild.

## What the caller gets

The surface the generated bindings expose, unchanged from Swift and Python:

- open an index with a load path, close it deterministically;
- search with the engine's options — result count, per-stage depth, re-rank depth and mode,
  time budget, strict mode, explanations;
- per hit: the caller's identifier, the passage text, the fused score, the re-rank score where
  the stage scored it, chunk provenance, and the explanation with its eight features;
- the stage report: candidate counts, degradation and its reason, re-rank counts, the
  time-limit flag, elapsed milliseconds;
- the index information: documents, format version, model fingerprints, candidate depth, fusion
  constant, re-rank depth and mode, and the recorded dense compaction share.

## Determinism

The same index, query and configuration give the same hits in the same order as on the host,
with the lexical and fused score bits identical and the model-computed scores — dense and
re-rank — within 1e-3 per document (FR-004, the rule Features 009 and 019 already apply across
devices). This is the module's central promise and its test: the committed
`swift/Xtriever/Tests/Fixtures/expected.json`, replayed on the device at re-rank depths 0, 5,
10 and 20.

The two model stages cannot promise more across compilation targets: their arithmetic runs
through matrix kernels compiled for this target with half precision enabled, which round their
reductions differently. Measured on 2026-09-20 over 80 hits per mode: identifiers, order, BM25
bits and fused bits identical; dense scores differing by at most 1.3e-7, re-rank scores by at
most 3.3e-6 — four orders of magnitude inside the tolerance.

## Errors

The engine's error taxonomy reaches Kotlin as the generated typed failures. Nothing is
translated, swallowed or re-worded by the module; a missing file, a corrupt index, a format
mismatch and a read-only index arrive as the engine named them.

## What the packaging script guarantees

`scripts/build-android-package.sh`, run from a clean checkout with the prerequisites installed:

1. checks the toolchain, as the iOS script does, and stops if the native toolkit or the build
   driver is missing, naming what to install;
2. builds the native library for `arm64-v8a` with half precision enabled and symbols kept;
3. generates the Kotlin bindings from that same library — never from a host build;
4. stages what was asked for: the models, the fixture index and its goldens, the corpus slice;
5. reports the staged size against a budget and fails over it, as the iOS script does;
6. leaves nothing generated inside version control.
