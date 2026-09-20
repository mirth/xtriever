---

description: "Task list for Feature 026 — eight-bit precision end to end"
---

# Tasks: Eight-Bit Precision End to End

**Input**: Design documents from `/specs/026-eight-bit-precision/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md),
[data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: included and written first. Principle II and Agent Operating Rule 4 make them
mandatory: each pull request's tests are committed failing before its implementation.

**Organization**: by user story, in the spec's priority order. Two pull requests: **PR A** is
Phases 1–3 (the vectors), **PR B** is Phases 4–6 (the models, the artefacts, the footprint).
Each stays under Rule 3's ~800 changed lines; the artefact rebuild is machine time, not diff.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an unfinished task
- **[Story]**: the user story the task serves
- Every task names the file it touches

## Path Conventions

`crates/xtriever-dense/` holds the format and the embedder, `crates/xtriever-rerank/` the
cross-encoder, `reference/` the fixtures and manifests, `docs/adr/` the decision record. No new
crate.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: the decision record that unblocks the constitution gate, and the artefacts on disk.

- [X] T001 Write `docs/adr/0015-eight-bit-vectors-and-models.md`: the dense format changes to version 3 (rows become `id u32 · norm f32 · scale f32 · codes dim×i8`, 396 bytes at dimension 384) and both loaders accept a second artefact format; the measured cost (0.0006 nDCG@10 on SciFact and NFCorpus, Recall@100 unchanged, 99.5 % candidate agreement at depth 100); the owner's decision to drop the float vectors rather than rescore; what is rejected (a global scale, four bits, an accelerator); and the review trigger. **This clears the two FAIL rows in [plan.md](plan.md); nothing else in this feature starts until it is accepted.**
- [X] T002 Add the eight-bit artefacts to `reference/models/manifest-q8.json`: `leliuga/all-MiniLM-L6-v2-GGUF` at its current revision with `all-MiniLM-L6-v2.Q8_0.gguf` (25.0 MB) and `cstr/ms-marco-MiniLM-L-6-v2-GGUF` with `ms-marco-MiniLM-L-6-v2-q8_0.gguf` (24.7 MB), each pinned by repository, revision and sha256 exactly as `reference/models/manifest.json` pins the float weights
- [X] T003 `scripts/fetch-model.sh` needed **no change**: it already takes `--manifest`, reads `local_dir`, and verifies size and sha256 per file. Both artefacts fetch and verify into `reference/models/all-MiniLM-L6-v2-q8/` and `reference/models/ms-marco-MiniLM-L-6-v2-q8/`, leaving the float directories (and their tokenizers) untouched — research D6

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the quantisation itself, in one place, before anything stores or scores with it.

**⚠️ CRITICAL**: no user story work begins until this phase is complete.

- [X] T004 Write the failing test first in `crates/xtriever-dense/src/quantise.rs`: symmetric quantisation round-trips within the scheme's own bound, a zero vector yields scale 1.0 and zero codes rather than a division by zero, no code is ever −128, and `scale` is strictly positive (data-model "Invariants")
- [X] T005 Implement `crates/xtriever-dense/src/quantise.rs`: `scale = max|component| / 127`, `code = round(component / scale)` clamped to [−127, 127], and the recovery the scan uses; documented as approximate, never exact
- [X] T006 [P] Add a property test in `crates/xtriever-dense/tests/quantise_prop.rs`: over random vectors, the recovered vector's cosine with the original stays above the bound the study measured, and quantising twice is idempotent

**Checkpoint**: one quantisation scheme exists, tested, before any file or kernel uses it.

---

## Phase 3: User Story 1 — The stored vectors take a quarter of the space (Priority: P1) 🎯 MVP

**Goal**: dense format version 3, with the integer kernel and the refusals.

**Independent Test**: build the fixture index and a BEIR index, compare the rankings with the
float index's by FR-009's thresholds, and compare the file sizes.

### Tests for User Story 1 (written first, committed failing) ⚠️

- [X] T007 [US1] Write `crates/xtriever-dense/tests/format_v3.rs`: a committed index has 396-byte rows at dimension 384, the manifest says `format_version` 3 and names the scheme, and the file is under a third of the float file for the same documents (contracts/dense-format-v3.md)
- [X] T008 [P] [US1] Write `crates/xtriever-dense/tests/format_refusals.rs`: a version-1 file, a version-2 file, an unknown scheme, a zero scale and a row count inconsistent with the file length are each refused by name with the rebuild instruction
- [ ] T009 [P] [US1] ~~Write `crates/xtriever-dense/tests/agreement.rs`~~ **Re-scoped in review round 1 (PR A)**: the float index no longer exists in the crate, and on synthetic vectors the agreement sits at the bound (98.7–99.2 % at depth 100 on uniform and Gaussian 384-dim data) where real embeddings measured 99.5 %, so a test on random vectors would test random vectors, not SC-001. The determinism half lives in `tests/format_v3.rs`; the agreement is measured on real corpora by `reference/int8_vectors_study.py` and re-measured by T024's three-dataset gate in PR B, whose numbers the report quotes

### Implementation for User Story 1

- [X] T010 [US1] Change the row layout in `crates/xtriever-dense/src/index/mod.rs` and `src/index/bytes.rs`: `id u32 · norm f32 · scale f32 · codes dim×i8`, fixed width, little-endian; the manifest, tombstones, generations and commit protocol from Feature 024 are untouched
- [X] T011 [US1] Write `format_version` 3 and the scheme name into the manifest header in `crates/xtriever-dense/src/index/manifest.rs`, and refuse versions 1 and 2 by name with the rebuild instruction
- [X] T012 [US1] Implement the scan in `crates/xtriever-dense/src/index/scan.rs`: quantise the query with the same scheme, accumulate in `i32`, dequantise once per row by multiplying the two scales (research D4); `i32` cannot overflow for 384 terms of at most 127 × 127, and the code says so
- [X] T013 [US1] Update the compaction and append paths in `crates/xtriever-dense/src/index/mod.rs` for the new row width, keeping every Feature 024 guarantee: a crash at any byte boundary leaves the previous state or the new one
- [X] T014 [P] [US1] Write `reference/gen_026_fixtures.py`: recompute the format-3 oracle in Python — quantisation, integer scoring and the ranking — and regenerate `crates/xtriever-dense/tests/support/` goldens; `--check` reports zero mismatches
- [X] T015 [P] [US1] Update `crates/xtriever-dense/benches/scan.rs` for the new row shape and record the result in the report; this feature claims size and quality, not speed (research D4)
- [X] T015a [US1] Review round 1 (PR A), nine findings, all applied: the generator's rounding (half away from zero, in `f32`) and scale floor match the crate; a denormal peak no longer stores a zero scale; the magic is `XTDENSE3` and version 2 is refused with the rebuild instruction by magic and by header; the header names the scheme and `tests/format_v3.rs` / `tests/format_refusals.rs` exist (T007, T008, T011 had been ticked without them); the scan decides the metric once, not per row; the stored norm is the recovered row's, so cosine is bounded; the `vector()`, crate and bench docs say what format 3 does; the reference quantiser lives once in `tests/support`; the retired v1→v2 converter and its tests are deleted. A dimension bound (132,104 after round 2; 133,144 in round 1, which forgot a −128 byte on disk) was added while there, because the `i32` accumulator would otherwise wrap silently past it
- [X] T015b [US1] Review round 2 (PR A), eight findings, all applied: the evaluation cache keeps the embedder's floats in `vectors.f32.bin` (cache key version 3), `export-vectors` and the hybrid baseline read them rather than the index's eight-bit recovery, so the embedder check keeps its 1e-3 tolerance and the hybrid index quantises once; `reference/int8_vectors_study.py` reads those floats and the engine's format-3 rows, checks the rows are the scheme byte for byte, and measures SC-001's agreement on the real corpus; the dimension bound counts a −128 byte on disk (132,104); the 40-document fixture index and goldens are re-minted so the model-backed suites are green between PR A and PR B; one quantisation per vector at `add`, carried in the pending set as codes, and one zero-norm check; the oracle's expectations are written by `reference/` (`--write-oracle`) and CI checks both the goldens and the oracle; `FlatIndex::open` docs and the Android README say version 3
- [X] T015c [US1] Review round 3 (PR A), nine findings, all applied: the oracle checker's error path names the file it checks; a cache hit requires a complete float sidecar, so the rebuild advice works; the study rounds half away from zero in `f64`, as `f32::round` and the generator do; `vector(id)` returns `Result<Option<_>>` and refuses a damaged row exactly as the scan does; both fixture manifests' `rescored_by_sha256` are asserted by the fixture-validity tests, like `generator_sha256`; the scan decodes scale and norm once per row and hands them to the scorer; the hybrid baseline no longer materialises a row per document as an existence check; `validate_query` keeps one zero-norm check; and NFCorpus and FiQA were evaluated in this pull request, as Rule 5 requires for a ranking-affecting change to `dense` (dense-baseline-v1, hybrid-baseline-v2, hybrid-rerank-v3, deltas in the PR description), with the study's agreement on all three datasets
- [X] T015d [US1] Review round 4 (PR A), six findings, all applied: the scheme lives once in `reference/dense_format3.py`, `gen_004_fixtures.py` and `gen_005_fixtures.py` import it and mint format-3 goldens themselves (run fresh, both reproduce the committed goldens byte for byte), the manifests pin `scheme_sha256` beside `generator_sha256` and the fixture-validity tests assert both, and `gen_026_fixtures.py` is the stdlib checker; `gen_024_fixtures.py` is deleted and ADR-0013 says why; `int8_reranker_study.py` reads the float sidecar; `vector()`'s doc, `assert_recovered` and the persistence test state the floored bound; the scan's least norm is hoisted and the Euclidean arm recovers from the checked scale; the eval cache is `<dataset>/{cache.json, vectors.f32.bin, index/}`, so the sidecar never sits in the dense crate's directory
- [ ] T016 [US1] PR A gate: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo nextest run --workspace`, `cargo deny check`, the three cross-target checks, `cargo bench -p xtriever-dense --bench scan`, `python3 reference/gen_026_fixtures.py --check`; write `specs/026-eight-bit-precision/pr-description-a.md`. **⛔ Checkpoint C1 — the owner commits**, pushes, opens and merges PR A

