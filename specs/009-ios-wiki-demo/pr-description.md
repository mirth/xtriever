# Feature 009 — The iOS Wikipedia Demo App

**Written for**: the reviewer of the `009-ios-wiki-demo` branch.

A thin SwiftUI app at `apps/ios-wiki-demo/` over the 007 package and the 008 index: it opens
the bundled Simple English Wikipedia index in place, warms it, and for every submitted question
shows the fused list first (339 ms median on the iPhone 16e), then the re-ranked order with
what moved (2.3 s), each hit's explanation under the engine's feature names, the engine's stage
report, settings (depth / budget / strict) and an About screen with the corpus identity and the
CC BY-SA attribution. Peak footprint **533 MB vs the 600 MB ceiling: PASS**. The app owns no
retrieval logic; nothing under `crates/`, `swift/Xtriever/Sources/` or `.github/` changed.
Report: [report.md](./report.md); device record: [runs/](./runs/).

## Commits

| commit | content | hand-written lines |
|---|---|---|
| PR 1 | `project.yml` (the harness's shape), `--demo` in the build script, `.gitignore`, README, the red test target (support, `ChangeMarkTests`, `DemoModelTests`, `AboutTests`, `DemoMeasurementTests`) | ~640 |
| PR 2 | the model: `ChangeMark`, `Settings`, `Preparation`, `CorpusSidecar`, `SearchState`, `DemoModel` | ~385 |
| PR 3 | the views (7 files), `DisplayedHit`, the UI walk, the test-host guard | ~440 |
| PR 4 | the device record, report, PR description, `extract-device-run.py` (reads the 009 record shape) | docs + JSON |

## Numbers (iPhone 16e, Release, airplane mode, default settings)

- Footprint: baseline 12 MB, after open + warm-up 532 MB, **peak 533.0 MB / 600 MB PASS**.
- Latency (20 measurement queries, after warm-up): fused median **339 ms** (max 361), re-ranked
  median **2,288 ms** (max 2,731), total median **2,631 ms** (max 3,070) — SC-001 met on the
  medians; the max is reported beside them.
- The budget (4,000 ms) never cut a stage: 20 of 20 pairs scored on every query.
- Open 1,079 ms, warm-up 477 ms; per re-ranked pair ≈ 97 ms (008: 97 ms); 6 threads (the processor count, candle's default).
- Peak footprint **533.0 MB vs 600 MB: PASS**.

## Tests

Simulator (Debug, against the 007 fixture and its goldens): `ChangeMarkTests` 5, `DemoModelTests`
8 — fused and re-ranked hits **bit-identical** to the goldens, cancellation 20 / 20, main-thread
cadence ≥ 90 %, strict/non-strict budgets, empty query — and `DemoUITests` 1 (a walk through
search, report, Settings, About, empty). Device (Release): `AboutTests` (sidecar and attribution
equality) and `DemoMeasurementTests` (the record above), each in its own process (report F-002).

## Gate

Rust gate unchanged and green (fmt, clippy `-D warnings`, nextest 251 / 251, deny, `cargo check` on iOS / iOS-sim / Android). No eval
delta due — the app adds no computation. CI unchanged (standing rule: no simulator or device
job). `git diff --stat main -- crates/ swift/Xtriever/Sources/ .github/` is empty.

## Review round 1

Copilot, six comments (report table): warm-up failure now fails preparation; the tapped hit
travels with the navigation; the measurement records only completed re-ranked queries; the
record carries the effective thread count and its source; the gate lists the cross-target
checks; the sidecar `try?` and T028's ownership argued and kept.

## Deliberately not done (research D11)

No thread knob, no streaming search, no "re-rank this list" entry point (all engine changes);
no as-you-type search (owner decision Q1); no App Store packaging. The fused-first design
re-runs the lexical and dense stages once per query — ~0.34 s of the 2.63 s median — stated in
the report.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
