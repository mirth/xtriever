# Report: The Demo Re-ranks at Depth 10

**Feature**: `018-demo-rerank-depth-10` | **Date**: 2026-09-16 | **Status**: done — the demo re-ranks 10 by default; the phone answers in 1.7 s instead of 2.6 s

## Verdict

An app setting, chosen on two measurements and confirmed by a third. The engine keeps depth
20 (the quality-maximising setting every baseline is measured at); the demo, whose user is
waiting on a phone, re-ranks the first 10 fused candidates: Feature 014 measured that at
−0.3 mean nDCG@10 (0.4881 vs 0.4913) for half the cross-encoder calls, Feature 017 measured
the harness on the phone at 1.41 vs 2.31 s median, and this feature's own device run of the
demo measures the end-to-end effect: **median total 1,705 ms against 009's 2,631 ms** (fused
unchanged at 342 vs 339 ms; re-ranked 1,369 vs 2,288 ms — −40 %), the worst query 1,934 ms
where 009's was 3,070 ms, footprint 305 MB. Settings and About say what the default is and
why, with the numbers; a persisted choice of 20 survives the upgrade.

## The record beside 009's (iPhone17,5, iOS 26.6.2, Release, mmap, default threads, airplane mode)

| | 009 `…T092434` (depth 20) | **018 `…T192434` (depth 10)** |
|---|---|---|
| fused search, median / max | 339 / 361 ms | 341.5 / 356 ms |
| re-ranked search, median / max | 2,288 / 2,731 ms | **1,369 / 1,602 ms** |
| total per query, median / max | 2,631 / 3,070 ms | **1,704.5 / 1,934 ms** |
| footprint after open / peak | ~509 MB / 536 MB (008-era) | 290 MB / **305 MB** |
| settings in the record | depth 20, 4,000 ms, non-strict | depth 10, 4,000 ms, non-strict |

The re-ranked time falls by 919 ms — the harness predicted 894 ms (017: 2,306 − 1,412) — and
the fused time does not move: the first stage is untouched. The footprint drop is Feature
010's id map, first seen on a Wikipedia device record in 017.


## Red checkpoint (Rule 4, T003)

`xcodebuild test … -only-testing:XtrieverWikiDemoTests/SettingsTests` on the simulator:

```text
apps/ios-wiki-demo/Tests/SettingsTests.swift:32:29: error: type 'Settings' has no member 'depthExplanation'
** TEST FAILED **
```

(the default assertion, 10 vs 20, and the choices assertion, `[0, 5, 10, 20]` vs `[0, 5, 20]`,
would fail next). Before any change the demo suite was green on the simulator (T001): 12 test
cases including the UI test.

## Simulator suite after the change (T007)

`xcodebuild test … -skip-testing:XtrieverWikiDemoTests/DemoMeasurementTests` on the iPhone 15
Pro simulator, package built with `--with-models --with-fixtures --demo`: **18 passed, 1
skipped** (`AboutTests` — needs the Wikipedia bundle, as before), 0 failures, `TEST
SUCCEEDED`. The four `SettingsTests` are green (default 10, choices 0 / 5 / 10 / 20, a
persisted 20 survives, the footer quotes 10 / 20 / 0.3 / 1.4 / 2.3); the UI flow (search →
hit → report → Settings → About) passes with the new Settings section and About rows.
(A first attempt, staged without the models, skipped the model-backed tests and failed the UI
flow at "the search field appears once the index is ready" — a staging mistake, recorded so
the next person rebuilds with `--with-models`.)

## Success criteria

| criterion | result |
|---|---|
| SC-001 a fresh install re-ranks 10 | **PASS** — `Settings().rerankDepth == 10` (test); the record's settings show depth 10 |
| SC-002 median re-ranked ≥ 30 % below 2,288 ms; fused within 20 % of 339 ms; footprint PASS | **PASS** — 1,369 ms (−40 %); 341.5 ms (+0.7 %); 305 MB |
| SC-003 009's figures hold (fused ≤ 1 s, re-ranked ≤ 3 s) | **PASS** — 342 ms / 1,369 ms; the max total 1,934 ms is now under 3 s too (009's max was 3,070) |
| SC-004 Settings and About explain; README and 009 documents point here | **PASS** |
| SC-005 `git diff --stat main -- crates/ swift/ specs/*/baselines` empty | **PASS** |

## Review round 1 (Copilot, three comments, all taken)

1. `testPersistedChoiceWins` only round-tripped `Settings` through `JSONEncoder`, so a
   `SettingsStore.load()` that ignored the stored value would still have passed. It now goes
   through `SettingsStore.save` / `load` on the real key (the previous value restored in
   `defer`), and also checks that an empty store yields the fresh default. Green on the simulator.
2. The Feature 018 note in the 009 report had been inserted inside its latency table, breaking
   the two rows below it; moved after the table.
3. The 009 spec's assumption now says explicitly that the pipeline's default is still 20 and
   only the demo app overrides it to 10.

## Deliberately not done

The engine's default stays 20 (ADR-0012's configuration; no goldens, baselines or harness
tests move); no device parity run (017 covered depth 10: parity PASS); no budget change
(4,000 ms never cut a query at depth 20, so it cannot at 10); no per-corpus depth.
