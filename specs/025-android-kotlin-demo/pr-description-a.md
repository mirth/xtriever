## 025 (PR A) — the Android library module: Kotlin bindings, a linked native library, and the goldens replayed on a device

The engine has type-checked for `aarch64-linux-android` on every push since Feature 001. This
makes it run. `android/xtriever/` is a Gradle library module carrying the generated Kotlin
bindings, one native library for 64-bit ARM and a thin wrapper mirroring the Swift package's;
`scripts/build-android-package.sh` produces all of it from source, as its iOS counterpart does,
and nothing generated is committed.

**The proof is the existing oracle.** `swift/Xtriever/Tests/Fixtures/expected.json` — the
goldens the Swift package is checked against, minted on the host — replayed on an ARM emulator
with and without the re-ranker, at the re-rank depth each golden query records (the fixture
mints them all at 5, as the Swift parity tests also replay them), plus depth 0 checked against
the no-re-ranker goldens. Six instrumented tests, all passing. Per-depth goldens would widen
this, and would change the fixture the Swift tests share; that is noted, not done here.

**Two things the link attempt found, both now asserted by the script.** The build stops at
`gemm-f16` with `instruction requires: fullfp16` unless half precision is enabled, because the
inference engine's kernel writes inline assembly the base 64-bit ARM profile rejects; and the
release profile strips the symbols the binding generator reads, exactly as it does for the
Python wheel, so the module uses a new `[profile.android]` that keeps them. The script checks
for metadata symbols and 16 KB segment alignment on every run and fails rather than shipping
something unverified.

**The hardware floor is recorded** in [ADR-0014](../../docs/adr/0014-android-half-precision-floor.md):
Android requires ARMv8.2 half precision, which excludes 64-bit processors older than roughly
2018. `DeviceSupport` refuses those before anything native is touched, because there the
failure is an illegal instruction that no `catch` can reach.

**One requirement was revised, with the owner's decision and measured evidence.** Over 80 hits
per mode the hit identifiers, their order, every BM25 bit and every fused bit are identical to
the host's; 67 dense scores differ by at most 1.3e-7 and 32 re-rank scores by at most 3.3e-6,
because the matrix kernels compiled for this target round differently. FR-004 and SC-001 now
hold Android to the cross-device rule this project already applies to the iPhone — exact
identifiers, order, lexical and fused bits; model scores within 1e-3 — and a census test prints
the measured gaps on every run so drift cannot hide.

**One workaround, contained and loud.** The binding generator at its newest release (0.32.1)
emits Kotlin that does not compile when an error variant carries a field called `message`, and
seven of ours do. Renaming those fields would break the Swift and Python surfaces, so the
packaging script applies the fix the compiler asks for and stops the build if a future
generator stops producing that shape.

**Not ranking-affecting.** No crate changes: `git diff main -- crates/ deny.toml` is empty. The
only workspace file touched is `Cargo.toml`, for the profile.

Gate: fmt, clippy with warnings denied, `cargo nextest run --workspace` 322 passed, deny, the
three cross-target checks, and the module's five instrumented tests on the emulator.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
