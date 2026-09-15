# Research: Shrink the Id Map's Resident Memory

**Feature**: `010-id-map-memory` | **Date**: 2026-09-15 | **Plan**: [plan.md](./plan.md)

Every number below was measured on this host (M-series Mac, `--release`, a counting global
allocator that sums requested bytes) against the 008 index at `target/xt-wiki/index/` —
427,947 passages, 239,436 distinct parents, `ids.json` 32,037,349 B (sha256 `20028054…`).

## D1 — What the id map costs today (the baseline, by the method the tests will use)

| | bytes | per passage |
|---|---|---|
| `read_to_string` of `ids.json` | 32.0 MB | — |
| `OnDisk` parse, text still held | 117.9 MB (peak during read) | — |
| one `IdMap` (`Vec<Option<String>>` + `HashMap<String,u32>` + `BTreeMap<u32,ChunkInfo>`) | **88.5 MB** | **207 B** |
| two (`committed_ids` + `pending_ids`) | **174.7 MB** | 408 B |
| whole `HybridIndex::open_with(mapped, read_only)` — heap after open | **178.2 MB** | — |
| open time | 361 ms | — |

The id maps are 98 % of the pipeline's heap after open; the rest (tantivy's reader, the
descriptor, the mapped dense stage's handle) is 3.5 MB. 008 F-002's "roughly 100 MB" per copy
was an over-estimate by 12 %; the two-copies claim was exact. The bytes that carry information
are small: **3,441,429 B of external ids** (average 8.0) and **1,421,907 B of distinct parent
ids** — 4.9 MB, 5.5 % of what is held.

**Decision**: the baseline of record is the table above, reproduced by the red test at the
tests-first commit (same method, same file) so "before" and "after" are one command apart.

## D2 — The compact shape (owner decision Q1 = B)

Hold each fact once, in contiguous storage, with fixed-width entries per slot:

| field | type | per passage | Wikipedia |
|---|---|---|---|
| id bytes | `Vec<u8>`, all external ids concatenated | ids' own bytes | 3.4 MB |
| spans | `Vec<(u32, u32)>` start/end into the id bytes; an empty span is a deleted slot (empty ids are refused at assign, so the encoding is unambiguous) | 8 B | 3.4 MB |
| reverse | `hashbrown::HashTable<u32>` of live internal ids, hashed and compared on the id bytes | ≤ 11.5 B (4 B + 1 control byte at ≥ 50 % load) | 2.6 MB |
| chunk slots | `Vec<ChunkSlot { byte_range: Option<(u64, u64)>, parent: u32, ordinal: u32 }>` — 32 B; empty (len 0) until the first chunk is assigned, then one per slot with `parent == NONE` (`u32::MAX`) for "no chunk" | 32 B | 13.7 MB |
| parent bytes + offsets + table | the same arena/table pair for distinct parents; never removed (a parent whose chunks are all removed stays until the next reopen) | ids' bytes + ≤ 16 B per parent | 3.7 MB |
| **total** | | **≈ 52 B + id bytes, + 16 B + parent bytes per parent** | **≈ 27 MB (63 B/passage)** |

Predicted saving on the host counter: 174.7 − 27 ≈ **148 MB**. On the device the saving is
larger: iOS's allocator rounds each of today's ~1.3 M small `String` blocks (7–9 B requested)
up to 16 B, which the host counter does not see (~22 MB more RSS today); the compact shape is a
dozen large blocks with no rounding. SC-001's 150 MB is therefore expected with margin, but
the number the record shows is the number — Rule 6 applies if it falls short.

**Why 32-byte chunk slots and not 24**: packing "no range" into a sentinel (`start == u64::MAX`)
or the "has range" flag into `parent`'s top bit would make one representable `ChunkInfo` value
unrepresentable or cap parents at 2^31. `Option<(u64, u64)>` inside the slot keeps every
`ChunkInfo` value round-trippable, at 3.4 MB for this corpus. Fidelity over 8 B.

**Alternatives rejected**:
- *Sorted `Vec<u32>` + binary search instead of a hash table*: zero dependencies, but the
  writer path inserts in ingestion order — O(n) per insert into a sorted vector is O(n²) over
  a 428k build.
- *`std::collections::HashMap<String, u32>` kept as is*: it is the 24 B + heap per key that
  the feature removes; std has no stable API for keys stored outside the map.
- *Hand-written open addressing*: a hash table is a commodity (Principle I); `hashbrown` is
  the one std itself uses, pure Rust, `no_std`.
- *Deriving the parent from the id* (`"{page}#{ordinal}"` → `page`): true for 008's corpus,
  not for the type; correctness must not depend on the corpus (spec assumption).

## D3 — `hashbrown::HashTable` (0.17.1, `default-features = false`)

