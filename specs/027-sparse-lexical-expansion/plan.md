# Implementation Plan: Optional Sparse Lexical Expansion

**Branch**: `027-sparse-lexical-expansion` | **Date**: 2026-09-23 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/027-sparse-lexical-expansion/spec.md`

## Summary

An index may be created with sparse expansion switched on. The build then encodes every
document with the pinned OpenSearch document encoder (`opensearch-neural-sparse-encoding-doc-v3-distill`,
DistilBERT with its masked-LM head, Apache-2.0, pinned since Feature 012) and writes its weighted
vocabulary into a reserved lexical field `_sparse`, each weight as `round(weight × 10)`
occurrences of the term `s<token id>`. A search of that index adds the query's own token ids —
those the encoder's published query-side table weights above zero — as terms on `_sparse` at
boost 1.0, beside the ordinary match on the document text. No model runs at query time, and
the index carries the 1.6 MB its query side needs (the encoder's tokenizer and table, copied
byte for byte and verified by hash), so devices need nothing else. The option is off by
default, changes nothing when off, and ships as the owner's opt-in for FiQA-shaped corpora on the
spike's measured +0.0174 nDCG@10 on FiQA and +0.0041 on the three-set mean.

The encoder is our own forward pass over candle's layers, because candle's DistilBERT module
approximates GELU where the model and its reference use the exact form (research D2); it is
verified against a PyTorch oracle at 1e-4. The work lands in three pull requests: the encoder
and oracle, the pipeline and measurement, the surfaces.

## Technical Context

**Language/Version**: Rust (toolchain pinned in `rust-toolchain.toml`), edition 2024; Python 3.12
for `reference/` (PyTorch + `transformers`, `reference/requirements-027.txt`)

**Primary Dependencies**: candle 0.9.2 (`candle_nn::{embedding, linear, layer_norm}`,
`candle_nn::VarBuilder::from_slice_safetensors`, `candle_core::Tensor::{gelu_erf, relu, max,
affine, log}`), tokenizers 0.23.2 (`Tokenizer::{from_file, from_bytes, with_truncation, encode,
token_to_id}`, `Encoding::get_ids`), tantivy 0.26.2 through `xtriever-lexical` (a text field under
`standard`, `LexicalQuery::{Bool, Match, Term}`). No dependency is added.

**Storage**: the descriptor (`xtriever-pipeline.json`) gains a `sparse` record and version 3 for
sparse indexes; a sparse index gains `sparse/{tokenizer.json, query-table.json}` and the lexical
field `_sparse` (ADR-0016)

**Testing**: cargo nextest (model-free unit and property tests in CI; model-backed tests
`#[ignore]`d, run locally), proptest (the term-frequency and query-term rules), golden fixtures
from `reference/gen_027_fixtures.py`, `xtriever-eval` on SciFact, NFCorpus and FiQA

**Target Platform**: host + `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`
(the encoder compiles everywhere and runs on the build host only); wasm32 best-effort

**Project Type**: library (engine crates), evaluation harness, CLI, FFI with Python/Swift/Kotlin
bindings

**Performance Goals**: none claimed. Recorded: encoding throughput (documents per second, threads),
each dataset's lexical index size with and without the option (SC-005: ≤ 4× at scale 10)

