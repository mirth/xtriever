# Data Model: The iOS Wikipedia Demo App

**Feature**: `009-ios-wiki-demo` | **Date**: 2026-09-14 | **Plan**: [plan.md](./plan.md)

All types are Swift, in the app target (`apps/ios-wiki-demo/App/Model/`). The engine's types
(`XtrieverIndex`, `IndexInfo`, `SearchResponse`, `Hit`, `HitExplain`, `StageReport`,
`SearchOptions`, `LoadPath`, `XtrieverError`, `Measure`, `HarnessResources`) come from the 007
package unchanged.

## Preparation (state machine)

```
idle ─start()─▶ loadingModels ─▶ openingIndex ─▶ warming ─▶ ready(ReadyInfo)
                     │                │             │
                     └── failed(PreparationFailure) ◀┘
```

| Case | Carries | Transition |
|---|---|---|
| `idle` | — | `start()` |
| `loadingModels` | — | `XtrieverIndex.open` in flight (both models + index; one call — the label changes on a timer from the open's `embedderLoadMs` afterwards, honestly: "opening index and models") |
| `warming` | — | one depth-0 search of a fixed query, result discarded |
| `ready` | `ReadyInfo { info: IndexInfo, openMs, warmMs, corpus: CorpusSidecar?, attribution: String? }` | searches allowed |
| `failed` | `PreparationFailure.missingResource(name, stagingFlag)` or `.engine(message)` | retry allowed |

`ReadyInfo.corpus` / `attribution` are `nil` when the fixture (not the Wikipedia index) is
bundled — the About screen then says which index is open.

## Settings

| Field | Type | Values | Default |
|---|---|---|---|
| `rerankDepth` | `UInt32` | 0, 5, 20 | 20 |
| `budgetMs` | `UInt64?` | nil, 500, 1_000, 2_000, 4_000, 8_000 | 4_000 |
| `strict` | `Bool` | | false |

Persisted with `@AppStorage`. `SearchOptions` for the two searches:
fused = `(k: 10, rerankDepth: 0, explain: true)`; re-ranked = `(k: 10, rerankDepth:
rerankDepth, maxTimeMs: budgetMs, strict: strict, explain: true)`.

## SearchState

| Field | Type | Notes |
|---|---|---|
| `id` | `UUID` | one per submission; the model ignores results whose id is not current |
| `query` | `String` | trimmed; empty → `.empty` without calling the engine |
| `phase` | `.fusing`, `.reranking`, `.done`, `.empty`, `.failed(String)` | |
| `fused` | `SearchResponse?` | set when the first search returns |
| `reranked` | `SearchResponse?` | set when the second returns; `nil` if depth 0 (then `phase == .done` after `fused`) |
| `marks` | `[String: ChangeMark]` | by `externalId`, computed when `reranked` lands |
| `dropped` | `[Hit]` | fused hits absent from the re-ranked top 10 |
| `fusedMs`, `rerankedMs` | `UInt64?` | the app's wall times around each call (the engine's `elapsedMs` is shown too, from the response) |
| `footprintBytes` | `UInt64?` | `Measure.snapshot()` after the last response |

Invariants: `reranked != nil ⇒ fused != nil`; `marks.keys == Set(reranked.hits.externalId)`;
a stale (cancelled) search never mutates the model.

## ChangeMark

`enum ChangeMark: Equatable { case new, same, up(Int), down(Int) }` — `up(n)` means the hit
rose by `n` positions from its fused rank. `compute(fused:reranked:) -> (marks: [String:
ChangeMark], dropped: [Hit])`; pure; O(n).

## DisplayedHit (view model, derived)

`rank: Int`, `title: String`, `passage: String` (from `hit.titleAndPassage`; when `nil`, the
whole `hit.text` as passage and the external id as title — a non-008 index), `url: URL?`,
`ordinal: UInt32?`, `mark: ChangeMark?`, `features: [(name: String, value: Float)]` (from
`HitExplain.features()`; `.nan` → "not seen by this stage"), `score: Double`,
`rerankScore: Float?`.

## CorpusSidecar (decoded from `corpus.json`, 008 data-model)

`schema_version: Int`, `corpus_identity: String`, `snapshot: { edition, snapshot_date }`,
`counts: { articles, selected, passages }` — the fields the About screen shows; unknown keys
ignored.

## Device run record (`specs/009-ios-wiki-demo/runs/*.json`)

The 008 record shape (`schemaVersion: 2`) with `feature: "009-ios-wiki-demo"`, `corpus:
"wikipedia"`, and per query `{ id, fusedMs, rerankedMs, engineFusedMs, engineRerankedMs,
hits, rerankCandidates, rerankScored, footprintAfterBytes }`; `perDepthMeanMs` becomes
`{ "fused": …, "reranked": … }` plus `medianFusedMs`, `medianRerankedMs`, `medianTotalMs`
(SC-001 is judged on the medians; maxima recorded beside them). Footprint and verdict fields
unchanged (ceiling 600 MB). No parity section: the goldens were checked by 008's harness on
the same index; the app's fidelity is tested against the fixture goldens (SC-003).
