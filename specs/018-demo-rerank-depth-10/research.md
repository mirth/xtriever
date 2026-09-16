# Research: The Demo Re-ranks at Depth 10

**Feature**: `018-demo-rerank-depth-10` | **Date**: 2026-09-16 | **Plan**: [plan.md](./plan.md)

## D1 — Where the default lives

`apps/ios-wiki-demo/App/Model/Settings.swift`: `struct Settings { var rerankDepth: UInt32 =
20; var budgetMs: UInt64? = 4_000; var strict = false; static let depths: [UInt32] = [0, 5,
20]; … }`, persisted as JSON under one `UserDefaults` key by `SettingsStore` (`load()` falls
back to `Settings()` when nothing is stored — so a persisted 20 survives and only the fresh
default changes, spec edge case 1). `SettingsView` builds the picker from `Settings.depths`;
`AboutView` shows `info.info.rerankDepth` under "Engine" (the index's recorded depth, 20).
**Decision**: `rerankDepth = 10`, `depths = [0, 5, 10, 20]`; the Settings picker gains a
footer sentence with the numbers and the engine default; About's "re-rank depth" row is
labelled "re-rank depth (engine default)" and an "app default" row (`Settings().rerankDepth`)
is added beside it. The doc comment on `Settings` cites 014 / 017 instead of 008.

## D2 — The numbers the UI quotes

014 `runs/table.md`: `lin-0.5` depth 10 mean 0.4881, depth 20 mean 0.4913 (−0.0032); 017
`runs/iPhone17,5-20260916T153735-mmap-threadsdefault.json`: median 1,412 ms at depth 10 vs
2,306 ms at depth 20 (harness search, k 10, default threads). **Decision**: the UI says
"−0.3 mean nDCG@10 on the BEIR sets, half the cross-encoder calls, 1.4 s instead of 2.3 s on
the reference phone (014, 017)"; the README gives the same with the feature pointers.

## D3 — The tests

`apps/ios-wiki-demo/Tests/DemoModelTests.swift` sets depths explicitly (5 / 0 / 20);
`AboutTests` and `DemoMeasurementTests` use `Settings()` — so the measurement runs at the new
default by construction. No existing test asserts the default. **Decision**: add
`SettingsTests.swift` (the red test): `Settings().rerankDepth == 10`, `Settings.depths ==
[0, 5, 10, 20]`, `rerankedOptions.rerankDepth == 10`, and `SettingsStore` round-trip keeps an
explicit 20 (persistence honoured). The demo's `DemoModelTests` against the 007 fixture
goldens use depth 5 explicitly and are untouched.

## D4 — The device run

009 quickstart Step 3: `scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki
--demo`, then `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project
apps/ios-wiki-demo/XtrieverWikiDemo.xcodeproj -scheme XtrieverWikiDemo-Measure … -only-testing:
XtrieverWikiDemoTests/DemoMeasurementTests` (alone in its process — 009 F-002), extracted with
`scripts/extract-device-run.py <log> specs/018-demo-rerank-depth-10/runs/`. The record's
`feature` field is the harness's constant (`009-ios-wiki-demo`); the report says which
feature ran it, as 017 did. The record carries `settings` (depth, budget, strict) — checked
before the run is accepted: depth must read 10. Expected against 009's record (default
threads): fused median ≈ 339 ms unchanged; re-ranked median ≈ 2,288 − 900 ≈ 1.4 s; total
≈ 1.75 s; footprint unchanged (~307 MB per 017).

## D5 — Documents

`apps/ios-wiki-demo/README.md` (the re-ranking paragraph 017 added: the default depth and the
trade-off); `specs/009-ios-wiki-demo/spec.md` Assumptions "Default budget: 3,000 ms and
re-rank depth 20 …" gains a "superseded by 018" note; `specs/009-ios-wiki-demo/report.md`
gains a pointer under its latency table.

## D6 — Not done

No engine, pipeline, FFI or format change (the engine default stays 20 — ADR-0012's
configuration; the harness package's tests and goldens do not move); no parity run (017
covered depth 10); no budget change (4,000 ms never cut a query at depth 20, so it never cuts
one at 10).
