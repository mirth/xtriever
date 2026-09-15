# Report: Shrink the Id Map's Resident Memory

**Feature**: `010-id-map-memory` | **Date**: 2026-09-15 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Verdict

The id map that 008 F-002 named as the only corpus-sized term of the phone's footprint is
now one shared copy in a compact shape: on the reference device the full Wikipedia index opens
at **332.6 MB instead of 509.0 MB — 176.5 MB less** (SC-001 asked for 150) — and peaks at
**358.8 MB against the 600 MB ceiling** (008: 535.8); the demo app, rebuilt and otherwise
untouched, opens at 315.9 MB instead of 531.9. Every result is the same bit for bit: the
005–008 goldens, `wiki expected` on all 20 queries × 3 depths, BEIR on SciFact / NFCorpus /
FiQA per query, device parity 20 / 20; `ids.json` reproduces byte for byte from the 008 index.
Open is faster (899 vs 1,009 ms on the phone; 171 vs 361 ms on the host), search medians are
within 1 %. On the host the map went from 206.9 to 65.9 bytes per passage (88.5 → 28.2 MB)
and from two copies to one; the growth rate of the term is now 100 MB per 1.5 M passages.
One clause of SC-001 is not met as written (F-004): the app's saving exceeds the harness's by
40 MB, not within 10 MB — the peaks agree within 4 MB. Confined to `xtriever-pipeline`; no
format, trait, FFI, package or app change.

## What was built

`xtriever-pipeline/src/ids.rs`: `IdMap` holds every external id's bytes once in one arena with
a `(start, end)` span per slot, the reverse lookup as a `hashbrown::HashTable<u32>` hashed and
compared on the arena, chunk provenance as one 32-byte slot per id with the parent interned in
a second arena; `read` streams `ids.json` into that shape through a serde visitor
(`FileSeed` → `ExternalSeq` / `PushId`, `ChunkMap` / `ChunkKey`) so the old
`Vec<Option<String>>` + `BTreeMap<String, ChunkInfo>` never exists in memory; `write` is the
same bytes as before. `index.rs`: `committed_ids` and `pending_ids` are one `Arc<IdMap>` until
`stage_one`/`delete` call `Arc::make_mut`, and `commit` rejoins them. `search.rs`: one line
(`chunk` is owned now). Two crates via `cargo add`: `hashbrown 0.17.1` (no default features,
no transitive dependency) and, dev-only, `peak_alloc 0.3.0` as the unit-test binary's counting
allocator. Nothing under `xtriever-core`, the stage crates, the FFI, the Swift package or the
app changed; `ids.json` and `FORMAT_VERSION` are untouched.

## Host: the id map, by one method (`ids::tests::id_map_cost_per_passage`, `--release`)

| input | slots | **held after read** | per passage | **peak during read** | read |
|---|---|---|---|---|---|
| synthetic Wikipedia shape, before | 100,000 | 20,913,078 B | 209.1 B | 32,317,510 B | 57 ms |
| synthetic Wikipedia shape, **after** | 100,000 | **6,175,389 B** | **61.8 B** | **13,884,749 B** | 28 ms |
| 008 `ids.json`, before | 427,947 | 88,529,028 B | 206.9 B | 138,643,027 B | 287 ms |
| 008 `ids.json`, **after** | 427,947 | **28,181,860 B** | **65.9 B** | **61,198,263 B** | **148 ms** |

