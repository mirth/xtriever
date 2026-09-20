# Data Model: The Android Kotlin Wikipedia Demo (Feature 025)

Phase 1. Nothing here is a new on-disk format. The engine's formats are unchanged; what
follows is what the module and the application hold, and the states they move through.

## On the device

### The prepared resource set

Everything the engine opens lives in the application's private files directory, written once
at first launch from the assets inside the package.

| Path (under the application's files directory) | What it is | Size |
|---|---|---|
| `corpus/index/` | the hybrid index — lexical segments, `dense/manifest.bin`, `dense/vectors.0.bin`, passages, id map, descriptor | 24 MB |
| `corpus/corpus.json` | the corpus sidecar: edition, snapshot date, counts, identity, chunker block | small |
| `corpus/ATTRIBUTION.txt` | the licence attribution, shown verbatim | small |
| `models/embedder/` | the pinned embedding model | 87 MB |
| `models/reranker/` | the pinned cross-encoder | 87 MB |
| `.prepared` | the completion marker, written last, holding the staged resource version | one line |

**Rules**: every file is written to a temporary name and renamed. The marker is written after
the last rename and read before anything is opened. A files directory carrying no marker is
treated as absent and re-extracted, whatever it contains (FR-011).

### Preparation state

```text
Absent ──extract──▶ Extracting ──marker written──▶ Ready
   ▲                    │
   └────interrupted─────┘                Insufficient space ──▶ Refused (names the amount)
```

| State | What the person sees | What the engine may do |
|---|---|---|
| `Absent` | "preparing…" with a progress fraction | nothing is opened |
| `Extracting` | the same, advancing | nothing is opened |
| `Ready` | the search screen | index and both models open |
| `Refused` | the space required and what to free | nothing is opened |
| `Unsupported` | the processor requirement, plainly | nothing is loaded at all (D5) |

## In the module

### `XtrieverIndex` (the wrapper)

Mirrors `swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift`: holds the opened handle, the
paths it opened, the time each open took, and the index information the engine reports. It adds
no capability the Swift wrapper lacks (FR-001). The generated bindings underneath carry the
surface as-is: search options, hits, explanations, the stage report and the index information,
including `denseCompactDeadShare`.

### `DeviceSupport`

A single query answered before any native call: are `fphp` and `asimdhp` present in
`/proc/cpuinfo`? Its answer is either "supported" or a named refusal carrying the requirement
(D5). Nothing else in the module runs until it says yes.

## In the application

### A search, and what it produces

One submitted question becomes two engine calls — the fused stage, then the re-ranked stage —
exactly as on the other two platforms. The application holds, per search: the two hit lists,
the mark per hit computed from their order (moved up by n, moved down by n, new, unchanged),
the identifiers that left the head, the engine's stage report, and a wall clock around each
call.

| Field of a displayed hit | Source |
|---|---|
| title, passage | the hit text, split at the first blank line (Feature 008 convention) |
| article link | the title, percent-encoded outside `A–Z a–z 0–9 - _ . ~ /` |
| fused score, re-rank score | the engine's hit |
| the eight explained features | the engine's explanation, "not seen by this stage" where absent |
| the mark | computed from the two lists' orders — the only arithmetic the application does |

### Settings

| Setting | Values | Default | Persisted |
|---|---|---|---|
| re-rank depth | 0, 5, 10, 20 | 10 (Feature 018) | yes, survives restart |

## The record

Written after the measurement run, in the shape Features 009, 018 and 019 use, with the fields
this platform adds.

| Field | Meaning |
|---|---|
| `device` | the emulated device and its system image |
| `emulated` | `true` — and the record states what that bounds (FR-017) |
| `host` | the machine running the emulator |
| `os` | the Android version and API level |
| `threads`, `threadsSource` | the engine's effective pool size and where it came from |
| `perDepthMedianMs`, `perDepthMaxMs` | per re-rank depth over the twenty measurement queries |
| `peakResidentBytes` | measured, with the project's 600 MB phone ceiling quoted beside it for comparison only |
| `parity` | the verdict against the host's answers, with counts of identical bits |
| `index` | documents, format version, corpus identity, model fingerprints |
