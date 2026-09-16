# Data Model: The Demo Re-ranks at Depth 10

**Feature**: `018-demo-rerank-depth-10` | **Date**: 2026-09-16

| entity | shape / rule |
|---|---|
| **`Settings`** (demo) | `rerankDepth: UInt32 = 10` (was 20); `budgetMs: UInt64? = 4_000`; `strict = false`; `static depths = [0, 5, 10, 20]`; `rerankedOptions.rerankDepth == rerankDepth`; `fusedOptions` unchanged (depth 0) |
| **Persistence** | `SettingsStore` JSON under one `UserDefaults` key; a stored value wins over the default (a persisted 20 stays 20); nothing stored → `Settings()` |
| **Settings copy** | the depth picker's footer: the app default (10), the cost (−0.3 mean nDCG@10 on the BEIR sets, 014), the gain (1.4 s vs 2.3 s median re-ranked search on the reference phone, 017), the engine default (20) |
| **About copy** | "re-rank depth (engine default)" = the index's recorded depth; "re-rank depth (app default)" = `Settings().rerankDepth` |
| **Demo run record** | the 009 shape: device, os, build, `feature` (harness constant `009-ios-wiki-demo`), settings, per-query `fusedMs` / `rerankedMs` / `engineFusedMs` / `engineRerankedMs` / hits, `latency {medianFusedMs, maxFusedMs, medianRerankedMs, maxRerankedMs, medianTotalMs, maxTotalMs}`, footprint, verdicts; the settings in the record MUST show depth 10 |

Invariants: `git diff --stat main -- crates/ swift/ specs/*/baselines` empty; the demo tests
that set a depth explicitly are unchanged; the record's medians satisfy spec SC-002/SC-003.
