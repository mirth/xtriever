# Implementation Plan: The Dense Stage

**Branch**: `004-dense-stage` | **Date**: 2026-09-12 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/004-dense-stage/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

Make `xtriever-core`'s `Embedder` and `VectorIndex` true in `xtriever-dense`. The embedder loads
`all-MiniLM-L6-v2` at Feature 001's pinned revision from three size-and-SHA-256-verified files,
asserts the model's shape from the files, and runs candle 0.9.2 **one text at a time at a fixed
256-token length** so that vectors are bit-identical whatever batch they arrive in
(research D2). The index is a flat, exact store — one `index.bin` per generation, replaced by
`rename`, scores accumulated in `f64` and rounded once — verified against a NumPy oracle whose
goldens contain only *designed* exact ties (D7). Both artifacts have a safe buffered path by
default and a memory-mapped path behind the `mmap` feature, sharing every line after the bytes are
obtained, so the crate's entire `unsafe` surface is one `memmap2` call under one ADR (D1,
ADR-0007). The Feature 003 harness gains a `dense-baseline-v1` configuration, an embedding cache
keyed by fingerprint and corpus hash, and one optional `stage` key in the report; the three
absolute baselines are recorded and `--verify-run`-checked. Two research findings correct earlier
numbers and are carried into the design rather than around it: candle copies weights to the heap
on both load paths, so 001's "39.5×" was an ordering artifact (D1); and candle folds a batch into
the matmul's `M`, so batching is not free of bit-drift (D2).

## Technical Context

**Language/Version**: Rust, edition 2024, toolchain 1.91.1

**Primary Dependencies**: `xtriever-dense` — `candle-core` / `candle-nn` / `candle-transformers`
**0.9.2** (ADR-0001 pin, `cargo add …@0.9.2`), `tokenizers` **0.23.2** (`default-features =
false`, `fancy-regex`), `serde`, `serde_json`, `sha2`, `thiserror`; optional `memmap2` behind
`mmap` (already in the lockfile at 0.9.11). Dev: `proptest`, `tempfile`, `serde_json`.
`xtriever-eval` — unchanged library graph; **dev**-dependency on `xtriever-dense` for the example.
Python oracle — Feature 001's exact `torch 2.14.0 / transformers 5.17.0 / tokenizers 0.23.2 /
numpy 2.5.3` in `reference/.venv-004` (D14).

**Storage**: model files in git-ignored `reference/models/all-MiniLM-L6-v2/` (fetched by script,
pinned by hash); vector index as `index.bin` in a directory (format v1, data-model); embedding
cache = an index directory plus `cache.json` under `target/xt-dense-cache/<dataset>/`; baselines
as committed JSON under `specs/004-dense-stage/baselines/`.

**Testing**: `cargo nextest`; **offline suite** (search/mutation/persistence goldens, format and
fingerprint errors, property tests, harness config/cache/report tests) needs no model and runs in
CI; **model-backed suite** is `#[ignore]` and runs where the model directory exists (embedding
goldens, batch and thread-count bit-identity, load-path parity under `--features mmap`, fingerprint
equality). Properties: results ⊆ `allowed`; order is `(score DESC, id ASC)`; `add`/`delete`/`commit`
round-trips `len`; buffered ≡ mapped on random sets. Ranking: `xtriever-eval` on SciFact /
NFCorpus / FiQA with `--verify-run`.

**Target Platform**: host for everything that runs; `cargo check` on `aarch64-apple-ios`,
`aarch64-apple-ios-sim`, `aarch64-linux-android` for the default feature set (and `mmap` on iOS);
wasm32 best-effort (still fails at `errno` via tantivy — unchanged). Nothing runs on hardware.

**Project Type**: library crate (`xtriever-dense`) + additive library change (`xtriever-eval`) +
example subcommands + shell script + Python generator.

