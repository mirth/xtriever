---

description: "Task list for Feature 027 — optional sparse lexical expansion"
---

# Tasks: Optional Sparse Lexical Expansion

**Input**: Design documents from `/specs/027-sparse-lexical-expansion/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md),
[data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: included and written first. Principle II and Agent Operating Rule 4 make them
mandatory: each pull request's tests are committed failing before its implementation.

**Organization**: by user story. The spec ranks User Stories 1–3 equally (P1), so dependency
sets the order: the encoder and its oracle (US2) come first because indexing (US1) and
measurement (US3) both need the encoder. Three pull requests (research D11): **PR A** is Phases
1–3 (the encoder, the query side, the oracle), **PR B** is Phases 4–5 (the index, ADR-0016, the
measurement), **PR C** is Phase 6 (the surfaces). Phase 7 is the report.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an unfinished task
- **[Story]**: the user story the task serves
- Every task names the file it touches

## Path Conventions

`crates/xtriever-dense/src/sparse.rs` holds the encoder and the query side,
`crates/xtriever-pipeline/` the option, the format and the query, `crates/xtriever-eval/` the
measurement, `reference/` the oracle, `docs/adr/` the decision record. No new crate, no new
dependency, no `xtriever-core` or `deny.toml` change.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: the pinned encoder on disk, the reference environment, the compiled-in pins.

- [X] T001 Fetch and verify the pinned encoder with `scripts/fetch-model.sh --manifest reference/models/manifest-sparse-doc-v3.json` into `reference/models/opensearch-neural-sparse-encoding-doc-v3-distill/`, and confirm `git check-ignore` covers that directory (the weights are never committed)
- [X] T002 [P] Write `reference/requirements-027.in` (the `torch` and `transformers` versions pinned in `reference/requirements-012.txt`, plus `numpy`) and compile `reference/requirements-027.txt` with hashes; create the environment with `scripts/setup-reference-venv.sh 027`
- [X] T003 [P] Add `PINNED_SPARSE` to `crates/xtriever-dense/src/model.rs` — repository `opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill`, revision `babf71f3c48695e2e53a978208e8aba48335e3c0`, and each of `config.json`, `tokenizer.json`, `model.safetensors`, `idf.json` with the manifest's bytes and SHA-256 — and `crates/xtriever-dense/tests/sparse_pins.rs` asserting it equals `reference/models/manifest-sparse-doc-v3.json` field for field (green at the red checkpoint, as the other pin tests are)

---

## Phase 2: Foundational

**Purpose**: none. The encoder module is User Story 2's own work, and everything later depends
on it through that story.

---

## Phase 3: User Story 2 — The encoder's output is checked against a reference (P1) — PR A

**Goal**: an encoder in Rust whose weights match the pinned model's PyTorch reference at 1e-4,
and the query-side rule, both usable by the pipeline.

**Independent Test**: `cargo nextest run -p xtriever-dense --run-ignored all -E 'binary(sparse_oracle)'`
passes against `reference/fixtures/027/`; the model-free rules pass in CI.

### Tests for User Story 2 (written first, committed failing) ⚠️

- [X] T004 [US2] Write `reference/gen_027_fixtures.py`: 12 fixture documents authored in the script (at least one over 512 tokens, one non-ASCII, one of a few words) and 8 queries; encode each document **alone** on CPU in float32 with `transformers.DistilBertForMaskedLM` from the pinned directory, tokenised with truncation at 512 including special tokens — logits' maximum over positions, `log1p(relu(·))`, `log1p(·)`, special ids zeroed, entries > 0 kept ascending (research D3); for each query the distinct token ids, special ids excluded, with a positive `idf.json` entry, ascending (research D7); record each document's untruncated length; write `reference/fixtures/027/{documents.json, expansions.json, queries.json, manifest.json}` (documents with their token ids; queries with their terms; the manifest with every file's SHA-256, the generator's SHA-256, the model revision, the library versions and the tolerances) and support `--check`, which re-derives everything and exits 1 on any difference
- [X] T005 [P] [US2] Write `crates/xtriever-dense/tests/sparse_oracle.rs` (`#[ignore = "needs the sparse encoder"]`): every fixture weight within **1e-4** absolute of the reference; the kept entry sets equal apart from reference weights below 1e-4; every term frequency at scale 10 equal apart from entries whose reference `weight × 10` lies within 1e-3 of a half; `truncated` equal to the fixture's length > 512; each query's terms equal the fixture's exactly; encoding the fixtures twice gives identical bits
- [X] T006 [P] [US2] Write `crates/xtriever-dense/tests/sparse_rules.rs` (model-free): `field_text` — `s<id>` repeated `round(weight × scale)` times with halves rounded away from zero, zero-occurrence entries dropped, ascending ids, single spaces, the empty expansion as `""`; a property test that every entry's occurrence count equals `round(weight × scale)` for scales 1–100; the query-term rule on synthetic ids and a synthetic table (distinct, ascending, positive entries only, special ids excluded); `SparseQuery::open` refusing a file whose SHA-256 differs with `Error::Corrupt` naming the file and both hashes; `SparseEncoder::load` on a missing directory refusing with `Error::Model` naming the file
- [X] T007 [US2] Add `crates/xtriever-dense/src/sparse.rs` with the contract's public items (`contracts/sparse-option.md` §`xtriever-dense::sparse`) returning `Error::Model` "not implemented", export it from `crates/xtriever-dense/src/lib.rs`, run the tests to show them failing for that reason, and generate the fixtures with T004. **⛔ Red checkpoint — the owner commits the failing tests and the fixtures**