**Checkpoint**: an index built by this engine is a quarter the size and ranks within the stated bound.

---

## Phase 4: User Story 2 — Both models ship at eight bits (Priority: P1)

**Goal**: both stages load the pinned eight-bit artefacts, with float still loadable.

**Independent Test**: the three BEIR datasets through the evaluation harness with both eight-bit
models, every delta inside 0.005 of its committed baseline.

### Tests for User Story 2 (written first, committed failing) ⚠️

- [ ] T017 [US2] Write `crates/xtriever-dense/tests/quantised_embedder.rs`: the pinned artefact loads, its fingerprint names the eight-bit file, a flipped byte is refused by checksum, and a file whose declared architecture or shape disagrees with the stage is refused by name (contracts/model-artefacts.md)
- [ ] T018 [P] [US2] Write `crates/xtriever-rerank/tests/quantised_scorer.rs`: the same, plus the classification-head check — an artefact without `classifier.weight` and `classifier.bias` is refused, because it would otherwise produce embeddings that look like relevance scores (FR-008)
- [ ] T019 [P] [US2] Write `crates/xtriever-dense/tests/fingerprint_mismatch.rs`: an index built with the float embedder refuses to open with the eight-bit one and the reverse, both as hard errors at open (FR-007)

### Implementation for User Story 2