Added with `cargo add hashbrown -p xtriever-pipeline --no-default-features` (Rule 7: version
from the resolver, `0.17.1`, already in the lock as safetensors' 0.16.1 sibling). Without
default features it pulls **no** dependency (`cargo tree`: `hashbrown v0.17.1` is a leaf — no
`foldhash`, no `allocator-api2`, no `equivalent`). `#![no_std]` (`src/lib.rs:12`), no build
script, MIT OR Apache-2.0; `cargo deny check` passes with it.

Items used (`src/table.rs`, hashbrown-0.17.1):
- `HashTable::with_capacity(n)` (`:90`), `HashTable::new()` (`:70`), `len()` (`:962`).
- `find(&self, hash: u64, eq: impl FnMut(&T) -> bool) -> Option<&T>` (`:228`) — the lookup:
  hash the query bytes, compare against the arena slice of each candidate id.
- `find_entry(&mut self, hash, eq) -> Result<OccupiedEntry, AbsentEntry>` (`:304`) and
  `OccupiedEntry::remove(self) -> (T, VacantEntry)` (`:2081`) — removal.
- `entry(&mut self, hash, eq, hasher: impl Fn(&T) -> u64) -> Entry` (`:412`) with
  `Entry::insert`/`or_insert_with` (`:1806`, `:1890`) — find-or-intern for parents.
- `insert_unique(&mut self, hash, value, hasher) -> OccupiedEntry` (`:701`) — insertion after
  `find` returned `None` (assign, read).
- `allocation_size()` (`:1573`) — reported in the accounting test's breakdown.
- `impl Clone for HashTable` (`:1627`) — `IdMap: Clone` derives.

The hasher is `std::hash::BuildHasherDefault<std::hash::DefaultHasher>` via
`BuildHasher::hash_one(&[u8])` (std, stable since 1.71): deterministic, no extra crate; SipHash
over 8-byte keys costs ~20–30 ms for 428k inserts at open, inside FR-009's budget.

## D4 — Streaming the parse (SC-004)

`serde_json::from_str::<OnDisk>` materialises `Vec<Option<String>>` and
`BTreeMap<String, ChunkInfo>` — 86 MB of transient on top of the 32 MB text — before a single
byte reaches the compact shape. SC-004 (peak ≤ final + file + 16 MB) cannot be met by any
post-processing of that struct; the reader must build the arena while the parser runs.

**Decision**: a hand-written `serde::de::Visitor` for the top-level object and two
`DeserializeSeed`s that append straight into the arena — the standard serde mechanism for
"deserialize into existing storage" (serde_core-1.0.229 `src/de/mod.rs`:
`DeserializeSeed` `:803`, `Visitor` `:1317` with `visit_str` `:1526` / `visit_borrowed_str`
`:1543` / `visit_none` `:1637` / `visit_some` `:1647` / `visit_unit` `:1658`,
`SeqAccess::next_element_seed` `:1759`, `MapAccess::next_key`/`next_value_seed` `:1897`/`:1860`,
`Deserializer::deserialize_struct` `:1152`, `deserialize_option` `:1094`, `deserialize_str`
`:1052`, `deserialize_seq` `:1124`, `deserialize_map` `:1146`; `IgnoredAny` for unknown fields
`src/de/ignored_any.rs:111`; `Error::duplicate_field` `:296`, `missing_field` `:289`). The file
text stays `read_to_string` + `from_str` (serde_json 1.0.151): `from_reader` is documented as
slower and would cost the 10 % open-time budget for 32 MB of savings that SC-004 already
allows.

Behaviour preserved exactly: unknown top-level fields ignored (today's derive has no
`deny_unknown_fields`); a duplicated `external`/`chunks` field is an error (as the derive's
`duplicate_field`); `format_version` checked after the parse with the same message; a duplicate
external id → `Corrupt` "external id {ext:?} appears twice in ids.json" (detected when the
reverse table is built, after the sequence); a non-numeric chunk key → `Corrupt` "chunk key
{id:?} is not an internal id". Field order in the file is not assumed (`chunks` before
`external` grows the slot vector as needed and the count is reconciled at the end).

**One new refusal**, stated: a chunk key naming a slot beyond the `external` array is today
stored silently and served by `chunk(id)` for an id that has no slot; the streaming reader
refuses it as `Corrupt` "chunk key {id} has no slot in ids.json". No writer of this format can
produce such a file (chunks are only recorded for assigned ids); it is a corrupt file, and the
error class matches the neighbouring refusals.

**Transient budget** (SC-004, same bound as SC-003 + file + 16 MB): the text (32 MB), the
arena and spans growing by doubling (≤ 2× of 3.4 MB each), the chunk slots — pre-sized to the
slot count when `external` has already been read (the order `write` produces), so no doubling
and no `shrink_to_fit` copy of the largest vector — and a `ChunkInfo` (one `String`) per chunk
entry alive for one iteration. Expected peak ≈ 27 + 32 + 8 ≈ 67 MB against a bound of
31 + 32 + 16 = 79 MB.

