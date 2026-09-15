# Report: The Wikipedia Corpus and Shipped Index

**Feature**: `008-wiki-corpus` | **Date**: 2026-09-14 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

## Verdict

The whole Simple English Wikipedia — 241,787 articles, 239,436 after the manifest's exclusions
— is a shipped hybrid index of **427,947 passages, 1,076,413,167 bytes**, built by one resumable
command in **11.1 hours** on the reference laptop (93.3 ms per passage at 4 threads, exactly
Feature 004's number), verified passage by passage (**100 % inside the embedder's window**, the
longest at exactly 256 positions; **0 URL mismatches** against the snapshot), and reproducible:
a second full build from the cache took 2.8 minutes and gave a byte-identical corpus identity,
passage store and vector file, and identical goldens for every measurement query at every
depth. The chunker is byte-identical to its Python reference on all 57 golden cases. On the
iPhone 16e the index is opened **in place inside the read-only bundle** (no copy-out — the 007
lock-file caveat is resolved by a read-only open in the lexical backend) and the full pipeline
peaks at **535.8 MB against the 600 MB ceiling: PASS on both runs**, with lexical parity
bit-identical 20 / 20 and dense / re-rank scores within 4.8e-6 of the host's. BEIR: zero delta
on all three datasets, per query. **This corpus's retrieval quality is unmeasured** — owner
decision (spec Q2); nothing here says the index is *good*, only that it is what the build says
it is.

## The build (`specs/008-wiki-corpus/build-record.json`, committed verbatim)

| | |
|---|---|
| Snapshot | `wikimedia/wikipedia` `20231101.simple` parquet, 156,885,218 B, `31bded16…`; derived JSONL 303,703,690 B, `bf69f110…` — both pinned and re-verified before a line is read |
| Articles read / excluded / selected | 241,787 / 2,351 (title suffix 814, "may refer to" 1,159, "may mean" 378) / 239,436 |
| Passages | 427,947 (1.79 per article; median article 81 words) |
| Phases | fetch+verify 0.4 s · read+exclude 48 s · chunk 49 s · **embed 39,932 s (11.09 h)** · ingest 14 s · commit 4 s · merge 3 s · verify 58 s · total 668.5 min |
| Cache | 105 shards of 4,096; 2 hits (from the development build), 103 misses |
| Artefact | dense `index.bin` 660,750,511 B · `passages.bin` 276,466,788 B · lexical (one segment) 106,404,447 B · `ids.json` 32,037,349 B · **total 1,076,413,167 B** |
| Verify | 427,947 passages tokenised, `max_tokens_seen` = 256 = window, over-window 0, URL mismatches 0 → PASS |
| Identity | `ea0fc78c4dce30cebf066033327cc7a442385c605be8c67eadb772329eab2027` |
| Host | macOS / aarch64, 4 threads (004 F-005: bit-identical at any thread count) |

**Reproducibility (SC-001)**: second full build — 105 / 105 cache hits, 2.8 min — `corpus.json`,
`passages.bin`, `dense/index.bin` byte-identical; `expected.json` (20 queries × 3 depths, score
bits) byte-identical. The lexical directory differs in file names and 2.9 KB of layout (tantivy
names segments by a fresh id and its merge order can differ); every hit and score is the same.

**Resumability**: a build interrupted mid-embedding leaves no `<out>` (only `<out>.partial`,
removed at the next start); the re-run hits every finished shard and embeds only the changed one
(dev run: shards 0–1 hit, shard 2 re-embedded). The full build's first two shards came from the
development build's cache.

## Estimated vs measured

| Quantity | Estimate (research D4 / D9 / plan) | Measured |
|---|---|---|
| Passages | ~480k (word bound of 150) | **427,947** (piece bound) |
| Index size | ~1.3 GB (740 MB vectors + 300 MB text + 250 MB lexical) | **1.08 GB** (661 + 276 + 106 + 32 MB ids) |
| Build time | ~12.5 h at ~94 ms/passage | **11.1 h**, 93.3 ms/passage |
| Staged resources | ~1.5 GB with models, budget 2.0 GB | **1,259,528,003 B** (logical bytes) |
| Device footprint | "expected to hinge on the models and the re-rank transient, as in 007" | **509 MB after open, 536 MB peak** — the corpus-sized part is the id map, not the vectors (F-002) |
| Depth-0 latency on device | 0.1–0.5 s warm, seconds cold | **339 ms median (default threads), 490 ms (1 thread); 2.0 s cold first query** |

## The chunker (US2)