- [ ] T020 [US2] Read the artefact in `crates/xtriever-dense/src/embedder.rs` with `candle_core::quantized::gguf_file::Content::read`, mapping its tensors to the existing forward pass: `QMatMul::from_qtensor` for the 37 weight matrices, `QTensor::dequantize` for the float pieces (research D5)
- [ ] T021 [US2] Relax the pinned assertions in `crates/xtriever-dense/src/model.rs` from "these exact bytes" to "the artefact the manifest names", keeping the checksum check, and extend the fingerprint to name the artefact (FR-006); the float path keeps working (FR-012)
- [ ] T022 [US2] Do the same in `crates/xtriever-rerank/src/scorer.rs` for the 38 matrices, plus the classification head, keeping the head in float as the artefact ships it
- [ ] T023 [US2] Make `crates/xtriever-dense/src/embedder.rs` and `crates/xtriever-rerank/src/scorer.rs` choose the artefact by what the manifest names rather than by a compile-time constant, so an installation picks its precision without a code change (FR-012)
- [ ] T024 [US2] Rebuild every evaluation cache under `target/xt-dense-cache/` and run the quality gate: for each of SciFact, NFCorpus and FiQA, the dense and hybrid-rerank configurations through `cargo run --release -p xtriever-eval --example beir`, comparing with the baselines committed under `specs/*/baselines/`. **⛔ No dataset may fall more than 0.005 below its baseline on nDCG@10 or Recall@100 (FR-009); a larger drop stops the feature and is reported, never answered by moving the threshold.** About two hours, nearly all of it FiQA

**Checkpoint**: both models are eight-bit and the quality gate has passed on all three datasets.

---

## Phase 5: User Story 3 — The device footprint falls below the ceiling (Priority: P2)

**Goal**: regenerate every artefact once, and record what the footprint became.

**Independent Test**: the host measurement over the rebuilt corpus reports parity against its new
goldens and a peak resident size below 600 MB.