**Performance Goals**: none claimed. FR-023 *records* index size, peak RSS, per-path model memory,
embed and search wall time for FiQA with method; no `criterion` bench — no latency budget is stated
by the spec and the index is deliberately the simplest exact scan. Estimate for planning only:
100–150 ms per passage single-threaded (D2 measurement) ⇒ FiQA 1–2.5 h once, then cached.

**Constraints**: `xtriever-core`, `xtriever-lexical`, `xtriever-eval`'s metric and dataset layers,
`deny.toml` untouched (FR-025); no C/C++ in the default features (candle is pure Rust); no async,
no threads spawned by the crate (candle's rayon pool is the engine's, sized by
`RAYON_NUM_THREADS`, recorded not set); no `Instant` in library code (timings in the example
binary); dependencies downward (`dense → core`; example `eval → dense → core`).

**Scale/Scope**: one crate implemented (~900 lines), one crate extended (~250), one example
extended (~200), one script, one generator (~450 lines), three fixture files, three baselines, one
ADR, one constitution amendment (v1.2.0, done at plan time). **Over Rule 3's ~800 lines — four PRs** (below).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Authority: `.specify/memory/constitution.md` (v1.2.0, ratified 2026-09-10, last amended 2026-09-12).
Every row below MUST be marked **PASS**, **FAIL**, or **N/A** with a one-line justification — an
empty verdict is a FAIL.

- Any **FAIL** blocks the phase. Do not proceed, and do not weaken the gate to make it pass.
- Any **N/A** MUST state why the principle cannot apply to this feature.
- A violation that is genuinely required is recorded in Complexity Tracking below and REQUIRES an
  accepted ADR in `docs/adr/`; the row stays **FAIL** until that ADR is merged.
- These are plan-time gates. They do not replace the blocking CI gates in the constitution's
  "Quality Gates (CI)" table.

### Core Principles