**Constraints**: option off ⇒ byte-identical indexes and results (FR-002); CI runs no encoder
(FR-011); pure crates stay pure (the pipeline uses the dense crate's types, adds no dependency);
no `xtriever-core` trait or `deny.toml` change

**Scale/Scope**: 4 crates in PR A–B (`xtriever-dense`, `-pipeline`, `-eval`, and `reference/`),
3 more in PR C (`-cli`, `-ffi`, the bindings); FiQA's 57,638 documents are the largest encode

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
| I | Reuse Before Build | Commodity components come from `tantivy` (inverted index/BM25), `tokenizers`, `candle`, `roaring`. No hand-written inverted index, ANN graph, or tensor runtime without an accepted ADR showing the reused crate fails a *measured* requirement. | PASS | The field and BM25 are tantivy's, tokenisation is `tokenizers`', every layer and operation of the encoder is candle's; only the forward pass is composed here, as Feature 026 composed the quantised BERT, because candle's module computes a different function (tanh GELU, research D2) — composition, not a runtime. |
| II | Executable Oracles Over Prose (NON-NEGOTIABLE) | Acceptance tests are written and committed failing before implementation. Behaviour with a reference implementation is pinned to golden fixtures from `reference/` with tolerances stated in the spec. Ranking-affecting work reports nDCG@10 / Recall@100 deltas from `xtriever-eval` on SciFact / NFCorpus / FiQA. Invariants (analyzer determinism, index round-trips, filter algebra) are property-tested. | PASS | Each PR opens with its failing tests; `reference/gen_027_fixtures.py` (PyTorch) is the encoder's oracle at 1e-4 (research D9); the term-frequency and query-term rules are property-tested; the option's deltas on all three datasets and the option-off reproduction go in PR B's description (research D10). |
| III | Portability Is a Feature | `xtriever-core`, `-analysis`, `-pipeline`, `-ltr`, `-eval` stay pure Rust and `std`-only: no C/C++ build deps, no `async`, no `tokio`, no unconditional threads, no `std::time::Instant` in library code. C/C++ deps live only in `xtriever-dense`, `-rerank`, `-ffi` behind non-default features enforced by `deny.toml`. `cargo check` passes on host, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`. On-device RSS stays under the spec's ceiling (default 600 MB for the full pipeline — 100k-chunk hybrid index with embedder and cross-encoder loaded, ADR-0010); a smaller measured configuration says so and claims nothing for the reference one. | PASS | New code is pure Rust in the dense leaf crate; the pipeline and harness gain no dependency; the cross-target checks run in every PR's gate. The device ceiling's reference configuration (the Wikipedia artefact) does not use the option, and nothing is claimed for a sparse index on a device beyond what it carries (1.6 MB and a larger lexical index, recorded). |
| IV | Measured, Not Asserted | Every performance claim is backed by a `criterion` benchmark against budgets stated in the spec (p50/p99 latency, index size, RSS). Benches and eval runs are reproducible: fixed seeds, model revisions pinned by SHA, pinned dataset versions. `explain()` reports per-stage scores and features for every hit. | PASS | No speed claim (research D12); the size bound is measured per dataset; the encoder is pinned by revision and hashes; `explain()`'s lexical score includes the `_sparse` contribution and its documentation says so. |
| V | Small, Explicit Interfaces | The `xtriever-core` traits (`Analyzer`, `LexicalIndex`, `Embedder`, `VectorIndex`, `Reranker`, `Ranker`), the on-disk format, and error semantics are unchanged — or the change has human review plus an ADR in `docs/adr/`. Core APIs are synchronous; async wrappers only in `xtriever-ffi` or server crates. Internal `DocId` is a dense `u32` and backends never see external string ids. One crate per stage, dependencies pointing downward only: `core` ← stage crates ← `pipeline` ← `ffi`/`cli`. | **FAIL** (by design, until ADR-0016 is accepted) | No core trait changes and the dependency direction holds (the pipeline already depends on the dense crate), but the on-disk format changes for sparse indexes: descriptor version 3, the `sparse` record, the reserved `_sparse` field and the `sparse/` directory. ADR-0016 is written in PR B and needs your review. |
| VI | Graceful Degradation and Determinism | Failure or timeout of an ML stage degrades to the previous stage's results, never to an error, unless the caller opted into strict mode. Same index + same query + same config yields identical results, ties broken by ascending `DocId`. Indexes carry the embedder fingerprint and a format version; mismatches are hard errors at open time, never silent. | PASS | No model runs at query time, so search has no new failure to degrade; encoding is a build step and fails the `add` as the embedder does today. One document per encoder call makes weights a function of the document alone; the stored query side is verified by hash at open and a version mismatch is refused by name. |
| VII | Rust Hygiene | Edition 2024, toolchain pinned in `rust-toolchain.toml`, `Cargo.lock` committed, dependencies added with `cargo add` at current versions — never from memory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `cargo deny check` all pass. No `unwrap`/`expect`/`panic!`/`todo!` in library code; library errors are `thiserror` enums, `anyhow` only in binaries. Hand-written `unsafe` only in `xtriever-dense` and `xtriever-rerank`, and there only in SIMD kernels and in read-only memory mapping behind a non-default feature; each block preceded by `// SAFETY:` and tested against the safe path (ADR-0007, ADR-0009). `xtriever-ffi` may carry a crate-level `#![allow(unsafe_code)]` for uniffi scaffolding, hand-written modules re-declaring `#![deny(unsafe_code)]` (ADR-0003). All public items documented (`missing_docs` on). | PASS | No dependency added, no `unsafe` written (weights are read through the existing `bytes::read`), errors are the existing `thiserror` variants, the full gate runs before each PR. |

### Agent Operating Rules

| # | Rule | Gate | Verdict | Justification |
|---|---|---|---|---|
| 1 | Never invent an API | Every external crate item this plan relies on was read from the docs for the pinned version and is cited here by item path. | PASS | Items read in the registry sources for the pinned versions and cited in Technical Context; the GELU finding came from reading `candle-transformers-0.9.2/src/models/distilbert.rs` beside `models/bert.rs`. |
| 2 | Don't touch the contract uninvited | This plan modifies `xtriever-core` traits or `deny.toml` only if the spec explicitly says so; otherwise it stops and asks. | PASS | Neither is touched: the pipeline holds the dense crate's concrete types (research D5), and no dependency is added. |
| 3 | One spec, small PRs | Work is scoped to this spec on its own branch, and the planned change stays under ~800 changed lines — or the plan states the split. | PASS | Three pull requests (research D11), each planned under the limit; fixtures and run records are counted separately in each description. |
| 4 | Tests first | Task ordering puts the acceptance tests first, committed failing, ahead of any implementation task. | PASS | Each PR's first task is its failing tests: the oracle and the rules (A), the format and query shape (B), the surfaces (C). |
| 5 | Full gate before done | The plan budgets for running the whole local gate and pasting eval/bench deltas into the PR before any task is called done. | PASS | Each PR ends with the full gate; PR B's with the three-dataset table and the overnight FiQA encode. |
| 6 | Never weaken the oracle | On a metric drop or a target that fails to build, the plan's response is to stop and report — never to relax tests, tolerances or thresholds. | PASS | A breach of SC-002 (the spike sat at −0.0048 against −0.005) stops the feature and is reported; the 1e-4 tolerance is fixed here, before any weight is computed. |
| 7 | Prefer boring code | No macros for their own sake, no trait gymnastics, no premature generics. | PASS | Concrete types, one module, an attach-the-encoder setter modelled on `set_reranker`; no trait, no generic. |

**Initial gate (pre-Phase 0)**: **FAIL** — 2026-09-23, evaluated by the plan's author. One row
is FAIL by design: Principle V, because sparse indexes change the on-disk format. It clears when
ADR-0016 is accepted; nothing else blocks.

**Post-design gate (post-Phase 1)**: **FAIL, unchanged and expected** — 2026-09-23, after
`research.md`, `data-model.md`, `contracts/` and `quickstart.md`. The design added no dependency,
no crate and no core-trait change; it removed one requirement's risk (the query side travels in
the index, research D6) and found one reuse limit (candle's GELU, research D2), handled inside
Principle I.

## Project Structure

### Documentation (this feature)

```text
specs/027-sparse-lexical-expansion/
├── plan.md              # This file
├── research.md          # D1–D12
├── data-model.md        # the option, the record, the layout, the lifecycle
├── quickstart.md        # validation, per pull request
├── contracts/
│   ├── sparse-option.md         # the dense module and the pipeline (PR A, PR B)
│   └── surfaces-and-eval.md     # the harness (PR B), CLI and bindings (PR C)
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/
├── xtriever-dense/
│   ├── src/sparse.rs              # PR A: SparseEncoder (DistilBERT + MLM head, exact GELU), SparseQuery, field_text
│   ├── src/model.rs               # PR A: the encoder's pins beside the embedder's
│   └── tests/{sparse_oracle.rs, sparse_rules.rs, sparse_pins.rs}
├── xtriever-pipeline/
│   ├── src/{types.rs, descriptor.rs, index.rs, search.rs}   # PR B: the option, v3, add/add_encoded, the query
│   └── tests/sparse_index.rs
├── xtriever-eval/
│   ├── src/run.rs                 # PR B: hybrid-sparse-v1, hybrid-sparse-rerank-v1, the sparse cache
│   └── examples/beir.rs           # PR B: --sparse-encoder-dir, --sparse-cache-dir
├── xtriever-cli/src/wiki/         # PR C: --sparse-encoder
└── xtriever-ffi/src/              # PR C: IndexConfig.sparse, IndexInfo.sparse, the index's own version

python/, swift/, android/          # PR C: bindings and their tests
reference/
├── gen_027_fixtures.py            # PR A: the PyTorch oracle
├── requirements-027.{in,txt}
└── fixtures/027/                  # PR A: documents, queries, weights, manifest
docs/adr/0016-sparse-expansion-field.md   # PR B
```

**Structure Decision**: the encoder and query side are a module of the dense crate, the neural
encoder crate that already pins, verifies and loads models (research D1); the pipeline holds its
concrete types, a dependency it already has. Direction stays `core` ← `lexical`/`dense`/`rerank`
← `pipeline` ← `ffi`/`cli`/`eval`.

## Complexity Tracking

| Violation | Principle / Rule | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------|------------------|------------|--------------------------------------|-----|
| On-disk format change for sparse indexes (descriptor v3, `sparse` record, `_sparse` field, `sparse/` directory) | V | The option must be recorded, its query side carried, and an older engine must refuse such an index by name instead of searching it without the expansion | An unversioned optional field (Features 015, 024): an older engine would open the index and silently ignore the expansion | docs/adr/0016-sparse-expansion-field.md (PR B) |
