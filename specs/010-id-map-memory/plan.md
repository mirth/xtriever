# Implementation Plan: Shrink the Id Map's Resident Memory

**Branch**: `010-id-map-memory` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/010-id-map-memory/spec.md`

## Summary

`HybridIndex::open` holds the id map twice — 174.7 MB of the 178.2 MB the pipeline allocates
for the 008 index (measured, research D1) — in a shape that spends 207 bytes per passage to
hold 8 bytes of id. The feature replaces `IdMap`'s three std collections with one arena
(id bytes + spans), a `hashbrown::HashTable<u32>` keyed on the arena, fixed-width chunk
slots and an interned parent table (≈ 63 B/passage, ≈ 27 MB), reads `ids.json` into that
shape while the parser runs (a serde `Visitor`, so the transient is bounded), and shares one
`Arc<IdMap>` between the committed and pending views until a change is staged. `ids.json`
stays byte-identical; every result stays bit-identical; the change is confined to
`xtriever-pipeline`. Expected: ≥ 150 MB less after open on the phone (owner decision Q1 = B).

## Technical Context

**Language/Version**: Rust, edition 2024, toolchain pinned by `rust-toolchain.toml` (1.91.1)

**Primary Dependencies**: `hashbrown 0.17.1` (new, `default-features = false`, no transitive
deps — research D3); `serde 1.0.229` / `serde_json 1.0.151` (already pinned; the streaming
reader uses their `de` traits — D4); `std::sync::Arc` (D5). Dev: `peak_alloc 0.3.0` (new; the
counting allocator for the accounting tests — D7).

**Storage**: `<index>/ids.json`, format version 2 — **unchanged, byte-identical** (D6,
contract §1).

**Testing**: `cargo nextest` (unit tests in `ids.rs` and `index.rs` with the counting
allocator; an integration test for the golden); the unchanged 005–008 suites; `xtriever wiki
verify` / `wiki expected` on the full index; `xtriever-eval` on SciFact / NFCorpus / FiQA
(local); the 007 harness's `DeviceMeasurementTests` on the reference device.

**Target Platform**: host + `aarch64-apple-ios`, `aarch64-apple-ios-sim`,
`aarch64-linux-android` (wasm32 best-effort); `x86_64-pc-windows-msvc` clippy-checked.

**Project Type**: library (`xtriever-pipeline`, pure, `std`-only) + records.

**Performance Goals**: after-open footprint on the reference device ≥ 150 MB lower than
008 run 2 (509.0 → ≤ 359 MB); id map held ≤ `id_bytes + parent_bytes + 52·slots +
16·parents + 64 KiB`; parse transient ≤ that + file + 16 MB; `read` of the 008 file within
+10 % of today's; open on device within +10 % of 1,009 ms; search medians within ±10 %.

**Constraints**: no format change, no ADR (Principle V untouched); no `unsafe`, no C deps, no
`async`, no `Instant` in library code (III); no change outside `xtriever-pipeline` except
records, fixtures and the two `Cargo.toml`/`Cargo.lock` lines; CI unchanged (no device job,
SciFact smoke only).

**Scale/Scope**: one crate; ~350 lines in `ids.rs` (of which ~120 the streaming reader and
~150 tests), ~25 in `index.rs`, 1 in `search.rs`, ~120 of new tests, one golden fixture pair,
one device record. Under 800 changed lines; two commits (red → green) plus one for records.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.4.0, ratified 2026-09-10, last amended 2026-09-13). Every row below MUST be
marked **PASS**, **FAIL**, or **N/A** with a one-line justification — an empty verdict is a FAIL.

- Any **FAIL** blocks the phase. Do not proceed, and do not weaken the gate to make it pass.
- Any **N/A** MUST state why the principle cannot apply to this feature.
- A violation that is genuinely required is recorded in Complexity Tracking below and REQUIRES an
  accepted ADR in `docs/adr/`; the row stays **FAIL** until that ADR is merged.
- These are plan-time gates. They do not replace the blocking CI gates in the constitution's
  "Quality Gates (CI)" table.

### Core Principles

| # | Principle | Gate | Verdict | Justification |
|---|---|---|---|---|
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | The hash table is `hashbrown::HashTable` (the one std uses), not hand-written open addressing; the counting allocator is `peak_alloc`, not a hand-written `GlobalAlloc`; the streaming parse uses serde's own `Visitor`/`DeserializeSeed` mechanism. The arena (a `Vec<u8>` and a `Vec<(u32,u32)>`) is not a component. Research D2/D3/D7. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | Red commit first: the per-passage bound, the transient bound, the `Arc` sharing, the golden byte-identity (D7, D6). The reference implementation for the file bytes is the pre-change code: `reference/fixtures/010/ids-golden.json` + its script, generated at the red commit, tolerance zero; the 008 `ids.json` sha256 is a second, full-scale oracle. Results: the 005–008 goldens (bit-exact), `wiki expected` (20 × 3 depths), BEIR on all three datasets re-run locally with deltas in the PR (identical expected). Existing property tests (`fusion_prop`, `rerank_prop`) unchanged; the ids round-trip test is strengthened to compare through accessors for every id (not weakened). |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | **PASS** | `hashbrown` without default features is `no_std`, pure Rust, dependency-free (D3); `peak_alloc` is dev-only; no `Instant` in library code (the timing in the accounting test is test code). The four cross-target checks are in the gate. RSS: the feature *lowers* the reference configuration's footprint; the record is taken on the full pipeline on the reference device (D8). |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | The claims are memory and open time, measured by a counting allocator with a fixed synthetic input (seeded shape, 100k passages) and the pinned 008 file (sha256 stated), the "before" taken by the same test at the red commit (D1, D7); the device number by the 007 harness's ledger on the pinned index (D8). No `criterion` bench: the workspace has none and the claims are not latency budgets (as 005/008 recorded); the open/read times are reported beside the memory figures with their method. `explain()` untouched. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | Core untouched; the format is byte-identical (oracles in D6); error classes and messages preserved (contract §2) with one added `Corrupt` refusal on a file no writer can produce (D4, stated). `HybridIndex`'s public surface unchanged; `IdMap` is `pub(crate)`. `DocId` stays the dense `u32`; backends see nothing new. Dependency direction unchanged (`hashbrown` is a leaf under `pipeline`). |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | No stage logic touched; ids and provenance come out identical (oracles); the hash table's iteration order is never observed (lookups only); the format-version and fingerprint refusals are unchanged and tested. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | Both crates added with `cargo add` (`hashbrown 0.17.1`, `peak_alloc 0.3.0`); `cargo deny check` green with them. No `unsafe` anywhere in the change — the counting allocator lives in a published crate (that is why `peak_alloc` and not a ten-line `GlobalAlloc`). Slot arithmetic uses checked conversions returning `Corrupt`; no `unwrap` in library code. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | hashbrown 0.17.1 `src/table.rs`: `HashTable::with_capacity` `:90`, `find` `:228`, `find_entry` `:304`, `entry` `:412`, `insert_unique` `:701`, `allocation_size` `:1573`, `OccupiedEntry::remove` `:2081`, `Clone` `:1627`. serde_core 1.0.229 `src/de/mod.rs`: `DeserializeSeed` `:803`, `Visitor` `:1317` (`visit_str` `:1526`, `visit_borrowed_str` `:1543`, `visit_none` `:1637`, `visit_some` `:1647`, `visit_unit` `:1658`), `SeqAccess::next_element_seed` `:1759`, `MapAccess::next_key` `:1897` / `next_value_seed` `:1860`, `Deserializer::deserialize_struct` `:1152` / `deserialize_option` `:1094` / `deserialize_str` `:1052` / `deserialize_seq` `:1124` / `deserialize_map` `:1146`, `Error::duplicate_field` `:296`; `IgnoredAny` `src/de/ignored_any.rs:111`. peak_alloc 0.3.0 `src/lib.rs`: `PeakAlloc::current_usage`, `peak_usage`, `reset_peak_usage`, `GlobalAlloc` impl incl. `realloc` `:168`. std: `Arc::make_mut`, `Arc::ptr_eq`, `BuildHasher::hash_one`, `BuildHasherDefault<DefaultHasher>`. (Research D3, D4, D5, D7.) |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither is touched (FR-010); `git diff --stat main -- crates/xtriever-core deny.toml` empty in the gate. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Branch `010-id-map-memory`; ≈ 650 hand-written lines (Scale/Scope) plus the golden fixture and the device record; commits: red suite + golden → implementation → records and report. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | Phase 2 of tasks.md: the bound test, the transient test, the sharing test, the golden and its script (generated by today's code), the accessor-based round trip — all committed red (the golden test green by construction, stated) before any change to `ids.rs`/`index.rs`. |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Steps 2–6: host accounting before/after, `wiki verify` + `wiki expected` diff, BEIR × 3 (local), the device record, fmt/clippy (host + Windows target)/nextest/deny/cross-target checks/no-stubs; the PR description carries the memory table and the eval deltas. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | The bounds are constants in the tests; a device saving under 150 MB, an open-time regression over 10 %, any non-identical result or a build failure on any target is a stop-and-report (quickstart Step 5 says so in the expected line). |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | Plain `Vec`s and one `HashTable`; a `Visitor` is the ordinary serde way to stream, no derive tricks; `Arc<IdMap>` + `make_mut`, not a persistent data structure; the write path is left as it is because nothing needs it faster (D6); 32-byte slots over bit-packing (D2). |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-15, agent (all rows PASS on the spec alone; no ADR required).

**Post-design gate (post-Phase 1)**: PASS — 2026-09-15, agent (design in research D1–D9, data-model, contract; the one new refusal recorded in D4 and the contract; no row changed).

## Project Structure

### Documentation (this feature)

```text
specs/010-id-map-memory/
├── plan.md              # This file
├── research.md          # D1 baseline measurement, D2 shape, D3 hashbrown, D4 streaming, D5 Arc, D6 bytes, D7 accounting, D8 device, D9 unchanged
├── data-model.md        # IdMap fields and invariants, HybridIndex pair state machine, ids.json (unchanged)
├── quickstart.md        # red → green → results unchanged → BEIR → device → gate
├── contracts/id-map.md  # file format restated, refusals table, memory bounds, writer semantics
├── runs/                # the device record (Step 5)
├── report.md            # written at the end
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/xtriever-pipeline/
├── Cargo.toml                 # + hashbrown (no default features); dev: + peak_alloc
├── src/
│   ├── lib.rs                 # #[cfg(test)] #[global_allocator] static ALLOC: peak_alloc::PeakAlloc (unit-test binary only)
│   ├── ids.rs                 # IdMap: arena + spans + HashTable + chunk slots + parent table; streaming read; write unchanged; unit tests (cost, transient, round trip via accessors)
│   ├── index.rs               # committed_ids / pending_ids: Arc<IdMap>; make_mut in stage_one/delete; commit shares; unit test: ptr_eq shared → split → shared
│   └── search.rs              # chunk(id) now owned: drop the .cloned()
└── tests/
    └── ids_golden.rs          # reference/fixtures/010/ids-golden.json byte identity: read→write and script→write; XTRIEVER_IDS_JSON full-file sha256 (ignored unless set)

reference/fixtures/010/
├── ids-golden.json            # written by the pre-change code (commit 1d45490's ids.rs) from the script below
└── ids-golden-script.json     # the assign / assign-with-chunk / replace / remove sequence, in order

specs/010-id-map-memory/runs/  # iPhone17,5-<stamp>-mmap-threadsdefault.json
```

**Structure Decision**: one crate, `xtriever-pipeline`, all of it below `ffi`/`cli` and above
`core`; `hashbrown` enters as a leaf dependency of `pipeline` only. The FFI, the Swift package,
the app and the CLI compile unchanged and benefit at their next build. `git diff --stat main
-- crates/xtriever-core deny.toml crates/xtriever-lexical crates/xtriever-dense
crates/xtriever-rerank crates/xtriever-ffi swift/ apps/ .github/` is empty at the end.

## Complexity Tracking

No violations. (The one behavioural addition — refusing a chunk key with no slot — is a new
`Corrupt` case on a file the format's writer cannot produce; recorded in research D4 and the
contract, not a Principle V change.)