Bounds (contract §3): held ≤ 31,013,092 B and peak ≤ 79,827,657 B for the 008 file — met with
9 % and 23 % to spare; the synthetic bound predicts the full file at 61.8 × 427,947 = 26.4 MB,
within 7 % of the 28.2 MB measured (SC-003's 20 %). Whole `HybridIndex::open_with(mapped,
read_only)` of the 008 index (scratch, same allocator): heap after open **178.2 MB → 31.6 MB**,
peak during open 178.2 → 61.2 MB, open time 361 → 171 ms. Per map the reduction is 3.1×; per
open, with the second copy gone, 5.6×. Read time is 48 % lower, not 10 % higher (FR-009).

**Bytes per passage**: 65.9 on this corpus (8.0-byte ids, 1.8 chunks per parent). At that
rate the id map costs **100 MB per 1.5 M passages** (before: per 480 k). Longer ids add their
own bytes; the fixed part is ~52 B per slot and ~16 B per distinct parent.

## Results unchanged (US2)

- The crate's suites: 76 / 76 (`nextest -p xtriever-pipeline`), the 005–008 goldens untouched.
- Model-backed release suites, serially: 13 / 13.
- `xtriever wiki verify` on the 008 index: **PASS**, 427,947 passages re-read through the new
  map (2 min 48 s wall including the snapshot read).
- `xtriever wiki expected` on the 008 index: **byte-identical** to `target/xt-wiki/expected.json`
  (20 queries × 3 depths, scores bit for bit; even `generated_by` matched).
- The 008 `ids.json` (32,037,349 B): read → write reproduces sha256 `20028054c295…3a39994`; the
  010 golden written by the pre-change code reproduces from replay and from reopen.
- BEIR `hybrid-rerank-v1`, local, cached vectors, `RAYON_NUM_THREADS=4`: **identical to the
  006 baselines per query** — SciFact nDCG@10 0.70386 / Recall@100 0.94167 (300 queries),
  NFCorpus 0.36029 / 0.32072 (323), FiQA 0.37421 / 0.70711 (648); `per_query` blocks equal.
- The package's simulator suite: 18 / 18 (Release, arm64; `WikipediaTests` on the full index).

## Device — iPhone 16e (`iPhone17,5`), Release, models mapped, index in place, default threads

Harness (`DeviceMeasurementTests`, corpus `wikipedia`, alone in its process), both thread
settings, each beside its 008 twin; records verbatim under [`runs/`](./runs/).

| | 008 run 2 (default threads) | **010 (default threads)** | 008 run 1 (1 thread) | 010 (1 thread) |
|---|---|---|---|---|
| Baseline | 13.3 MB | 13.2 MB | 13.1 MB | 13.3 MB |
| **After open** | 509.0 MB | **332.6 MB (−176.5)** | 508.9 MB | 332.6 MB (−176.3) |
| **Peak (ledger)** / verdict vs 600 MB | 535.8 MB PASS | **358.8 MB PASS (−177.0)** | 535.8 MB PASS | 356.8 MB PASS |
| Open (embedder / re-ranker within) | 1,009 ms (193 / 159) | **899 ms** (233 / 164) | 1,089 ms | 950 ms |
| Depth 0 median / max | 338.5 / 481 | **341.5** / 1,273 (cold first query) | 489.5 / 2,008 | 534.0 / 1,789 |
| Depth 5 median / max | 863.5 / 1,024 | **854.5** / 1,026 | 1,544.0 / 1,881 | 1,576.5 / 2,215 |
| Depth 20 median / max | 2,284.5 / 2,738 | **2,284.0** / 2,737 | 4,443.5 / 5,326 | 4,546.5 / 5,300 |
| Per pair (derived) | 97.2 ms | 95.0 ms | 194 ms | 190.5 ms |
| Parity (20 queries) | PASS | **PASS** — lexical 20 / 20, fused order 20 / 20, dense Δ 1.79e-7, re-rank Δ 4.77e-6 | PASS | PASS |

Thermal nominal, iOS 26.6.2, Release, `openedInPlace: true`, index 1,076,416,088 B. Default
threads: every median within 1 % of 008 run 2, open 11 % faster. One thread: depth 0 +9 %,
depth 5 +2 %, depth 20 +2 % (F-005). Headroom under the ceiling: **241 MB** (was 64).

**Demo app** (`DemoMeasurementTests`, alone in its process, the 009 app unchanged, rebuilt on
this branch's package; [`runs/demo-…`](./runs/)): baseline 12.1 MB, after open + warm-up
**315.9 MB** (009: 531.9 → **−216.0 MB**), peak **355.2 MB PASS** (009: 533.0), open 732 ms,
warm-up 1,595 ms (009: 1,079 / 477 — the warm-up paid the cold page-in this time), fused
median **342 ms** (009: 339), re-ranked **2,295 ms** (2,288), total 2,643 ms (2,631), 6 threads.

**Success criteria**

| | asked | measured | |
|---|---|---|---|
| SC-001 | harness after-open ≥ 150 MB lower; app the same within 10 MB; PASS vs 600 | −176.5 MB (≤ 359 → 332.6); app −216.0 MB; PASS | met, except the 10 MB clause (F-004) |
| SC-002 | every golden identical | fixture goldens, `wiki expected` byte-identical, BEIR × 3 per query identical, parity 20 / 20 | met |
| SC-003 | synthetic ≥ 100k proves the bound, fails before; bound × 427,947 within 20 % of the measurement | 61.8 B/passage on 100k (209.1 before); 26.4 vs 28.2 MB predicted vs measured, 7 % | met |
| SC-004 | peak during read ≤ after + file + 16 MB | 61.2 MB ≤ 28.2 + 32.0 + 16.8 = 77.0 (bound as tested: 79.8) | met |
| SC-005 | open within +10 %; medians within ±10 % | host read −48 %, open −53 %; device open −11 %; medians +1 % / −1 % / 0 % | met |
| SC-006 | existing suites unmodified and green | workspace 263 / 263 (251 + 12 new), model-backed 13 / 13, package simulator 18 / 18; one existing test strengthened (T008), none weakened; nothing outside `xtriever-pipeline` changed | met |

## Findings

### F-001 — Two refusals a file no writer can produce (contract §2)

The streaming reader refuses a chunk key with no slot in `external` (today's derive stored it
silently and served it for an id without a slot) and an empty string in `external` (today's
derive accepted it as a live id that `assign` could never have written; in the arena an empty
span *is* the deleted encoding, so accepting it silently would have changed `live()`). Both are
`Corrupt`, both are tested, neither can come out of `write`.

### F-002 — The "before" transient is larger by the final method than by the scratch

Research D1 measured the parse peak at 117.9 MB with the text still held; the accounting test
measured 138.6 MB for the same file — the test measures the whole of today's `read`, which
builds the reverse `HashMap` and the chunk `BTreeMap` while the parsed `OnDisk` is still alive.
The report uses the test's figure (the method the after-figure uses too).

### F-003 — Xcode 27.0 blocked every link until its licence was accepted

Installed between 009 and 010; `cc` failed with "You have not agreed to the Xcode license
agreements" on every Rust test binary. `sudo xcodebuild -license accept` by the owner; noted
in the quickstart's Step 0.

### F-004 — SC-001's "within 10 MB" clause compares two different moments

The harness's after-open is taken before any query; the app's is taken after its warm-up
query. In 009 that put the app 23 MB *above* the harness (531.9 vs 509.0); today it is 17 MB
*below* (315.9 vs 332.6), so the two savings differ by 40 MB (216 vs 176.5) and the clause as
written — "drops by the same amount within 10 MB" — is not met. The peaks, which are taken at
the same kind of moment (the ledger's maximum over the run), agree within 3.6 MB (355.2 vs
358.8). Recorded as not met; the clause was mis-specified, and the spec is not edited after
the fact. What a reader should take from it: the app benefits fully, and by more than the
harness's number, not less.

### F-005 — The 1-thread run's depth-0 median is 9 % above 008 run 1

534 vs 489.5 ms; depth 5 and 20 within 2 %, and the default-threads run within 1 % at every
depth. The change removes work from the open path and adds none to the search path (`chunk`
allocates one `String` per hit, as `.cloned()` did before); the 1-thread run was the first
after the phone had idled with the app killed (cold page cache: its depth-0 max is the cold
first query, 1,789 ms), and at one thread the dense stage's page-ins are not overlapped.
Within SC-005's ±10 %; reported, not explained away — a second 1-thread run would settle it
and was not taken (the default-threads configuration is the spec's reference).

### F-006 — A shell that outlives an Xcode update carries a dead `SDKROOT`

After the owner's `xcodebuild -runFirstLaunch`, `xcrun` still failed with "SDK MacOSX26.5.sdk
cannot be located": the session's environment had `SDKROOT` set to the previous Xcode's SDK
path. `unset SDKROOT` fixed every Apple tool call; nothing in the repository was involved.

## Gate (Rule 5)

`cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` (host and
`--target x86_64-pc-windows-msvc`) 0 · `cargo nextest run --workspace` **263 / 263** ·
`cargo deny check` advisories / bans / licenses / sources ok · `cargo check` on
`aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` clean, wasm32 best-effort
fails at `getrandom` as tracked · `scripts/check-no-stubs.sh` PASS · model-backed release suites
`-j 1` 13 / 13 · package simulator suite 18 / 18 · `git diff --stat main -- crates/xtriever-core
deny.toml crates/xtriever-lexical crates/xtriever-dense crates/xtriever-rerank
crates/xtriever-ffi swift/ apps/ .github/` **empty** (Rule 2; FR-010). BEIR × 3 identical
(above). The regenerated Swift fixture `expected.json` differed only in its `generated_by`
commit line and was restored, as in 008.

## Deliberately not done

- **A new on-disk id map** (spec option C): a format bump, an ADR, a rebuild of every shipped
  index — for ~30 MB over this design. Available later if a corpus needs the map to cost
  nothing.
- **A streaming `write`**: the writer path is host-side; matching the `BTreeMap<String, _>`
  key order byte for byte is more code for no user (research D6).
- **`serde_json::from_reader`** to drop the 32 MB text: slower by its own documentation; the
  transient bound already allows the file (D4).
- **Deriving the parent from the id** (`"{page}#{ordinal}"`): true of 008's corpus, not of the
  type (D2).
- **24-byte chunk slots**: would make one `ChunkInfo` value unrepresentable or cap parents at
  2^31 (D2); 3.4 MB kept for fidelity.

## Before (red commit)

The final method (`ids::tests::id_map_cost_per_passage`, counting allocator, `--release`) run
against the pre-change `IdMap` at the red commit:

| input | slots | id bytes | parent bytes | parents | **held after read** | per passage | **peak during read** | read |
|---|---|---|---|---|---|---|---|---|
| synthetic Wikipedia shape | 100,000 | 727,930 | 264,219 | 50,045 | 20,913,078 B | **209.1 B** | 32,317,510 B | 57 ms |
| `target/xt-wiki/index/ids.json` (008) | 427,947 | 3,441,429 | 1,421,907 | 239,436 | **88,529,028 B** | **206.9 B** | **138,643,027 B** | 287 ms |

Bounds the tests demand: held ≤ 7,058,405 B (synthetic) / 31,013,092 B (008 file); peak ≤
31,046,830 B / 79,827,657 B. Both fail on both inputs. Two copies are held by `HybridIndex`
(research D1: 174.7 MB of the 178.2 MB the open allocates). Write-back of the 008 file:
byte-identical, sha256 `20028054c295e4afa396654b98c0539a1fb28c799740127efc42fd55b3a39994`.

Red state of the lib test binary: `index::tests::committed_and_pending_share_until_a_change_is_staged`
does not compile (`Arc::ptr_eq` on plain `IdMap` fields) — the one compile error; with it
absent, `id_map_cost_per_passage` and `refuses_chunk_key_without_slot` fail, the rest pass.
