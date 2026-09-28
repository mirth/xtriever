---

description: "Task list for Feature 028 — accelerated inference spike"
---

# Tasks: Accelerated Inference Spike

**Input**: Design documents from `/specs/028-accelerated-inference-spike/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md),
[data-model.md](data-model.md), [contracts/spike-surface.md](contracts/spike-surface.md),
[quickstart.md](quickstart.md)

**Tests**: included and written first (Principle II, Agent Operating Rule 4). The spike's own
code — the compute-path labels, the batched re-ranker, the hits digest — gets red tests before it
exists; the existing golden tests are the oracle every path is measured against.

**Organization**: every story measures through the same plumbing (the features, the labels, the
digest, the packager flag, the host script), so that is Phase 2 and blocks all three. Then one
phase per story: US1 latency, US2 memory / parity / repeatability / quality, US3 build throughput.
One pull request is planned (about 500 lines of code, plan Rule 3); if the code passes 800 lines,
**PR A** is Phases 1–2 and **PR B** is Phases 3–6.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: different files, no dependency on an unfinished task
- **[Story]**: the user story the task serves
- Every task names the file it touches

## Path Conventions

The spike's code lives in `crates/xtriever-dense/src/spike.rs` and
`crates/xtriever-rerank/src/spike.rs` (plus the two `Device::Cpu` sites in each crate's loader),
the three crates' `Cargo.toml`, the three harnesses, `scripts/build-ios-package.sh` and
`scripts/spike-028-host.sh`. Records go to `specs/028-accelerated-inference-spike/runs/`. No
`xtriever-core`, `deny.toml`, format or identity change; nothing on by default.

---

## Phase 1: Setup

**Purpose**: the inputs every measurement needs, confirmed present.

- [X] T001 Confirm the prerequisites of `quickstart.md` §0 — both eight-bit models verified by `scripts/fetch-model.sh`, `target/xt-wiki/index` with `target/xt-wiki/expected.json`, SciFact / NFCorpus / FiQA verified by `scripts/fetch-beir.sh`, `apps/python-wiki-demo/.venv`, the iPhone 16e listed as available by `xcrun devicectl list devices` — and create `specs/028-accelerated-inference-spike/runs/README.md` stating the record naming (`<device>-<computePath>-<batch|single>-<timestamp>.json`, the variant also inside each record) and that device and team identifiers never appear in a record

---

## Phase 2: Foundational — the plumbing every story measures through

**Purpose**: the compile-time paths (research D3–D5), the batched re-ranker (D7), the labels and
the digest in the three harnesses (D8), the packager flag (D9) and the host script (D10).

**⚠️ No story's measurement starts before this phase's checkpoint.**

### Tests (written first, committed failing) ⚠️

- [X] T002 [P] Write `crates/xtriever-dense/tests/spike_compute_path.rs`: `xtriever_dense::spike::COMPUTE_PATH` equals `"cpu"` with no spike feature, `"accelerate"` under `spike-accelerate`, `"metal"` under `spike-metal` (one `#[test]` per `cfg`); and, `#[ignore]` (needs the models), `MiniLmEmbedder::load` on `reference/models/all-MiniLM-L6-v2-q8` succeeds and its `fingerprint()` equals today's `FINGERPRINT_Q8` under every feature (research D6: "identity is unchanged under every feature")
- [X] T003 [P] Write `crates/xtriever-rerank/tests/spike_compute_path.rs`: the same for `xtriever_rerank::spike::COMPUTE_PATH` and `MiniLmCrossEncoder::load` on `reference/models/ms-marco-MiniLM-L-6-v2-q8`, its `model_id()` equal to today's `MODEL_ID_Q8`
- [X] T004 [P] Write `crates/xtriever-rerank/tests/spike_batch.rs`, compiled only under `spike-batch`, `#[ignore]` (needs the models): for the query–passage pairs of the re-rank golden fixture that `crates/xtriever-rerank/tests/score_golden.rs` reads, `Reranker::rerank` with an unlimited budget returns `Some` for every pair, in the input order, each within 1e-3 of `MiniLmCrossEncoder::score` on the same pair; the largest difference is printed for the report
- [X] T005 [P] Write `apps/python-wiki-demo/tests/test_measure_spike.py`: (a) the hits digest of the data model ("SHA-256 over every measured response, in order: for each query, each depth, each hit in rank order — the external id, the fused score's bits, the re-rank score's bits or `-`") over a fixed two-query, two-depth response set equals a pinned 64-hex value; (b) `wikidemo measure` accepts `--compute-path {cpu,accelerate,metal}` and `--rerank-batch`, defaulting to `cpu` and `false`, and writes `computePath`, `rerankBatch`, `hitsDigest` into the record
- [X] T006 [P] Write `swift/Xtriever/Tests/XtrieverTests/SpikeDigestTests.swift` and `apps/ios-wiki-demo/Tests/SpikeDigestTests.swift`: each harness's digest function over the same fixed response set as T005 returns the same pinned hex (one digest definition in three languages); a missing `compute-path.json` reads as `"unknown"`, never a guess
- [X] T007 Add the stubs so everything compiles and fails for the right reason: the three features in `crates/xtriever-dense/Cargo.toml`, `crates/xtriever-rerank/Cargo.toml` (plus `spike-batch`) and `crates/xtriever-ffi/Cargo.toml` (forwarding only), `crates/xtriever-dense/src/spike.rs` and `crates/xtriever-rerank/src/spike.rs` with `COMPUTE_PATH = "unset"`, the batched `rerank` returning `Error::Model("spike-batch: not implemented")`, the Python and Swift digest functions returning an empty string and the flags accepted but not recorded; run T002–T006 to show them failing for those reasons. **⛔ Red checkpoint — the owner commits the failing tests and the stubs**