- [ ] T025 [US3] Rebuild the Wikipedia corpus with `cargo run --release -p xtriever-cli -- wiki build --out target/xt-wiki` (hours) and its goldens with `wiki expected`, replacing the format-2 artefact
- [ ] T026 [P] [US3] Rebuild the 40-document fixture index and its goldens through `scripts/build-ios-package.sh --with-fixtures`, which every platform's parity test depends on
- [ ] T027 [P] [US3] Rebuild the demo corpus slices with `apps/python-wiki-demo/.venv/bin/wikidemo build --limit 2000 --out target/xt-wiki-slice-py` and the Rust slice, and regenerate the Android demo's measurement goldens
- [ ] T027a [US3] Teach the packagers the eight-bit artefacts (FR-005, FR-010; Copilot on PR A): `scripts/build-ios-package.sh --with-models` and `scripts/build-android-package.sh` stage the two pinned GGUF files from `reference/models/manifest-q8.json` and `manifest-rerank-q8.json` beside the `config.json` and `tokenizer.json` they borrow from the float directories (`tokenizer_from`), and the iOS and Android loaders open them; until then a packaged app ships float weights whatever the index was built with
- [ ] T028 [US3] Record the measurement in `specs/026-eight-bit-precision/runs/`: `wikidemo measure` over the rebuilt corpus — parity against the new goldens, and the peak resident size against the 600 MB ceiling and the previous record (SC-005, FR-011)
- [ ] T029 [P] [US3] Re-run each demonstration's own checks against the rebuilt artefacts: `apps/python-wiki-demo/tests`, `android/xtriever` and `apps/android-wiki-demo` on the emulator, and `apps/ios-wiki-demo` on the simulator

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T030 [P] Update the format documentation in `crates/xtriever-dense/src/lib.rs` and the artefact tree in `specs/008-wiki-corpus/contracts/artefact.md` for format 3
- [ ] T031 [P] Update `android/xtriever/README.md`, `apps/python-wiki-demo/README.md` and `apps/ios-wiki-demo/README.md` where they name the index size, the model sizes or the format version
- [ ] T032 Write `specs/026-eight-bit-precision/report.md`: the verdict, both pull requests, the evaluation table for all three datasets, the footprint before and after, the benchmark, and "deliberately not done" — no rescoring pass, no four-bit, no accelerator, no arbitrary user-supplied models (the next feature)
- [ ] T033 PR B gate: the full local gate, the three-dataset evaluation table, the measurement record, `grep -rn "$(hostname -s)\|$USER" specs/026-eight-bit-precision reference/models` finding nothing; write `specs/026-eight-bit-precision/pr-description-b.md`. **⛔ Checkpoint C2 — the owner commits**, pushes and opens PR B

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001, the decision record, gates everything — two constitution rows fail
  until it is accepted.
- **Foundational (Phase 2)**: needs Phase 1; blocks both stories, because both store and score
  with the same scheme.
- **User Story 1 (Phase 3)**: needs Phase 2. Ends at Checkpoint C1, which is PR A.
- **User Story 2 (Phase 4)**: needs User Story 1 merged, because the evaluation gate measures
  both halves together against the committed baselines.
- **User Story 3 (Phase 5)**: needs User Story 2, since the artefacts are embedded with the
  eight-bit models.
- **Polish (Phase 6)**: needs Phase 5.

### Within Each User Story

Tests first and failing, then the layout or the loader, then the paths that depend on it.

### Parallel Opportunities

- Phase 2: T006 alongside T005 once the scheme exists.
- Phase 3: T008 and T009 together; T014 and T015 once the layout lands.
- Phase 4: T018 and T019 together; T020 and T022 are different crates and can go together.
- Phase 5: T026, T027 and T029 are separate artefacts; only T025 is the long pole.
- Phase 6: T030 and T031 together.

### Sequential Spine

T001 → T004 → T005 → T007 (red) → T010 → T012 → T016 (**C1**, PR A merged) → T017 (red) →
T020 → T021 → T024 (**the quality gate**) → T025 → T028 → T032 → T033 (**C2**).

---

## Implementation Strategy

### MVP (User Story 1 only)

Phases 1 to 3 give a quarter-sized dense file whose ranking is measured against the float one.
That is shippable on its own: the models stay float, no artefact outside the dense file changes
shape, and the footprint already falls by about 490 MB on the shipped corpus.

### Incremental Delivery

PR A: the format, the kernel, the oracle, the decision record. PR B: the artefacts, the quality
gate, the rebuild and the measurement. The gate in T024 is the point of no return — everything
after it assumes the eight-bit models are acceptable.

---

## Notes

- **The threshold is set before the numbers are known** (FR-009, 0.005 on either metric, per
  dataset), following Feature 022's decision rule. If T024 fails it, the answer is to report and
  stop, not to widen it (Rule 6).
- **Nothing is quantised locally.** The artefacts come from the owner as pinned files; the
  studies under `reference/` were simulations that justified the attempt and are not the gate.
- **The tokenizer stays in the float model directory**, which no task deletes.
- **No continuous-integration change**: the three-dataset evaluation is local, as the standing
  resource rule requires.
- The owner commits, pushes and merges at every ⛔ checkpoint.