`xtriever_analysis::chunk` — pure, `std`-only, a cost closure instead of a tokenizer (D5). Golden
sets: **A** 48 cases (12 bodies × 4 budgets, cost = words), **B** 9 articles (8 real, 813 priced
units, plus one synthetic at budget 6 to reach the fragment branch) — all byte-identical in text,
byte range and cost. Properties (400 cases each): tiling, bound, determinism, text idempotence
modulo the paragraph joiner. `MiniLmEmbedder::token_count` matches every one of the 813 Python
unit costs and is additive over a space for ten pairs spanning CJK, URLs, accents and a
45-letter word (D7).

## Device measurement (US4) — iPhone 16e (`iPhone17,5`), iOS 26.6.2, Release, both models mapped, opened in place

| run | threads | thermal | open | after open | sampled max | ledger peak | **vs 600 MB** | depth 0 med | depth 5 med | depth 20 med | per pair |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 1 | nominal | 1,089 ms | 508.9 MB | 512.3 MB | **535.8 MB** | **PASS** | 490 ms | 1,544 ms | 4,444 ms | 194 ms |
| 2 | default | nominal | 1,009 ms | 509.0 MB | 535.8 MB | **535.8 MB** | **PASS** | 339 ms | 864 ms | 2,285 ms | 97 ms |

Baseline 13.1 MB; embedder 169 ms + re-ranker 174 ms of the open; index 1,076,416,088 B in the
bundle, `openedInPlace: true`, no copy. Depth-0 max 2,008 ms (run 1's first query — the mapped
vectors paged in cold; every later query 476–560 ms). Parity vs the host's `expected.json`, both
runs: lexical bit-identical 20 / 20, fused order at depth 0 identical 20 / 20, dense max |Δ|
1.79e-7, re-rank max |Δ| 4.77e-6 (tolerance 1e-3), matched by id. Per-pair cost is **lower**
than 007's SciFact (194 vs 462 ms at 1 thread; 97 vs 299 ms default): Wikipedia passages fit
256 positions by construction, SciFact abstracts run longer, and the cross-encoder's cost is
∝ pair length (006 F-001). Run records verbatim under [`runs/`](./runs/).

This is the ADR-0010 review trigger: the ceiling was derived for ~100k chunks; this index has
4.3× that and passes with 64 MB to spare. The vectors are not the cost (F-002).

## Shipping (US3)

`scripts/build-ios-package.sh --with-wiki` stages `index/`, `wiki-build.json`, `ATTRIBUTION.txt`,
`queries.json`, `expected.json`; 1,259,528,003 logical bytes of the 2,000,000,000-byte budget.
Simulator: the 007 suite 16 / 16 (minus the removed `WritableCopyTests`) plus `WikipediaTests`
2 / 2 against the full index (18 / 18 in one run) — opened in place, bundle untouched, every hit carries a title line, a derivable URL and
`page#ordinal` provenance. The 007 harness needed one change to run this corpus: the corpus is
selected by `TEST_RUNNER_XTRIEVER_CORPUS`.

## Gate (Rule 5)

fmt · clippy `-D warnings` (workspace; also `--target x86_64-pc-windows-msvc` after F-004) ·
nextest workspace 251 / 251 · model-backed release suites 32 / 32 serially (`-j 1`, F-007) · deny (advisories, bans, licences, sources ok) · iOS /
iOS-sim / Android check · wasm32 best-effort fails at `getrandom` (tracked) · no-stubs
(now also scanning `xtriever-analysis`, `xtriever-cli`) · containment PASS · `git diff main --
crates/xtriever-core deny.toml` empty (Rule 2) · BEIR `hybrid-rerank-v1` re-run on SciFact /
NFCorpus / FiQA: nDCG@10 and Recall@100 identical to the committed baselines, per query.

Files changed under the guarded crates, each named in the plan: `xtriever-lexical`
(`readonly.rs`, `index.rs`, `error.rs`, `lib.rs`), `xtriever-dense` (`embedder.rs`),
`xtriever-pipeline` (`index.rs`, `types.rs`, `lib.rs`), `xtriever-ffi` (`index.rs`,
`ffi/mod.rs`, `lib.rs`) — plus their tests.

## Findings

### F-001 — A words bound cannot keep passages inside the window; the cost has to be word-pieces

Measured on the snapshot before any code: at 150 words per passage, 15 % of passages exceed 256
pieces (worst 1,589); at 120 words, 11.8 %. Simple English's names, dates and table residue
tokenise at 1.5–3 pieces per word with a long tail. Hence D5: the chunker prices with a closure
the build points at the embedder's own tokenizer, and the verify pass proves the assumption on
every passage (`max_tokens_seen` = 256, never 257).

### F-002 — The corpus-sized resident memory is the id map, not the vectors