### Implementation

- [X] T008 [P] Implement `crates/xtriever-dense/src/spike.rs` and wire it: `COMPUTE_PATH` by `cfg`; `compute_device() -> Result<Device>` returning `Device::Cpu` (no feature, `spike-accelerate`) or `Device::new_metal(0)` mapped to `Error::Model` naming `spike-metal` and candle's message (`spike-metal`; no fallback); a `compile_error!` naming both features when `spike-accelerate` and `spike-metal` are both on; the features in `Cargo.toml` enabling `candle-core/…`, `candle-nn/…`, `candle-transformers/…` (`accelerate` or `metal`); replace `Device::Cpu` at the two loader sites in `crates/xtriever-dense/src/embedder.rs` (float and eight-bit) with `spike::compute_device()?`; the sparse encoder (`sparse.rs`) untouched; module docs label it spike code (FR-012)
- [X] T009 [P] Implement `crates/xtriever-rerank/src/spike.rs` the same way and replace `Device::Cpu` at the two loader sites in `crates/xtriever-rerank/src/scorer.rs`
- [X] T010 Implement the batched re-ranker in `crates/xtriever-rerank/src/scorer.rs` under `spike-batch` (research D7): each pair tokenised exactly as `encode` does today, padded to the longest with `[PAD]` (id 0), token type 0 and attention mask 0 on padding; one `QuantisedBert::forward` (or the float encoder's) over the batch; pooler and classifier on each CLS row; the budget checked once before the batch; all scores or none; non-finite logits still `Error::Model` (FR-008 of Feature 006); the `Reranker` trait unchanged
- [X] T011 [P] Implement the Python harness additions in `apps/python-wiki-demo/wikidemo/measure.py` and `apps/python-wiki-demo/wikidemo/cli.py`: the two flags, the three record fields, the digest per the data model; the rest of the record unchanged
- [X] T012 [P] Implement the Swift harness additions: `HarnessResources.computePathFile` (the staged `XtrieverData/compute-path.json`, labelled spike) in `swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift`; `computePath`, `rerankBatch`, `hitsDigest` in the run records of `swift/Xtriever/Tests/XtrieverTests/DeviceMeasurementTests.swift` (all depths) and `apps/ios-wiki-demo/Tests/DemoMeasurementTests.swift` (fused then re-ranked)
- [X] T013 Implement `--spike-compute accelerate|metal` and `--spike-batch` in `scripts/build-ios-package.sh` (contracts §iOS packager): the matching `xtriever-ffi` features on the staticlib build, `XtrieverData/compute-path.json` staged on every build (`{"computePath":"cpu","rerankBatch":false}` without the flags), and the `OTHER_LDFLAGS` line printed for the chosen path (`-framework Accelerate`, or `-framework Metal -framework Foundation -framework CoreGraphics`); without the flags the framework is built exactly as today
- [X] T014 Write `scripts/spike-028-host.sh` (contracts §Host script): `measure` builds the wheel with `maturin build --release --features …` into `apps/python-wiki-demo/.venv` and runs `wikidemo measure` with matching labels, writing the record under `specs/028-accelerated-inference-spike/runs/`; `build` runs `beir run --dataset scifact --config hybrid-rerank-v3` with `--features xtriever-dense/<f>,xtriever-rerank/<f>` and a fresh `--cache-dir target/spike-028/<path>`, then warm, and writes the build measurement; `mixed` runs the CPU path against `target/spike-028/metal`; `quality` runs SciFact, NFCorpus and FiQA with `--out` into `runs/` and prints `beir delta` against `specs/026-eight-bit-precision/runs/hybrid-rerank-v3.<dataset>.json`
- [X] T015 Gate. **Default build (SC-004)**: the whole local gate of `CLAUDE.md`, including `scripts/check-demos.sh`, all passing as before. **Each spike feature**: `cargo clippy -p xtriever-dense -p xtriever-rerank --all-targets --features <f>` with warnings denied, `cargo check -p xtriever-ffi --target aarch64-apple-ios --features <f>`, T002–T006 green, and the existing golden tests `cargo nextest run -p xtriever-dense -p xtriever-rerank --release --features "<f> mmap" --run-ignored all -j 1`, whose outcome per feature goes to `specs/028-accelerated-inference-spike/runs/goldens-<path>.txt` — a golden failing under a feature is that path's result, never loosened. **⛔ Checkpoint — the owner commits the plumbing (PR A if split)**

**Checkpoint**: every variant can be built and labelled; the default build is unchanged.

---

## Phase 3: User Story 1 — Each compute path gets a latency on the phone and the laptop (P1) 🎯

**Goal**: the latency table — every path × batching × device cell filled or carrying its error.

**Independent Test**: `runs/` holds one host and one phone demo record per variant (or the
recorded error), and `report.md`'s latency table cites each.

- [X] T016 [P] [US1] Host: `scripts/spike-028-host.sh <path> measure` and `scripts/spike-028-host.sh <path> --batch measure` for `cpu`, `accelerate`, `metal`, the machine otherwise idle; records in `specs/028-accelerated-inference-spike/runs/`
- [X] T017 [US1] Phone, link and first run per path: `scripts/build-ios-package.sh --with-models --with-wiki --demo --app --spike-compute <path>` and the demo's build with the printed `OTHER_LDFLAGS`, for `accelerate` and `metal`; a link failure, a missing Metal kernel or a device that will not open is recorded with its exact error in `specs/028-accelerated-inference-spike/runs/errors.md` and ends that path on the phone (spec Edge Cases) — candle and the engine's defaults are not patched
- [X] T018 [US1] Phone, the demo's measured run (`DemoMeasurementTests`, `XtrieverWikiDemo-Measure`, Release) for each path that builds, single and batched (`--spike-batch`), thermal state `nominal` or repeated; `scripts/extract-device-run.py` into `specs/028-accelerated-inference-spike/runs/`; the CPU single run is the in-spike reference beside Feature 026's record
- [X] T019 [US1] Write the latency table in `specs/028-accelerated-inference-spike/report.md`: per device, path and batching — query embedding (fused-phase median), re-ranked phase median and max, total median, model load and warm-up — against today's record, with every cell citing its record or error

**Checkpoint**: US1 answers "is any path faster, and where".

---

## Phase 4: User Story 2 — Every path's cost in memory, numbers and repeatability is known (P1)

**Goal**: the cost table and, for any candidate, the three-dataset quality.

**Independent Test**: `report.md`'s cost table has peak memory, score and order differences and a
repeatability verdict per path and device, each from a committed record.

- [X] T020 [US2] Phone: the Swift package harness (`XtrieverHarnessApp-DefaultThreads`, `DeviceMeasurementTests`, `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia`) **twice** per path that builds, single, and once batched; records into `specs/028-accelerated-inference-spike/runs/`
- [X] T021 [P] [US2] Host: a second `scripts/spike-028-host.sh <path> measure` per variant, for the digest comparison
- [X] T022 [US2] Write the cost table in `specs/028-accelerated-inference-spike/report.md` from the records (data-model §Parity comparison): peak footprint against 600 MB, max dense and re-rank differences, lexical bit-identity, fused order at depth 0, order differences at depths 5 / 10 / 20, and `repeatable` = equal `hitsDigest` across the two runs; the batched runs' re-rank differences beside the single runs'
- [X] T023 [US2] Apply FR-010 conditions 1–3 and the tolerance part of 4; for each path that passes them, `scripts/spike-028-host.sh <path> quality` on SciFact, NFCorpus and FiQA (records `runs/<path>.hybrid-rerank-v3.<dataset>.json`) and the deltas into `report.md`. **⛔ A path whose nDCG@10 moves more than 0.005 on any dataset is no-go: report it — the threshold does not move**

**Checkpoint**: US2 says which faster paths are also safe.

---

## Phase 5: User Story 3 — Index building throughput on the laptop is measured (P2)

**Goal**: the build table and the mixed case.

**Independent Test**: `report.md`'s build table has passages per second per path, the Wikipedia
projection and the mixed case's nDCG@10.

- [X] T024 [P] [US3] `scripts/spike-028-host.sh <path> build` for `cpu`, `accelerate`, `metal` (fresh cache each, then warm); build measurements into `specs/028-accelerated-inference-spike/runs/build-<path>.json`
- [X] T025 [US3] `scripts/spike-028-host.sh cpu mixed`: the CPU path searching the Metal-built SciFact cache; its report into `specs/028-accelerated-inference-spike/runs/mixed-scifact.json`
- [X] T026 [US3] Write the build table in `specs/028-accelerated-inference-spike/report.md`: passages per second, wall time, threads, `projectedWikipediaHours` against today's 12.4 h, and the mixed case's nDCG@10 against the all-CPU SciFact record

**Checkpoint**: US3 says whether the build gets faster, and whether a GPU-built index serves CPU queries.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T027 Write the verdict in `specs/028-accelerated-inference-spike/report.md`: per path and device, FR-010's four conditions with their numbers, `go` or `no-go` naming the failing condition, and the follow-up a `go` implies (the path, the CPU fallback of Principle VI, the identity decision of research D6, the `criterion` bench, removing or promoting each `spike` item of FR-012) — one paragraph a follow-up spec can adopt verbatim (SC-003); findings in the house style
- [X] T028 Grep `specs/028-accelerated-inference-spike/` and the whole working tree for the device UDID and the team id (none may appear); re-run the default-build gate (SC-004); write `specs/028-accelerated-inference-spike/pr-description.md` with the latency, cost and build tables, the verdict and, for any recommended path, the nDCG@10 / Recall@100 deltas. **⛔ Final checkpoint — the owner commits, pushes and merges**

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (1)** → **Foundational (2)** → **US1 (3)**, **US2 (4)**, **US3 (5)** → **Polish (6)**
- The red checkpoint (T007) precedes all implementation; the plumbing checkpoint (T015) precedes
  every measurement.

### User Story Dependencies

- **US1** needs only Phase 2.
- **US2** needs Phase 2; its quality step (T023) needs US1's latency (FR-010 condition 1) and
  its own cost table.
- **US3** needs only Phase 2 and can run on the laptop while the phone runs US1 / US2.

### Within Each Story

Records before tables; tables before the verdict; the CPU reference before the other paths on the
same device.

### Parallel Opportunities

- T002–T006: five test files in five places.
- T008 / T009 / T011 / T012: four different crates or harnesses.
- Host work (T016, T021, T024–T025) runs beside phone work (T017–T018, T020) — different
  machines. Two host measurements never run at once (they would time each other).

---

## Parallel Example: Foundational tests

```bash
Task: "T002 spike_compute_path.rs in xtriever-dense"
Task: "T003 spike_compute_path.rs in xtriever-rerank"
Task: "T004 spike_batch.rs in xtriever-rerank"
Task: "T005 test_measure_spike.py in the Python demo"
Task: "T006 SpikeDigestTests.swift in the package and the demo"
```

## Parallel Example: User Stories 1 and 3

```bash
Task: "T018 phone demo runs, path by path"        # the iPhone
Task: "T024 host build throughput, path by path"  # the laptop, meanwhile
```

---

## Implementation Strategy

### MVP First

Phases 1–2, then US1 on the phone's re-ranker alone (T017–T018): if no path cuts the re-ranked
phase by 30% and the host build is not doubled, FR-010 condition 1 already says no-go for every
path — the rest of US2 is still recorded for the report (memory, parity, repeatability), and the
quality runs (T023) are not needed.

### Incremental Delivery

1. Plumbing + red tests → green, default build untouched (T001–T015)
2. Latency (US1) → the question answered
3. Costs (US2) → the answer made safe, or ruled out
4. Build (US3) → the host question
5. Verdict (T027–T028)

### Stop Conditions

- A path that does not link, lacks a kernel or cannot open its device: record the error, end that
  path on that device (T017). Never patch candle or change a default to get past it.
- A golden test failing under a feature (T015), a footprint over 600 MB, a digest mismatch, an
  nDCG@10 move over 0.005: that is the path's result — record it; no tolerance or threshold moves
  (Rule 6).
- Any change the plumbing would need in `xtriever-core`, `deny.toml`, a format or a model
  identity: stop and ask (Rule 2).

## Notes

- [P] tasks touch different files and depend on nothing unfinished.
- Every record names its variant inside it; the file name is for people.
- The owner runs every git command; each ⛔ checkpoint comes with the exact commands.
