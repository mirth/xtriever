# Feature 008 — The Wikipedia Corpus and Shipped Index

**Written for**: the reviewer of the `008-wiki-corpus` branch.

The whole Simple English Wikipedia (241,787 articles → 427,947 passages) as a shipped hybrid
index: a pure, golden-tested chunker; `xtriever wiki build / verify / expected` — one resumable
command with a content-keyed embedding cache; a read-only open so the app opens the index
inside its bundle (007 F-001 resolved); staging under a bundle budget; and the constitution's
600 MB ceiling measured for the first time at corpus scale: **PASS, 535.8 MB peak**. Full report:
[report.md](./report.md); build record: [build-record.json](./build-record.json); device runs:
[runs/](./runs/).

## Commits

| commit | content | lines (hand-written) |
|---|---|---|
| `impl0` | everything in plan phases 1–4 plus the device harness change: manifest, fetch script, converter, chunker + goldens + properties, `token_count`, read-only open (lexical/pipeline/ffi), `open_with` + `merge`, the CLI, Swift changes, staging | 4,284 (+) / 263 (−) across 55 files; plus 3,960 generated lines (fixtures, pinned requirements, `Cargo.lock`) |
| `fix for win` | `#[cfg(unix)]` gating of the permission-based tests (report F-004) | 21 / 27 |
| `fix after copilot review` | review round 1 (report table): hard-fail `verify` without the snapshot, lock-when-possible FFI open, logical staging sizes, `Nullable`, lib target removed | 501 / 443 |
| (this) | build record, two device run records, report, PR description | JSON + docs |

The plan's five-PR split (~600–700 lines each) was not followed — `impl0` is one commit of
~4,300 hand-written lines. Reviewing it by area: `xtriever-cli` 2,023, `xtriever-analysis` 536,
`xtriever-pipeline` 329, `xtriever-lexical` 284, Swift 194, `xtriever-ffi` 118, `xtriever-dense`
123, scripts 126, reference 444.

## Numbers

- **Build**: 11.1 h at 4 threads, 93.3 ms/passage (004's number); 105 shards; second build from
  cache 2.8 min with `corpus.json`, `passages.bin`, `dense/index.bin` and the goldens
  byte-identical (SC-001). Verify: 427,947 / 427,947 passages inside the 256-position window
  (longest exactly 256), 0 URL mismatches.
- **Artefact**: 1,076,413,167 B (dense 661 MB, passages 276 MB, lexical 106 MB, ids 32 MB);
  staged with models 1,259,528,003 logical bytes of the 2.0 GB budget.
- **Device** (iPhone 16e, iOS 26.6.2, mapped, in place): peak **535.8 MB / 600 MB PASS** on both
  runs; open ~1.0 s; depth-0 median 339 ms (default threads) / 490 ms (1 thread), 2.0 s cold;
  depth 20 median 2.3 s / 4.4 s; 97 / 194 ms per re-ranked pair; parity lexical 20 / 20
  bit-identical, dense Δ ≤ 1.8e-7, re-rank Δ ≤ 4.8e-6.
- **Chunker**: 57 golden cases byte-identical; `token_count` = Python for 813 units.

## Eval delta (Rule 5)

`hybrid-rerank-v1` re-run on SciFact / NFCorpus / FiQA against the committed 006 baselines:
nDCG@10 **0.703862 / 0.360287 / 0.374214**, Recall@100 **0.941667 / 0.320720 / 0.707111** —
identical, per query. No scoring code changed; the lexical crate gained a read-only directory
and a lock-failure mapping, the pipeline a `merge` the eval does not call.

## What touches the guarded crates (SC-006), each named in the plan

- `xtriever-lexical`: `readonly.rs` (new — a delegating `Directory` with a no-op lock),
  `index.rs` (`open_read_only`, `is_read_only`, refusals), `error.rs` (`LockFailure(IoError)` →
  `Error::Io`; `LockBusy` stays `Backend`), `lib.rs`.
- `xtriever-pipeline`: `OpenOptions` + `open_with` (public), `merge`, `passage_text`,
  `external_id`; `Error::Backend` instead of `unreachable!` for mapped-without-feature.
- `xtriever-dense`: `MiniLmEmbedder::token_count` (a second untruncated tokenizer).
- `xtriever-ffi`: locked open when the directory permits, lock-free fallback on
  `PermissionDenied`; docs; one new test.
- `xtriever-core` and `deny.toml`: unchanged (`git diff main` empty).

No ADR: no core trait, no on-disk format, no error variant changed (the lock-failure mapping
refines which existing variant an I/O fact lands in). No new `unsafe`. New dependencies only in
the `xtriever-cli` binary (`clap`, `anyhow`, `serde`, `serde_json`, `sha2`) and as
dev-dependencies (`proptest`, `serde_json`, `serde`, `sha2` in `xtriever-analysis`; `tempfile`
in the CLI), all via `cargo add`. CI: no job added; the chunker fixtures run in the workspace
test job; never the fetch or the build (standing rule).

## Stated, not claimed small

- **Retrieval quality of this corpus is unmeasured** (owner decision, spec Q2 / FR-013).
- ~200 MB of the 509 MB resident after open is the id map held twice (report F-002) — a lever,
  not changed here.
- The first query after open pays ~2 s to page the vectors in.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