## D5 — One copy until a change is staged (FR-001)

`committed_ids: Arc<IdMap>`, `pending_ids: Arc<IdMap>` (std). `open`/`create` build one map
and clone the `Arc`; every writer entry point (`stage_one`, `delete`) goes through
`Arc::make_mut(&mut self.pending_ids)` (std `alloc::sync::Arc::make_mut`: clones the inner
value only when the `Arc` is shared) — the first staged change after an open or a commit pays
one copy of the compact map (27 MB for Wikipedia, ~10 ms); `commit` sets
`committed_ids = Arc::clone(&pending_ids)` and the two are one again. Searches read
`committed_ids` as today; the invariant "a search sees the committed map while changes are
staged" is unchanged and now tested directly (`Arc::ptr_eq` before/after a staged add and
after commit, in a unit test with a two-line stub embedder).

Today's `commit` already deep-clones the whole map (`committed_ids = pending_ids.clone()`);
the new path clones a quarter of the bytes and only when a further change follows.

## D6 — The write path stays byte-identical

`IdMap::write` keeps building today's `OnDisk` (`Vec<Option<String>>` +
`BTreeMap<String, ChunkInfo>`) and `serde_json::to_vec` — the writer path is host-side and
unconstrained (spec assumption), and the alternative (a streaming `Serialize` that must emit
the chunk keys in *string* order, `"0","1","10","100",…`, to match the `BTreeMap<String,_>`)
is more code for no user. Oracle: the 008 index's `ids.json` sha256 `20028054c295…3a39994` must
reproduce from read → write (an env-gated test), and a committed golden
`reference/fixtures/010/ids-golden.json`, written by the pre-change code from a scripted
sequence (assign / reuse / chunk replace / remove) over the 005 fixture ids, must reproduce
both from read → write and from replaying the sequence. Byte-identical builds (008 SC) are
thereby preserved.

## D7 — Measuring what the process holds (FR-013)

`peak_alloc` 0.3.0 (MIT; `PeakAlloc` with `current_usage()`, `peak_usage()`,
`reset_peak_usage()`; wraps `std::alloc::System`, counts `layout.size()` on `alloc`,
`alloc_zeroed`, `realloc`, `dealloc` — `src/lib.rs`) as a **dev-dependency**, declared
`#[global_allocator]` in the crate's own unit-test binary (`#[cfg(test)]` in `lib.rs`). It is
the same method as D1's scratch measurement; using a published crate keeps hand-written
`unsafe` out of the workspace (Principle VII forbids it outside dense/rerank, tests included)
and reuses a commodity (Principle I). `cargo deny check` passes with it. `nextest` runs each
test in its own process, so counters do not bleed between tests.

The accounting test (`ids.rs`, unit) has two modes with one body: a synthetic `ids.json` in
the Wikipedia shape (100,000 passages, ~1.8 chunks per parent, `"{page}#{ordinal}"` ids with
byte ranges) written by `IdMap::write` into a temp dir; or, when `XTRIEVER_IDS_JSON` names a
file, that file. It prints the breakdown (id bytes, parent bytes, held after read, peak during
read, per passage, and `HashTable::allocation_size`) and asserts the SC-003/SC-004 bounds.
Run: `XTRIEVER_IDS_JSON=target/xt-wiki/index/ids.json cargo test -p xtriever-pipeline
--release --lib id_map_cost -- --nocapture`.

## D8 — The device record

The 007 harness's `DeviceMeasurementTests` with `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia`, run
alone in its process on the reference device after `scripts/build-ios-package.sh --with-wiki`
(rebuilds the XCFramework from this branch; the 1.26 GB staging is unchanged) — the 008 method
verbatim, so the record compares line by line with 008 run 2. Extracted with
`scripts/extract-device-run.py` into `specs/010-id-map-memory/runs/`. The app's measurement
(`DemoMeasurementTests`) is re-run if the phone is available for a second session; the spec
allows "within 10 MB of the harness" and the report says which was taken.

## D9 — What is *not* changed, and why

- `ChunkInfo`, `DocId`, every `xtriever-core` type: untouched (FR-010). `chunk(DocId)` now
  returns an owned `ChunkInfo` (`pub(crate)`; one caller, `search.rs`, which already `.cloned()`).
- `ids.json`: byte-identical (D6). `FORMAT_VERSION` stays 2. No ADR needed under Principle V
  (no trait, format or error-semantics change; the one new refusal is on a file no writer can
  produce and is recorded here).
- `xtriever-lexical`, `-dense`, `-rerank`, `-ffi`, the Swift package, the app: untouched.
- The writer path's transient at commit (`OnDisk` materialised for `write`): kept; host-only.
- Open-time `read_to_string`: kept (D4).
