# Tasks: Shrink the Id Map's Resident Memory

**Input**: Design documents from `/specs/010-id-map-memory/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/id-map.md](./contracts/id-map.md),
[quickstart.md](./quickstart.md); the 008 index at `target/xt-wiki/index/` (`ids.json`
sha256 `20028054c295e4afa396654b98c0539a1fb28c799740127efc42fd55b3a39994`); the
pre-change `ids.rs` at `main` (`1d45490`) — the reference implementation for the file bytes.

**Tests**: **Mandatory** (Principle II; spec FR-011). Phase 2 lands every acceptance test red
against today's `ids.rs`/`index.rs` and records the "before" numbers by the final method.
Story phases contain implementation only and end with the task that turns their tests green.

**Organization**: Setup (deps, allocator) → Red suite + golden (the "before" line; US2's
in-crate proofs live here) → US1 (the compact map, the streamed read, the shared `Arc` — one
change that must satisfy US1 and US2 at once) → US2 (the proofs at scale: full index, BEIR)
→ US3 (the evidence: host accounting, device record, report) → Polish (gate, PR).
Commits: **C1** = Phases 1–2 (red), **C2** = Phases 3–4 (green), **C3** = Phases 5–6.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US3 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers, data-model sections
  and the contract — read them before implementing. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

Crate `crates/xtriever-pipeline/` (`src/ids.rs`, `src/index.rs`, `src/search.rs`,
`src/lib.rs`, `tests/`); fixtures `reference/fixtures/010/`; records
`specs/010-id-map-memory/runs/`; device id `XXXXXX`, team
`XXXXX`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: The two dependencies and the counting allocator every test in Phase 2 needs.

- [ ] T001 Confirm `hashbrown = { version = "0.17.1", default-features = false }` under `[dependencies]` and `peak_alloc = "0.3.0"` under `[dev-dependencies]` in `crates/xtriever-pipeline/Cargo.toml` (both already added with `cargo add` during planning; `Cargo.lock` updated); run `cargo deny check` and `cargo tree -p xtriever-pipeline -e normal | grep -A1 hashbrown` — hashbrown must be a leaf (research D3)
- [ ] T002 Declare the counting allocator for the unit-test binary in `crates/xtriever-pipeline/src/lib.rs`: `#[cfg(test)] #[global_allocator] static TEST_ALLOC: peak_alloc::PeakAlloc = peak_alloc::PeakAlloc;` with a doc comment naming research D7 (unit tests only; integration tests link the lib without `cfg(test)`), plus a `#[cfg(test)] pub(crate) fn test_alloc() -> &'static peak_alloc::PeakAlloc` accessor; `cargo nextest run -p xtriever-pipeline --lib` still green

---

## Phase 2: Foundational — the red suite and the golden (Rule 4)

**Purpose**: Every acceptance test, failing against today's code, plus the byte-identity
golden written by today's code. **⛔ C1 is committed at the end of this phase with the tests
red — do not touch `ids.rs`/`index.rs` logic before then.**

