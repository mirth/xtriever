# Tasks: The Dense Stage

**Input**: Design documents from `/specs/004-dense-stage/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/dense-stage.md](./contracts/dense-stage.md),
[quickstart.md](./quickstart.md), [ADR-0007](../../docs/adr/0007-unsafe-readonly-mmap-in-dense.md)
(Accepted; constitution v1.2.0)

**Tests**: **Mandatory** (Principle II, spec FR-028). Phase 2 lands every test red: offline tests
fail at runtime on the `NotImplemented` scaffold; model-backed tests are `#[ignore]` and run where
the model directory exists (FR-028's split). Story phases contain implementation only and end with
the task that turns their tests green.

**Organization**: Setup → Oracle & red suite → Foundational → US1 → US2 → US3 → US4 → Polish. The
four-PR split from plan.md is marked at the checkpoints: **PR 1** = Phases 1–2, **PR 2** = Phases
3–4, **PR 3** = Phase 5, **PR 4** = Phases 6–8.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Measured facts and cited crate items are research D-numbers —
  read them before implementing; the hashes, sizes and line citations there are the ones to use,
  verbatim. **Rule 6 stop-points are marked ⛔**: on those failures, stop and report; never
  loosen the test.

## Path Conventions

Crate `crates/xtriever-dense/` (`src/`, `tests/`); harness `crates/xtriever-eval/` (`src/run.rs`,
`src/report.rs`, `examples/beir.rs`, `tests/`); Python oracle `reference/gen_004_fixtures.py`;
fixtures `reference/fixtures/004/`; model pins `reference/models/manifest.json`; model files
`reference/models/all-MiniLM-L6-v2/` (git-ignored); embedding cache `target/xt-dense-cache/`;
baselines `specs/004-dense-stage/baselines/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies at the pinned versions, the model pins and fetch script, the Python oracle
environment.

- [X] T001 Add dependencies to `crates/xtriever-dense/Cargo.toml` **with `cargo add`, at the pinned versions** (research D13): `cargo add -p xtriever-dense candle-core@0.9.2 candle-nn@0.9.2 candle-transformers@0.9.2`, `cargo add -p xtriever-dense tokenizers@0.23.2 --no-default-features --features fancy-regex`, `cargo add -p xtriever-dense serde --features derive`, `cargo add -p xtriever-dense serde_json sha2 thiserror`, `cargo add -p xtriever-dense memmap2 --optional`, `cargo add -p xtriever-dense --dev proptest tempfile serde_json`; then edit the file: copy the ADR-0001 pin comment from `crates/xtriever-ffi/Cargo.toml` above the three candle lines verbatim, add `[features] default = []` and `mmap = ["dep:memmap2"]`, set `description = "Xtriever: dense stage — candle embedder and flat exact vector index"`, keep `[lints] workspace = true`
- [X] T002 Verify after T001: `cargo tree -p xtriever-dense -e normal --prefix none | grep -Ei '(-sys|^cc |onig|openssl)'` prints nothing (FR-024); `cargo deny check` passes with **no** `deny.toml` change; `cargo check -p xtriever-dense --target aarch64-apple-ios` and `--target aarch64-linux-android` pass (FR-029 baseline before any code)
- [X] T003 [P] Write `reference/models/manifest.json` per data-model "Pinned Model": `repository`, `revision 1110a243fdf4706b3f48f1d95db1a4f5529b4d41`, `files` for exactly `config.json` (612 B, `953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41`), `tokenizer.json` (466,247 B, `be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037`), `model.safetensors` (90,868,376 B, `53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db`) — research D4 verbatim — plus `dim: 384`, `max_tokens: 256`, `f32_tensors: 103`
- [X] T004 [P] Write `scripts/fetch-model.sh` (contract "Scripts"): read the manifest with `jq`, for each file `curl -sSL --retry 5 --retry-delay 5 --retry-all-errors --connect-timeout 20` from `https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/<revision>/<file>` into `reference/models/all-MiniLM-L6-v2/` only if absent, verify `bytes` (`stat -f %z` / `stat -c %s`) and `shasum -a 256`; a download failure and a hash failure print **distinct** messages, the latter naming the path and both hashes, exit 1; idempotent — a second run downloads nothing and re-verifies; prints one `verified <file> <bytes> <sha256>` line per file
- [X] T005 [P] Create `reference/requirements-004.in` with **exactly** Feature 001's pins (research D14: `torch==2.14.0`, `transformers==5.17.0`, `tokenizers==0.23.2`, `safetensors==0.8.0`, `huggingface-hub==1.31.0`, `numpy==2.5.3`) and a header comment saying why they are 001's; compile `reference/requirements-004.txt` with `uv pip compile --python-version 3.12 --generate-hashes`; run `./scripts/setup-reference-venv.sh 004` and confirm `reference/.venv-004/bin/python -c 'import torch, transformers, numpy'` works
- [X] T006 Run `./scripts/fetch-model.sh` from an empty `reference/models/all-MiniLM-L6-v2/` and confirm three `verified` lines with the D4 values; run it again and confirm nothing downloads; confirm `git status` shows nothing under `reference/models/` (already ignored)

---

## Phase 2: Oracle & Red Suite (Blocking Prerequisite)

**Purpose**: Goldens from the reference implementations, the crate scaffold, and every test — red.
This phase is **PR 1**.

**⚠️ CRITICAL**: No story implementation begins until T023 confirms the suite fails for want of an
implementation, not for want of a fixture.

### Python oracle and goldens

- [X] T007 Create `reference/gen_004_fixtures.py` scaffold: interpreter guard (3.12 + venv, the 001 pattern), thread-pinning env vars set **before** `import torch` (copy the block from `gen_001_fixtures.py:50-55`), `--seed`, `--out`, `--model-dir` (default `reference/models/all-MiniLM-L6-v2`), `write_json`, `sha256_file`, `manifest.json` emission with per-file SHA-256 and `generator_sha256`, `--refresh-manifest`; verify the three model files against `reference/models/manifest.json` before loading anything
- [X] T008 In `reference/gen_004_fixtures.py`, implement `embeddings.json` per data-model "Embedding Goldens": tokenization with `tokenizers.Tokenizer.from_file` + `enable_truncation(256)` + `enable_padding(length=256, pad_id=0, pad_token="[PAD]")` (copy `gen_001_fixtures.py:332-338`), embedding with `AutoModel` + mask-weighted mean + `normalize(p=2)` (copy lines 357-375); cases with the required ids `short`, `long_over_256` (≥ 400 words; assert `n_real_tokens == 256`), `empty` (assert 2 real tokens), `whitespace_only`, `oov_unicode` (emoji + CJK + accented Latin), `punctuation_only`, `beir_like_title_text`, `duplicate_of_short` (identical text to `short`; assert identical vector), plus ≥ 4 varied sentences; each case carries `input_ids`, `attention_mask`, `n_real_tokens`, `vector` (384 floats via `float(x)`); top level `fingerprint` built from the same fields as research D6 **so the Rust constant can be asserted equal**, `tolerance { cosine_min: 0.9999, max_abs_diff: 0.001, unit_norm_abs: 1e-5 }`
- [X] T009 In `reference/gen_004_fixtures.py`, implement `search.json` per data-model "Search Goldens" and research D7: sets `dim8_cosine_ties`, `dim8_dot`, `dim8_euclidean`, `dim384_cosine` (500 rows), each with seeded `float32` rows serialised via `float(np.float32(x))`, **designed duplicates** (≥ 3 pairs, one pair placed so it ties at the `k`-th rank for some case), queries with cases for `k ∈ {0, 1, 5, 10, n, n+5}` and `allowed ∈ {null, [], subset, subset ∪ {unseen ids}}`; scores computed in `float64` from the f32 values (`cosine = dot/(|q||d|)`, `dot`, `euclidean = −sqrt(Σ(q−d)²)`), ordered by `(−score, id)`; **refuse** (reroll seed, print why) if any two consecutive distinct scores differ by < `tie_margin = 1e-5` or if any exact tie is not between identical rows; record `score_abs_tol: 1e-6`, `tie_margin`
- [X] T010 In `reference/gen_004_fixtures.py`, implement `mutations.json` per data-model: `dim 8`, `metric dot`, `fingerprint "test-fp"`, a step list covering add → commit → expect (len, results); add existing id with a new vector → expect unchanged **before** commit → commit → expect replaced, id once; delete known + unknown → commit → expect len decreased by the known count; add then `reopen` **without** commit → expect the add gone; a final expect where `k == len` and the last rank is a designed tie — expected results computed by the same float64 oracle as T009
- [X] T011 In `reference/gen_004_fixtures.py`, implement `--verify-embed <vectors.jsonl>`: each line `{"doc_id", "text", "vector"}`; re-embed `text` with torch, report per-line cosine and max-abs against `vector`, the worst of each, and exit 1 if any line violates the tolerance (research D14; used by quickstart Step 6)
- [X] T012 Run `reference/.venv-004/bin/python reference/gen_004_fixtures.py --seed 4 --out reference/fixtures/004/`; commit `embeddings.json`, `search.json`, `mutations.json`, `manifest.json`; confirm `git check-attr text eol -- reference/fixtures/004/search.json` reports `text: unset`; note in the commit message whether the generator rerolled

### Crate scaffold

- [X] T013 Create the scaffold in `crates/xtriever-dense/src/lib.rs` + `src/scaffold.rs`: the full public surface from [contracts/dense-stage.md](./contracts/dense-stage.md) (`model::{PinnedFile, PinnedModel, PINNED, FINGERPRINT, verify_files}`, `LoadPath`, `MiniLmEmbedder::{load, load_path, thread_count}`, `FlatIndex::{create, open, open_for}` + `open_mapped`/`open_mapped_for` under `#[cfg(feature = "mmap")]`, `FORMAT_VERSION`, both trait impls) with **`PINNED` and `FINGERPRINT` real** (the constants from T003 / research D6 — `model_pins` is green at the red checkpoint), every fallible fn returning `Err(Error::backend(NotImplemented("…")))`, infallible getters returning values that fail their assertions (`dim() = 0`, `len() = 0`, `metric() = Metric::Dot`); `missing_docs` satisfied; `#![deny(unsafe_code)]` not needed (workspace lint) — deleted in T051
- [X] T014 Extend `scripts/check-no-stubs.sh` to scan `crates/xtriever-dense/src/` for `NotImplemented` (add the crate to the loop and the PASS message)

### Tests (red except `fixtures_valid` and `model_pins`)

- [X] T015 [P] Write `crates/xtriever-dense/tests/support/mod.rs` (fixture loaders for `reference/fixtures/004/`; `model_dir()` from `XTRIEVER_MODEL_DIR` else `reference/models/all-MiniLM-L6-v2` — the 001 `support` pattern; `cosine(a, b)`, `max_abs(a, b)`, `bits_equal(a, b)` helpers; `parse_metric("cosine"|"dot"|"euclidean")`) and `tests/fixtures_valid.rs` asserting every `manifest.json` hash — **green** at the red checkpoint
- [X] T016 [P] Write `crates/xtriever-dense/tests/model_pins.rs`: `model::PINNED` equals `reference/models/manifest.json` field by field (repository, revision, the three names/bytes/hashes, dim, max_tokens); `model::FINGERPRINT` equals `embeddings.json`'s `fingerprint` and contains the revision, the weights hash, `dim=384`, `pool=mean-mask`, `norm=l2`, `max_tokens=256`, `dtype=f32`, `prefix=none`, `engine=candle-0.9.2` (FR-004, research D6) — **green** at the red checkpoint
- [X] T017 [P] Write `crates/xtriever-dense/tests/model_load.rs` (`#[ignore]`, model-backed) covering **US1 scenarios 1–2** and edge cases: `MiniLmEmbedder::load(model_dir, Buffered)` reports `dim() == 384`, `metric() == Cosine`, `max_input_tokens() == Some(256)`, `fingerprint() == FINGERPRINT`; a temp copy with one byte of `config.json` flipped fails with `Error::Model` whose message names `config.json` and **both** hashes; a copy with `tokenizer.json` truncated fails naming the file and both sizes; a copy with `tokenizer.json` removed fails naming it; **no** partial load — each failure occurs before any parse (assert the message mentions the hash/size, not a parse error) (FR-003)
- [X] T018 [P] Write `crates/xtriever-dense/tests/embed_golden.rs` (`#[ignore]`) covering **US1 scenarios 3, 5, 6** and FR-006: for every case in `embeddings.json`, first assert the crate's tokenization matches `input_ids`/`attention_mask` exactly (expose it via `embed`'s observable effect is impossible — instead assert through a `#[doc(hidden)] pub fn tokenize_for_test` on `MiniLmEmbedder`, declared in the scaffold), then `embed(&[text], Passage)` yields one vector of 384 with `|‖v‖ − 1| ≤ 1e-5`, `cosine ≥ 0.9999`, `max_abs ≤ 1e-3`; the `long_over_256`, `empty`, `whitespace_only`, `oov_unicode` cases are asserted by id so their presence is enforced (SC-001)
- [X] T019 [P] Write `crates/xtriever-dense/tests/embed_determinism.rs` (`#[ignore]`) covering **US1 scenarios 4, 7** and FR-005/FR-007: all golden texts embedded (a) in one batch, (b) one per call, (c) in reverse order in two batches — bit-identical per text (`to_bits`) across the three arrangements (SC-002); `Query` vs `Passage` bit-identical (FR-007); empty slice ⇒ empty `Vec`; and a **cross-process** test in the 002 `boundary_child` style: `thread_child` embeds the golden set and prints `to_bits` per component; the parent spawns `current_exe() --exact thread_child --nocapture` with `RAYON_NUM_THREADS=1` and again with `=4` and asserts identical output ⛔ (research D3)
- [X] T020 [P] Write `crates/xtriever-dense/tests/load_paths.rs` (`#[ignore]`, `#![cfg(feature = "mmap")]`) — ADR-0007 condition 3: `load(dir, Buffered)` and `load(dir, Mmap)` embed the whole golden set bit-identically ⛔; both report `FINGERPRINT`; and `tests/fingerprint.rs` (`#[ignore]`): two `load`s of the same directory report the same fingerprint and produce bit-identical vectors for the golden set (SC-010)
- [X] T021 [P] Write `crates/xtriever-dense/tests/index_golden.rs` covering **US2 scenarios 1–3** and FR-010–FR-013: for every set / query / case in `search.json`, `FlatIndex::create(tempdir, dim, metric, "test-fp")`, `add` every row, `commit`, `search(q, allowed, k)` ⇒ ids **and order exactly** equal to `expected`, each score within `score_abs_tol` (SC-003); the cases with a designed tie at rank `k` are asserted by set/query id so they cannot be silently dropped; under `#[cfg(feature = "mmap")]` the same loop runs again through `open_mapped(dir)` and asserts **bit-identical** scores to the `open` path (ADR-0007 condition 3)
- [X] T022 [P] Write `crates/xtriever-dense/tests/index_mutation.rs` covering **US2 scenarios 4–5** (drive `mutations.json` step by step: `expect` compares `len()` and `search` results; `reopen` drops the handle and `open`s), `tests/index_persist.rs` covering **scenario 6** and edge cases (commit, drop, `open` ⇒ bit-identical results for every `dim384_cosine` query (SC-004); **stale handle**: A and B open the same dir, B adds + commits, A's `len()` and results unchanged until A reopens; a leftover `index.bin.tmp` is ignored by `open`), `tests/index_errors.rs` covering **scenarios 7–8** and FR-015/FR-017 (`open_for` with a stub `Embedder` whose fingerprint differs ⇒ `FingerprintMismatch { index, current }` naming both; header with `format_version: 2` written by the test ⇒ `Corrupt` mentioning `2` and `1`; `add` of a 383-wide vector ⇒ `DimensionMismatch { expected: 384, actual: 383 }`, same for `search`; NaN / ∞ in `add` ⇒ `Schema`, in `search` ⇒ `InvalidQuery`; zero vector under `Cosine` ⇒ `Schema` on add, `InvalidQuery` on search; `k == 0` ⇒ empty; `k > len` ⇒ all live; empty `allowed` ⇒ empty; `allowed` with unseen ids ⇒ results ⊆ live ∩ allowed (SC-005)), and `tests/index_prop.rs` (proptest ≥ 300 cases, `dim 4..16`, rows `0..64`, metric any): results ⊆ `allowed` when given; every consecutive pair is `(score DESC, id ASC)`; `len()` after add/delete/commit equals the live set size; `search(q, Some(all_ids), k)` equals `search(q, None, k)` exactly; under `mmap`, `open` ≡ `open_mapped` bit-for-bit
- [X] T023 Run `cargo nextest run -p xtriever-dense` (offline) and `cargo nextest run -p xtriever-dense --run-ignored only` (model present): `fixtures_valid` and `model_pins` **pass**, every other test **fails** on `not implemented` / a failed assertion against a placeholder, no failure names a fixture, parse or hash problem. Commit red (Rule 4). **Checkpoint — PR 1.**

---

## Phase 3: Foundational Implementation (Blocking Prerequisite)

**Purpose**: Error helpers, the pins and their verification, and the byte-loading layer with the
one `unsafe` block — the three things both stories read.

- [X] T024 Implement `crates/xtriever-dense/src/error.rs` (002 pattern): `model_err(msg)` ⇒ `Error::Model { model: "all-MiniLM-L6-v2", message }`, `schema_err`, `invalid_query`, `corrupt`; `dim_mismatch(expected, actual)`; unit tests in-file
- [X] T025 Implement `crates/xtriever-dense/src/model.rs`: `PINNED` / `FINGERPRINT` (moved from the scaffold), `verify_files(dir)` — for each pinned file in order: `metadata().len() == bytes` else `Model` naming the file and both sizes; then stream SHA-256 in 1 MiB chunks (copy the spike's loop, `xtriever-ffi/src/spike/embed.rs:200-224`) `== sha256` else `Model` naming the file and both hashes (FR-003, research D4); nothing is parsed here
- [X] T026 Implement `crates/xtriever-dense/src/bytes.rs`: `pub(crate) enum Bytes { Owned(Vec<u8>), #[cfg(feature = "mmap")] Mapped(memmap2::Mmap) }` with `as_slice()`; `read(path, LoadPath) -> Result<Bytes>`; and under `#[cfg(feature = "mmap")]` the **single** `#[allow(unsafe_code)] fn map_readonly(file: &File) -> io::Result<Mmap>` with the `// SAFETY:` comment stating ADR-0007 condition 2 verbatim (the mapped file is never modified or truncated during the map's lifetime: weights are a read-only hash-verified resource; `index.bin` is only ever replaced by `rename` of a fully written `.tmp`, never modified in place); the item-scoped allow is the only one in the crate
- [X] T027 Wire `crates/xtriever-dense/src/lib.rs` to the real `error`, `model`, `bytes`; keep the scaffold for the embedder and index; run `cargo nextest run -p xtriever-dense --test model_pins --test model_load --run-ignored all` — `model_pins` green, the `model_load` **failure** cases (flipped byte, truncated, missing) green, the successful-load case still red; `grep -c 'unsafe {' crates/xtriever-dense/src/bytes.rs` prints 1 and `grep -rn 'allow(unsafe_code)' crates/xtriever-dense/src/` lists `bytes.rs` only

**Checkpoint**: pins enforced, bytes obtainable both ways. Story work begins.

---

## Phase 4: User Story 1 — Text becomes vectors that match the reference model (Priority: P1) 🎯 MVP

**Goal**: `MiniLmEmbedder` implements `Embedder` in full, bit-deterministic, within the 001
tolerance of torch. Completes **PR 2**.

**Independent Test**: `tests/{model_load,embed_golden,embed_determinism,fingerprint}.rs` green with
the model present; `load_paths.rs` green under `--features mmap`.

- [X] T028 [US1] Implement loading in `crates/xtriever-dense/src/embedder.rs` per research D4/D5: `MiniLmEmbedder::load(dir, load_path)` = `model::verify_files(dir)` → read `config.json`, parse `candle_transformers::models::bert::Config`, **assert** `hidden_size == 384`, `model_type == "bert"`, `max_position_embeddings >= 256`, `vocab_size == 30522` (each ⇒ `Model` naming the field and both values) → read `tokenizer.json` bytes, `Tokenizer::from_bytes`, `with_truncation(Some(TruncationParams { max_length: 256, .. }))`, `with_padding(Some(PaddingParams { strategy: Fixed(256), pad_id: 0, pad_token: "[PAD]", .. }))`, assert the tokenizer's pad id is 0 → `bytes::read(model.safetensors, load_path)`, parse the safetensors header (copy the spike's header walk, `embed.rs:163-197`) and **assert** 103 `F32` tensors and only `embeddings.position_ids` as `I64` → `VarBuilder::from_slice_safetensors(bytes.as_slice(), DTYPE, &Device::Cpu)` → `BertModel::load(vb, &config)`; store `tokenizer`, `model`, `load_path`; `thread_count()` = `candle_core::utils::get_num_threads()`
- [X] T029 [US1] Implement the forward pass in `crates/xtriever-dense/src/embedder.rs` per research D2/D5: `fn embed_one(&self, text: &str) -> Result<Vec<f32>>` — `encode(text, true)`, tensors of shape `(1, 256)` for ids / type ids / mask, `model.forward(&ids, &type_ids, Some(&mask))`, mask-weighted mean then L2 normalise using the spike's exact tensor expressions (`embed.rs:290-320`), `to_vec2`, assert 384; `#[doc(hidden)] pub fn tokenize_for_test(&self, text) -> (Vec<u32>, Vec<u32>)`; `impl Embedder`: `embed(texts, _kind)` loops `embed_one` in order (**batch dimension is always 1** — D2), `dim`, `metric = Cosine`, `fingerprint = FINGERPRINT`, `max_input_tokens = Some(256)`; every candle error mapped through `model_err`
- [X] T030 [US1] Run `RAYON_NUM_THREADS=1 cargo nextest run -p xtriever-dense --run-ignored only` — `model_load`, `embed_golden`, `embed_determinism` (including the cross-process 1-vs-4 threads test ⛔ — if it fails, stop and report per research D3), `fingerprint` **green**; then `cargo nextest run -p xtriever-dense --features mmap --run-ignored only` — `load_paths` **green** ⛔ (ADR-0007 condition 3). Record the wall time of `embed_golden` per text for the report. **Checkpoint — MVP: the embedder is a contract. PR 2.**

---

## Phase 5: User Story 2 — Vectors are stored, found exactly, and survive reopening (Priority: P1)

**Goal**: `FlatIndex` implements `VectorIndex` in full with format v1, exact search and the total
tie-break. Completes **PR 3**.

**Independent Test**: `tests/{index_golden,index_mutation,index_persist,index_errors,index_prop}.rs`
green offline; the same under `--features mmap`.

- [X] T031 [US2] Implement `crates/xtriever-dense/src/index/format.rs` per data-model "On disk": `Header { format_version, dim, metric, fingerprint, count }` (serde, keys in that order, `metric` as `cosine|dot|euclidean`); `encode(header, ids: &[u32], norms: &[f32], rows: &[f32]) -> Vec<u8>` writing magic `XTDENSE1`, `u64` LE header length, JSON, then ids / norms / vectors as LE; `decode(bytes: &[u8]) -> Result<(Header, offsets)>` validating magic, `format_version == FORMAT_VERSION` (else `Corrupt("format version {v}, this build reads 1")`), `count` fits `usize` (else `Corrupt`), total length `== 16 + hdr_len + count·(8 + 4·dim)` (else `Corrupt` naming both lengths), ids strictly ascending (else `Corrupt`); accessors `id_at`, `norm_at`, `row_at(i) -> impl Iterator<Item = f32>` over `chunks_exact(4)` + `from_le_bytes` — **no transmute, no bytemuck**
- [X] T032 [US2] Implement `crates/xtriever-dense/src/index/search.rs` per research D7/D9: `validate_query(q, dim, metric)` ⇒ `DimensionMismatch` / `InvalidQuery` (non-finite; zero norm under Cosine); `score(metric, q, q_norm: f64, row: impl Iterator<f32>, row_norm: f32) -> f32` accumulating in **`f64`** in index order and rounding once (`Cosine = dot/(q_norm·row_norm)`, `Dot`, `Euclidean = −sqrt(Σ(q−d)²)`); `top_k(scored: Vec<(f32, u32)>, k)` = `sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Equal).then(a.1.cmp(&b.1)))` then truncate — `partial_cmp`, not `total_cmp` (−0.0 ties +0.0 by id, D7); no threads, no SIMD (FR-027)
- [X] T033 [US2] Implement `crates/xtriever-dense/src/index/mod.rs` per data-model "In memory" and research D8/D9: `FlatIndex { dir, header, committed: Generation { ids, norms, rows: Rows::Owned(Vec<f32>) | #[cfg(feature="mmap")] Rows::Mapped(Bytes, offsets) }, pending: BTreeMap<DocId, Option<Vec<f32>>> }`; `create(dir, dim, metric, fingerprint)` (`create_dir_all`, dir must be empty else `Corrupt`; writes an empty generation immediately); `open(dir)` / `open_mapped(dir)` via `bytes::read(index.bin, …)` + `format::decode`; `open_for` / `open_mapped_for` = open then `fingerprint == embedder.fingerprint()` else `FingerprintMismatch { index, current }`, `dim == embedder.dim()` and `metric == embedder.metric()` else `Corrupt`; `impl VectorIndex`: `add` validates width → `DimensionMismatch`, finiteness → `Schema`, Cosine zero-norm → `Schema`, stages `Some(v.to_vec())`; `delete` stages `None` (unknown ids: no-op); `commit` — no-op if pending is empty; else merge committed ids with pending in ascending id order (pending `Some` replaces, `None` removes), compute each new row's norm in `f64` → `f32`, `format::encode`, write `index.bin.tmp`, `sync_all`, `rename` over `index.bin`, reload through the same path the handle was opened with, clear pending; `search` = validate, `k == 0` or `allowed == Some(empty)` ⇒ `Ok(vec![])`, score every committed row (skipping ids not in `allowed`), `top_k`, map to `Hit { id: DocId(id), score }`; `len` = committed count
- [X] T034 [US2] Run `cargo nextest run -p xtriever-dense` — every index test **green** (SC-003, SC-004, SC-005); then `cargo nextest run -p xtriever-dense --features mmap` — green including the `open ≡ open_mapped` parity assertions ⛔ (ADR-0007 condition 3); `cargo check -p xtriever-dense --features mmap --target aarch64-apple-ios` passes. **Checkpoint — PR 3.**

---

## Phase 6: User Story 3 — The dense stage's baseline is measured through the eval harness (Priority: P2)

**Goal**: `dense-baseline-v1` on SciFact, NFCorpus and FiQA through the 003 harness, cached,
reproducible, `--verify-run`-checked, recorded as an **absolute** baseline.

**Independent Test**: `crates/xtriever-eval/tests/dense_run.rs` offline (stub embedder + stub
index); SciFact end to end twice, byte-identical, with 0 embedded on the second run.

- [X] T035 [P] [US3] Write `crates/xtriever-eval/tests/dense_run.rs` (offline) covering **US3 scenarios 1–4** and FR-019/FR-020: `DenseConfig::dense_baseline_v1()` has `k == 100`, `passage { title_then_text: true, separator: " ", omit_empty_title: true }` and `validate()` rejects `k < 100`; `build_passages` on the `support::synthetic_dataset` joins `title + " " + text`, uses `text` alone for an empty title, assigns `DocId(i)` in corpus order with a reversible `IdMap`; `execute_dense` against a **stub `Embedder`** (returns a fixed vector per text) and a **stub `VectorIndex`** (returns canned hits) submits every judged query with `TextKind::Query`, `k = 100`, preserves order; `EmbeddingCacheKey::write` then `matches` is true, and false after changing any one of `config`, `dataset`, `embedder_fingerprint`, `corpus_sha256`, `documents` (scenario 3); extend `crates/xtriever-eval/tests/report.rs`: every committed `specs/003-eval-harness/baselines/*.json` deserialises and re-serialises **byte-identically** (report extension is additive); a report with `stage` serialises it as the **last** key; `delta` of two reports with different `config` is `Err` mentioning "different configurations" (FR-021) and `smoke` still passes the 003 cases — all **red** except the round-trip until T036–T038
- [X] T036 [US3] Implement the harness extension in `crates/xtriever-eval/src/run.rs` per contract "Harness extension" and research D10: `PassageSpec`, `DenseConfig` (+ `validate()` sharing the `k ≥ 100` rule, `dense_baseline_v1()`), `build_passages(dataset, cfg) -> Result<(Vec<String>, IdMap)>`, `execute_dense(embedder, index, ids, dataset, cfg) -> Result<Run>` (judged queries ascending, `embedder.embed(&[text], TextKind::Query)`, `index.search(&v, None, cfg.k)`, external ids via `IdMap`, `unjudged_queries: 0`), `EmbeddingCacheKey { format_version: 1, config, dataset, embedder_fingerprint, corpus_sha256, documents }` with `write(dir)` (pretty JSON to `cache.json`) and `matches(dir)` (parse + field equality, `false` on any error); trait objects only — the library must not name a dense type (`cargo tree -p xtriever-eval -e normal` unchanged)
- [X] T037 [US3] Implement the report extension in `crates/xtriever-eval/src/report.rs`: `StageInfo { kind, embedder_fingerprint, load_path, thread_count, baseline }`; `EvalReport.stage: Option<StageInfo>` declared **last** with `#[serde(default, skip_serializing_if = "Option::is_none")]`; `Observations` gains `embed_corpus_ms`, `search_ms`, `model_bytes_buffered`, `model_bytes_mmapped` as `Option<u64>` with the same attributes; `delta(before, after) -> Result<Delta>` returning `Error::Run("reports are for different configurations (`a` vs `b`); a delta is only meaningful within one configuration — FR-021")` when any paired reports' `config` differ; update `smoke` and the example's `delta` for the new return type; add the one-line note to `specs/003-eval-harness/contracts/eval-harness.md` "Report file": optional trailing `stage` key and four optional `observations` keys added by Feature 004
- [X] T038 [US3] Extend `crates/xtriever-eval/examples/beir.rs` per contract "`beir` example commands": `cargo add -p xtriever-eval --dev xtriever-dense` (path, **dev only**); `--config dense-baseline-v1` with `--model-dir` (default `reference/models/all-MiniLM-L6-v2`), `--cache-dir` (default `target/xt-dense-cache`), `--load-path buffered|mmap` (`mmap` only when the example is built with `--features mmap`, else `bail!`): `MiniLmEmbedder::load`; `build_passages`; if `EmbeddingCacheKey::matches(cache/D)` → `FlatIndex::open_for` (or `open_mapped_for`), print `embedded 0 passages (cache hit)`; else `remove_dir_all`, `create`, embed every passage with `TextKind::Passage` printing progress every 1,000 to **stderr**, `add`, one `commit`, `write` the key; `execute_dense`, `score`, set `report.stage = Some(StageInfo { kind: "dense", embedder_fingerprint, load_path, thread_count: MiniLmEmbedder::thread_count(), baseline: "absolute" })`; print `embedded N passages in S s` and `searched Q queries in S s` to stderr only (never into the JSON — D10); add `model-memory --model-dir M --load-path P` (load, embed one sentence, print fingerprint, exit) and `export-vectors --cache-dir C --dataset D --sample N --out F` (open the cached index, write `{"doc_id","text","vector"}` for N evenly spaced rows, re-reading the passages via `build_passages`)
- [X] T039 [US3] Run `cargo nextest run -p xtriever-eval` — **green** (T035 tests, the 003 suite unchanged); `cargo tree -p xtriever-eval -e normal | grep -E 'candle|tantivy|memmap2'` prints nothing; then `cargo run --release -p xtriever-eval --example beir -- smoke --dataset scifact --baseline specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json` still exits 0 (quickstart Step 7)
- [X] T040 [US3] Produce the SciFact dense baseline (quickstart Step 6, `RAYON_NUM_THREADS` exported and noted): `cargo run --release -p xtriever-eval --example beir -- run --dataset scifact --config dense-baseline-v1 --cache-dir target/xt-dense-cache --out specs/004-dense-stage/baselines/dense-baseline-v1.scifact.json --export-run target/dense-run-scifact.jsonl`; verify with `reference/.venv-003/bin/python reference/gen_003_fixtures.py --verify-run target/dense-run-scifact.jsonl --qrels reference/datasets/beir/scifact/qrels/test.tsv --report specs/004-dense-stage/baselines/dense-baseline-v1.scifact.json` — agreement within 1e-6 (SC-007); run again to `/tmp/scifact-again.json` — stderr shows `embedded 0`, `diff` identical (SC-006); record the first run's embed and search seconds
- [X] T041 [US3] Produce the NFCorpus and FiQA dense baselines the same way into `specs/004-dense-stage/baselines/dense-baseline-v1.{nfcorpus,fiqa}.json`, each `--verify-run`-checked; FiQA under `/usr/bin/time -l` with the cache cleared first (quickstart Step 6), capturing `embedded … in S s`, `searched … in S s`, `maximum resident set size`, and `stat -f %z target/xt-dense-cache/fiqa/index.bin`; then `export-vectors --dataset scifact --sample 50` + `gen_004_fixtures.py --verify-embed` — every sampled corpus embedding within tolerance ⛔
- [X] T042 [US3] Confirm FR-021 mechanically: `cargo run -p xtriever-eval --example beir -- delta specs/003-eval-harness/baselines/lexical-baseline-v1.scifact.json specs/004-dense-stage/baselines/dense-baseline-v1.scifact.json` exits non-zero naming both configurations; `delta` of the SciFact dense baseline against its `/tmp/scifact-again.json` copy prints an all-zero table

**Checkpoint**: three absolute dense baselines exist, cross-checked, cached, reproducible.

---

## Phase 7: User Story 4 — Memory and time are observed, not assumed (Priority: P3)

**Goal**: FR-023's observations with method, the per-path model-memory numbers from cold, and the
labelled 100k extrapolation.

**Independent Test**: the FiQA report's `observations` block carries every field with a method.

- [X] T043 [US4] Measure model memory per load path **in fresh processes** (research D12): `/usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- model-memory --load-path buffered 2>&1 | grep 'maximum resident'` and the same with `--features mmap … --load-path mmap`; run each three times, record min/max; compute `buffered_peak − mapped_peak` and its ratio to `buffered_peak`
- [X] T044 [US4] Apply **ADR-0007 condition 5**: if the ratio from T043 is `< 0.10`, delete `LoadPath::Mmap` from `crates/xtriever-dense/src/embedder.rs` and `src/bytes.rs`'s weight use (the index mapping and `map_readonly` stay), update `tests/load_paths.rs` to cover only the index parity, update the contract and ADR-0007 with an "Outcome" line and the numbers; otherwise record the numbers and keep both paths. Either way this task ends with the decision and the evidence written into `specs/004-dense-stage/report.md`
- [X] T045 [US4] Fill the FiQA report's `observations` in `specs/004-dense-stage/baselines/dense-baseline-v1.fiqa.json` by hand (the 003 method): `index_dir_bytes` (= `index.bin` size from T041), `peak_rss_bytes`, `embed_corpus_ms`, `search_ms`, `model_bytes_buffered`, `model_bytes_mmapped` (from T043; omit the latter if T044 deleted the path), and a `method` string naming each command, the thread count, the host and that peak RSS covers the whole harness process (corpus + passages + index); re-run `--verify-run` on the edited file to prove the numbers are untouched (SC-008)
- [X] T046 [US4] Write the extrapolation in `specs/004-dense-stage/report.md` (Story 4 scenario 3): from FiQA's 57,638 rows to 100,000 — index bytes and embed time scaled linearly, model memory constant — **labelled "extrapolation"** with the assumptions listed and the two measured points (lexical from 003: 17.5 MiB index / 327.5 MiB harness RSS at 57.6k; dense from here) placed on the constitution's 300 MB line without a verdict

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Scaffold removal, the report, the PR description, the full gate. Completes **PR 4**.

- [X] T047 [P] Write `specs/004-dense-stage/report.md`: verdict; nextest summaries (offline / model-backed / `mmap`); the three baselines with `--verify-run` agreement (absolute — no band, no delta, FR-021 stated); SC-001/002/010 evidence; the thread-count result (D3); the load-path numbers and the condition-5 verdict (T044); FiQA observations with method; the 100k extrapolation; the `git diff --stat main -- crates/xtriever-core crates/xtriever-lexical deny.toml crates/xtriever-eval/src/metrics.rs crates/xtriever-eval/src/dataset.rs` output (empty — FR-025, SC-009); findings, including D1 and D2 as corrections to Feature 001's record
- [X] T048 [P] Write `specs/004-dense-stage/pr-description.md`: summary, the four-commit split with line counts, the baseline table, the observation table, the constitution v1.2.0 / ADR-0007 note, and the line **"eval delta: not applicable — this feature establishes the dense stage's absolute baseline (FR-021); lexical smoke unchanged"**
- [X] T049 [P] Update `specs/004-dense-stage/quickstart.md` status note to "executed" with the measured FiQA embed time and the condition-5 outcome; update `specs/001-ios-build-spike/report.md` with a short dated "Correction (Feature 004, research D1)" paragraph under "The headline" so the 39.5× claim is not left standing uncorrected
- [X] T050 [P] Add a `## Feature 004` section to `crates/xtriever-dense/src/lib.rs` crate docs: what the crate provides, the `mmap` feature and ADR-0007, the determinism promise and its architecture caveat (D3), the fingerprint format (D6), the on-disk format version
- [X] T051 Delete `crates/xtriever-dense/src/scaffold.rs` and every `NotImplemented` reference; `./scripts/check-no-stubs.sh` **passes**
- [X] T052 Run the full gate from quickstart Step 8: `cargo fmt --all --check`; `RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets`; `RUSTFLAGS="-D warnings" cargo clippy -p xtriever-dense --features mmap --all-targets`; `cargo nextest run --workspace`; `cargo nextest run -p xtriever-dense --features mmap`; `RAYON_NUM_THREADS=1 cargo nextest run -p xtriever-dense --run-ignored only` and again with `--features mmap`; `cargo deny check`; `cargo check --workspace --target aarch64-apple-ios` / `aarch64-apple-ios-sim` / `aarch64-linux-android`; `cargo check -p xtriever-dense --features mmap --target aarch64-apple-ios`; `cargo check --workspace --target wasm32-unknown-unknown` (best-effort, record the failure point); `./scripts/check-no-stubs.sh`; `./scripts/check-toolchain.sh`; the containment greps (exactly one `unsafe {` in `bytes.rs`, `allow(unsafe_code)` in `bytes.rs` only); the eval purity `cargo tree`; the FR-025 `git diff --stat` (empty). Paste the results into `report.md` and `pr-description.md`. Any failure ⛔ — stop and report

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies; T003–T005 in parallel; T006 needs T003–T004
- **Oracle & red suite (Phase 2)**: T007–T012 need T005–T006 (venv + model); T013–T014 need T001; T015–T022 need T012 (fixtures) and T013 (scaffold); T023 needs everything above → **PR 1**
- **Foundational (Phase 3)**: needs T023; T024 → T025 → T026 → T027
- **US1 (Phase 4)**: needs T027; T028 → T029 → T030 → **PR 2**
- **US2 (Phase 5)**: needs T027 (not US1 — the index is model-free); T031 → T032 → T033 → T034 → **PR 3**
- **US3 (Phase 6)**: needs US1 **and** US2; T035 first (red), then T036 → T037 → T038 → T039 → T040 → T041 → T042
- **US4 (Phase 7)**: needs T041; T043 → T044 → T045 → T046
- **Polish (Phase 8)**: needs US4; T047–T050 in parallel; T051; T052 last → **PR 4**

### User Story Dependencies

- **US1 (P1)** and **US2 (P1)** are independent of each other after Phase 3 and can be built in either order or in parallel
- **US3 (P2)** integrates US1 + US2 through the harness
- **US4 (P3)** reads US3's FiQA run

### Rule 6 stop-points (⛔)

T019/T030 thread-count bit-identity; T020/T030 and T021/T034 load-path parity; T041 corpus
embeddings vs torch; T052 any gate failure. The response is a report, never a looser test.

### Parallel Opportunities

- Phase 1: T003 ‖ T004 ‖ T005
- Phase 2: T015 ‖ T016 ‖ T017 ‖ T018 ‖ T019 ‖ T020 ‖ T021 ‖ T022 (eight test files, one scaffold)
- Phases 4 and 5 in parallel (different modules, both on Phase 3)
- Phase 8: T047 ‖ T048 ‖ T049 ‖ T050

---

## Parallel Example: Phase 2 red suite

```text
after T012 (fixtures) and T013 (scaffold):
  T015 tests/support + fixtures_valid   T016 tests/model_pins        T017 tests/model_load
  T018 tests/embed_golden               T019 tests/embed_determinism T020 tests/load_paths + fingerprint
  T021 tests/index_golden               T022 tests/index_{mutation,persist,errors,prop}
then T023 (red checkpoint, PR 1)
```

## Implementation Strategy

1. **PR 1** (Phases 1–2): pins, script, oracle, goldens, scaffold, red suite. Nothing runs yet;
   everything that will be asserted exists.
2. **PR 2** (Phases 3–4) — **MVP**: the embedder is a contract. Stop here if the thread-count or
   load-path tests disagree with research D3 / ADR-0007.
3. **PR 3** (Phase 5): the index, offline-testable, independent of the model.
4. **PR 4** (Phases 6–8): the harness extension, the three absolute baselines, the observations,
   the condition-5 decision, the report and the gate.
