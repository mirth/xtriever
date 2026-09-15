# Contract: the id map — file format (unchanged) and in-memory guarantees

**Feature**: `010-id-map-memory` | **Date**: 2026-09-15

## 1. `<index>/ids.json` — format version 2, byte-for-byte as before

This feature changes nothing about the file. The contract is restated so that the
byte-identity oracle (research D6) has a normative text to check against.

- One JSON object, no whitespace, produced by `serde_json::to_vec`:
  `{"format_version":2,"external":[…],"chunks":{…}}`.
- `external`: an array with one entry per assigned internal id, in id order; a string for a
  live id, `null` for a removed one. Entries are never reordered or dropped by a writer.
- `chunks`: an object mapping the **decimal** internal id (as a string) to
  `{"parent":"…","ordinal":N,"byte_range":[start,end]}` (`"byte_range":null` when unknown);
  keys in string order (`"0","1","10","100",…`). The whole member is **omitted** when no
  chunk is recorded.
- Non-ASCII in ids is written as UTF-8, not escaped; `"`, `\` and control characters are
  escaped as serde_json does.

Oracles: (a) `sha256(ids.json)` of the 008 index, `20028054c295e4afa396654b98c0539a1fb28c799740127efc42fd55b3a39994`,
must reproduce from `read` → `write`; (b) `reference/fixtures/010/ids-golden.json` (written by
the pre-change code) must reproduce from `read` → `write` and from replaying the scripted
sequence in `reference/fixtures/010/ids-golden-script.json`.

## 2. Reading — accepted inputs and refusals

| input | outcome |
|---|---|
| a file written by any 2.x pipeline | reads; every `external(id)`, `internal(ext)`, `chunk(id)`, `len()`, `live()` equal to today's |
| `format_version` ≠ 2 | `Corrupt` "… is format version N, this build reads 2" |
| an external id appearing twice among live entries | `Corrupt` "external id {ext:?} appears twice in ids.json" |
| a chunk key that is not a decimal `u32` | `Corrupt` "chunk key {key:?} is not an internal id" |
| a chunk key with no slot in `external` | `Corrupt` "chunk key N has no slot in ids.json" (new; no writer can produce it — research D4) |
| unknown top-level members | ignored |
| a top-level member repeated | error (serde `duplicate_field`), surfaced as `Corrupt` "… is not a valid id map: …" |
| members in any order | accepted |
| unreadable file | `Corrupt` "cannot read {path}: {io}" (as today) |

## 3. Memory — what `open` may hold

- Exactly one `IdMap` while no change is staged (`Arc::ptr_eq(committed, pending)`).
- After `read`: held bytes ≤ `id_bytes + parent_bytes + 52·slots + 16·parents + 64 KiB`.
- During `read`: peak ≤ the bound above + file size + 16 MB, for files in the order `write`
  produces.
- Time: `read` of the 008 file not slower than today's by more than 10 % (host, `--release`).

## 4. Writer semantics (unchanged, restated as the test list)

assign new → next id · assign known → same id, chunk replaced · assign with `None` → chunk
removed · assign `""` → `Schema` · remove live → `Some(id)`, slot reserved forever · remove
unknown → `None` · `len()` counts reserved slots, `live()` does not · a search or `contains`
during staged changes sees the committed map only · commit → written map = pending map,
committed = pending · reopen → identical map.
