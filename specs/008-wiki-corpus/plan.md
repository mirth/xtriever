# Implementation Plan: The Wikipedia Corpus and Shipped Index

**Branch**: `008-wiki-corpus` | **Date**: 2026-09-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/008-wiki-corpus/spec.md`

## Summary

Turn the pinned `20231101.simple` snapshot (241,787 articles, 269 MB of text) into a shipped
hybrid index: a pure, cost-function-driven chunker in `xtriever-analysis` verified byte for
byte against a Python reference; `xtriever-cli`'s first real command (`xtriever wiki build /
verify / expected`) that streams the corpus through a sharded, resumable embedding cache into
the pipeline's unchanged format v2 and ends with a merge; a read-only open in the lexical
backend so the app opens the index inside its bundle (resolving 007 F-001 and halving on-device
disk); staging with attribution under a bundle budget; and the 007 device harness run over the
result against the 600 MB ceiling. No quality oracle (owner decision); the report says so.
Estimated artefact ~1.5 GB with models; estimated build ~12 h, paid once and cached.

## Technical Context

**Language/Version**: Rust 1.91.1 (pinned), edition 2024; Python 3.12 in `reference/.venv-008` (pyarrow, tokenizers 0.23.2) for the converter and fixtures; Swift 5.9 / Xcode for staging and the harness.

**Primary Dependencies**: existing — tantivy 0.26.2 (`Directory`, `MmapDirectory`, `DirectoryLock`, `Index::open`), tokenizers 0.23.2 (`Tokenizer::with_truncation/with_padding`), candle 0.9.2 via `MiniLmEmbedder`; new in `xtriever-cli` via `cargo add` — `clap` (derive), `anyhow`, `serde`, `serde_json`, `sha2`; no new dependency in any pure crate.

**Storage**: pipeline format v2 unchanged; `corpus.json` sidecar inside the index; `wiki-build.json` + `ATTRIBUTION.txt` beside it; embedding cache shards under `target/xt-wiki-cache/`; snapshot under `reference/datasets/wiki/` (gitignored, manifest-pinned).

**Testing**: nextest (analysis goldens + proptest properties; lexical/pipeline/dense/ffi read-only and API tests; cli unit tests over manifest, rules, cache, URL); the build's own `verify` phase (100 % window, URL, tiling); simulator XCTest (`WikipediaTests`); device `DeviceMeasurementTests` over the corpus; BEIR three-dataset re-run expecting zero delta.

**Target Platform**: host (the build); aarch64-apple-ios / -sim (the artefact opened read-only); Android/wasm `cargo check` unchanged.

**Project Type**: cli + library additions + Swift package resources.

**Performance Goals**: build ≤ ~15 h wall on the reference laptop, resumable at shard granularity; verify phase ≤ 10 min; device open ≤ 2 s (mapped, in place); depth-0 latency measured (the exact scan over ~740 MB of mapped vectors — expected 0.1–0.5 s warm, seconds cold); footprint verdict against 600 MB (ADR-0010).

**Constraints**: chunker pure (`std`-only, no tokenizers); no core trait / `deny.toml` / format change; FFI wire unchanged; CI never fetches or builds; index shipped in the bundle under a 2.0 GB resources budget.

**Scale/Scope**: ~480k passages (estimate; measured by the build); crates touched: `xtriever-analysis` (chunker), `xtriever-lexical` (read-only directory + open), `xtriever-pipeline` (`open_with`, `merge`, sidecar tolerance), `xtriever-dense` (`token_count`), `xtriever-ffi` (read-only open; `readonly.rs`), `xtriever-cli` (the tool), `swift/Xtriever` (remove `writableCopy`, `wikipediaURL`, corpus-selectable harness), `scripts/` (fetch-wiki, build-ios-package), `reference/` (converter, fixtures, manifest, queries).

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | Index, tokenization and inference are the existing tantivy / tokenizers / candle paths. The only new algorithm is the chunker — not a listed commodity; `text-splitter` was considered and rejected because a byte-exact golden contract needs an algorithm the Python oracle can implement independently (research D5). The read-only `Directory` is a delegating wrapper over tantivy's own `MmapDirectory`, not a directory implementation (D11). |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | Chunker goldens from `reference/gen_008_fixtures.py` (two cost models, byte-exact, tolerance zero) committed red first; tiling / bound / determinism / idempotence property-tested; the build's `verify` is an executable check over 100 % of passages; the three BEIR baselines are re-run and must be byte-identical (nothing in scoring changes — the delta is the proof). The corpus itself has no oracle by owner decision (Q2), which the spec and report state; this row is about the code's oracles, which are complete. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | **PASS** | The chunker takes its cost as a closure so `xtriever-analysis` stays `std`-only with no tokenizer (D5); `check-containment.sh` still covers the five pure crates; `Instant` only in the CLI binary. The device run measures the ceiling on a configuration **larger** than the reference one (~480k vs 100k chunks) — a stricter test, stated as such (spec "Why"). |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | Snapshot pinned by SHA-256 (parquet and derived JSONL), models by the existing manifests; the build record measures every phase, size and cache count; the device record measures footprint and latency per depth; the plan's numbers are labelled estimates and the report replaces them with measurements (D4, D9). No new benchmark is claimed; the chunker's throughput is recorded in the build record, not asserted. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | No core trait changes; format v2 untouched (the corpus identity is a sidecar the pipeline ignores — D12); additive APIs only: `TantivyIndex::open_read_only`, `HybridIndex::open_with` + `merge`, `MiniLmEmbedder::token_count`. Read-only open adds a success case where a defect produced an error (007 F-001) — the contract already said "read-only"; no error variant or meaning changes. The CLI depends on `pipeline`, `dense`, `analysis`, `core` — downward. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | Search behaviour untouched. The build is deterministic by construction: passage order = snapshot order × ordinal, `DocId` = that order, embeddings one-at-a-time under the 004 contract, a single merged segment; SC-001 tests two builds. Cache hits are keyed by content and fingerprint, so a stale vector cannot be served silently. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** | New deps only in the `xtriever-cli` binary via `cargo add` (`clap`, `anyhow`, `serde`, `serde_json`, `sha2`); `anyhow` there only; library additions use the crates' `thiserror` enums; no `unsafe` anywhere new (the read-only directory is safe delegation); `check-containment.sh` unchanged and passing. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | tantivy 0.26.2: `Directory` trait methods (`directory/directory.rs:107–231`), default `acquire_lock` → `try_acquire_lock` (`:74–87`, `:191`), `DirectoryLock: From<Box<T>>` (`:49–53`), `META_LOCK` taken in `reader/mod.rs:194`, `Index::open<T: Into<Box<dyn Directory>>>` (`index/index.rs:510`), `MmapDirectory::open` read-only (`mmap_directory/mod.rs:236`), `TantivyIndex::merge` (ours, `xtriever-lexical/src/index.rs:128`); tokenizers 0.23.2: `Tokenizer::with_truncation(None)`, `with_padding(None)` (`tokenizer/mod.rs:426,433`), `encode(input, add_special_tokens)` (`:871`); pipeline: `HybridIndex::{create, open, open_mapped, add_embedded, commit}` (`index.rs:98,147,337,371`), `SourceDocument` (`types.rs:40`), `ChunkInfo` (`core/types.rs:115`); dense: `Embedder::embed(&[&str], TextKind)` (`core/traits.rs:59`), `load_tokenizer` configuration (`embedder.rs:238–255`). The parquet URL, size, hash, row count and column set were fetched and verified (D1); the URL derivation was verified on every article (D6). |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither is touched; quickstart Step 8 diffs both against `main` and expects empty. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Five PRs (tasks will bind them): **PR 1** manifest + fetch script + converter + chunker goldens + red suites (~600); **PR 2** chunker + `token_count` + read-only open + `open_with`/`merge` (~700); **PR 3** `xtriever-cli` build/verify/expected (~700); **PR 4** Swift: `writableCopy` removal, `wikipediaURL`, corpus-selectable harness, staging (~400); **PR 5** the build record, run records, report (~300 + JSON). The 12-hour build runs between PR 3 and PR 5. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | PR 1 is fixtures and red tests only; every later PR's tests are written before its code within the PR (quickstart Step 1). |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Steps 6 and 8; the three BEIR baselines are re-run (cached embeddings; ~1 h) and their zero delta pasted; the build record and device records are the bench evidence. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | A passage over the window, a URL mismatch, a BEIR delta, a footprint over 600 MB, a bundle over 2.0 GB: each is a build/test failure or a ⛔ report, never a loosened number. The bundle budget may be revised only by an explicit report entry with the measured artefact — after the fact, in the open, not to pass. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | A `&dyn Fn(&str) -> usize` closure instead of a cost trait; a delegating struct instead of a generic directory adaptor; `clap` derive; raw little-endian `f32` shards with a JSON sidecar instead of a cache format; `OpenOptions` as a plain struct with two `bool`s. |

**Initial gate (pre-Phase 0)**: PASS — 2026-09-13, Claude (Opus 5), before research.

**Post-design gate (post-Phase 1)**: PASS — 2026-09-13, Claude (Opus 5), after research D1–D14 and the contracts; no row changed verdict; no Complexity Tracking entries.

## Project Structure

### Documentation (this feature)

```text
specs/008-wiki-corpus/
├── plan.md              # This file
├── research.md          # D1–D14 (snapshot, chunker cost function, read-only open, cache, …)
├── data-model.md        # manifest, article, passage, identity, build record, cache, run record
├── quickstart.md        # Steps 0–9
├── contracts/
│   ├── chunker.md       # the normative algorithm + costs + properties
│   ├── cli.md           # xtriever wiki build / verify / expected
│   └── artefact.md      # layout, hit semantics, URL derivation, read-only open, staging
├── build-record.json    # committed after the full build (PR 5)
├── runs/                # device run records (PR 5)
├── report.md            # PR 5
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/
├── xtriever-analysis/src/chunk.rs            # NEW: chunk(), Passage — pure, std-only; tests/chunk_golden.rs, tests/chunk_prop.rs (proptest dev-dep)
├── xtriever-lexical/src/readonly.rs          # NEW: ReadOnlyDirectory (tantivy Directory by delegation, no-op lock); index.rs: open_read_only; tests/read_only.rs
├── xtriever-pipeline/src/index.rs            # open_with(OpenOptions), merge(); tests/open_with.rs, tests/merge.rs, sidecar tolerance test
├── xtriever-dense/src/embedder.rs            # token_count() + second Tokenizer; tests/token_count.rs (model-backed, release)
├── xtriever-ffi/src/index.rs                 # open → read_only: true; docs; tests/readonly.rs gains the 0o555 case
├── xtriever-cli/                             # NEW content: src/main.rs (clap), src/wiki/{manifest,rules,chunking,cache,build,verify,expected,record}.rs; tests/
reference/
├── datasets/wiki-manifest.json               # NEW (pinned snapshot + rules)
├── wiki_to_jsonl.py, requirements-008.{in,txt}   # NEW converter (pyarrow), pinned
├── gen_008_fixtures.py                       # NEW: chunker reference → reference/fixtures/008/{chunk_a,chunk_b,manifest}.json
└── fixtures/008/queries.json                 # NEW: 20 measurement queries
scripts/
├── fetch-wiki.sh                             # NEW (curl + sha256 + convert + sha256), mirrors fetch-beir.sh
└── build-ios-package.sh                      # --with-wiki, bundle budget gate
swift/Xtriever/
├── Sources/Xtriever/XtrieverIndex.swift      # writableCopy removed; Hit.wikipediaURL / titleAndPassage
├── Tests/XtrieverTests/{WritableCopyTests.swift → deleted, WikipediaTests.swift NEW, DeviceMeasurementTests.swift (corpus env)}
└── Sources/Xtriever/XtrieverData/wikipedia/  # staged, gitignored
.gitignore                                    # reference/datasets/wiki/, target/xt-wiki*, XtrieverData/wikipedia
```

**Structure Decision**: one crate per concern, dependencies downward: `core` ← `analysis`
(chunker, no deps beyond core) / `lexical` / `dense` ← `pipeline` ← `ffi` and `cli`. The CLI
is the only crate that sees the snapshot, the manifest and the cache; the pipeline never learns
what a corpus is; the analysis crate never learns what a tokenizer is.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

None.
