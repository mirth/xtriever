# Quickstart: validating the iOS Build Spike

**Feature**: `001-ios-build-spike` | **Date**: 2026-09-10 | **Plan**: [plan.md](./plan.md)

> **Unblocked**: [ADR-0001](../../docs/adr/0001-pin-candle-0-9-2.md) and
> [ADR-0002](../../docs/adr/0002-unsafe-mmap-safetensors-measurement.md) were accepted 2026-09-10,
> and the Constitution Check passes on all 14 rows. The commands below are the validation contract,
> not a record of a run — nothing here has been executed yet.

This guide is written so SC-002 and SC-010 are achievable: someone with a physical iPhone and no
prior context can reproduce the run, and a reviewer without an iPhone can check the report.

---

## Step 0 — Assert the toolchain is the pinned one (do not skip)

A Homebrew Rust install shadowing rustup's ignores `rust-toolchain.toml` and ships only the host
`std`, so every cross-target command fails with `can't find crate for 'std'` — a **false FAIL** that
looks exactly like the iOS portability failure this spike tests for (research D14).

This was live on the original machine and was fixed on 2026-09-11, but the check stays: CI runners
and other machines can hit it, and `cargo` and `rustc` shadow **independently** — removing one is
not enough. Run the script rather than eyeballing `which cargo`.

```bash
./scripts/check-toolchain.sh   # cargo AND rustc provenance, active toolchain, both iOS targets, iphoneos SDK
```

If it fails, `export PATH="$HOME/.cargo/bin:$PATH"` and re-run; if that fixes it, remove the
competing install (`brew uninstall rust`) rather than relying on shell state.

**Any cross-target verdict recorded without this check is void.** If a target is missing, that is an
environment problem, not a Story 1 failure, and must not be reported as one.

---

## Step 1 — Generate the golden fixtures (Story 1 prerequisite, host only)

Principle II requires the oracles to exist and fail before implementation.

```bash
python3 reference/gen_001_fixtures.py --seed 1 --out reference/fixtures/001/
```

Produces the corpus, the expected ranking, the expected token sequence and the expected embedding.
Verify the pinned model before trusting any of it:

```bash
# FR-016: exact revision, exact size, recorded hash
# NOTE: weights live in gitignored reference/models/001/, NOT in the committed fixtures dir.
test "$(stat -f%z reference/models/001/model.safetensors)" = 90868376 || echo "WRONG WEIGHTS"
shasum -a 256 reference/models/001/model.safetensors
```

Expected: revision `1110a243fdf4706b3f48f1d95db1a4f5529b4d41`, `model.safetensors` exactly
**90,868,376** bytes, all-F32.

**The tests must fail at this point** (FR-013). Confirm that they do, and that they fail for the
right reason — missing implementation, not a broken fixture:

```bash
cargo nextest run -p xtriever-ffi --features spike   # expected: FAIL, not error
```

---

## Step 2 — Story 1: the build matrix (host only, no device needed)

This is the story that needs no iPhone and no provisioning, and it produces the spike's most
valuable finding if it fails.

```bash
for t in aarch64-apple-ios aarch64-apple-ios-sim; do
  echo "=== $t ==="
  cargo check -p xtriever-ffi --features spike --target "$t"
done
```

Then prove the C-free claim per target rather than asserting it (FR-003):

```bash
# must print nothing at all
cargo tree -p xtriever-ffi --features spike -e normal,build --target aarch64-apple-ios \
  | grep -iE "onig|zstd|-sys v|cc v[0-9]"

# and no C dependency may be reachable from any pure crate
for c in xtriever-core xtriever-analysis xtriever-pipeline xtriever-ltr xtriever-eval; do
  cargo tree -p "$c" -e normal,build | grep -iE "cc v[0-9]|-sys v" && echo "VIOLATION in $c"
done
```

`esaxx-rs` appearing with **no features** is expected and is not a violation — its C++ build is
gated behind its own `cpp` feature, which is off (research D2). The decisive signal is the absence of
`cc`.

Record one verdict per crate per target — 8 minimum (SC-001). A pass on the simulator is never
recorded as a pass on the device.