### Implementation for User Story 2

- [X] T008 [US2] Implement `SparseEncoder::load` in `crates/xtriever-dense/src/sparse.rs`: verify every `PINNED_SPARSE` file (the existing `verify_file`) before parsing anything; parse `config.json` and assert `dim` 768, `n_layers` 6, `n_heads` 12, `hidden_dim` 3072, `vocab_size` 30522, `max_position_embeddings` 512, `activation` `gelu`, naming any that differ; load `tokenizer.json` with truncation at 512 (`TruncationParams`, longest-first); read `model.safetensors` through `bytes::read` and build a `VarBuilder::from_slice_safetensors` in `DType::F32`
- [X] T009 [US2] Write the forward pass in `crates/xtriever-dense/src/sparse.rs` over `candle_nn::{embedding, linear, layer_norm}` with the tensor names of `candle-transformers-0.9.2/src/models/distilbert.rs`: embeddings (word + position, `LayerNorm` eps 1e-12), six blocks (`q_lin`, `k_lin`, `v_lin`, `out_lin`, softmax attention with nothing masked, `sa_layer_norm`, `ffn.lin1` → `gelu_erf` → `ffn.lin2`, `output_layer_norm`), and the masked-LM head (`vocab_transform` → `gelu_erf` → `vocab_layer_norm` → the tied `distilbert.embeddings.word_embeddings` weight plus `vocab_projector.bias`); exact GELU everywhere (research D2), with a comment citing candle's tanh approximation as the reason
- [X] T010 [US2] Implement `SparseEncoder::encode` in `crates/xtriever-dense/src/sparse.rs`: one document per call at its own length; the logits' maximum over positions; `relu`, `affine(1.0, 1.0)` then `log`, twice; special ids (the tokenizer's `[CLS]`, `[SEP]`, `[PAD]`, `[UNK]`, `[MASK]`, via `token_to_id`) zeroed; entries > 0 ascending; `truncated` from the untruncated encoding's length; and `identity()` as data-model `SparseRecord.encoder`
- [X] T011 [P] [US2] Implement `SparseQuery::open` (verify both SHA-256s before parsing; load the tokenizer and the table's positive entries) and `SparseQuery::terms` (tokenise without truncation, apply the query-term rule), and the free function `field_text`, in `crates/xtriever-dense/src/sparse.rs`
- [X] T012 [US2] Document the module in `crates/xtriever-dense/src/lib.rs` (a Feature 027 section: the encoder, exact GELU and why, the query side, build-host only) and run the oracle (quickstart §1), including a second run with `RAYON_NUM_THREADS=1` compared byte for byte
- [X] T013 [US2] PR A gate: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` with warnings denied, `cargo nextest run --workspace`, `cargo deny check`, the three cross-target checks, `reference/.venv-027/bin/python reference/gen_027_fixtures.py --check`, the oracle with its largest weight difference recorded; write `specs/027-sparse-lexical-expansion/pr-description-a.md`. **⛔ Checkpoint A — the owner commits, pushes and merges PR A**

**Checkpoint**: the encoder exists, matches its reference, and nothing else in the engine uses it yet.

---

## Phase 4: User Story 1 — An installation opts an index into sparse expansion (P1) — PR B

**Goal**: `HybridConfig.sparse` builds a sparse index that searches with the expansion
automatically, carries its own query side, and refuses what it must.

**Independent Test**: `cargo nextest run -p xtriever-pipeline --run-ignored all -E 'binary(sparse_index)'`
passes; every existing pipeline test passes unchanged (FR-002).

### Tests for User Story 1 (written first, committed failing) ⚠️

- [X] T014 [P] [US1] Write model-free unit tests in `crates/xtriever-pipeline/src/descriptor.rs` and `crates/xtriever-pipeline/src/search.rs` (`#[cfg(test)]`): the descriptor round-trips a `sparse` record at version 3; version 3 without a record, and 2 with one, are `Corrupt`; an index without the option writes version 2 and its search query is `LexicalQuery::Match(None, text)` exactly; a sparse index's query is `Bool { should: [Match(Some(f), text) for every user text field in schema order] + [Term("_sparse", "s<id>") for each term, ascending] }` built from a small tokenizer written in the test
- [X] T015 [P] [US1] Write `crates/xtriever-pipeline/tests/sparse_index.rs`. Model-free: `create` refuses a user field named `_sparse`, `scale` 0 or above `sparse::MAX_SCALE`, and a `boost` that is not finite or not above 0, each naming the value, writing nothing, and refuses a valid option naming `create_sparse`; an index without the option is unchanged. `#[ignore = "needs the sparse encoder"]` (the dense side is the fixture's table embedder): `create_sparse` writes descriptor version 3, the `sparse` record (scale 10, boost 1.0, field `_sparse`, the encoder identity) and `sparse/{tokenizer.json, query-table.json}` byte-identical to the pinned files; `add` with the encoder attached then `commit` and `search` returns results that differ from the same documents without the option; reopening without the encoder searches identically; `add` without the encoder is `Error::Model` naming it; `add_embedded` is refused; a document supplying `_sparse` is refused; `add_encoded` with the encoder's expansions gives bit-identical results to `add`, refuses an invalid expansion, and is refused on an index without the option; truncated documents are counted; an altered `sparse/tokenizer.json`, or a version-3 descriptor without its record, is `Corrupt` (tantivy's segment names are random, so byte-identical index files are not a property to test — identical results are). Plus `the_query_side_is_copied_byte_for_byte` in `crates/xtriever-dense/tests/sparse_oracle.rs`
- [X] T016 [US1] Add the contract's pipeline items (`contracts/sparse-option.md` §`xtriever-pipeline`) as stubs refusing with a "not implemented" error, run T014–T015 to show them failing, and **⛔ red checkpoint — the owner commits the failing tests**

### Implementation for User Story 1

- [X] T017 [US1] Add `SparseOption { scale, boost }` and `HybridConfig.sparse: Option<SparseOption>` (default `None` in `HybridConfig::new`) in `crates/xtriever-pipeline/src/types.rs`, and `SparseRecord { scale, boost, field, encoder, tokenizer_sha256, table_sha256 }` beside it; validate `scale` in `1..=sparse::MAX_SCALE` and `boost` finite and > 0; `add_encoded` refuses an expansion that fails `Expansion::validate` (by way of `field_text`)
- [X] T018 [US1] In `crates/xtriever-pipeline/src/descriptor.rs`: `sparse: Option<SparseRecord>`, written as format version 3 exactly when present and 2 otherwise; the reader accepts `(2, None)` and `(3, Some)` and refuses the rest by name; the id map and passage store keep their own versions (`crates/xtriever-pipeline/src/ids.rs`, `passages.rs` untouched)
- [X] T019 [US1] In `crates/xtriever-pipeline/src/index.rs` (and `SparseEncoder::write_query_side` in the dense crate): `create_sparse` refuses a user `_sparse` field, appends `_sparse` to the schema (text, `standard`, indexed, not stored, the option's boost), verifies and copies the encoder's `tokenizer.json` and `idf.json` into `<dir>/sparse/` (the latter as `query-table.json`) and records their hashes; `open*` verifies them and builds the `SparseQuery`; `set_sparse_encoder`, `sparse()`, a count of truncated documents; `add` encodes each document's dense passage and writes `field_text` into `_sparse`, refusing without an encoder; `add_embedded` refuses on a sparse index; `add_encoded` takes caller-supplied vectors and expansions
- [X] T020 [US1] In `crates/xtriever-pipeline/src/search.rs`: `search` builds research D7's query for a sparse index and keeps `Match(None, text)` otherwise; `search_lexical` unchanged, its documentation saying it adds no expansion; the `explain` documentation saying the lexical score includes `_sparse`
- [X] T021 [P] [US1] Write `docs/adr/0016-sparse-expansion-field.md`: the option, descriptor version 3 for sparse indexes only and why an unversioned field would be silently ignored, the reserved `_sparse` field and its `s<id>` terms, the query side stored in the index, what an older engine does, what was rejected (a core trait, a separate list with its own scorer — Feature 016's design)
- [X] T022 [US1] Update `crates/xtriever-pipeline/src/lib.rs` documentation (format version 3 for sparse indexes, the option) and run T014–T015 green

**Checkpoint**: a sparse index builds, searches and refuses correctly; nothing changes without the option.

---

## Phase 5: User Story 3 — The option's quality is measured and recorded (P1) — PR B, measurement in PR C

PR B merged with the harness (T023–T025) and without the measurement; by the owner's decision
the results (T026–T028) land in PR C, and T029's gate was run on the merged code on `main`
(2026-09-23: all steps pass but the tracked wasm32 failure).

**Goal**: the harness measures the option on three datasets through fusion and re-ranking, with
its size and throughput, and confirms the option-off baselines.

**Independent Test**: the six option-on reports under `specs/027-sparse-lexical-expansion/runs/`
and the re-run baselines, checked against SC-001–SC-003 and SC-005.

### Tests for User Story 3 (written first, committed with PR B's red checkpoint) ⚠️

- [X] T023 [P] [US3] Write `crates/xtriever-eval/tests/sparse_config.rs` (model-free): `hybrid-sparse-v1` is `hybrid-baseline-v2` plus `SparseOption { scale: 10, boost: 1.0 }` and `hybrid-sparse-rerank-v1` is `hybrid-rerank-v3` plus the same; the sparse cache's key compares the encoder identity, the corpus SHA-256 and the document count, and any difference is a miss; `weights.bin` round-trips a synthetic set of expansions exactly

### Implementation for User Story 3

- [X] T024 [US3] In `crates/xtriever-eval/src/run.rs`: the two configurations, `SparseCacheKey`, and the cache's read and write (`<cache>/<dataset>/{key.json, weights.bin}`: per document in corpus order a `u32` count then `(u32 id, f32 weight)` pairs)
- [X] T025 [US3] In `crates/xtriever-eval/examples/beir.rs`: `--sparse-encoder-dir` and `--sparse-cache-dir`; for a sparse configuration, load or encode the weights, create the index with the option, and build it with `add_encoded`. **As built**: the report's `stage` block records the settings and the encoder identity only; the throughput, the truncation count and the lexical index's bytes go to stderr — a report stays byte-identical across runs (spec 004 SC-006) — and are kept in `runs/measure-027.log`, the source of `runs/sizes.json`
- [X] T026 [US3] Cross-check on SciFact: `hybrid-sparse-rerank-v1` against the spike's `rerank_runs.rs` over the same Rust-encoded weights exported to its format — equal lists, since both use the pipeline's fusion and interpolation (a mismatch is a harness bug, fixed before any number is read)
- [X] T027 [US3] Run quickstart §3: `hybrid-sparse-v1` and `hybrid-sparse-rerank-v1` on SciFact, NFCorpus and FiQA (FiQA's first encode is overnight), writing `specs/027-sparse-lexical-expansion/runs/{hybrid-sparse-v1,hybrid-sparse-rerank-v1}.<dataset>.json`; re-run `hybrid-baseline-v2` and `hybrid-rerank-v3` to confirm them unchanged (SC-003). **⛔ If FiQA gains less than 0.010 nDCG@10 (SC-001), or any dataset falls more than 0.005 on either metric (SC-002), stop and report — never adjust the setting or the bound**
- [X] T028 [US3] Record each dataset's lexical index bytes with and without the option (SC-005: at most 4×) and the encoding throughput in `specs/027-sparse-lexical-expansion/runs/sizes.json`
- [ ] T029 [US3] PR B gate: the full local gate, the pipeline's model-backed tests, the three-dataset table; write `specs/027-sparse-lexical-expansion/pr-description-b.md` with the nDCG@10 and Recall@100 deltas per dataset and the sizes. **⛔ Checkpoint B — the owner reviews ADR-0016 (Principle V's gate clears on its acceptance), commits, pushes and merges PR B**

**Checkpoint**: the option works end to end in the engine and its cost and benefit are on record.

---

## Phase 6: User Story 4 — Bindings build and search sparse indexes (P2) — PR C

**Goal**: FFI, Python, Swift and Kotlin open and search sparse indexes; Python and the command
line build them; the packagers never carry the encoder.

**Independent Test**: a sparse index built through Python and through the command line
searches with the engine's own results and reports the option in its information.

### Tests for User Story 4 (written first, committed failing) ⚠️

- [X] T030 [P] [US4] Write `crates/xtriever-ffi/tests/sparse.rs` (`#[ignore = "needs the sparse encoder and both models"]`): `IndexHandle::create` with `IndexConfig.sparse` builds a sparse index; `info()` reports `sparse` (scale, boost, encoder) and format version 3; `IndexHandle::open` with no new argument searches it with the same hits and scores as the pipeline's own `search`; an index without the option reports format version 2 and no `sparse`
- [X] T031 [P] [US4] Write `python/tests/test_sparse.py` (marked `models`): create with the option, add, commit, search, `info().sparse`; results equal a second open's
- [X] T032 [P] [US4] Write `crates/xtriever-ffi/tests/packagers.rs` (model-free): neither `scripts/build-ios-package.sh` nor `scripts/build-android-package.sh` names `manifest-sparse-doc-v3.json` or the encoder's directory (FR-012, FR-013)
- [X] T033 [US4] Add the FFI records as stubs, run T030–T032 to show them failing, **⛔ red checkpoint — the owner commits the failing tests**

### Implementation for User Story 4

- [X] T034 [US4] In `crates/xtriever-ffi/src/ffi/types.rs`: `SparseOptionConfig { encoder_dir, scale, boost }`, `IndexConfig.sparse: Option<SparseOptionConfig>`, `SparseInfo { scale, boost, encoder }`, `IndexInfo.sparse: Option<SparseInfo>`, in `crates/xtriever-ffi/src/index.rs`: `create` loads the `SparseEncoder` and calls `create_sparse` when the option is set (`info` already reports the index's own descriptor version since PR B)
- [X] T035 [P] [US4] Python: rebuild the wheel, expose nothing hand-written beyond `sparse` in `python/src/xtriever/__init__.py`, which re-exports `IndexInfo`, and document the option in `python/README.md`
- [ ] T036 [P] [US4] Swift and Kotlin: the wrappers alias the generated types (`IndexInfo`, `IndexConfig`, `StageReport`), so nothing hand-written changes; `scripts/build-ios-package.sh` and `scripts/build-android-package.sh` regenerate the bindings with `sparse`, `SparseInfo`, `SparseOptionConfig` and `sparse_skipped`, and both packages build (checked in T038's gate; no separate red test — there is no behaviour of their own to test)
- [X] T037 [US4] Command line: `--sparse-encoder DIR`, `--sparse-scale`, `--sparse-boost` on `xtriever wiki build` in `crates/xtriever-cli/src/wiki/mod.rs` and `build.rs` (`sparse_option`, red tests committed with T033), creating the index with `create_sparse`, whose attached encoder expands each passage in `add_embedded` (PR C review: `add_embedded` expands on a sparse index, as `add` does); the build record (`crates/xtriever-cli/src/wiki/record.rs`) gains the encoder identity, the truncation count and the encoding throughput; run quickstart §4
- [ ] T038 [US4] PR C gate: the full local gate, the FFI and Python model-backed suites, the Kotlin library tests on the emulator; write `specs/027-sparse-lexical-expansion/pr-description-c.md`. **⛔ Checkpoint C — the owner commits, pushes and merges PR C**

**Checkpoint**: applications can use the option.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T039 Write `specs/027-sparse-lexical-expansion/report.md`: the verdict, the three pull requests, the three-dataset table beside the spike's, the oracle's largest difference, sizes and throughput, and "deliberately not done" (not the default, not applied to the Wikipedia artefact, no speed claim, no four-bit or eight-bit encoder)
- [X] T040 [P] Mention the option in `crates/xtriever-pipeline/src/lib.rs`'s overview, with the rule of when to switch it on (corpora without titles and with vocabulary mismatch, searched with the re-ranker), and in `python/README.md`. **As built**: the repository has no top-level `README.md`, so the crate docs and the Python README carry it

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1** → **Phase 3** (US2 needs the encoder on disk, the reference environment and the pins)
- **Phase 3** → **Phase 4** (the pipeline holds the encoder and query types)
- **Phase 4** → **Phase 5** (the harness builds sparse indexes)
- **Phase 5** → **Phase 6** (the surfaces ship an option whose measurement is on record)
- **Phase 6** → **Phase 7**

### User Story Dependencies

- **US2** stands alone. **US1** needs US2's module. **US3** needs US1. **US4** needs US1 (and,
  by the owner's clarification, lands after US3's measurement).

### Within Each User Story

- Tests first, committed failing at the story's red checkpoint; then the types, the behaviour,
  the documentation; the story's gate last.

### Parallel Opportunities

- T002 ∥ T003 (Phase 1); T005 ∥ T006 (US2 tests); T011 alongside T009–T010 (different items in
  the same file: sequence the commits, not the thinking); T014 ∥ T015; T021 alongside T017–T020;
  T030 ∥ T031 ∥ T032; T035 ∥ T036.
- The FiQA encode (T027) is machine time: PR C's tests (T030–T032) can be written while it runs,
  but not committed before PR B merges.

---

## Parallel Example: User Story 2

```text
T005 crates/xtriever-dense/tests/sparse_oracle.rs    — the oracle comparison, model-backed
T006 crates/xtriever-dense/tests/sparse_rules.rs     — the rules and refusals, model-free
```

## Parallel Example: User Story 1

```text
T014 unit tests in descriptor.rs and search.rs       — format and query shape, model-free
T015 crates/xtriever-pipeline/tests/sparse_index.rs  — create/open/add/search, mostly model-backed
T021 docs/adr/0016-sparse-expansion-field.md         — while T017–T020 are written
```

---

## Implementation Strategy

### MVP First

PR A alone is a working, oracle-checked encoder with no effect on the engine: safe to merge,
and the base every later step builds on. The feature's value arrives with PR B, which is where
the option exists for the engine and its measurement decides whether the numbers hold.

### Incremental Delivery

1. PR A (Phases 1–3): encoder, query side, oracle → merge.
2. PR B (Phases 4–5): index, ADR-0016, measurement → review the ADR → merge.
3. PR C (Phase 6): surfaces → merge.
4. Phase 7: report.

### Stop Conditions

- The oracle fails at 1e-4 (T012): stop and report; the tolerance stays.
- SC-001 or SC-002 fails (T027): stop and report; the setting and the bound stay.
- A pull request heads past ~800 changed lines excluding fixtures and run records: stop and
  propose a split before continuing.

---

## Notes

- `#[ignore]`d tests need the encoder (268 MB) and run locally; CI runs the model-free tests
  only (FR-011).
- Commit after each checkpoint; the owner commits (never the agent).
- The spike (`crates/xtriever-eval/examples/splade_field.rs`, `rerank_runs.rs`) stays as the
  measured basis and cross-check until PR B's harness supersedes it; PR B removes
  `splade_field.rs` or records why it stays.