After open the phone shows 509 MB, 247 MB more than SciFact with the same models. The vectors
are mapped and clean file-backed pages are not charged (007 F-002), so the difference is heap:
`IdMap` holds, per passage, an external id `String`, a `ChunkInfo` (with its own `parent`
`String`) and a reverse-map entry — roughly 100 MB for 428k passages — and `HybridIndex::open`
keeps **two** copies (`committed_ids` and `pending_ids`), plus the 32 MB `ids.json` parse. At
~0.5 KB per passage, twice, this is the term that grows with the corpus. Under the ceiling
today with 64 MB to spare; the obvious levers (share the two maps copy-on-write, a compact
id encoding) are pipeline changes and belong to a feature of their own. Not changed here.
**Resolved by Feature 010** (`specs/010-id-map-memory/report.md`): one shared map in a compact
shape — 88.5 MB → 28.2 MB per map on the host, one copy instead of two; the device figure is
in that report.

### F-003 — Fragments are unreachable under the real cost; covered synthetically

BERT's WordPiece turns any pre-token over 100 characters into a single `[UNK]` piece, and its
pre-tokenizer splits on punctuation, so no word can cost more than ~100 pieces against a ~250
budget — the corpus's longest token (a 392-character URL) costs 218 and fits. Step 4 of the
contract is therefore covered by one synthetic article at budget 6 in set B, labelled as such.

### F-004 — A `#[cfg(unix)]` test leaves dead helpers on Windows under `-D warnings`

The read-only tests set POSIX permissions and were gated per test, leaving their helper struct
and an import dead on the Windows CI runner. The lexical test is now whole-file `#![cfg(unix)]`
(the FFI suite exercises the read-only open on every OS), the pipeline test gates its helper;
the workspace is now also linted locally for `x86_64-pc-windows-msvc` (a check-only target).

### F-005 — `TEST_RUNNER_*` is an environment variable of `xcodebuild`, not a trailing argument

`xcodebuild … TEST_RUNNER_XTRIEVER_CORPUS=wikipedia` is a build setting and never reaches the
test (it skipped, defaulting to SciFact); `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia xcodebuild …`
does. Recorded in the quickstart.

### F-006 — Thread count is a throughput knob only (004 F-005 reconfirmed at corpus scale)

The development build ran at 1 thread (140 ms/passage), the full build at 4 (93 ms); the two
shards both produced are bit-identical (the full build took them as cache hits by content key
and the artefact's vectors are byte-identical across builds).

### F-007 — The model-backed suites must run serially: the 007 budget test is load-sensitive

`xtriever-ffi::budget::a_short_time_budget_yields_a_partial_rerank_without_an_error` asserts that
a 200 ms budget leaves at least one of eight queries *partially* re-ranked. Run as one nextest
invocation with the dense and CLI model-backed suites (32 tests on all cores), the CPU is
saturated and every query's budget is gone before the re-ranker scores a pair — all degrade
fully, none partially — and the test fails; alone, per crate, or with `-j 1` it passes (7 / 7
runs today). The test measures the pipeline on an unloaded machine, which is its stated
premise; the gate now runs the model-backed suites with `-j 1` (quickstart Step 8). The test
itself is unchanged (Rule 6).

## Review round 1

GitHub Copilot, five comments, all taken (details in the commit `fix after copilot review`).

| # | Comment | Action |
|---|---|---|
| 1 | Standalone `verify` skipped the URL check when the snapshot was absent | A missing snapshot is a hard error; the check is part of the contract |
| 2 | `read_only: true` for every FFI open bypasses the meta lock that protects a reader from *another* writer's garbage collection | Correct in principle. The FFI now opens **with** the lock whenever the directory permits and retries lock-free only when the directory refuses the lock file (`PermissionDenied` — the bundle); `xtriever-lexical` maps that specific lock failure to `Error::Io` (a busy lock stays `Backend`, 002 D14). `OpenOptions.read_only` documents the precondition the caller owns |
| 3 | `du -sk` measures allocated blocks, not the bundle's logical bytes | `stat -f%z` sum over files |
| 4 | Synthesized Codable omits a nil optional; the record schema requires `firstLaunchCopyMs: null` | A `Nullable<T>` wrapper that encodes `null` |
| 5 | A library target returning `anyhow::Result` violates Principle VII | The lib target is gone; the six test files are `#[cfg(test)]` unit tests in their modules |

## Known costs (stated, not claimed small)

- **Quality is unmeasured on this corpus** (owner decision, Q2). BEIR guards the stages; nothing
  guards this corpus's chunking or selection against a defect except use.
- The build is 11 hours once; the cache makes a rebuild minutes, and a chunker change re-embeds
  only the shards whose passages changed.
- The app is ~1.26 GB of resources; sideload only.
- The first query after open pays ~2 s to page the vectors in; every later one ~0.35–0.5 s at
  depth 0. Feature 009's UX should warm the index once.
- Half a gigabyte resident after open, of which ~200 MB is the id map twice (F-002).