**`cargo check` is not sufficient on its own.** It performs no codegen and no linking. Story 1 is
only complete once the staticlib actually builds:

```bash
cargo build -p xtriever-ffi --features spike --release --target aarch64-apple-ios
lipo -info target/aarch64-apple-ios/release/libxtriever_ffi.a
```

---

## Step 3 — Story 2: bindings and the simulator

```bash
cargo run -p xtriever-ffi --features cli --bin uniffi-bindgen -- \
  target/aarch64-apple-ios/release/libxtriever_ffi.a build/swift --swift-sources
cargo run -p xtriever-ffi --features cli --bin uniffi-bindgen -- \
  target/aarch64-apple-ios/release/libxtriever_ffi.a build/swift/Headers --headers
cargo run -p xtriever-ffi --features cli --bin uniffi-bindgen -- \
  target/aarch64-apple-ios/release/libxtriever_ffi.a build/swift/Modules \
  --modulemap --module-name xtriever_ffiFFI --modulemap-filename module.modulemap
```

In practice, run `scripts/build-ios-harness.sh`, which performs all three invocations plus the
XCFramework assembly. Two flag details are load-bearing and cost real time to find (report F-006):

- **`--module-name xtriever_ffiFFI`** — the generated Swift opens with
  `#if canImport(xtriever_ffiFFI)`. Without it the modulemap declares `xtriever_ffi`, `canImport`
  is quietly false, and every FFI type reports "cannot find type ... in scope".
- **No `--xcframework`**, despite the name. It emits `framework module`, which needs a real
  `.framework` layout; our slices are static libraries, so Clang never matches the module — the same
  wall of errors, a different cause.

**Expect friction here.** The UniFFI guide does not document end-to-end device+simulator XCFramework
assembly — `xcodebuild -create-xcframework` appears nowhere in the v0.32.1 tree (research risk R2).

Simulator success criteria (FR-006 to FR-008): the app launches, all three operations return, and a
deliberately induced Rust error arrives in Swift as a caught `SpikeError` — **not** as a crash. Test
that last one explicitly by pointing the model path at a missing file; a process abort there means
some path is panicking instead of returning `Result`.

---

## Step 4 — Story 3: the device run

Build settings, which are part of the measurement and must be recorded (FR-019, research D11):

- **Release** configuration
- "Debug executable" **off**
- code coverage and all runtime sanitizers **off**

Run the harness twice on the same commit and device (FR-022), recording for each run: device model,
iOS version, build configuration, `ProcessInfo.thermalState`, and the thread count (must be 1).

**Note the variable name.** candle **0.9.2** reads `RAYON_NUM_THREADS`, *not* `CANDLE_NUM_THREADS` —
which later versions read, and which research D15 originally recorded in error. Exporting the wrong
one leaves the pool sized by the device's cores while the report claims it was pinned, quietly
invalidating both the footprint and the reproducibility comparison.

Per operation — index, model load, query, embed — record wall time from
`clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`, `phys_footprint` from `task_info(TASK_VM_INFO)`, and the
run's peak from `ri_lifetime_max_phys_footprint`. Also capture `os_proc_available_memory()` so the
device's **own** limit can be derived and reported separately from the 300 MB constitutional ceiling.
Those are two different thresholds (research D9).

Run model load **twice**, once per `LoadPath`, and record both footprints. That comparison is the
whole justification for ADR-0002; if the two paths produce different embeddings, the mmap path is a
finding and gets deleted.

Then check the oracles:

| check | criterion | FR |
|---|---|---|
| token sequence | **exact** equality vs `ExpectedTokens` — check this **first** | FR-015 |
| ranking | **exact** equality vs `ExpectedRanking`, including order and scores | FR-014 |
| `segment_count` | must be `1`, or the tie-break order is not comparable at all | D5 |
| embedding | cosine ≥ 0.9999 **and** max abs diff ≤ 1e-3 | FR-015 |
| peak footprint | ≤ 300 MB, reported as the measured value | FR-020 |

Checking tokens before the embedding is what stops a tokenizer disagreement from being misfiled as
an iOS embedding failure — and `tokenizer.json`'s baked-in 128-token truncation makes that a live
risk, not a hypothetical one (research D6).