| # | Principle | Gate | Verdict | Justification |
|---|---|---|---|---|
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | **PASS** | Tokenization is `tokenizers`, inference is `candle` (the named crates, at ADR-0001's pin). The vector index is a **flat exact scan** — the spec forbids an ANN structure (FR-010/FR-027) precisely because none has a measured justification yet; a flat scan is ~150 lines and is the reference an ANN would later be measured against, not a hand-built commodity. No tensor runtime, no kernels (D7: scalar `f64` loop, no SIMD). |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | **PASS** | PR 1 is goldens + red suite. Embedding oracle: Feature 001's torch pipeline at its exact pins, tolerance cosine ≥ 0.9999 / max-abs ≤ 1e-3 (spec Assumptions), with the token ids in the golden so a tokenizer disagreement fails first (R5). Search oracle: NumPy `float64` over the same `f32` values, ids/order exact, scores 1e-6, ties designed (D7). Ranking: three absolute baselines through the 003 harness, each `--verify-run`-checked (FR-022); **no delta line** — FR-021 makes the dense number an absolute baseline for a new stage and `delta` refuses mixed configurations. Properties: allowed-subset, ordering, len round-trip, buffered ≡ mapped. |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android` (wasm32 best-effort). On-device RSS stays under the spec's ceiling (default 300 MB for a 100k-chunk index including loaded models). | **PASS** | `xtriever-eval`'s library graph stays `core + serde + serde_json + sha2` (quickstart Step 8 greps for `candle`/`memmap2`); the dense dependency is dev-only for the example. `xtriever-dense` carries **no C/C++ at all** — candle's CPU path is pure Rust (gemm) and Feature 001 `cargo check`ed it on all three targets; `deny.toml` unchanged. The crate spawns no threads (candle's pool is the engine's). **RSS clause**: this feature records FiQA numbers and a labelled 100k extrapolation (FR-023, Story 4); no on-device configuration is claimed, and D1 corrects the model-memory expectation downward from 001's claim. |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | **PASS** | No performance claim is made, so no bench; every number in FR-023 is an observation with its method, measured **in fresh processes** for the load paths (D12) — the measurement 001 owed. Reproducibility: model pinned by revision + three hashes (D4), engine version in the fingerprint (D6), datasets pinned by 003's manifest, generator seeded and hashed, thread count recorded in the report. Two research items were *measured* before being decided (D2 timing, D4 hashes) and two were *read* from the pinned sources rather than assumed (D1, D3). `explain()` N/A: no stage chain yet; `Hit.score` is the per-stage score. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **PASS** | Core untouched (FR-025; quickstart diff must be empty); both traits implemented **in full** with no new core `Error` variant (D9). The index's on-disk format is **new**, versioned (`format_version: 1`, magic) and documented in the data-model — nothing existing changes. The 003 report format is **extended additively** (one optional trailing key, four optional sub-keys); every committed lexical report round-trips byte-identically, tested. The harness maps `DocId(i)` ↔ corpus position (003 D5) and the index never sees a string id. Public surface: two types, one enum, one const module (contract). Synchronous throughout; `dense → core` only. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | **PASS** | Determinism engineered, not inherited: per-text forward at fixed length (D2), `f64` accumulation with a fixed scalar order (D7), total `(score DESC, id ASC)` sort including the `k`-th rank (FR-011; ADR-0005's obligation on `VectorIndex` implementations), cross-process thread-count test (D3). Fingerprint + `format_version` in every `index.bin`; `open_for` → `FingerprintMismatch` naming both, version → `Corrupt`, dim → `Corrupt`/`DimensionMismatch` (FR-015). **Degradation N/A here**: falling back to the lexical stage is the pipeline's job (Feature 005); this crate's contract is to *fail loudly* (`Model`, `Corrupt`), which is what the pipeline needs to degrade on. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | **PASS** (v1.2.0) | Deps via `cargo add` with the ADR-0001 pin syntax (D13), core `Error` for library errors, `anyhow` only in the example, `missing_docs` on. The crate's **one** hand-written `unsafe` block — `memmap2::MmapOptions::map` in `bytes::map_readonly` — is exactly the case constitution v1.2.0 admits: read-only memory mapping, behind the non-default `mmap` feature, item-scoped `#[allow]`, `// SAFETY:` stating the no-in-place-modification invariant the crate itself guarantees (D8), tested bit-for-bit against the buffered path for both the weights and the index (ADR-0007, **Accepted 2026-09-12**, conditions 1–5 in force; condition 5's from-cold measurement is owed by the report). The default feature set compiles zero `unsafe`. Passes without exception; no Complexity Tracking row. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | **PASS** | Every candle, tokenizers and memmap2 item is cited by `crate-version/path:line` in research D1–D5 and D7–D8, read from the local registry sources; the two load-path findings (D1, D2) came from reading, not recall. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | **PASS** | Neither is touched (FR-025). `xtriever-eval`'s *metric* and *dataset* layers are also untouched; its `run`/`report` layers are extended additively as FR-019 directs. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | **PASS** | Branch `004-dense-stage`. Split: **PR 1** plan artifacts, ADR-0007, model manifest + fetch script, generator + goldens, red suite (~750 lines of tests + fixtures); **PR 2** embedder + load paths (~400); **PR 3** vector index (~500); **PR 4** harness extension, example, baselines, report (~450). Fixture JSON is counted separately as in 002/003. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | **PASS** | PR 1 ends at a red checkpoint: `fixtures_valid` and `model_pins` green, everything else failing at runtime on `NotImplemented` (the 002/003 scaffold; `check-no-stubs.sh` extended to this crate). |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | **PASS** | Quickstart Step 8 is the gate plus the containment greps and the purity check; the PR description carries the three absolute baselines, the `--verify-run` agreement, and the FR-023 observations in place of a delta (FR-021). |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances, or thresholds. | **PASS** | Three named stop-points: thread-count bit-identity fails (D3), load-path parity fails (ADR-0007 c.3), any golden outside tolerance. ADR-0007 condition 5 *deletes* the weights-mmap path on a poor measurement rather than softening the claim. Tolerances are the spec's; the generator refuses ambiguous goldens rather than the tests loosening. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | **PASS** | Per-text loop instead of a batching scheduler; one file rewritten per commit instead of generations/segments; `sort_unstable_by` over a heap; `from_le_bytes` over `chunks_exact` instead of transmute/bytemuck; `enum Rows { Owned, Mapped }` with two match arms; no trait of the crate's own, no generics over storage. |

**Initial gate (pre-Phase 0)**: **FAIL on VII only** under v1.1.0 — 2026-09-12, Claude (agent).
The violation was the one the spec's FR-008 commissions and the user chose; ADR-0007 was drafted in
this phase for the human's decision. All other 13 rows PASS. Proceeded to research with that row
open, as the template allows for a violation "genuinely required".

**Post-design gate (post-Phase 1)**: **PASS on all 14 rows** — 2026-09-12, Claude, after the
repository owner chose ADR-0007 option (a) and the constitution was amended to **v1.2.0**. Design
reduced the mapped path to its minimum (one function, one feature, both artifacts through the same
block — D1 made `from_mmaped_safetensors` unnecessary). No other row changed. `/speckit-tasks` may
run.

## Project Structure

### Documentation (this feature)

```text
specs/004-dense-stage/
├── plan.md              # This file
├── research.md          # Phase 0 — D1–D14 + risks
├── data-model.md        # Phase 1 — pins, embedder, index format v1, goldens, config, cache, report extension
├── quickstart.md        # Phase 1 — Steps 0–9
├── contracts/
│   └── dense-stage.md   # Phase 1 — public surface of xtriever-dense + harness extension + commands
├── baselines/           # PR 4 — dense-baseline-v1.{scifact,nfcorpus,fiqa}.json
├── checklists/requirements.md
├── tasks.md             # /speckit-tasks
└── report.md            # closing report

docs/adr/0007-unsafe-readonly-mmap-in-dense.md   # Accepted 2026-09-12 — constitution amended to v1.2.0
```

### Source Code (repository root)

```text
crates/xtriever-dense/
├── Cargo.toml                 # candle 0.9.2 (pinned comment), tokenizers 0.23.2, serde, serde_json, sha2, thiserror;
│                              # [features] default = [], mmap = ["dep:memmap2"]
├── src/
│   ├── lib.rs                 # crate docs; pub use {MiniLmEmbedder, FlatIndex, LoadPath, FORMAT_VERSION, model}
│   ├── error.rs               # helpers → core Error::{Model, Schema, InvalidQuery, Corrupt} (002 pattern)
│   ├── model.rs               # PINNED, FINGERPRINT, verify_files (size → sha256, 1 MiB chunks)
│   ├── bytes.rs               # read_verified() ; #[cfg(feature="mmap")] map_readonly() — THE unsafe block
│   ├── embedder.rs            # MiniLmEmbedder: load (assertions), tokenize (256/256), forward (batch 1), pool, normalise
│   └── index/
│       ├── mod.rs             # FlatIndex, pending map, add/delete/commit, open/open_for/open_mapped
│       ├── format.rs          # Header (serde), encode/decode index.bin, validation → Corrupt
│       └── search.rs          # f64 scoring per Metric, (score DESC, id ASC) sort, allowed handling
└── tests/
    ├── support/mod.rs         # fixtures dir, model dir (XTRIEVER_MODEL_DIR), load helpers
    ├── fixtures_valid.rs      # manifest hashes of reference/fixtures/004/*
    ├── model_pins.rs          # PINNED == reference/models/manifest.json; FINGERPRINT fields
    ├── model_load.rs          # #[ignore] — verify failures name file + both values; assertions; missing tokenizer
    ├── embed_golden.rs        # #[ignore] — tokens then vectors vs embeddings.json (SC-001)
    ├── embed_determinism.rs   # #[ignore] — 3 batch arrangements; cross-process RAYON_NUM_THREADS 1 vs 4 (SC-002)
    ├── load_paths.rs          # #[ignore] #[cfg(feature="mmap")] — buffered ≡ mapped bit-for-bit (ADR-0007 c.3)
    ├── fingerprint.rs         # #[ignore] — two loads, same fingerprint, same bits (SC-010); Query ≡ Passage (FR-007)
    ├── index_golden.rs        # search.json: every set/query/case, both open paths under mmap (SC-003)
    ├── index_mutation.rs      # mutations.json; replace; delete unknown; pending invisible; drop discards
    ├── index_persist.rs       # reopen bit-identical (SC-004); stale second handle; crash-safe tmp+rename
    ├── index_errors.rs        # fingerprint / version / dim mismatches (SC-005); non-finite; zero-norm; k=0; empty allowed
    └── index_prop.rs          # proptest: allowed ⊆, ordering, len round-trip, owned ≡ mapped

crates/xtriever-eval/
├── src/run.rs                 # + DenseConfig, PassageSpec, dense_baseline_v1, build_passages, execute_dense, EmbeddingCacheKey
├── src/report.rs              # + StageInfo, Observations optional fields, delta() config guard
├── examples/beir.rs           # + --config dense-baseline-v1, --model-dir, --cache-dir, --load-path; model-memory; export-vectors
└── tests/{dense_run,report}.rs # + build_passages goldens, cache key match/mismatch, lexical reports round-trip, delta guard

reference/
├── gen_004_fixtures.py        # embeddings / search / mutations / manifest; --verify-embed; --refresh-manifest
├── requirements-004.{in,txt}  # 001's pins + numpy
├── fixtures/004/{embeddings,search,mutations,manifest}.json
└── models/manifest.json       # the three pins (read by fetch-model.sh; asserted equal to PINNED)

scripts/fetch-model.sh         # curl + shasum, retry/timeout discipline of fetch-beir.sh
scripts/check-no-stubs.sh      # + xtriever-dense
.gitignore                     # + target/xt-dense-cache/ (already under target/); reference/models/ already ignored
specs/003-eval-harness/contracts/eval-harness.md   # one-line note: optional trailing `stage` key
```

**Structure Decision**: one stage crate implemented in place (`xtriever-dense` exists as a
placeholder), one additive extension to `xtriever-eval`'s run/report layers, and the example as the
only place that names both `MiniLmEmbedder` and `TantivyIndex`. Dependency direction unchanged:
`dense → core`; `eval → core` (library); `eval → {dense, lexical} → core` (example, dev-only).
`xtriever-pipeline` is not touched — fusion is Feature 005.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| *(none after v1.2.0)* — the `mmap` block was a VII violation under v1.1.0 and is now the case the principle admits | — | — | — | [ADR-0007](../../docs/adr/0007-unsafe-readonly-mmap-in-dense.md), Accepted 2026-09-12 |

## Decisions this plan takes that the spec left open (recorded for the human)

1. **Per-text inference, fixed 256 padding** (D2) — trades throughput for by-construction
   bit-identity. Batching is a later, measured optimisation.
2. **`f64` accumulation, `f32` result** (D7) — makes the 1e-6 oracle tolerance comfortable and
   exact ties exact.
3. **All three `Metric`s implemented** (D7) — three lines apart; the index is not tied to one
   embedder.
4. **Engine version in the fingerprint** (D6) — a candle bump invalidates indexes by design.
5. **Fingerprint excludes architecture** (D3) — same fingerprint ⇒ bit-identical *per
   architecture*; cross-architecture is within tolerance (001 measured). The contract says so.
6. **One `index.bin` per generation, rewritten on commit** (D8) — O(n) per commit, ~0.1 s at FiQA
   scale; incremental generations need a measured reason.
7. **Timings never enter the report JSON** (D10) — stderr only, so reports are byte-reproducible;
   observations are copied in by hand as in 003.
8. **`delta` refuses mixed configurations** (D10) — the mechanical form of FR-021.
9. **ADR-0007 condition 5** — the weights-mmap path is deleted if the fresh-process measurement
   shows < 10 % peak benefit; the index mapping stays. Accepted with the ADR on 2026-09-12.
