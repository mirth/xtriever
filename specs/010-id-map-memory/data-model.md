# Data Model: Shrink the Id Map's Resident Memory

**Feature**: `010-id-map-memory` | **Date**: 2026-09-15

Nothing on disk changes. This document describes the in-memory entities the feature replaces
and the invariants that must survive the replacement.

## `IdMap` (crate-private, `xtriever-pipeline/src/ids.rs`)

| field | type | invariant |
|---|---|---|
| `id_bytes` | `Vec<u8>` | every live or deleted slot's external id, in slot order, concatenated; never compacted in memory (a reopen drops deleted ids) |
| `spans` | `Vec<(u32, u32)>` | `spans.len()` = assigned-id space (`len()`); `spans[i] = (start, end)` into `id_bytes`; `start == end` ⇔ slot `i` deleted (an empty id is refused at assign — `Schema` "external id must not be empty"); spans are non-overlapping and ascending; `end ≤ id_bytes.len() ≤ u32::MAX` (exceeding it → `Corrupt` "id bytes exceed u32", the sibling of "internal id space exhausted (u32)") |
| `reverse` | `hashbrown::HashTable<u32>` | exactly the live slots; hashed and compared on `id_bytes[spans[i]]`; `reverse.len()` = `live()` |
| `chunks` | `Vec<ChunkSlot>` | empty until the first chunk is assigned; otherwise `chunks.len() == spans.len()` and `chunks[i].parent == NONE` (`u32::MAX`) ⇔ slot `i` has no chunk |
| `parent_bytes`, `parent_offsets` | `Vec<u8>`, `Vec<u32>` (`offsets.len() = parents + 1`) | distinct parent ids, interned at first use, never removed while open |
| `parents` | `hashbrown::HashTable<u32>` | one entry per distinct parent; index into `parent_offsets` |

`ChunkSlot { byte_range: Option<(u64, u64)>, parent: u32, ordinal: u32 }` — 32 bytes;
`parent == NONE` means no chunk (the other fields are then zero / `None`).

### Operations (unchanged signatures except `chunk`)

| op | behaviour (identical to today unless noted) |
|---|---|
| `assign(external, chunk) -> Result<DocId>` | empty → `Schema`; known → the existing id, chunk replaced (or removed if `None`); unknown → next slot, `u32` exhaustion → `Corrupt`; the parent is interned |
| `remove(external) -> Option<DocId>` | live → removed from `reverse`, span emptied, chunk slot set to `NONE`; unknown → `None`; the slot is never reused |
| `external(DocId) -> Option<&str>` | the arena slice, `None` for deleted or out-of-range |
| `internal(&str) -> Option<DocId>` | one hash + at most a few byte comparisons |
| `chunk(DocId) -> Option<ChunkInfo>` | **owned** (was `Option<&ChunkInfo>`): parent copied out of the arena; one caller (`search.rs`) |
| `len()`, `live()` | as today |
| `write(dir)` | as today, byte-identical output (research D6) |
| `read(dir)` | streaming (research D4); same refusals plus the one stated there |

### Derived bounds (the numbers the tests enforce)

- held after read ≤ `id_bytes + parent_bytes + 52·slots + 16·parents + 64 KiB`
  (spans 8, chunk slot 32, reverse ≤ 11.5, parent offsets 4 + table ≤ 11.5, rounded up)
- peak during read ≤ that bound + `ids.json` size + 16 MB

## `HybridIndex` (`index.rs`)

| field | before | after |
|---|---|---|
| `committed_ids` | `IdMap` | `Arc<IdMap>` — what `search`, `contains`, `external_id`, `external_of` read |
| `pending_ids` | `IdMap` | `Arc<IdMap>` — mutated only through `Arc::make_mut` in `stage_one` and `delete` |

State machine of the pair:

```text
open / create ──► shared (ptr_eq)  ──stage_one / delete──►  split (pending is a private copy)
                       ▲                                              │
                       └──────────────────── commit ──────────────────┘
```

`commit` writes `pending_ids`, then `committed_ids = Arc::clone(&pending_ids)`. A failed
commit leaves the pair split and `dirty`, as today.

## `ids.json` (on disk — unchanged, format version 2)

```json
{"format_version":2,"external":["a","b",null,"c"],"chunks":{"1":{"parent":"p","ordinal":1,"byte_range":[0,10]}}}
```

Serialized by `serde_json::to_vec` of `OnDisk` (no whitespace, field order
`format_version`, `external`, `chunks`; `chunks` omitted when empty; chunk keys are decimal
internal ids in **string** order — `BTreeMap<String, _>`). See
[contracts/id-map.md](./contracts/id-map.md).
