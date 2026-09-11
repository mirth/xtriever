# Running the spike on a physical iPhone

Target device for Feature 001: iPhone 16e (`iPhone17,5`)

FR-022 requires **at least two runs** on the same commit and device. Do the whole thing twice.

## Before you start

- iPhone connected by USB, unlocked, this Mac trusted.
- Developer Mode on: Settings ▸ Privacy & Security ▸ Developer Mode, then reboot.
- A signing team in Xcode ▸ Settings ▸ Accounts. A free Apple ID is enough — it gives a 7-day
  profile, which is plenty. Find the team id with:
  ```sh
  security find-identity -v -p codesigning        # or read it from Xcode ▸ Settings ▸ Accounts
  ```
- Device cool and plugged in. Thermal state is recorded with every measurement, and a throttled
  device produces wall times that cannot be compared between the two runs.

## 1. Build the harness with the model bundled

```sh
cd "$(git rev-parse --show-toplevel)"
scripts/build-ios-harness.sh --with-model
```

`--with-model` matters: on device there is no host filesystem to read from, so the 87.1 MiB of
weights must ride along in the bundle. Without it the embedding measurement is skipped and the run
is incomplete.

## 2. Run

```sh
cd "$(git rev-parse --show-toplevel)/harness/ios/XtrieverSpikeApp"

xcodebuild test \
  -project XtrieverSpikeApp.xcodeproj \
  -scheme XtrieverSpikeApp \
  -configuration Release \
  -destination 'platform=iOS,id=ABC123' \
  -skipMacroValidation \
  -allowProvisioningUpdates \
  ARCHS=arm64 \
  DEVELOPMENT_TEAM=<your-team-id>
```

Every part of that is load-bearing:

| flag | why |
|---|---|
| `-project XtrieverSpikeApp.xcodeproj` | **not** the SwiftPM package. A device rejects a hostless test bundle — "Tool-hosted testing is unavailable on device destinations" — so the tests need an app to host them |
| `-configuration Release` | a Debug measurement is not a measurement (research D11). The harness records `buildConfiguration` and flags the run if it sees Debug |
| `ARCHS=arm64` | the Rust slices are arm64-only by design (Principle III names the two aarch64 triples). Without it a Release build also tries x86_64 and fails with "symbol(s) not found for architecture x86_64" — which reads like a missing symbol but is a missing slice nobody asked for. It must be on the **command line**: a project-level setting does not reach the SwiftPM package target |
| `-allowProvisioningUpdates` | lets Xcode mint the profile rather than failing on signing |

`RAYON_NUM_THREADS=1` is set by the scheme, so it needs no flag — candle would otherwise size its
thread pool from the core count, perturbing both footprint and float summation order (research D15).

If a build fails oddly after any change here, clear stale state first — it cost a confusing
detour during validation:

```sh
rm -rf ~/Library/Developer/Xcode/DerivedData/XtrieverSpikeApp-*
```

## 3. Capture the output

The run prints one JSON block:

```
=== XTRIEVER DEVICE RUN BEGIN ===
{ ... }
=== XTRIEVER DEVICE RUN END ===
```

Paste **both runs'** blocks back. They are also attached to the `.xcresult` as `device-run.json`.

## What the run checks

Beyond the numbers, it verifies the oracles in the order that keeps causes distinguishable:
one segment → ranking **bit-identical** to the host golden → embedding within cosine 0.9999 and
1e-3 → both weight-load paths agreeing bit-for-bit (ADR-0002 condition 4).

## If it fails

That is a **result**, not something to work around (FR-028, Agent Operating Rule 6).

- **App killed by the OS** — the memory verdict is FAIL. Do not shrink the corpus and rerun; the
  spec forbids it explicitly. Send the output and we record a finding.
- **Ranking mismatch** — check `segment_count` first. If it is not 1 the golden is not comparable
  and the mismatch is uninterpretable rather than a real difference.
- **Signing errors** — an environment problem, not a spike result.

## Known caveat before you see the numbers

`os_proc_available_memory()` returns 0 on the Simulator, so `observedMemoryLimitBytes` was
meaningless in validation. On device it should report the real limit — which is a **different
threshold** from the constitution's 300 MB ceiling and must not be conflated with it. Apple
publishes no per-device limits, which is exactly why the device model is recorded alongside.
