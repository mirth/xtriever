# Feature 010 — Shrink the Id Map's Resident Memory

**Written for**: the reviewer of the `010-id-map-memory` branch.

008 F-002 found that the corpus-sized part of the phone's footprint was the id map, held
twice at ~200 B per passage. `IdMap` is now one arena (every id's bytes once, a span per
slot), a `hashbrown::HashTable<u32>` over the arena, one 32-byte chunk slot per id with the
parent interned, read straight from `ids.json` through a serde visitor; `committed_ids` and
`pending_ids` share one `Arc<IdMap>` until a change is staged. **On the iPhone 16e the full
Wikipedia index opens at 332.6 MB instead of 509.0 (−176.5 MB) and peaks at 358.8 MB vs the
600 MB ceiling** (was 535.8). Results are identical bit for bit; `ids.json` and the format
version are untouched; the change is confined to `xtriever-pipeline`. Report:
[report.md](./report.md); records: [runs/](./runs/).

## Commits

| commit | content | lines |
|---|---|---|
| C1 (red) | the accounting, transient, refusal and `Arc`-sharing tests; the strengthened round trip; `tests/ids_golden.rs`; `reference/fixtures/010/` (golden written by the pre-change code + script + README); `hashbrown`, `peak_alloc` via `cargo add`; the counting allocator in the unit-test binary | ~660 (≈ 380 test Rust, the rest fixture JSON) |
| C2 (green) | `ids.rs` rewritten (~390 library lines incl. the streaming reader), `index.rs` (`Arc`, 15 lines), `search.rs` (1 line), crate docs; 008/009 report cross-references | ~480 |
| C3 | the three device records, `report.md`, this description | docs + JSON |

Rule 3: the branch is ~1,080 lines of Rust in one crate, ~650 of them tests; the red commit is
not independently mergeable (its `Arc` test does not compile against the old fields — the
recorded red state, as 007/009), so it is one PR with the red/green boundary visible in the
history rather than two.

## Numbers

**Host** (`ids::tests::id_map_cost_per_passage`, counting allocator, `--release`, the 008 `ids.json`):

| | before | after |
|---|---|---|
| one `IdMap` held after read | 88,529,028 B (206.9 B/passage) | **28,181,860 B (65.9 B/passage)** |
| copies in `HybridIndex` | 2 (174.7 MB) | 1 |
| peak during read | 138.6 MB | 61.2 MB |
| read | 287 ms | 148 ms |
| whole `open_with(mapped, read_only)` heap / time | 178.2 MB / 361 ms | 31.6 MB / 171 ms |

**Device** (harness, default threads, vs 008 run 2): after open **509.0 → 332.6 MB**, peak
**535.8 → 358.8 MB PASS**, open 1,009 → 899 ms, depth 0 / 5 / 20 medians 341 / 855 / 2,284 ms
(008: 339 / 864 / 2,285), parity PASS 20 / 20. One thread: 508.9 → 332.6 MB, medians +9 / +2 /
+2 % (report F-005). **Demo app** (unchanged, rebuilt): after open + warm-up 531.9 → 315.9 MB,
peak 533.0 → 355.2 MB PASS, fused / re-ranked medians 342 / 2,295 ms (009: 339 / 2,288).

## Eval deltas (Rule 5; local, `hybrid-rerank-v1`, cached vectors)

| dataset | nDCG@10 | Recall@100 | vs 006 baseline |
|---|---|---|---|
| SciFact (300) | 0.70386 | 0.94167 | identical, per query |
| NFCorpus (323) | 0.36029 | 0.32072 | identical, per query |
| FiQA (648) | 0.37421 | 0.70711 | identical, per query |

Also: `wiki expected` on the 008 index byte-identical (20 queries × 3 depths); `wiki verify`
PASS over 427,947 passages; the 008 `ids.json` reproduces from read → write (sha256
`20028054…`); the 010 golden reproduces from replay and reopen.

## Behaviour to review

- One representable value class changed on the *reader*: two files no writer can produce are
  now refused as `Corrupt` — a chunk key with no slot in `external` (was stored and served
  silently) and an empty string in `external` (was a live id; in the arena an empty span is
  the deleted encoding). Contract §2, report F-001.
- `IdMap::chunk` returns an owned `ChunkInfo` (crate-private; the one caller already cloned).
- Deleted ids keep their bytes in the arena until the next reopen; interned parents are never
  dropped while open. Both are bounded by what was once live.
- SC-001's "app within 10 MB of the harness" clause is **not met as written** — the app saved
  216 MB, the harness 176 MB; the peaks agree within 4 MB (report F-004).

## Gate

fmt · clippy `-D warnings` (host + Windows target) · nextest **263 / 263** · deny · iOS / iOS-sim
/ Android checks · no-stubs · model-backed 13 / 13 · simulator 18 / 18 · guarded-paths diff
against `main` empty. CI unchanged (SciFact smoke only; no device job).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
