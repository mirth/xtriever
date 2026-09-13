# Tasks: The Re-rank Stage

**Input**: Design documents from `/specs/006-rerank-stage/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/rerank-stage.md](./contracts/rerank-stage.md),
[quickstart.md](./quickstart.md), [ADR-0008](../../docs/adr/0008-pipeline-format-v2-passage-store.md),
[ADR-0009](../../docs/adr/0009-unsafe-readonly-mmap-in-rerank.md)

**Tests**: **Mandatory** (Principle II, spec FR-024). Phase 2 lands every test red: offline
tests fail at runtime on `NotImplemented` scaffolds; model-backed tests are `#[ignore]` and
separated from the offline suite. Story phases contain implementation only and end with the
task that turns their tests green.

**Organization**: Setup → Oracle & red suite → US1 → US2 → Foundational (passage store, format
v2) → US3 → US4 → US5 → US6 → Polish. The pipeline's foundational work sits after US1/US2
because the cross-encoder crate is independent of it and ships first. The four-PR split from
plan.md: **PR 1** = Phases 1–2, **PR 2** = Phases 3–4, **PR 3** = Phases 5–7, **PR 4** =
Phases 8–10.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US6 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers and data-model
  sections — read them before implementing. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

Crates `crates/xtriever-rerank/` (`src/`, `tests/`) and `crates/xtriever-pipeline/` (`src/`,
`tests/`); harness `crates/xtriever-eval/` (`src/run.rs`, `src/report.rs`, `examples/beir.rs`,
`tests/`); generator `reference/gen_006_fixtures.py` (runs in `reference/.venv-004`); fixtures
`reference/fixtures/006/`; models `reference/models/all-MiniLM-L6-v2/` (004) and
`reference/models/ms-marco-MiniLM-L-6-v2/` (this feature, git-ignored); the 004 embedding cache
`target/xt-dense-cache/<dataset>/`; re-ranked indexes `target/xt-rerank-index/<dataset>/`;
baselines `specs/006-rerank-stage/baselines/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies (all already locked), the model manifest and fetch script, the stub
check, the CI path filter.

- [X] T001 Add dependencies to `crates/xtriever-rerank/Cargo.toml` **with `cargo add`** (research D12): `cargo add -p xtriever-rerank candle-core@0.9.2 candle-nn@0.9.2 candle-transformers@0.9.2`, `cargo add -p xtriever-rerank tokenizers@0.23.2 --no-default-features --features fancy-regex`, `cargo add -p xtriever-rerank serde --features derive`, `cargo add -p xtriever-rerank serde_json sha2`, `cargo add -p xtriever-rerank memmap2@0.9.11 --optional`, `cargo add -p xtriever-rerank --dev serde_json tempfile`; then edit: copy the dense crate's "PINNED to 0.9.2 — do NOT `cargo add`-upgrade" comment block (ADR-0001) above the candle lines; `[features] default = []` and `mmap = ["dep:memmap2"]` with a one-line comment pointing at ADR-0009; `description = "Xtriever: rerank stage — candle cross-encoder"`; keep `[lints] workspace = true`
- [X] T002 Verify after T001: `cargo tree -p xtriever-rerank -e normal --prefix none | grep -Ei '(-sys|^cc |onig|openssl)'` prints nothing; `cargo tree -p xtriever-rerank -e normal --features mmap` shows `memmap2` and the default graph does not; `cargo deny check` passes with no `deny.toml` change; `cargo check -p xtriever-rerank --target aarch64-apple-ios` and `--target aarch64-linux-android` pass (FR-025 baseline)
- [X] T003 [P] Create `reference/models/manifest-rerank.json` (research D2, data-model "Pinned Re-rank Model"): `schema_version 1`, `repository "cross-encoder/ms-marco-MiniLM-L-6-v2"`, `revision "233902d25c440f23af6f7d6e94d2946bac0bee0a"`, `local_dir "ms-marco-MiniLM-L-6-v2"`, `files`: `config.json` 794 B `380e02c93f431831be65d99a4e7e5f67c133985bf2e77d9d4eba46847190bacc`, `tokenizer.json` 711396 B `d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66`, `model.safetensors` 90870598 B `821d1aa69520101d6e0737f78a042ae25b19e5cb9160701909d10434f4aeb0ae`; `hidden 384`, `max_tokens 512`, `f32_tensors 105`; add `!reference/models/manifest-rerank.json` to `.gitignore` under the existing `!reference/models/manifest.json` line
- [X] T004 [P] Extend `scripts/fetch-model.sh` with `--manifest FILE` (contract "Scripts"): parse the option before the positional `DEST_DIR`; default manifest unchanged; when the manifest carries `local_dir`, the default destination is `reference/models/<local_dir>`; everything else (download at the manifest's revision, size + sha256 verification, exit codes, idempotence) unchanged; update the usage comment; run `scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json` and confirm `PASS` with the three files verified, then `scripts/fetch-model.sh` (no args) still passes for the 004 model
- [X] T005 [P] Extend `scripts/check-no-stubs.sh` to scan `crates/xtriever-rerank/src/` for `NotImplemented` (loop list and PASS message)
- [X] T006 [P] Add `crates/xtriever-rerank/**` to the `eval-smoke` path filter in `.github/workflows/ci.yml` (already present since Feature 003 — verified, no change); change nothing else — the smoke stays SciFact-only and lexical, no model in CI (standing rule; spec FR-020)

---

## Phase 2: Oracle & Red Suite (Blocking Prerequisite)

**Purpose**: Reference scores and the ordering oracle, the scaffolds, and every test — red.
**PR 1.**

**⚠️ CRITICAL**: No story implementation begins until T027 confirms the suite fails for want of
an implementation, not for want of a fixture.

### Python oracle and goldens

- [X] T007 Create `reference/gen_006_fixtures.py` scaffold (research D14): the 004 interpreter guard (3.12 + venv; message names `scripts/setup-reference-venv.sh 004` and `reference/.venv-004`), thread pinning before `import torch`, `TOKENIZERS_PARALLELISM=false`, `--out`, `write_json` (Python `repr` floats — `json.dump` default), `sha256_file`, `manifest.json` with per-file SHA-256 and `generator_sha256`, `--refresh-manifest`; `MODEL_DIR = reference/models/ms-marco-MiniLM-L-6-v2`, `MANIFEST = reference/models/manifest-rerank.json`; verify the three files' sizes and hashes against the manifest before loading anything; assemble `MODEL_ID` from the manifest fields exactly as data-model "Pinned Re-rank Model" spells it (`{repository}@{revision};weights=sha256:{sha};max_tokens=512;trunc=longest_first;head=cls-pooler-tanh-linear;act=identity;dtype=f32;engine=candle-0.9.2`) and print it
- [X] T008 In `reference/gen_006_fixtures.py`, implement the reference scorer (research D1/D4): `AutoTokenizer.from_pretrained(MODEL_DIR)` (fast), `AutoModelForSequenceClassification.from_pretrained(MODEL_DIR).eval()`; `score(query, passage)` = `tokenizer(query, passage, truncation=True, max_length=512, return_tensors="pt")` → `model(**enc).logits[0, 0].item()`, one pair per call, no padding; also return `input_ids`, `token_type_ids` and `truncated = len(input_ids) == 512`; assert once at start-up that the manual head `classifier(tanh(pooler.dense(h[:,0])))` equals the pipeline logit on the first pair (guards the D1 head reading)
- [X] T009 In `reference/gen_006_fixtures.py`, emit `rerank.json` per data-model "Re-rank Goldens": `model_id`, `max_tokens 512`, `tolerance_abs 0.001`, `min_gap 0.01`; ~10 hand-written queries × 4–6 passages each (factual questions with one on-topic passage, near-topic distractors and an off-topic one) including the named cases `over-length` (one passage of ≥ 600 words ⇒ `truncated: true`, 512 ids), `empty-passage` (`""` — the single-sequence encoding, research D4), `empty-query`, `both-empty`, `near-tie` (two passages differing by one word); per passage `text`, `input_ids`, `token_type_ids`, `truncated`, `score`; per query `order` (indexes by score descending, ties impossible by construction); **refuse to emit** (exit 1 naming the query) if any adjacent gap in a query's sorted scores is `< min_gap`, and assert the `near-tie` gap is in `[0.01, 1.0]` — edit the texts by hand until it passes
- [X] T010 In `reference/gen_006_fixtures.py`, emit `pipeline_order.json` per data-model "Pipeline Order Goldens": a module-level `order_reranked(fused_ids, scores, d, k)` implementing research D8 independently — scored `(i < d and scores[i] is not None)` first sorted by `(-score, id)`, then the rest in fused order, cut at `k` — and the ten named cases `full`, `partial-m-lt-d`, `none-scored`, `d-gt-k`, `d-lt-k`, `d-ge-len`, `d-zero`, `ties-by-id` (equal scores, ascending id), `k-lt-scored` (`k` smaller than the scored count), `single`; each `{name, fused, scores, d, k, expected: [[id, score|null], …]}`
- [X] T011 In `reference/gen_006_fixtures.py`, implement `--verify-rerank <explain.jsonl>` (contract "`beir` example commands"): each line `{"query_id", "fused": [ext ids], "rerank": [[rank, ext id, score], …], "hits": [ext ids]}`; check the scored prefix of `hits` is exactly the `rerank` ids ordered by `(-score, ext id)` compared as tie blocks (the pipeline breaks ties by internal id, the oracle cannot see it) and the suffix equals `fused` with the scored ids removed, in order; print queries checked, exit 1 on any disagreement; and `--verify-scores <sample.jsonl>` (`{"query", "passage", "score"}` lines) re-scoring each pair and reporting the max absolute difference, exit 1 above `tolerance_abs`
- [X] T012 Run `reference/.venv-004/bin/python reference/gen_006_fixtures.py --out reference/fixtures/006/`; commit `rerank.json`, `pipeline_order.json`, `manifest.json`; record the printed `MODEL_ID` for T015; confirm `git check-attr text eol -- reference/fixtures/006/rerank.json` reports `text: unset`

### Crate scaffolds

- [X] T013 Create the `xtriever-rerank` scaffold per [contracts/rerank-stage.md](./contracts/rerank-stage.md): `src/lib.rs` (crate docs placeholder, `pub mod model`, `pub use scorer::MiniLmCrossEncoder`, `pub enum LoadPath { Buffered, #[cfg(feature = "mmap")] Mmap }` with the external-writer precondition in the doc comment), `src/error.rs` (`model_err(msg) -> Error::Model { model: MODEL_NAME, message }`), `src/model.rs` (**real** `PinnedFile`, `PinnedModel`, `PINNED` with the T003 values, `MODEL_NAME = "ms-marco-MiniLM-L-6-v2"`, `MODEL_ID` via the 004 `concat!` macro device, and `verify_files` returning `Err(Error::backend(NotImplemented))`), `src/scorer.rs` (`MiniLmCrossEncoder` with `load(dir, LoadPath)`, `load_path()`, `score`, `thread_count` (real: `candle_core::utils::get_num_threads`), `tokenize_for_test` — fallible fns `NotImplemented`), `src/budget.rs` (`pub(crate) fn rerank_with(scorer: &dyn Fn(&str, &str) -> Result<f32>, query, passages, budget) -> Result<Vec<Option<f32>>>` `NotImplemented`; expose it `#[doc(hidden)] pub` for the offline budget test), the `Reranker` impl delegating to it; `missing_docs` satisfied; scaffold bodies deleted by T033/T037
- [X] T014 Extend the `xtriever-pipeline` scaffold for the new surface (contract "`xtriever-pipeline` (changes to the 005 contract)"): in `src/types.rs` add the **real** fields `HybridConfig.rerank_depth` (set to 20 in `new`), `SearchOptions.rerank_depth`, `HybridHit.{rerank_score, text}`, `HitExplain.{rerank_score, rerank_rank}`, `features()` returning 7 entries with `RERANK_RANK`, `StageReport.rerank`, `RerankReport`; `pub const RERANK_RANK: &str = "rerank.rank"` in `src/lib.rs` (`FORMAT_VERSION` stays 1 until Phase 5 so the 005 persist suite stays green — bumped by T039); `src/rerank.rs` with `pub fn order_reranked(fused: &[(DocId, f64)], scores: &[Option<f32>], k: usize) -> Vec<(DocId, f64, Option<f32>)>` returning an empty `Vec`; `src/passages.rs` with `pub(crate) struct PassageStore` whose `create/open/read/stage/commit` return `NotImplemented`; `HybridIndex::{set_reranker, reranker}` real (field added), `search` unchanged for now — the 005 suites must stay green; `HybridHit.text` filled with `String::new()` until Phase 5

### Tests — `xtriever-rerank` (red except `fixtures_valid`, `model_pins`)

- [X] T015 [P] Write `crates/xtriever-rerank/tests/support/mod.rs` (fixtures dir `reference/fixtures/006/`, model dir `reference/models/ms-marco-MiniLM-L-6-v2` (env override `XTRIEVER_RERANK_MODEL_DIR`), `rerank.json` loader on `serde_json` with `float_roundtrip`, a `tempfile` copy of the model dir for tamper tests) and `tests/fixtures_valid.rs` (every `manifest.json` hash and the generator hash) and `tests/model_pins.rs` (`PINNED` equals `reference/models/manifest-rerank.json` field by field and file by file; `MODEL_ID == rerank.json.model_id`; `MODEL_ID` contains the revision, the weights hash, `;max_tokens=512;`, `;trunc=longest_first;`, `;head=cls-pooler-tanh-linear;`, `;act=identity;`, `;dtype=f32;`, `;engine=candle-0.9.2`) — **green** at the red checkpoint
- [X] T016 [P] Write `crates/xtriever-rerank/tests/budget.rs` covering **US2 scenarios 1, 3, 4** and FR-007 with a stub scorer over `rerank_with` (SC-003 item part): scorer returns `f32::from(passage.len() as u8)`-style deterministic values; 10 passages: `max_items` ∈ {0, 1, 5, 10, 20} ⇒ exactly the first `n.min(10)` are `Some`, the rest `None`, length always 10; `max_time = Some(Duration::ZERO)` ⇒ all `None`; `Budget::default()` ⇒ all `Some`; a scorer that errors on the third passage ⇒ `Err` (the first error aborts); an empty passage list ⇒ empty `Vec`
- [X] T017 [P] Write `crates/xtriever-rerank/tests/model_load.rs` (`#[ignore]`) covering **US1 scenario 1** and FR-002/FR-003: `load(dir, Buffered)` succeeds and `model_id() == MODEL_ID`; on a temp copy: append one byte to `model.safetensors` ⇒ `Error::Model` whose message contains the path, `90870598` and the actual size; flip one byte keeping the size ⇒ message contains both hashes; `hidden_size` edited to 768 in `config.json` (hash then differs — assert the hash error fires first, so also test with `verify_files` skipped via a `#[doc(hidden)]` `load_unverified_for_test` **only if** the plan's implementation exposes one; otherwise assert the hash error and stop); a missing `tokenizer.json` ⇒ `Error::Model` naming it
- [X] T018 [P] Write `crates/xtriever-rerank/tests/score_golden.rs` (`#[ignore]`) covering **US1 scenarios 2, 4, 5** and FR-005/FR-006 (SC-001): for every golden pair, `tokenize_for_test(query, passage)` equals the fixture's `input_ids`/`token_type_ids` **first** (a mismatch names the pair before any score is compared); then `score(query, passage)` within `tolerance_abs` of the golden; for every query the descending-score order of `score` equals `order` exactly; the five named cases are asserted present and the `over-length` pair has 512 ids; the `empty-passage` pair's ids equal `[CLS] query [SEP]` (no second `[SEP]`)
- [X] T019 [P] Write `crates/xtriever-rerank/tests/score_determinism.rs` (`#[ignore]`) covering **US1 scenario 3** and FR-005 (SC-002): over the golden set, `rerank` in one call, in per-pair calls, and in reversed order gives bit-identical `Some` values (`to_bits`); a child process (`std::process::Command` on the test binary with `--exact` and `RAYON_NUM_THREADS` 1 then 4, the 004 `thread_child` pattern) prints the bits of every score; parent compares — 0 differing bits
- [X] T020 [P] Write `crates/xtriever-rerank/tests/budget_time.rs` (`#[ignore]`) covering **US2 scenario 2** (SC-003 time part): 40 passages of ~600 words each (max length), `Budget { max_time: Some(100 ms), max_items: None }` ⇒ scored count `≥ 1` and `< 40`, the `Some`s form a prefix; `max_items: Some(3)` with no time limit ⇒ exactly 3
- [X] T021 [P] Write `crates/xtriever-rerank/tests/load_paths.rs` (`#[ignore]`, `#![cfg(feature = "mmap")]`) covering FR-003 and ADR-0009 condition 3: `load(dir, Buffered)` and `load(dir, Mmap)` score every golden pair bit-identically; `load_path()` reports each

### Tests — `xtriever-pipeline` (red) and `xtriever-eval` (red)

- [X] T022 [P] Extend `crates/xtriever-pipeline/tests/support/mod.rs` with stub re-rankers (research D13): `TableReranker { scores: BTreeMap<u32, f32>, limit: Option<usize>, calls: Arc<Mutex<Vec<Budget>>> }` (`model_id "table-reranker"`; scores by `Passage.id`, unknown id ⇒ `Error::Model`; returns `Some` for the first `limit` passages and `None` after; records every `Budget` received), `FailingReranker` (`Err(Error::Model)`), `WrongLengthReranker` (one entry short), `NanReranker` (`Some(f32::NAN)` first), and `rerank_options(d) -> SearchOptions` helper; write `tests/rerank_golden.rs`: for every case in `pipeline_order.json`, `order_reranked(fused as (DocId, f64 by position), scores, k)` equals `expected` in ids, order and scores (SC-004); the ten names asserted present; `tests/rerank_prop.rs` (proptest ≥ 500 cases): output ids ⊆ fused, no duplicates, `len == min(k, fused.len())`; the `Some`-scored prefix is sorted `(score DESC, id ASC)`; the suffix equals the fused order with the scored ids removed; `scores` all `None` ⇒ output equals fused truncated
- [X] T023 [P] Write `crates/xtriever-pipeline/tests/rerank.rs` covering **US3 scenarios 1, 2, 4, 5, 6** and FR-009/FR-011–FR-013 (SC-004): build the 005 fixture index (`support::build_from_fixture`), `set_reranker(Some(Box::new(TableReranker)))` with scores that reverse the fused order; `search(text, None, 10, rerank_options(5))` ⇒ the first 5 fused ids re-ordered by the stub's scores then fused ids 6–10, `stages.rerank == Some(RerankReport { candidates: 5, scored: 5, skipped: None })`, `rerank_score` `Some` on exactly those 5; `TableReranker { limit: Some(2) }` ⇒ 2 scored first, then the 3 unscored selected candidates in fused order, then the rest, `scored == 2`; `k = 3, d = 10` ⇒ 3 hits drawn from the re-ordered top 10 (a candidate at fused rank 7 with the top stub score appears first); `d = 0` and `set_reranker(None)` ⇒ `hits` equal (ids, scores, order, `rerank_score: None`) to a search without a re-ranker and `stages.rerank == None`; `d` larger than the fused list ⇒ `candidates == len`; two hits with equal stub scores ordered by ascending id; same search twice ⇒ identical responses; every hit's `text` equals the fixture passage for that id; the `Budget` the stub received has `max_items == opts.budget.max_items`
- [X] T024 [P] Write `crates/xtriever-pipeline/tests/passages.rs` covering FR-010 and edge cases (Phase 5): after build + commit, reopen and `search` with `explain` ⇒ every hit's `text` equals `title + " " + text` of the fixture document (the dense passage); replace one document's text, commit, reopen ⇒ the new text on its hit, old text gone; delete a document, commit, reopen ⇒ `open` succeeds (five counts agree); a document whose text fields are empty ⇒ `text == ""` on its hit and searching with a re-ranker attached still `Ok`; a 005-shaped directory (write the descriptor with `"format_version": 1` and no `passages.bin`) ⇒ `open` is `Corrupt` whose message contains `1`, `2` and `rebuild`; `passages.bin` truncated by one byte ⇒ `Corrupt` naming the file; `passages.bin` whose header `count` is off by one ⇒ `Corrupt` naming both counts; a `commit.pending` left behind is still refused first
- [X] T025 [P] Extend `crates/xtriever-pipeline/tests/degrade.rs` covering **US3 scenario 3, 6** and FR-013–FR-015 (SC-005): `FailingReranker` ⇒ default mode `Ok`, `hits` equal to the no-re-ranker response, `stages.rerank == Some(RerankReport { candidates: 0, scored: 0, skipped: Some(StageError(..)) })`; strict ⇒ `Err(Error::Model)`; `WrongLengthReranker` and `NanReranker` ⇒ `Err(Error::Model)` in **both** modes; stub clock `|| 500 ms` with `max_time = 100 ms` and a `TableReranker` ⇒ dense degraded (005) **and** the re-ranker not called (calls empty) with `stages.rerank.skipped == Some(BudgetExceeded{..})`; a clock returning 0, 0, 30 ms on successive calls (A, B, C) with `max_time = 100 ms` ⇒ the stub receives `max_time == Some(70 ms)`; strict + spent at C ⇒ `Err(BudgetExhausted)` whose message contains `rerank`; `max_time` set with `elapsed: None` ⇒ the stub receives `max_time == None` and `time_limit_ignored == true`; open with `FailingEmbedder` + `TableReranker` ⇒ dense degraded, re-ranker **ran** over the lexical list (`stages.rerank.scored > 0`, `degraded.stage == "dense"`)
- [X] T026 [P] Extend `crates/xtriever-pipeline/tests/explain.rs` covering **US4 scenarios 1–3** and FR-016 (SC-006): with a `TableReranker { limit: Some(3) }` and `explain`, the three scored hits carry `rerank_score == Some(stub score)` and `rerank_rank` 1..=3 in output order, every other hit `None`/`None`; `features()` yields seven names exactly `bm25.score, bm25.rank, dense.score, dense.rank, fused.score, rerank.score, rerank.rank` with `NaN` for the absent re-rank values; the same search without `explain` gives identical `hits` (ids, `score`, `rerank_score`, order); with `FailingReranker` and `explain` ⇒ `Ok`, every hit's re-rank fields `None`; extend `tests/model_roundtrip.rs` (`#[ignore]`): attach `MiniLmCrossEncoder::load(rerank model dir, Buffered)` to the real-embedder index, search 5 short documents with `rerank_depth 5` ⇒ `stages.rerank.scored == 5`, the obvious document first, `rerank_score` finite; write `crates/xtriever-eval/tests/rerank_run.rs` (offline) covering **US5 scenarios 1–2** at the library level and FR-017/FR-018: `RerankConfig::hybrid_rerank_v1()` has `rerank_depth 20`, `hybrid == HybridConfig::hybrid_baseline_v1()`, `name "hybrid-rerank-v1"`; `validate()` rejects `rerank_depth 0` and `rerank_depth > hybrid.k`; a `StageInfo` with `reranker_model_id`/`rerank_depth` round-trips and one without omits both keys (serialise, assert absent); `Observations` likewise for `rerank_ms`, `rerank_pairs`, `rerank_model_bytes_buffered`, `rerank_model_bytes_mmapped`; the three 005 baseline files still deserialise and re-serialise byte-identically; `compare` of a hybrid and a rerank report yields rows for both metrics with both names
- [X] T027 Run `cargo nextest run -p xtriever-rerank --no-fail-fast`, `cargo nextest run -p xtriever-pipeline --no-fail-fast` and `cargo nextest run -p xtriever-eval --no-fail-fast`: `fixtures_valid` and `model_pins` **pass**, every 005 suite still passes, every other new test **fails** on `not implemented` or an assertion against a scaffold value, no failure names a fixture, parse or hash problem; `cargo nextest run -p xtriever-rerank --run-ignored only --no-fail-fast` fails on `not implemented` only. Commit red (Rule 4). **Checkpoint — PR 1.**

---

## Phase 3: User Story 1 — A query and a passage get the reference model's relevance score (Priority: P1) 🎯 MVP

**Goal**: `MiniLmCrossEncoder::load` verifies and asserts the pinned model, `score` reproduces
the reference logit per pair, bit-identically across arrangements, thread counts and load paths.

**Independent Test**: `tests/{model_load,score_golden,score_determinism,load_paths}.rs` green
with the model files present.

- [X] T028 [US1] Implement `crates/xtriever-rerank/src/model.rs::verify_files` (spec FR-003): the 004 implementation verbatim in shape — size before content, SHA-256 streamed in 1 MiB chunks, first failure wins, `Error::Model` naming the path and both sizes or both hashes
- [X] T029 [P] [US1] Implement `crates/xtriever-rerank/src/bytes.rs` (research D3, ADR-0009): `Bytes { Owned(Vec<u8>), #[cfg(feature = "mmap")] Mapped(memmap2::Mmap) }`, `as_slice`, `read(path, LoadPath)`, and `map_readonly` — the crate's **one** `unsafe` block: `#[cfg(feature = "mmap")] #[allow(unsafe_code)]` on the function only, a `// SAFETY (ADR-0009, ADR-0007 condition 2)` comment stating the real invariant (the weights are hash-verified and opened read-only; this crate never writes them; the external-writer precondition is the caller's, documented on `LoadPath::Mmap`)
- [X] T030 [US1] Implement loading in `crates/xtriever-rerank/src/scorer.rs::load(dir, load_path)` (data-model "Validation at load", research D1/D2): `verify_files` → `load_config` (parse `Config`; assert `hidden_size == 384`, `vocab_size == 30_522`, `pad_token_id == 0`, `max_position_embeddings >= 512`, raw `model_type == "bert"`, raw `architectures` contains `"BertForSequenceClassification"`, raw `id2label` has exactly one entry — each failure `Error::Model` naming the key and both values) → `load_tokenizer` (`Tokenizer::from_bytes`, `with_truncation(Some(TruncationParams { max_length: 512, ..Default::default() }))`, **no padding**, assert `[PAD] → 0`, `[CLS] → 101`, `[SEP] → 102`) → `bytes::read(weights, load_path)` → `assert_weights_header` (exactly 105 `F32`; only `bert.embeddings.position_ids` may be `I64`; `classifier.weight` present with shape `[1, 384]`) → `VarBuilder::from_slice_safetensors(bytes, DTYPE, &Device::Cpu)` → `BertModel::load(vb.pp("bert"), &config)`, `candle_nn::linear(384, 384, vb.pp("bert.pooler.dense"))`, `candle_nn::linear(384, 1, vb.pp("classifier"))`; drop the bytes; store `load_path`
- [X] T031 [US1] Implement `encode`, `score` and `tokenize_for_test` in `crates/xtriever-rerank/src/scorer.rs` (research D4/D5): `encode(query, passage)` = `tokenizer.encode(query, true)` when `passage.is_empty()`, else `tokenizer.encode((query, passage), true)`; `score`: `input_ids` and `token_type_ids` tensors `(1, seq)` from the encoding, `encoder.forward(&ids, &type_ids, None)`, `hidden.narrow(1, 0, 1)?.squeeze(1)?` → `pooler.forward` → `tanh` → `classifier.forward` → `squeeze(1)?.squeeze(0)?.to_scalar::<f32>()`; `if !logit.is_finite()` ⇒ `Error::Model` naming the value (FR-008); every candle error mapped through `model_err` with the step name
- [X] T032 [US1] Run `cargo nextest run -p xtriever-rerank --run-ignored only -E 'test(model_load) | test(score_golden) | test(score_determinism)'` and `cargo nextest run -p xtriever-rerank --features mmap --run-ignored only -E 'test(load_paths)'` — **green**; then `RAYON_NUM_THREADS=1` and `=4` for `score_determinism` (the child-process test) — 0 differing bits. A tolerance or order failure is ⛔ — report the pairs and the differences (candidates: the head, `longest_first`, the empty-passage rule) and stop; a thread-count difference is a finding, not a widened tolerance
- [X] T033 [US1] Delete the `NotImplemented` bodies in `crates/xtriever-rerank/src/{model,scorer}.rs`; `missing_docs` clean; `RUSTFLAGS="-D warnings" cargo clippy -p xtriever-rerank --all-targets` and `--features mmap` clean

---

## Phase 4: User Story 2 — Scoring stops at the budget and says which passages it did not reach (Priority: P1)

**Goal**: The `Reranker` impl scores in input order under item and time limits and returns
`None` for the rest.

**Independent Test**: `tests/budget.rs` green offline; `tests/budget_time.rs` green with the
model. Completes **PR 2**.

- [X] T034 [US2] Implement `crates/xtriever-rerank/src/budget.rs::rerank_with` (research D6, contract "`rerank`"): `out = vec![None; passages.len()]`; `let start = Instant::now()`; for `(i, p)`: `if budget.max_items.is_some_and(|n| i >= n) { break }`; `if budget.max_time.is_some_and(|t| start.elapsed() >= t) { break }`; `out[i] = Some(scorer(query, p.text)?)`; return `out` — the only clock use in the crate
- [X] T035 [US2] Implement `Reranker for MiniLmCrossEncoder` in `crates/xtriever-rerank/src/scorer.rs`: `model_id() -> MODEL_ID`; `rerank(query, passages, budget)` = `budget::rerank_with(&|q, p| self.score(q, p), query, passages, budget)`
- [X] T036 [US2] Run `cargo nextest run -p xtriever-rerank` (green offline) and `cargo nextest run -p xtriever-rerank --run-ignored only` (green, incl. `budget_time`); `grep -rn 'Instant' crates/xtriever-rerank/src/` names `budget.rs` only
- [X] T037 [US2] Delete every remaining `NotImplemented` in `crates/xtriever-rerank/src/`; `./scripts/check-no-stubs.sh` still FAILs only on the pipeline scaffold. **Checkpoint — PR 2.**

---

## Phase 5: Foundational — The passage store and pipeline format version 2 (Blocking Prerequisite for US3/US4)

**Purpose**: Every hybrid index stores its passage text (ADR-0008, research D7); `open` refuses
version 1; the five-count check. Nothing here re-ranks yet.

- [X] T038 Implement `crates/xtriever-pipeline/src/passages.rs::PassageStore` per data-model "Passage Store": `create(dir)` writes an empty store (count 0) via `.tmp` + `rename`; `open(dir, expected_count)` reads magic `XTPASS01`, `u64` LE header length, JSON `{format_version: 1, count}` (any other version ⇒ `Corrupt` naming both), `(count + 1)` `u64` LE offsets into `Vec<u64>` (checked: non-decreasing, `off[0] == 0`, `off[count] + text_start == file length` — otherwise `Corrupt` naming the file), `count == expected_count` or `Corrupt` naming both counts; `read(&self, id: DocId) -> Result<String>`: `id ≥ count` ⇒ `Corrupt("stage returned id {id} unknown to the passage store")`, else open the file, `seek` to `text_start + off[i]`, `read_exact` `off[i+1] − off[i]` bytes, `String::from_utf8` (`Corrupt` otherwise); `stage(&mut self, id, Option<String>)` into `pending: BTreeMap<u32, Option<String>>`; `commit(&mut self, next_len: usize)`: stream into `passages.bin.tmp` — for `i in 0..next_len`: pending entry (`None` ⇒ empty) else the previous generation's bytes if `i < count` else empty; write header, offsets, block; `sync_all`; `rename`; reload offsets; clear pending; checked arithmetic everywhere (`checked_add`, `usize::try_from`)
- [X] T039 Extend `crates/xtriever-pipeline/src/descriptor.rs` (data-model "HybridConfig / Descriptor"): field `rerank_depth: usize` after `rrf_k`; the version message becomes `"{path} is format version {found}, this build reads {FORMAT_VERSION}; rebuild the index"`; update the in-file tests (version 2 round trip, version 1 refused naming `1`, `2` and `rebuild`)
- [X] T040 Extend `crates/xtriever-pipeline/src/index.rs` (research D7/D8): field `passages: PassageStore`, field `reranker: Option<Box<dyn Reranker>>` (already added by T014), `HybridConfig::new` sets `rerank_depth: 20`; `create` → `PassageStore::create` and `rerank_depth` in the descriptor; `open_with` → after the marker check and before the stages, `PassageStore::open(dir, ids.len())` (add `IdMap::len()` = `external.len()` in `src/ids.rs`), then the four-count check unchanged (the store's count check is inside `open`); rebuild `HybridConfig` with `rerank_depth`; `stage_one` → `self.passages.stage(id.0, Some(passage_text))` — pass the passage text into `stage_one` (it is computed in `add` already; `add_embedded` must compute it too via `self.passage(&doc.fields)`); `delete` → `stage(id.0, None)`; `commit` → after `dense.commit()` and before `pending_ids.write`, `self.passages.commit(self.pending_ids.len())`; `Debug` prints `rerank_depth`
- [X] T041 Fill `HybridHit.text` in `crates/xtriever-pipeline/src/search.rs::{fuse, degraded}` via `self.passages.read(id)?` (one read per returned hit); keep `rerank_score: None` for now
- [X] T042 Run `cargo nextest run -p xtriever-pipeline`: `tests/passages.rs` **green**, all 005 suites green (`persist.rs`'s version test now expects the v2 message — adjust the *expected text* only, never the check), `rerank*`/`degrade`/`explain` extensions still red. **Checkpoint**: every hybrid index has its text; story work resumes.

---

## Phase 6: User Story 3 — The pipeline re-ranks its fused candidates and keeps a coherent order under any budget (Priority: P1)

**Goal**: Search step 9 — check point C, passages from the store, the remaining budget, the
ordering rule, per-stage degradation.

**Independent Test**: `tests/{rerank_golden,rerank_prop,rerank,degrade}.rs` green with the
stub re-rankers.

- [X] T043 [US3] Implement `crates/xtriever-pipeline/src/rerank.rs::order_reranked(fused, scores, k)` (research D8, the pure rule): scored = entries `i < scores.len()` with `Some`; sort scored by `(score DESC via partial_cmp on f32 — finite by validation, id ASC)`; unscored = the remaining entries in fused order; concatenate, truncate to `k`, return `(id, fused_score, Option<rerank_score>)`
- [X] T044 [US3] Implement step 9 in `crates/xtriever-pipeline/src/search.rs` (data-model "Search algorithm", contract "Semantics (changed rows)"): resolve `d = opts.rerank_depth.unwrap_or(self.config.rerank_depth)`; build the fused (or degraded) list to `max(k, d)` instead of `k` (change `fuse`/`degraded` to take the length and to defer hit construction: produce `Vec<(DocId, f64)>` plus the per-id explain data, build `HybridHit`s last); if `self.reranker.is_some() && d > 0 && !list.is_empty()`: check point C (`check_budget`; spent ⇒ strict `Err(BudgetExhausted("rerank stage: {elapsed} ms elapsed > {limit} ms limit"))`, default `RerankReport { 0, 0, skipped: Some(BudgetExceeded{..}) }`), read `min(d, len)` texts from the store into `Passage { id, text: &str }`s, `remaining = (limit, elapsed) both present ⇒ Some(limit.saturating_sub(elapsed()))`, else `None`; `Budget { max_items: opts.budget.max_items, max_time: remaining }`; `reranker.rerank(dense_text, &passages, &budget)`: `Err(e)` ⇒ strict `Err(e)`, default `skipped: Some(StageError(e.to_string()))` and the fused order; `Ok(scores)` with `scores.len() != passages.len()` or any non-finite `Some` ⇒ `Err(Error::Model { model: reranker.model_id(), message })` **in every mode**; else `order_reranked(&list, &scores, k)` and `RerankReport { candidates: passages.len(), scored: count of Some, skipped: None }`; otherwise `stages.rerank = None` and the list truncated to `k`; build `HybridHit`s with `rerank_score` and `text`
- [X] T045 [US3] Run `cargo nextest run -p xtriever-pipeline`: `rerank_golden`, `rerank_prop`, `rerank`, `degrade` **green** (the explain extensions may still be red); `search.rs`/`persist.rs`/`fusion_*` unchanged green

---

## Phase 7: User Story 4 — Every hit explains its re-rank score (Priority: P2)

**Goal**: `HitExplain` carries `rerank_score`/`rerank_rank`; `features()` has seven names;
explanation never changes hits.

**Independent Test**: `tests/explain.rs` green; `model_roundtrip` green with both models.
Completes **PR 3**.

- [X] T046 [US4] Fill `HitExplain.{rerank_score, rerank_rank}` in `crates/xtriever-pipeline/src/search.rs` when building hits after step 9: `rerank_rank` = 1-based position within the scored prefix of the output order, `None` for unscored hits and whenever the stage did not run; confirm `features()` in `src/types.rs` emits `(RERANK_SCORE, score or NaN)` and `(RERANK_RANK, rank or NaN)` as entries 6 and 7
- [X] T047 [US4] Run `cargo nextest run -p xtriever-pipeline`, `--features mmap`, and `--run-ignored only` (the model round trip with both models) — all **green**; delete the `NotImplemented` bodies in `crates/xtriever-pipeline/src/passages.rs`; `./scripts/check-no-stubs.sh` **passes**. **Checkpoint — PR 3.**

---

## Phase 8: User Story 5 — The re-ranked pipeline is measured as a delta against the guarded baseline (Priority: P2)

**Goal**: `hybrid-rerank-v1` in the harness; three baselines verified; `compare` tables against
`hybrid-baseline-v1`; the SC-008 verdict.

**Independent Test**: `crates/xtriever-eval/tests/rerank_run.rs` green offline; SciFact end to
end twice, byte-identical, 0 embedded, `--verify-rerank` PASS.

- [X] T048 [US5] Implement `RerankConfig` in `crates/xtriever-eval/src/run.rs` per contract "`xtriever-eval` (additive)": `{ name, hybrid: HybridConfig, rerank_depth }`, `validate()` (`1 ≤ rerank_depth ≤ hybrid.k`, `hybrid.validate()`), `hybrid_rerank_v1()` (`hybrid_baseline_v1()`, depth 20, name `"hybrid-rerank-v1"`); and in `src/report.rs`: `StageInfo.{reranker_model_id: Option<String>, rerank_depth: Option<usize>}` and `Observations.{rerank_ms, rerank_pairs, rerank_model_bytes_buffered, rerank_model_bytes_mmapped}: Option<u64>` — all `#[serde(default, skip_serializing_if = "Option::is_none")]`, trailing in key order; update the contract note in `specs/003-eval-harness/contracts/eval-harness.md` the way 004/005 did
- [X] T049 [US5] Extend `crates/xtriever-eval/examples/beir.rs` per contract "`beir` example commands": `cargo add -p xtriever-eval --dev --path crates/xtriever-rerank --features mmap`; `--config hybrid-rerank-v1` reuses `evaluate_hybrid` with a `rerank: Option<&RerankConfig>` parameter: after `commit`, `index.set_reranker(Some(Box::new(TimedReranker::new(MiniLmCrossEncoder::load(&rerank_model_dir(a), load_path)?))))` where `TimedReranker` (in the example) implements `Reranker` by delegating and accumulating wall time and `Some` count in `Arc<Mutex<(Duration, u64)>>`; `SearchOptions { explain: true, rerank_depth: Some(cfg.rerank_depth), .. }`; `--rerank-model-dir` (default `reference/models/ms-marco-MiniLM-L-6-v2`); the `--export-explain` line gains `"rerank": [[rank, ext id, score], …]` and `"hits"`; stderr prints per-query total and re-rank ms; `stage = StageInfo { kind: "hybrid-rerank", …, baseline: "guarded", reranker_model_id: Some(MODEL_ID), rerank_depth: Some(20) }`; `model-memory --model embedder|rerank --load-path P` (default `embedder`, so the 004 command is unchanged) loads the named model once and prints the same line as today; timings never enter the report (byte-identical re-runs)
- [X] T050 [US5] Run `cargo nextest run -p xtriever-eval` — **green**; `cargo tree -p xtriever-eval -e normal | grep -E 'candle|tantivy|memmap2|xtriever-pipeline|xtriever-rerank'` prints nothing; `cargo tree -p xtriever-pipeline -e normal | grep xtriever-rerank` prints nothing; the lexical smoke (quickstart Step 5) still exits 0
- [X] T051 [US5] Produce the SciFact re-ranked baseline (quickstart Step 6, `RAYON_NUM_THREADS=4`): `run --dataset scifact --config hybrid-rerank-v1 --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index/scifact --out specs/006-rerank-stage/baselines/hybrid-rerank-v1.scifact.json --export-run target/xt-rerank-run.scifact.jsonl --export-explain target/xt-rerank-explain.scifact.jsonl`; `gen_003_fixtures.py --verify-run` PASS (SC-007); `gen_006_fixtures.py --verify-rerank` PASS; re-run to `/tmp/rerank-scifact-again.json` ⇒ `diff` identical and `embedded 0`; `mean_recall_100` byte-equal to `hybrid-baseline-v1.scifact.json`'s
- [X] T052 [US5] Produce the NFCorpus and FiQA re-ranked baselines the same way into `specs/006-rerank-stage/baselines/hybrid-rerank-v1.{nfcorpus,fiqa}.json`, each `--verify-run` and `--verify-rerank` PASS, Recall@100 byte-equal; FiQA under `/usr/bin/time -l` capturing the stderr timings (total and re-rank ms per query, pair count), `maximum resident set size`, `du -sk target/xt-rerank-index/fiqa`, `stat -f %z target/xt-rerank-index/fiqa/passages.bin` (SC-010 inputs)
- [X] T053 [US5] Run the three `compare`s (`hybrid-baseline-v1.$d.json` → `hybrid-rerank-v1.$d.json`) and apply **SC-008**: re-ranked nDCG@10 ≥ `hybrid-baseline-v1` (0.689727 / 0.345008 / 0.369210) on at least 2 of 3. Record the verdict and the three tables in `specs/006-rerank-stage/report.md`. A miss is ⛔ — stop, report the numbers and the candidates to investigate (depth, `longest_first` truncation of long passages, a head defect isolated by the parity test), and wait for the human; never adjust the bar. Confirm `beir delta` still refuses the mixed pair (exit 1)

---

## Phase 9: User Story 6 — Cost is observed, not assumed (Priority: P3)

**Goal**: Per-query and per-pair re-rank times, pair count, peak RSS, directory size with the
store, and the fresh-process model load per path, with method.

**Independent Test**: the FiQA report's `observations` carry every new key with a method string
naming the commands.

- [X] T054 [US6] Measure the cross-encoder's fresh-process load (ADR-0009 condition 5, research D11): `for p in buffered mmap; do for i in 1 2 3; do /usr/bin/time -l cargo run --release -p xtriever-eval --example beir -- model-memory --model rerank --load-path $p; done; done` at `RAYON_NUM_THREADS=4`; record the three peaks per path and the medians; no deletion clause applies — record the ratio beside ADR-0007's 13 %
- [X] T055 [US6] Fill the FiQA re-ranked report's `observations` by hand (`index_dir_bytes` = `du` of the directory, `peak_rss_bytes`, `embed_corpus_ms: 0`, `search_ms`, `rerank_ms`, `rerank_pairs`, `rerank_model_bytes_buffered`, `rerank_model_bytes_mmapped`, `method` naming every command and stating that the RSS is the harness's, not the pipeline's) and re-run `--verify-run` on the edited file; compute per-query and per-pair re-rank time for `report.md`

---

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Docs, report, PR description, the full gate. Completes **PR 4**.

- [X] T056 [P] Write `specs/006-rerank-stage/report.md`: verdict; nextest summaries (offline, mmap, ignored); the three re-ranked baselines with `--verify-run` and `--verify-rerank`; the three `compare` tables and the SC-008 verdict; Recall@100 unchanged check; observations (FiQA per-query/per-pair re-rank time, pairs, peak RSS, directory and `passages.bin` size, model memory per path) with method; the FR-022 diff (empty); the constitution v1.3.0 / ADR-0008 / ADR-0009 governance record; findings
- [X] T057 [P] Write `crates/xtriever-rerank/src/lib.rs` crate docs: the pinned model and identity string, the composed head and why (no head in candle 0.9.2), per-pair scoring at own length and the determinism promise, the budget loop and the clock, the empty-passage rule, the `mmap` feature and its precondition (ADR-0009); update `crates/xtriever-pipeline/src/lib.rs` docs: `passages.bin` and format version 2 (ADR-0008), the five-count check, step 9, the ordering rule, `RerankReport`, `rerank.rank`
- [X] T058 [P] Write `specs/006-rerank-stage/pr-description.md`: summary, four-commit split with hand-written line counts, the baseline table, **the delta section** (three `compare` tables + SC-008 verdict), observations, the governance line (constitution v1.3.0, ADR-0008, ADR-0009; one `unsafe` block in the new crate behind `mmap`), the standing CI rule (SciFact lexical smoke only, no model in CI)
- [X] T059 Run the full gate from quickstart Step 7: `cargo fmt --all --check`; `RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets`; `RUSTFLAGS="-D warnings" cargo clippy -p xtriever-pipeline --features mmap --all-targets`; `RUSTFLAGS="-D warnings" cargo clippy -p xtriever-rerank --features mmap --all-targets`; `cargo nextest run --workspace`; `cargo nextest run -p xtriever-pipeline --features mmap`; `cargo nextest run -p xtriever-rerank --features mmap`; `cargo nextest run -p xtriever-rerank --run-ignored only`; `cargo nextest run -p xtriever-rerank --features mmap --run-ignored only`; `cargo nextest run -p xtriever-pipeline --run-ignored only`; `cargo deny check`; `cargo check --workspace --target aarch64-apple-ios` / `aarch64-apple-ios-sim` / `aarch64-linux-android`; `cargo check --workspace --target wasm32-unknown-unknown` (best-effort, record the failure point); `./scripts/check-no-stubs.sh`; `./scripts/check-toolchain.sh`; `grep -rn 'unsafe' crates/xtriever-rerank/src/` (exactly `bytes.rs`: the allow attribute and the one block); `grep -rn 'unsafe' crates/xtriever-pipeline/src/` (nothing); `grep -rn 'Instant\|SystemTime\|std::thread' crates/xtriever-pipeline/src/` (nothing); the eval and pipeline purity `cargo tree`s; the FR-022 `git diff --stat main -- crates/xtriever-core crates/xtriever-lexical crates/xtriever-dense deny.toml crates/xtriever-eval/src/metrics.rs crates/xtriever-eval/src/dataset.rs` (empty). Paste the results into `report.md` and `pr-description.md`. Any failure ⛔ — stop and report

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T002; T003 ‖ T004 ‖ T005 ‖ T006 (T004 needs T003)
- **Oracle & red suite (Phase 2)**: T007 → T008 → T009 → T010 → T011 → T012 (needs T004's fetched model); T013 after T001; T014 independent; T015–T021 after T012 and T013; T022–T026 after T012 and T014; T027 last → **PR 1**
- **US1 (Phase 3)**: T028 ‖ T029 → T030 → T031 → T032 → T033
- **US2 (Phase 4)**: T034 → T035 → T036 → T037 (needs US1) → **PR 2**
- **Foundational (Phase 5)**: T038 ‖ T039 → T040 → T041 → T042 (independent of Phases 3–4 in code; ordered after them for the PR split)
- **US3 (Phase 6)**: T043 → T044 → T045 (needs Phase 5)
- **US4 (Phase 7)**: T046 → T047 (needs US3) → **PR 3**
- **US5 (Phase 8)**: T048 → T049 → T050 → T051 → T052 → T053 (needs US2 and US4)
- **US6 (Phase 9)**: T054 ‖ T055 (needs T052)
- **Polish (Phase 10)**: T056 ‖ T057 ‖ T058 → T059 → **PR 4**

### Rule 6 stop-points (⛔)

T032 (golden tolerance / order, or a thread-count bit difference); T053 (SC-008: re-ranked
nDCG@10 below the fused baseline on ≥ 2 datasets); T059 (any gate failure). The response is a
report, never a looser test, a wider tolerance or a moved bar.

### Parallel Opportunities

- Phase 1: T003 ‖ T005 ‖ T006 (then T004)
- Phase 2: T015 ‖ T016 ‖ T017 ‖ T018 ‖ T019 ‖ T020 ‖ T021 ‖ T022 ‖ T023 ‖ T024 ‖ T025 ‖ T026 (twelve test files)
- Phase 3: T028 ‖ T029
- Phase 5: T038 ‖ T039
- Phase 9: T054 ‖ T055
- Phase 10: T056 ‖ T057 ‖ T058

---

## Parallel Example: Phase 2 red suite

```text
after T012 (fixtures), T013 (rerank scaffold) and T014 (pipeline scaffold):
  T015 support + fixtures_valid + model_pins   T016 budget          T017 model_load
  T018 score_golden                            T019 score_determinism  T020 budget_time
  T021 load_paths                              T022 stubs + rerank_golden + rerank_prop
  T023 rerank                                  T024 passages        T025 degrade
  T026 explain + model_roundtrip + eval rerank_run
then T027 (red checkpoint, PR 1)
```

## Implementation Strategy

1. **PR 1** (Phases 1–2): manifest, fetch script, oracle, goldens, scaffolds, red suite.
2. **PR 2** (Phases 3–4) — **MVP**: the cross-encoder — reference scores, determinism, both load
   paths, the budget loop. Usable on its own through the core trait.
3. **PR 3** (Phases 5–7): the passage store and format v2, search step 9, degradation, explain —
   the product.
4. **PR 4** (Phases 8–10): the harness, the three re-ranked baselines, the first delta against a
   guarded number, the SC-008 verdict, the cost observations, the gate.