### Binary size

Only one number means "installed on device" (research D10):

```bash
xcodebuild -exportArchive -archivePath XtrieverSpike.xcarchive \
  -exportPath export/ -exportOptionsPlist ExportOptions.plist   # thinning: <thin-for-all-variants>
cat export/"App Thinning Size Report.txt"
```

The **uncompressed** figure is the installed size. The `.app`, the `.xcarchive` and the `.ipa` are
all invalid sources — Apple states none of them is suitable for measuring app size. Attribute bytes
to the Rust staticlib with a link map (`LD_GENERATE_MAP_FILE`), and expect the 87.1 MiB of weights to
appear as a bundle resource rather than in the executable.

---

## Step 5 — The full gate (Agent Operating Rule 5)

Everything below must pass before any task is called done. If something fails, stop and report — do
not adjust a fixture, tolerance, corpus size or threshold to get past it (FR-028, Rule 6).

> **One deliberate exception, expected red through PR 1b.** `--all-features` enables `spike` and
> therefore the acceptance tests, which FR-013 requires to be committed *failing* until PR 2
> implements the operations. CI runs `--workspace` **without** `--all-features`, so CI stays
> green; paste the red log into the PR body as FR-013 evidence. Do **not** silence it with
> `#[ignore]` — that is weakening an oracle (FR-028).

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features
cargo nextest run --workspace --all-features
cargo deny check
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target aarch64-apple-ios-sim
cargo check --workspace --target aarch64-linux-android
cargo check --workspace --target wasm32-unknown-unknown   # expected FAIL at getrandom; tracked
```

`cargo deny check` is the interesting one: it passes **only** because of the candle 0.9.2 pin. If it
starts reporting `onig_sys`, someone has upgraded candle and ADR-0001 needs revisiting rather than
`deny.toml` needing an exception.

Note `[graph] all-features = true` in `deny.toml` means the non-default `spike` feature **is**
enabled during the check, so the gating gives no protection — the dependencies have to be genuinely
clean, which they are.

---

## Step 6 — Close out (Story 4)

Mechanically checkable, and should be checked rather than eyeballed (SC-008, SC-009):

- Every item in FR-001–FR-022 has a verdict of `pass`, `fail` or `untested`. No blanks.
- Every `fail` has a `Finding` with a smallest reproduction.
- Every design-constraining `Finding` has an ADR in `docs/adr/`.
- Every workaround is disclosed as a deviation with its cost — never shown as an unqualified pass.
- The report states that 1,000 documents is **1%** of the 100,000-chunk configuration the 300 MB
  ceiling is written against, and does not claim the ceiling holds at 100k (FR-021).
- The report states that no performance budget was set, and that these numbers are the baseline for
  later specs (FR-023).

## If it goes wrong

The spike is **complete** — and still valuable — when the close-out holds, even if every
measurement failed. Some specific traps:

| Symptom | Most likely cause | Do this |
|---|---|---|
| `can't find crate for 'std'` on an iOS target | Homebrew cargo shadowing rustup (D14) | Fix `PATH`. **Not** a Story 1 failure |
| `cargo deny` reports `onig_sys` | candle upgraded past 0.9.2 | Revisit ADR-0001. Do not touch `deny.toml` (Rule 2) |
| `stdarch_neon_f16` E0658 | candle ≥ 0.10 on stable | Same — the pin is what prevents this |
| Ranking mismatch, `segment_count > 1` | writer used more than one thread | Fix to `writer_with_num_threads(1, …)` (D5); the golden is not comparable otherwise |
| Embedding off but tokens match | genuine numeric divergence | A real finding — record it |
| Embedding off **and** tokens differ | truncation/padding not overridden to 256 | Fix per D6; not an iOS finding |
| App killed by the OS | memory limit exceeded | Record as a **FAIL** memory verdict with an ADR. Do **not** shrink the corpus and re-report as a pass (FR-028) |
| Story 3 impossible — no device or provisioning | environment | Record the memory verdict as `untested`. Never infer it from simulator numbers, which Apple documents as not reflecting real limits (D9) |
