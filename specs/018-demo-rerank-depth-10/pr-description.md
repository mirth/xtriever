# 018 the demo re-ranks at depth 10

**One setting**: the iOS Wikipedia demo's default re-rank depth is 10 (was 20); the picker
offers 0 / 5 / 10 / 20; a persisted choice survives. The engine's default stays 20.

**Why**: Feature 014 measured depth 10 at −0.3 mean nDCG@10 (0.4881 vs 0.4913) for half the
cross-encoder calls; Feature 017 measured the phone at 1.41 vs 2.31 s per re-ranked search.
For a person waiting on a phone the 0.9 s is worth more than the 0.3 points; the engine keeps
the quality-maximising default the baselines are measured at.

**Measured** (the demo's own device run, iPhone17,5, airplane mode, default threads — beside
009's record):

| | 009 (depth 20) | **018 (depth 10)** |
|---|---|---|
| fused, median | 339 ms | 341.5 ms |
| re-ranked, median | 2,288 ms | **1,369 ms** (−40 %) |
| total, median / max | 2,631 / 3,070 ms | **1,705 / 1,934 ms** |
| footprint peak | 536 MB | 305 MB |

009's acceptance figures hold with room (fused ≤ 1 s, re-ranked ≤ 3 s — the worst query is
now under 2 s).

**Changes**: `apps/ios-wiki-demo/App/Model/Settings.swift` (default, choices, the footer text
with the numbers), `SettingsView` (the depth section's footer), `AboutView` ("re-rank depth
(engine default)" and "(app default)" rows), `Tests/SettingsTests.swift` (committed red first),
the README, pointers in the 009 spec/report, the record and report. Simulator suite: 18 passed,
1 skipped (needs the wiki bundle, as before).

**Unchanged**: everything under `crates/` and `swift/`, every baseline and golden, the harness
package tests.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