- [ ] T003 [P] Write the golden script `reference/fixtures/010/ids-golden-script.json`: an ordered list of operations over the 005 fixture's external ids plus synthetic ones — `{"op":"assign","external":"…","chunk":null|{"parent":"…","ordinal":N,"byte_range":[a,b]|null}}` and `{"op":"remove","external":"…"}` — covering: 24 assigns (8 plain, 16 chunked over 5 parents, two with `byte_range: null`), one re-assign of a known id with a different chunk (replace), one re-assign with `chunk: null` (chunk removal), 3 removes (one of a chunked id, one of a plain id, one unknown → no-op), one assign after the removes (a fresh slot, never a reused one), ids including a `#`, a `/`, a space, an accented letter and a 120-character id; a JSON escape case (`"` in an id)
- [ ] T004 Generate `reference/fixtures/010/ids-golden.json` by replaying T003's script through **today's** `IdMap` (a throwaway `#[test]` in `crates/xtriever-pipeline/src/ids.rs` run once with `cargo test -p xtriever-pipeline --lib -- --ignored golden_gen`, then deleted before commit; or a scratch binary in the scratchpad against the `main` checkout) and `IdMap::write`; record in the file's sibling `reference/fixtures/010/README.md` the commit (`1d45490`), the command and the sha256 of the golden — this is the reference implementation's output (Principle II)
- [ ] T005 [P] Write `crates/xtriever-pipeline/tests/ids_golden.rs` (integration; public API only): (a) `golden_reproduces_from_replay` — build a `HybridIndex` in a temp dir with `support::fixture_config` and `TableEmbedder` extended with the script's texts, replay the script through `add_embedded`/`delete` with `commit` after every 7 operations and at the end, then `assert_eq!(fs::read("ids.json"), fs::read(golden))` byte for byte; (b) `golden_reproduces_from_reopen` — copy the golden into a fresh index dir created by (a)'s procedure, reopen, `add_embedded` nothing, `commit` (no-op) — the file is unchanged; then `delete` one id, `commit`, reopen, compare every `external_id`/`contains` against the expected set. The full-file write-back oracle needs `IdMap` itself and lives in the unit tests (T007). (a) and (b) are **green today** by construction; say so in the file's doc comment
- [ ] T006 Write the accounting test `id_map_cost_per_passage` in `crates/xtriever-pipeline/src/ids.rs` `mod tests`: a helper `synthetic_wikipedia_shape(passages: usize) -> (IdMap, id_bytes, parent_bytes, parents)` building ids `"{page}#{ordinal}"` with 1–3 ordinals per page (deterministic: page count and ordinals from a fixed LCG seed `0x0010_0010`), each with `ChunkInfo { parent: page.to_string(), ordinal, byte_range: Some((ord*900, ord*900+880)) }`; write it to a temp dir; then, with `crate::test_alloc()`: `reset_peak_usage()`, record `current_usage()`, time and call `IdMap::read`, record `current_usage()` (held) and `peak_usage()` (peak), drop the map; **bound** `held ≤ id_bytes + parent_bytes + 52·slots + 16·parents + 65_536` (contract §3, data-model "Derived bounds"); **transient** `peak ≤ bound + file_len + 16·1024·1024`; print `passages, id_bytes, parent_bytes, held, peak, held/passages, read_ms, reverse_allocation`. Second mode: when `XTRIEVER_IDS_JSON` is set, read that file instead (its `parents`/`id_bytes` computed from the map after reading via `external`/`chunk` over every id) and print the same line — the report's "before"/"after" lines. Fails today on both bounds (~207 B/passage, peak ≈ 118 MB for the full file)
- [ ] T007 Write `full_file_write_back_is_byte_identical` in `crates/xtriever-pipeline/src/ids.rs` `mod tests` (unit, `XTRIEVER_IDS_JSON`-gated, no-op without it): `IdMap::read` the file, `IdMap::write` to a temp dir, `assert_eq!` the bytes, and print the sha256 (via `sha2`, already a dev-dependency) — expected `20028054…3a39994` for the 008 file; green today
- [ ] T008 Strengthen the existing `assign_reuse_remove_never_reuse_and_round_trip` in `crates/xtriever-pipeline/src/ids.rs` so it compares the written-and-read map to the original through the accessors for every id in `0..len()` (`external`, `chunk`) plus `internal` for every live external and `live()`/`len()` — not private fields (`back.reverse == m.reverse`). Add to it: an id that is a prefix of another (`"12"`, `"123"`), a non-ASCII id, an id containing `"`; assert `chunk(id)` round-trips `byte_range: None` and `Some`. Green today; will stay the same assertions after the change (Rule 6: strengthened, not weakened)
- [ ] T009 [P] Write the refusal tests in `crates/xtriever-pipeline/src/ids.rs` `mod tests` per contract §2, each writing a hand-made `ids.json` into a temp dir and matching the `Error::Corrupt` message substring: `format_version: 3` → "is format version 3, this build reads 2"; a duplicate live external → "appears twice in ids.json"; chunk key `"x"` → "is not an internal id"; chunk key `"7"` with a 3-entry `external` → "has no slot in ids.json" (**red today** — today's code stores it silently); members in the order `chunks`, `external`, `format_version` → reads identically to the canonical order (green today); an unknown member `"note":1` → ignored (green today); `external` repeated → `Corrupt` containing "duplicate field" (green today)
- [ ] T010 Run the red checkpoint per quickstart Step 1 and record the "before" line, **before T011 is written** (its `Arc::ptr_eq` cannot compile against today's fields and would block the binary): `cargo nextest run -p xtriever-pipeline --lib` (T006 red on both bounds, T009's no-slot case red, the rest green); `XTRIEVER_IDS_JSON=target/xt-wiki/index/ids.json cargo test -p xtriever-pipeline --release --lib id_map_cost -- --nocapture` → paste the printed line (held ≈ 88.5 MB, peak ≈ 117.9 MB, 207 B/passage, read time) into `specs/010-id-map-memory/report.md` as "Before (red commit)"; also `… --lib full_file_write_back -- --nocapture` → sha256 `20028054…` (green, the oracle's own check)
- [ ] T011 Write `committed_and_pending_share_until_a_change_is_staged` in a new `#[cfg(test)] mod tests` in `crates/xtriever-pipeline/src/index.rs`: a 6-line stub embedder (dim 2, fixed vector, fingerprint `"stub"`), `HybridIndex::create` in a temp dir, `add_embedded` two docs, `commit`, reopen; assert `Arc::ptr_eq(&index.committed_ids, &index.pending_ids)` after open; `add_embedded` a third → `!ptr_eq` and `contains(third) == false` and `committed_ids.len() == 2`; `commit` → `ptr_eq` again and `contains(third)`; `delete` → split again; `commit` → shared. Fails to compile today (`IdMap` is not `Arc`) — the recorded red state, as 009's did **⛔ Commit C1 here** (after T010's measurement): fixtures, README, the test additions, `Cargo.toml`/`Cargo.lock`, `lib.rs` allocator — the lib test binary no longer compiles (this test), which is the red state; T006/T007/T009's own red/green states were recorded by T010

---

## Phase 3: User Story 1 — The same index opens with far less resident memory (Priority: P1) 🎯 MVP

**Goal**: The compact `IdMap` (data-model), the streaming reader (research D4), the shared
`Arc` (D5); `write` unchanged (D6).

**Independent Test**: T006 passes on the synthetic shape and prints ≈ 63 B/passage for the
full file; T008 passes; T005/T007 still byte-identical.

- [ ] T012 [US1] Replace `IdMap`'s fields in `crates/xtriever-pipeline/src/ids.rs` with the data-model layout: `id_bytes: Vec<u8>`, `spans: Vec<(u32, u32)>`, `reverse: hashbrown::HashTable<u32>`, `chunks: Vec<ChunkSlot>`, `parent_bytes: Vec<u8>`, `parent_offsets: Vec<u32>` (starts with `[0]`), `parents: hashbrown::HashTable<u32>`, `hasher: BuildHasherDefault<DefaultHasher>`; `#[derive(Debug, Clone)]`, `impl Default` by hand (`parent_offsets: vec![0]`); `pub(crate) const NONE: u32 = u32::MAX`; `struct ChunkSlot { byte_range: Option<(u64, u64)>, parent: u32, ordinal: u32 }` with a `const NO_CHUNK` value; private helpers `id_at(&self, slot: usize) -> Option<&str>` (empty span → `None`; the arena only ever receives `&str` bytes, so `std::str::from_utf8(..).ok()` is exact and no `unsafe` is needed), `hash_bytes(&self, &[u8]) -> u64`, `find_slot(&self, external: &str) -> Option<u32>`, `parent_at(&self, idx: u32) -> &str`, `intern_parent(&mut self, &str) -> Result<u32>` (`NONE` reached → `corrupt("parent table exhausted (u32)")`), `push_id(&mut self, &str) -> Result<u32>` (`id_bytes.len() + s.len() > u32::MAX as usize` → `corrupt("id bytes exceed u32")`)
- [ ] T013 [US1] Re-implement the operations in `crates/xtriever-pipeline/src/ids.rs` on the new fields with the contract §4 semantics, signatures unchanged except `chunk(&self, DocId) -> Option<ChunkInfo>` (owned): `assign` (empty → `Schema`; `find_slot` hit → reuse; else `push_id`, `spans.push`, `reverse.insert_unique`, `u32::try_from(spans.len())` overflow → the existing "internal id space exhausted (u32)"); chunk handling: `Some(c)` → ensure `chunks.len() == spans.len()` (resize with `NO_CHUNK`), intern parent, store; `None` → set `NO_CHUNK` if `chunks` non-empty; `remove` (`find_entry(...).remove()`, span emptied `(start, start)`, chunk cleared); `external`, `internal`, `len`, `live` (= `reverse.len()`); `write` builds today's `OnDisk` from the accessors — **identical bytes** (contract §1; T005/T007 are the proof)
- [ ] T014 [US1] Implement the streaming `read` in `crates/xtriever-pipeline/src/ids.rs` (research D4; serde items cited in plan Rule 1): `read_to_string` then `serde_json::from_str` with a `Deserializer` that calls `deserialize_struct("OnDisk", &["format_version","external","chunks"], MapVisitor { map: &mut IdMap, seen: [bool; 3], format_version: Option<u32> })`; keys read as `Field` (`#[derive(Deserialize)] #[serde(field_identifier, rename_all = "snake_case")] enum Field { FormatVersion, External, Chunks, #[serde(other)] Other }`), unknown → `next_value::<IgnoredAny>()`, repeated → `Error::duplicate_field`; `external` → `next_value_seed(ExternalSeq(&mut map))` whose `visit_seq` loops `next_element_seed(PushId(&mut map))` (`visit_none`/`visit_unit` → push an empty span; `visit_some` → `deserialize_str(IdStr)` whose `visit_str` pushes bytes — no `String`), then builds `reverse` with `with_capacity(live)` detecting duplicates → `corrupt("external id {ext:?} appears twice in ids.json")`; `chunks` → `next_value_seed(ChunkMap(&mut map))` whose `visit_map` reads `next_key::<String>` (parse `u32` else `corrupt("chunk key {key:?} is not an internal id")`), `next_value::<ChunkInfo>()`, pre-sizes `chunks` to `spans.len()` when `external` was already read, else grows; a key ≥ `spans.len()` after both are read → `corrupt("chunk key {id} has no slot in ids.json")`; after the object: `format_version` missing → `missing_field`, ≠ `FORMAT_VERSION` → today's message; `shrink_to_fit` on `id_bytes`, `spans`, `chunks`, `parent_bytes`, `parent_offsets`. Map every serde error to `corrupt(format!("{} is not a valid id map: {e}", path.display()))` as today
- [ ] T015 [US1] Share the map in `crates/xtriever-pipeline/src/index.rs` (research D5): fields `committed_ids: Arc<IdMap>`, `pending_ids: Arc<IdMap>`; `create`/`open_with` build one `Arc` and clone it; `stage_one` and `delete` use `Arc::make_mut(&mut self.pending_ids)`; `commit` writes `self.pending_ids` (a `&IdMap` via deref) and sets `self.committed_ids = Arc::clone(&self.pending_ids)`; `external_of`/`external_id`/`contains` read through the `Arc` unchanged; update the two field doc comments to say "shared with `pending_ids` until a change is staged"
- [ ] T016 [US1] Adjust the one caller in `crates/xtriever-pipeline/src/search.rs` (line ~212): `chunk: self.committed_ids.chunk(c.id)` (owned now; drop `.cloned()`); `cargo build -p xtriever-pipeline` and `cargo clippy -p xtriever-pipeline --all-targets -- -D warnings` clean
- [ ] T017 [US1] Turn the suite green: `cargo nextest run -p xtriever-pipeline` (all, including T005–T011 and the unchanged 005–008 suites); then `XTRIEVER_IDS_JSON=target/xt-wiki/index/ids.json cargo test -p xtriever-pipeline --release --lib id_map_cost -- --nocapture` → held ≈ 27 MB, ≈ 63 B/passage, peak ≈ 67 MB, read time within +10 % of T011's; and `… --lib full_file_write_back -- --nocapture` → sha256 `20028054…`. **⛔** A bound missed, a byte differing, or read time over +10 % is stop-and-report

---

## Phase 4: User Story 2 — Nothing else changes: results, ids, provenance, errors (Priority: P1)

**Goal**: Prove identity beyond the crate's own tests — the full index, the model-backed
suites, the three BEIR baselines.

**Independent Test**: quickstart Steps 3–4 all empty diffs.

- [ ] T018 [US2] Run the model-backed suites serially: `cargo nextest run -p xtriever-pipeline -p xtriever-ffi --release --run-ignored only -j 1` (008 F-007) — all green, no test modified
- [ ] T019 [US2] Re-verify the full index through the new map: `cargo run --release -p xtriever-cli -- wiki verify --index target/xt-wiki/index --embedder-dir reference/models/all-MiniLM-L6-v2 --snapshot-dir reference/datasets/wiki` → `verdict: PASS`, 427,947 passages checked; note the wall time beside 008's in `specs/010-id-map-memory/report.md`
- [ ] T020 [US2] Regenerate the 20-query goldens and diff: `cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki/index --embedder-dir reference/models/all-MiniLM-L6-v2 --reranker-dir reference/models/ms-marco-MiniLM-L-6-v2 --queries reference/fixtures/008/queries.json --out /tmp/expected-010.json` then `diff <(jq 'del(.generated_by)' /tmp/expected-010.json) <(jq 'del(.generated_by)' target/xt-wiki/expected.json)` → empty (20 queries × 3 depths, scores bit-identical). **⛔** Any difference is stop-and-report
- [ ] T021 [US2] BEIR on the three datasets, locally (quickstart Step 4; CI untouched — SciFact smoke only): `hybrid-rerank-v1` on scifact, nfcorpus, fiqa with the cached vectors; `diff <(jq .metrics …) <(jq .metrics specs/006-rerank-stage/baselines/hybrid-rerank-v1.<d>.json)` empty for all three; paste the nDCG@10 / Recall@100 table (identical) into `specs/010-id-map-memory/pr-description.md`. **⛔** Commit C2 here (Phases 3–4)

---

## Phase 5: User Story 3 — The reduction is measured, not asserted (Priority: P2)

**Goal**: The host before/after by one method (already printed by T011/T017), the device
record, the bytes-per-passage figure and the growth headroom, written down.

**Independent Test**: `specs/010-id-map-memory/report.md` shows both host lines, the device
record compared to 008 run 2, and "passages per 100 MB".

- [ ] T022 [US3] Rebuild the package with this branch's engine and stage the Wikipedia index: `scripts/build-ios-package.sh --with-models --with-wiki --app` (staging unchanged: 1,259,528,003 B); confirm the XCFramework's build stamp is from this branch (`git rev-parse --short HEAD` in the script's output)
- [ ] T023 [US3] Device measurement (the owner plugs and unlocks the iPhone 16e; quickstart Step 5): `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild test -project swift/XtrieverHarnessApp/XtrieverHarnessApp.xcodeproj -scheme XtrieverHarnessApp -configuration Release -destination 'platform=iOS,id=XXXXX' -skipMacroValidation -allowProvisioningUpdates ARCHS=arm64 DEVELOPMENT_TEAM=XXXXX -only-testing:XtrieverHarnessAppTests/DeviceMeasurementTests 2>&1 | tee /tmp/wiki-device-010.log`, alone in its process; "Lost pending connection" → terminate the stale process and retry (009 F-006); `scripts/extract-device-run.py /tmp/wiki-device-010.log specs/010-id-map-memory/runs/`. Expected `afterOpen` ≤ 359 MB, peak PASS, open ≤ 1,110 ms, depth medians within ±10 % of 339 / 864 / 2,285, parity PASS. **⛔** Saving under 150 MB, open over +10 %, or any parity failure is stop-and-report — the record is committed either way
- [ ] T024 [US3] Optionally (if the phone is still available) the demo app's measurement per `specs/009-ios-wiki-demo/quickstart.md` (alone in its process; `scripts/build-ios-package.sh --with-models --with-wiki --demo` first) → `specs/010-id-map-memory/runs/`; the report states whether the "within 10 MB of the harness" clause of SC-001 was measured or not
- [ ] T025 [US3] Write `specs/010-id-map-memory/report.md`: verdict; the host table (before: T011's line; after: T017's line; whole-open heap 178.2 MB → measured after, same scratch method noted); the device table vs 008 run 2 (baseline / after open / peak / verdict / open / depth 0-5-20 medians / parity); bytes per passage and "passages per 100 MB" (= 1e8 / B-per-passage) with the id-length caveat; SC-001–SC-006 each with its number; findings F-001… (at least: the one new refusal; anything the measurement showed); "Deliberately not done" (format change / option C, streaming `write`, `from_reader`, parent derivation — research D2/D6/D9)

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T026 [P] Update the crate docs in `crates/xtriever-pipeline/src/lib.rs` (the "id map" bullet: one shared map, compact, streamed read; the file unchanged) and the module doc of `crates/xtriever-pipeline/src/ids.rs` (the layout in one paragraph, the bounds, research D2/D4 references)
- [ ] T027 [P] Resolve 008 F-002 in `specs/008-wiki-corpus/report.md` with one line pointing at this feature's report and the measured saving; update the "Known costs" bullet in `specs/009-ios-wiki-demo/report.md` ("~200 MB is the id map held twice") with the new figure and a pointer
- [ ] T028 Full gate (quickstart Step 6): `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings` on host and `--target x86_64-pc-windows-msvc`; `cargo nextest run --workspace`; `cargo deny check`; `cargo check --workspace --target aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`; wasm32 best-effort; `scripts/check-no-stubs.sh`; `git diff --stat main -- crates/xtriever-core deny.toml crates/xtriever-lexical crates/xtriever-dense crates/xtriever-rerank crates/xtriever-ffi swift/ apps/ .github/` empty; paste the results into the report's Gate section
- [ ] T029 Write `specs/010-id-map-memory/pr-description.md` (written for the reviewer): the commits C1–C3 with line counts, the host memory table, the device table, the BEIR deltas (T021), the one new refusal, what was not done; `Co-Authored-By` / generated-with lines per the repository convention. **⛔** Commit C3 (Phases 5–6); the owner merges

---

## Dependencies & Execution Order

- **Phase 1 → Phase 2 → C1 (red)**: T001–T002 first (the allocator and deps); T003 → T004 → T005 (the golden needs the script; the test needs the golden); T006–T009 independent of each other, then T010 (the measurement), then T011 (the compile-breaking sharing test) last.
- **Phase 3 (US1)** is the implementation: T012 → T013 → T014 (one file, sequential), then T015 → T016 → T017.
- **Phase 4 (US2)** needs a green Phase 3; T018–T021 are independent runs (sequential in practice — each is CPU-bound).
- **Phase 5 (US3)** needs C2 and the owner's phone for T023; T025 needs T023.
- **Phase 6**: T026/T027 any time after Phase 3; T028 last before T029.

### User story completion order

US1 (the change) → US2 (the proofs on the full index and BEIR) → US3 (the records). US2's
in-crate proofs are in the red suite (Phase 2), so US1 cannot go green without them.

### Parallel opportunities

- Phase 2: T003 ‖ T005 (script and test file) — then T004 between them; T006 → T007 → T008 → T009 are one file (`ids.rs` tests), sequential; T011 (`index.rs` tests) must come after T010's measurement.
- Phase 6: T026 ‖ T027.

## Implementation Strategy

**MVP** = Phases 1–3 (C1 + the implementation): the compact, shared, streamed map with the
crate's own oracles green and the full-file numbers printed. Phase 4 is the safety net at
scale; Phase 5 is the deliverable's evidence on the device; Phase 6 is the paperwork.

**Rule 6 stop-points**: T010/T011 (the red commit must be red), T017 (bounds, bytes, read time),
T020 (any score difference), T021 (any BEIR delta), T023 (under 150 MB, over +10 % open,
parity), T028 (any gate failure). At each: stop, report, do not adjust a bound.
