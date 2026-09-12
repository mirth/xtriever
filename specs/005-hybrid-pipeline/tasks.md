# Tasks: The Hybrid Pipeline

**Input**: Design documents from `/specs/005-hybrid-pipeline/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/hybrid-pipeline.md](./contracts/hybrid-pipeline.md),
[quickstart.md](./quickstart.md)

**Tests**: **Mandatory** (Principle II, spec FR-029). Phase 2 lands every test red: offline tests
fail at runtime on the `NotImplemented` scaffold; the one model-backed test is `#[ignore]`. Story
phases contain implementation only and end with the task that turns their tests green.

**Organization**: Setup → Oracle & red suite → Foundational → US1 → US2 → US4 → US3 → US5 →
Polish. US4 (explain) precedes US3 (degrade) because the degraded response is defined in terms of
the explanation fields. The four-PR split from plan.md: **PR 1** = Phases 1–2, **PR 2** = Phases
3–4, **PR 3** = Phases 5–7, **PR 4** = Phases 8–9.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US5 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers and data-model
  sections — read them before implementing. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

Crate `crates/xtriever-pipeline/` (`src/`, `tests/`); harness `crates/xtriever-eval/`
(`src/run.rs`, `src/report.rs`, `examples/beir.rs`, `tests/`); generator
`reference/gen_005_fixtures.py`; fixtures `reference/fixtures/005/`; the 004 embedding cache
`target/xt-dense-cache/<dataset>/`; hybrid indexes for the baseline `target/xt-hybrid-index/`;
baselines `specs/005-hybrid-pipeline/baselines/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies (all already in the workspace) and the stub check.

- [ ] T001 Add dependencies to `crates/xtriever-pipeline/Cargo.toml` **with `cargo add`** (research D9): `cargo add -p xtriever-pipeline --path crates/xtriever-lexical`, `cargo add -p xtriever-pipeline --path crates/xtriever-dense`, `cargo add -p xtriever-pipeline serde --features derive`, `cargo add -p xtriever-pipeline serde_json thiserror`, `cargo add -p xtriever-pipeline --dev tempfile proptest serde_json sha2`; then edit: `[features] default = []` and `mmap = ["xtriever-dense/mmap"]` with a one-line comment pointing at ADR-0007, `description = "Xtriever: hybrid pipeline — lexical + dense fusion, id mapping, degradation, explain"`, keep `[lints] workspace = true`
- [ ] T002 Verify after T001: `cargo tree -p xtriever-pipeline -e normal --prefix none | grep -Ei '(-sys|^cc |onig|openssl)'` prints nothing; `cargo deny check` passes with no `deny.toml` change; `cargo check -p xtriever-pipeline --target aarch64-apple-ios` and `--target aarch64-linux-android` pass (FR-030 baseline)
- [ ] T003 [P] Extend `scripts/check-no-stubs.sh` to scan `crates/xtriever-pipeline/src/` for `NotImplemented` (loop list and PASS message)

---

## Phase 2: Oracle & Red Suite (Blocking Prerequisite)

**Purpose**: Fusion and hybrid goldens, the crate scaffold, and every test — red. **PR 1.**

**⚠️ CRITICAL**: No story implementation begins until T019 confirms the suite fails for want of an
implementation, not for want of a fixture.

### Python oracle and goldens

- [ ] T004 Create `reference/gen_005_fixtures.py` scaffold: interpreter guard (3.12 + venv, the 001 pattern; runs in `reference/.venv-003`), `--seed`, `--out`, `write_json`, `sha256_file`, `manifest.json` with per-file SHA-256 and `generator_sha256`, `--refresh-manifest`; a module-level `rrf(lexical_ids, dense_ids, k=60)` that computes `score[d] = (1/(k+rank_lex) if d in lexical else 0.0) + (1/(k+rank_dense) if d in dense else 0.0)` **adding the lexical term first, then the dense term** (research D5 — the Rust side adds in the same order so ties are bit-exact), ranks 1-based, sorted by `(-score, id)`
- [ ] T005 In `reference/gen_005_fixtures.py`, emit `fusion.json` per data-model "Fusion Goldens": `rrf_k: 60`, `score_abs_tol: 1e-9`, and the ten required cases by id — `both_lists` (overlapping lists, 10 + 10 ids, k 10), `lexical_only`, `dense_only`, `disjoint`, `identical`, `reversed` (the same ids in opposite orders — every doc ties, order by id), `tie_at_k` (constructed so ranks `k` and `k+1` have the same rank pair), `depth_below_k` (5 + 5 ids, k 10 ⇒ ≤ 10 results), `k_zero`, `both_empty` — each with `lexical`, `dense` (ids in rank order), `k`, `expected: [{id, score}]`; assert every exact tie is between docs with identical rank pairs
- [ ] T006 In `reference/gen_005_fixtures.py`, emit `hybrid.json` per data-model "Hybrid fixture": `dim 8`, `fingerprint "table-fp"`, a schema with `title` and `text` (`Text("standard")`, boosts 2.0 / 1.0, not stored) and `source` (`Keyword`, indexed); ~40 documents with external ids `d001…`, random words from a bank plus planted terms so lexical queries hit known subsets, `source` ∈ {`a`,`b`,`c`}, at least 6 documents chunked (`chunk: {parent, ordinal, byte_range}`, two or three chunks per parent), each with `passage = title + " " + text` and a seeded `float32` 8-d `vector`; ~8 queries with `text`, an 8-d `vector`, `filter` ∈ {null, `Eq(source, a)`, `In(source,[a,b])`, `Not(Eq(source,c))`, `Ids([...])` referencing known **internal** positions is NOT allowed — filters name fields only}, and `expected_dense`: the 004-style `fsum` cosine oracle over all documents (or the filtered subset), top 100, with the same tie/margin rules as 004 (`tie_margin 1e-6`, reroll on ambiguity)
- [ ] T007 In `reference/gen_005_fixtures.py`, implement `--verify-fusion <explain.jsonl>`: each line `{"query_id", "lexical": [ids], "dense": [ids], "fused": [ids]}`; recompute `rrf(lexical, dense, 60)` and compare the id order with `fused` for every query (a fused list may be shorter — compare the prefix of the same length); print the number of queries checked and exit 1 on any disagreement
- [ ] T008 Run `reference/.venv-003/bin/python reference/gen_005_fixtures.py --seed 5 --out reference/fixtures/005/`; commit `fusion.json`, `hybrid.json`, `manifest.json`; confirm `git check-attr text eol -- reference/fixtures/005/hybrid.json` reports `text: unset`

### Crate scaffold

- [ ] T009 Create the scaffold in `crates/xtriever-pipeline/src/lib.rs` + `src/scaffold.rs`: the full public surface from [contracts/hybrid-pipeline.md](./contracts/hybrid-pipeline.md) (`HybridConfig` + `new`, `SourceDocument`, `SearchOptions` + `Default`, `Response`, `HybridHit`, `HitExplain` + `features`, `StageReport`, `Degradation`, `DegradeReason`, `HybridIndex::{create, open, open_mapped (cfg mmap), config, embedder, len, is_empty, contains, add, add_embedded, delete, commit, search, search_lexical}`, `rrf`, `FORMAT_VERSION`) — **real** data types (they are plain structs), every fallible fn returning `Err(Error::backend(NotImplemented("…")))`, `rrf` returning an empty `Vec`, `len` 0, `contains` false; `missing_docs` satisfied; deleted by T045

### Tests (red except `fixtures_valid`)

- [ ] T010 [P] Write `crates/xtriever-pipeline/tests/support/mod.rs` (fixture loaders for `reference/fixtures/005/`; `TableEmbedder` — `Embedder` impl over the fixture's `passage`/query-text → vector table, `dim 8`, `Metric::Cosine`, fingerprint `"table-fp"`, unknown text ⇒ `Error::Model` naming the text; `FailingEmbedder` — every `embed` ⇒ `Error::Model("stub failure")`, same fingerprint/dim so an index built with `TableEmbedder` opens with it; `fixture_config()` → `HybridConfig` from the fixture schema with `dense_fields [title, text]`; `build_from_fixture(dir) -> HybridIndex` (add all documents, one commit); `open_stages(dir) -> (TantivyIndex, FlatIndex)` opening `<dir>/lexical` and `<dir>/dense` **directly** for the composition check; `parse_filter`) and `tests/fixtures_valid.rs` asserting every `manifest.json` hash and the generator hash — **green** at the red checkpoint
- [ ] T011 [P] Write `crates/xtriever-pipeline/tests/fusion_golden.rs` covering **US2 scenarios 2, 4** and FR-009/FR-010: for every case in `fusion.json`, `rrf(&lexical_hits, &dense_hits, 60, k)` (build `Hit`s with dummy scores — RRF ignores them) equals `expected` in ids and order with scores within 1e-9 (SC-001); the ten case ids are asserted present; `tests/fusion_prop.rs` (proptest ≥ 500 cases): output ordered `(score DESC, id ASC)`; output ids ⊆ lexical ∪ dense; length = `min(k, |lexical ∪ dense|)`; a doc in both lists scores ≥ the same doc in either list alone; swapping the lists' *contents* between the two arguments leaves the fused **order** unchanged when both lists are identical
- [ ] T012 [P] Write `crates/xtriever-pipeline/tests/ingest.rs` covering **US1 scenarios 1–3, 6** and FR-002/FR-007 (SC-002): 1,000 synthetic documents with ids `doc-0000…` added in batches, one commit ⇒ `len() == 1000`, every id `contains`; re-adding 50 ids with new text then commit ⇒ `len()` unchanged and a search for the new text returns them once; deleting 100 known + 5 unknown ids then commit ⇒ `len() == 900`, `contains` false, no error; an empty external id ⇒ `Error::Schema`; the same id twice in one batch ⇒ one document with the second's fields; chunked documents keep `chunk` on hits (search for their planted term, assert `parent`/`ordinal`); `is_empty` on a fresh index
- [ ] T013 [P] Write `crates/xtriever-pipeline/tests/persist.rs` covering **US1 scenarios 4–5** and FR-003/FR-005/FR-006 (SC-003): build from the fixture, search every fixture query with `explain`, drop, `open` with a fresh `TableEmbedder` ⇒ identical `hits` (ids, scores, order) and `contains` for every id; `open` with an embedder whose fingerprint differs (a `TableEmbedder` variant with `"other-fp"`) ⇒ `Error::FingerprintMismatch` naming both; descriptor with `format_version: 2` written by the test ⇒ `Corrupt` mentioning `2` and `1`; a descriptor whose `schema` differs ⇒ `Corrupt`; **partial commit**: build, add 3 documents, then simulate a crash after the lexical commit by calling a `#[doc(hidden)] pub fn commit_lexical_only_for_test(&mut self)` declared in the scaffold, drop, `open` ⇒ `Corrupt` whose message contains all four counts; a second handle opened before another handle's commit keeps its snapshot (`len` unchanged) until reopened; `create` on a non-empty directory ⇒ `Corrupt`
- [ ] T014 [P] Write `crates/xtriever-pipeline/tests/search.rs` covering **US2 scenarios 1, 3, 5, 6** and FR-008/FR-011–FR-013 (SC-004): **composition check** — for every fixture query, `pipeline.search(text, filter, 100, explain)` equals `rrf(lexical.search(Match(None, text), Some(&Filter::Ids(resolved)), 100), dense.search(&vector, Some(&set), 100), 60, 100)` with the stages opened directly via `support::open_stages` and the set from `lexical.resolve_filter` (no filter ⇒ no restriction); the `dense_rank`/`dense_score` in each hit's explanation reproduce `expected_dense` from the fixture (SC-003 of 004 through the pipeline); filtered result == unrestricted result restricted to the resolved set for every filter; `k == 0` ⇒ empty response with `lexical_candidates == 0`; `k = 3` ⇒ 3 hits; a filter resolving to nothing (`Eq(source, "zzz")`) ⇒ empty, both `*_candidates` 0; an empty query ⇒ `Ok` (dense-only ranking); a query with `depth` 2 and `k` 10 ⇒ ≤ 4 hits; two hits with equal fused score ordered by ascending `id`; chunk hits carry provenance and are not grouped (two chunks of one parent both present)
- [ ] T015 [P] Write `crates/xtriever-pipeline/tests/explain.rs` covering **US4 scenarios 1–3** and FR-019–FR-021 (SC-006): explained hit retrieved by both stages has `bm25_*` and `dense_*` `Some` with 1-based ranks equal to the positions in the direct stage lists and `fused` equal to `score`; a lexical-only hit has `dense_* == None` and `features()` yields `NaN` for `dense.score`/`dense.rank` with the five names exactly `bm25.score, bm25.rank, dense.score, dense.rank, fused.score`; searching without `explain` gives identical `hits` (ids, scores, order, `explain == None`) and a `StageReport` with `lexical_candidates`/`dense_candidates` counts
- [ ] T016 [P] Write `crates/xtriever-pipeline/tests/degrade.rs` covering **US3 scenarios 1–5** and FR-014–FR-018 (SC-005): open the fixture index with `FailingEmbedder`: default mode ⇒ `Ok`, `hits` == the direct lexical list (order and `score == f64::from(lexical score)`), `stages.degraded == Some(Degradation { stage: "dense", reason: StageError(..) })`, `dense_candidates == None`, explanations have `dense_* == None`; strict ⇒ `Err(Error::Model)`; a stub clock `|| Duration::from_millis(500)` with `budget.max_time = 100 ms` ⇒ degraded with `BudgetExceeded { elapsed_ms: 500, limit_ms: 100 }` and the dense stage **not called** (a counting `TableEmbedder`); a clock that returns 0 ms on the first call and 500 ms on the second (check point B) ⇒ degraded and the dense candidates discarded; strict + exceeded ⇒ `Err(Error::BudgetExhausted)`; `max_time` set with `elapsed: None` ⇒ not degraded and `time_limit_ignored == true`; `budget.max_items = 3` ⇒ `dense_candidates == Some(3)`; a lexical failure (search on a corrupted `lexical/` directory, or a `Filter` naming an unknown field) ⇒ `Err` in both modes
- [ ] T017 [P] Write `crates/xtriever-pipeline/tests/model_roundtrip.rs` (`#[ignore]`, model-backed): `MiniLmEmbedder::load` → `HybridIndex::create` with a two-field schema → add 5 short documents → commit → search returns the obvious document first → reopen with a second `MiniLmEmbedder` ⇒ same hits; open with `TableEmbedder` (`"table-fp"`) ⇒ `FingerprintMismatch` naming the real fingerprint
- [ ] T018 [P] Write `crates/xtriever-eval/tests/hybrid_run.rs` (offline) covering **US5 scenarios 1–2** at the library level and FR-022: `HybridConfig::hybrid_baseline_v1()` has `k 100`, `candidate_depth 100`, `rrf_k 60`, `lexical == EvalConfig::lexical_baseline_v1()`, `dense == DenseConfig::dense_baseline_v1()`, `validate()` rejects `k < 100` and `candidate_depth < k`; `build_external` on `support::synthetic_dataset` yields `(id, fields)` in corpus order with the empty title omitted; `execute_external` with a closure returning canned ids runs every judged query in ascending id order, passes `k`, preserves order; extend `crates/xtriever-eval/tests/report.rs`: `compare` of a lexical and a dense report yields rows for both metrics, `a_config`/`b_config` set, and `to_markdown()` contains both names and **no** "ADR" text; `delta` on the same pair still errors
- [ ] T019 Run `cargo nextest run -p xtriever-pipeline --no-fail-fast` and `cargo nextest run -p xtriever-eval --no-fail-fast`: `fixtures_valid` **passes**, every other new test **fails** on `not implemented` or an assertion against a scaffold value, no failure names a fixture, parse or hash problem. Commit red (Rule 4). **Checkpoint — PR 1.**

---

## Phase 3: Foundational Implementation (Blocking Prerequisite)

**Purpose**: Error helpers, the descriptor and the id map — what every story reads.

- [ ] T020 Implement `crates/xtriever-pipeline/src/error.rs` (002/004 pattern): `schema_err`, `corrupt`; unit tests in-file
- [ ] T021 [P] Implement `crates/xtriever-pipeline/src/descriptor.rs` per data-model "Descriptor": serde struct with keys in the stated order (`format_version`, `schema`, `embedder_fingerprint`, `dense_fields`, `candidate_depth`, `rrf_k`, `live_docs`, `generation`); `write(dir)` to `xtriever-pipeline.json.tmp` + `sync_all` + `rename`; `read(dir)` ⇒ `Corrupt` on `format_version != FORMAT_VERSION` naming both, on missing/unparseable file naming the path; `check_identity(&self, schema: &Schema, fingerprint: &str)` ⇒ `Corrupt` naming both schemas / `FingerprintMismatch { index, current }`
- [ ] T022 [P] Implement `crates/xtriever-pipeline/src/ids.rs` per data-model "IdMap" and research D4: `IdMap { external: Vec<Option<String>>, chunks: BTreeMap<u32, ChunkInfo>, reverse: HashMap<String, u32> }`; `assign(&mut self, external: &str, chunk: Option<ChunkInfo>) -> Result<DocId>` (a replace overwrites the chunk entry; a delete removes it); `chunk(DocId) -> Option<&ChunkInfo>` (empty ⇒ `Schema("external id must not be empty")`; existing ⇒ its id; else push, never reusing a `null` slot; `u32` overflow ⇒ `Corrupt`); `remove(&mut self, external) -> Option<DocId>`; `external(DocId) -> Option<&str>`; `internal(&str) -> Option<DocId>`; `live() -> u64`; `read(dir)` / `write(dir)` (`ids.json`, `{"format_version": 1, "external": [...]}`, `.tmp` + `rename`, version check ⇒ `Corrupt`); unit tests in-file: assign/reuse/remove/never-reuse/round-trip
- [ ] T023 Wire `crates/xtriever-pipeline/src/lib.rs` to `error`, `descriptor`, `ids`; keep the scaffold for the index and search; `cargo nextest run -p xtriever-pipeline` — the in-file unit tests green, the suite otherwise unchanged (red)

**Checkpoint**: the two pipeline-owned files exist and round-trip. Story work begins.

---

## Phase 4: User Story 1 — Documents go in under their own ids and come back out under them (Priority: P1) 🎯 MVP

**Goal**: `HybridIndex::{create, open, open_mapped, add, add_embedded, delete, commit}` with the
id boundary, the fixed commit order and the four-count check. Completes **PR 2**.

**Independent Test**: `tests/{ingest,persist}.rs` green with the stub embedder;
`tests/model_roundtrip.rs` green with the model.

- [ ] T024 [US1] Implement `HybridConfig::new` and validation in `crates/xtriever-pipeline/src/index.rs`: defaults `candidate_depth 100`, `rrf_k 60`; `validate(&self)` ⇒ `Schema` when `dense_fields` is empty or names a field that is not `FieldKind::Text` in `schema`, `Corrupt`-free; `candidate_depth ≥ 1`, `rrf_k ≥ 1` else `Schema`
- [ ] T025 [US1] Implement `HybridIndex::create` in `crates/xtriever-pipeline/src/index.rs` per contract: validate; `dir` absent or empty else `Corrupt`; `create_dir_all`; `TantivyIndex::create(dir/lexical, schema.clone())`; `FlatIndex::create(dir/dense, embedder.dim(), embedder.metric(), embedder.fingerprint())`; empty `IdMap` written; descriptor written with `live_docs 0`, `generation 0`
- [ ] T026 [US1] Implement `HybridIndex::open` / `open_mapped` in `crates/xtriever-pipeline/src/index.rs` per research D3: `Descriptor::read` → `check_identity(schema, embedder.fingerprint())` → `IdMap::read` → `TantivyIndex::open(dir/lexical)` and assert `lexical.schema() == descriptor.schema` (`Corrupt` naming both) → `FlatIndex::open_for` / `open_mapped_for(dir/dense, embedder)` → **four-count check**: `descriptor.live_docs`, `ids.live()`, `lexical.stats()?.num_docs`, `dense.len()` all equal else `Corrupt("partial commit: descriptor {a}, id map {b}, lexical {c}, dense {d} live documents")`; accessors `config`, `embedder`, `len` (= `descriptor.live_docs`), `is_empty`, `contains` (committed map only — keep a `committed_ids` snapshot separate from the pending map, or track pending removals/additions so `contains` reflects the last commit)
- [ ] T027 [US1] Implement ingest in `crates/xtriever-pipeline/src/index.rs` per data-model "SourceDocument": `passage(&self, fields) -> String` joining `dense_fields` values that are `Value::Text` and non-empty with `" "` in `dense_fields` order; `add`: per document `ids.assign` (pending map), passage, `embedder.embed(&[&passage], TextKind::Passage)` ⇒ one vector, `lexical.add(&[Document { id, fields: doc.fields.clone(), chunk: doc.chunk.clone() }])`, `dense.add(id, &vector)`; `add_embedded`: same with the supplied vector, `v.len() == embedder.dim()` else `DimensionMismatch`; `delete`: for each known external id `ids.remove` (pending) and both stages `delete(&[id])`; `commit`: no-op if nothing pending, else `lexical.commit()` → `dense.commit()` → `ids.write` → descriptor with `generation + 1` and `live_docs = ids.live()`, then refresh the committed snapshot; add `#[doc(hidden)] pub fn commit_lexical_only_for_test` that performs only the first step (for `persist.rs`'s partial-commit test)
- [ ] T028 [US1] Run `cargo nextest run -p xtriever-pipeline --test ingest --test persist` — **green** except the search-dependent assertions in `persist.rs` (reopen-identical hits) which stay red until Phase 5; then `cargo nextest run -p xtriever-pipeline --test model_roundtrip --run-ignored only` — the create/open/mismatch parts green. **Checkpoint — MVP: the id boundary exists. PR 2.**

---

## Phase 5: User Story 2 — One query, one fused ranking (Priority: P1)

**Goal**: `rrf`, `search`, `search_lexical`, filters applied once to both stages.

**Independent Test**: `tests/{fusion_golden,fusion_prop,search}.rs` green; `persist.rs` fully green.

- [ ] T029 [US2] Implement `rrf` in `crates/xtriever-pipeline/src/fusion.rs` per research D5: `BTreeMap<u32, (f64, f64)>` of (lexical term, dense term) keyed by id, terms `1.0 / (f64::from(rrf_k) + rank as f64)` with 1-based ranks; `score = lex + dense` **in that order**; sort `(score DESC via partial_cmp, id ASC)`; truncate `k`; return `Vec<(DocId, f64)>`; unit test for `k == 0` and both-empty
- [ ] T030 [US2] Implement the search core in `crates/xtriever-pipeline/src/search.rs` per data-model "Search algorithm" steps 1–3 and 7 (steps 4–6 land in Phase 7 as a straight-through path here): `SearchOptions` + `Default`; `search(query, filter, k, opts)` = `search_lexical(&LexicalQuery::Match(None, query.to_owned()), …)`; `search_lexical`: `k == 0` ⇒ empty `Response`; resolve filter ⇒ `DocSet` (empty ⇒ empty response); depth = `opts.depth.unwrap_or(config.candidate_depth)`; lexical `search(query, Some(&Filter::Ids(set.iter().collect())) or None, depth)`; embed query (`TextKind::Query`), dense `search(&v, set.as_ref(), min(depth, budget.max_items))`; `rrf`; map each id through `ids.external` (unknown ⇒ `Corrupt` naming the id) and attach `chunk` from the id map's `chunks` entry (data-model "IdMap": provenance is kept by the pipeline, keyed by internal id, so hits carry it without a stage lookup — FR-007; `ids.rs` stores it at `assign` time and `IdMap::chunk(DocId)` reads it); build `HybridHit { external_id, id, score, chunk, explain: None }`; `StageReport { lexical_candidates, dense_candidates: Some(n), degraded: None, time_limit_ignored: false }`
- [ ] T031 [US2] Run `cargo nextest run -p xtriever-pipeline --test fusion_golden --test fusion_prop --test search --test persist` — **green** (SC-001, SC-003, SC-004); the explain assertions in `search.rs` stay red until Phase 6

---

## Phase 6: User Story 4 — Every hit can explain itself (Priority: P2)

**Goal**: `HitExplain` on every hit when requested; ranking unchanged.

**Independent Test**: `tests/explain.rs` green; `search.rs` fully green.

- [ ] T032 [US4] Implement explanation in `crates/xtriever-pipeline/src/search.rs` per research D8: while fusing, keep per-id `(bm25_score, bm25_rank)` and `(dense_score, dense_rank)` from the two candidate lists (1-based positions); when `opts.explain`, attach `HitExplain { …, fused: score }` to every hit; `HitExplain::features()` returns the five `(FeatureName, f32)` pairs in the order `bm25.score, bm25.rank, dense.score, dense.rank, fused.score` using `xtriever_core::features::*`, `f32::NAN` for `None`, ranks as `f32`, `fused as f32`
- [ ] T033 [US4] Run `cargo nextest run -p xtriever-pipeline --test explain --test search` — **green** (SC-006)

---

## Phase 7: User Story 3 — A failing or slow dense stage degrades to lexical results (Priority: P2)

**Goal**: Degradation, strict mode, item and time budgets with a caller-supplied clock.
Completes **PR 3**.

**Independent Test**: `tests/degrade.rs` green.

- [ ] T034 [US3] Implement steps 4–6 and 8 in `crates/xtriever-pipeline/src/search.rs` per research D7: check point A after the lexical search (`opts.elapsed` and `budget.max_time` both present and `elapsed() > max_time` ⇒ degrade with `BudgetExceeded { elapsed_ms, limit_ms }`); dense embed + search errors ⇒ `strict` ? return the error : degrade with `StageError(e.to_string())`; check point B after the dense stage ⇒ degrade and **discard** the dense candidates; strict + time exceeded ⇒ `Err(Error::BudgetExhausted(format!("dense stage: {elapsed_ms} ms > {limit_ms} ms")))`; `max_time` set with `elapsed == None` ⇒ `time_limit_ignored = true`; degraded response: hits = lexical candidates in lexical order with `score = f64::from(hit.score)`, `explain` (if requested) with `bm25_*` set, `dense_*` None, `fused = score`; `dense_candidates = None`; `Degradation { stage: "dense", reason }`
- [ ] T035 [US3] Run `cargo nextest run -p xtriever-pipeline` (whole offline suite) and `--features mmap` — **green** (SC-005); `cargo nextest run -p xtriever-pipeline --run-ignored only` — green; `cargo check -p xtriever-pipeline --features mmap --target aarch64-apple-ios` passes. **Checkpoint — PR 3.**

---

## Phase 8: User Story 5 — The fused baseline is measured and becomes the guarded number (Priority: P2)

**Goal**: The harness runs the pipeline through a closure, reuses the 004 cache by value,
records three fused baselines with `compare` tables against both stage baselines.

**Independent Test**: `crates/xtriever-eval/tests/hybrid_run.rs` green offline; SciFact end to
end twice, byte-identical, 0 embedded.

- [ ] T036 [US5] Implement the harness additions in `crates/xtriever-eval/src/run.rs` per contract "Harness extension": `HybridConfig { name, lexical: EvalConfig, dense: DenseConfig, candidate_depth, rrf_k, k }` + `validate()` (`k ≥ 100`, `candidate_depth ≥ k`, both sub-configs valid) + `hybrid_baseline_v1()`; `build_external(dataset, &EvalConfig) -> Vec<(String, BTreeMap<FieldName, Value>)>` sharing the field construction with `build` (refactor the loop body into a private helper used by both; `build`'s output must not change — its tests pin it); `execute_external(dataset, config_name, k, retrieve)` iterating judged queries ascending and building the `Run`
- [ ] T037 [US5] Implement `report::compare` in `crates/xtriever-eval/src/report.rs`: `Comparison { a_config, b_config, rows }` with the same `DeltaRow`s as `delta` (before = a, after = b, matched by dataset), **no** configuration check and **no** `adr_trigger`; `to_markdown()` prints a heading line `"comparison: {a_config} → {b_config}"` and the table, and never the ADR sentence
- [ ] T038 [US5] Extend `crates/xtriever-eval/examples/beir.rs` per contract "`beir` example commands": `cargo add -p xtriever-eval --dev --path crates/xtriever-pipeline`; `--config hybrid-baseline-v1` with `--index-dir` (default a temp dir), `--cache-dir`, `--model-dir`, `--load-path`, `--export-explain`: load the dataset; verify the 004 cache key (`EmbeddingCacheKey` for `dense-baseline-v1`) matches and `bail!` naming what differs otherwise (never re-embed silently — research R2); `FlatIndex::open(cache/D)`; `build_external`; `HybridIndex::create(index_dir, HybridConfig::new(schema from the lexical config, dense_fields [title, text]), Box::new(MiniLmEmbedder))`; `add_embedded` in corpus order with `cache.vector(DocId(i))` (`bail!` if `None`), one `commit`, print `embedded 0 documents (004 cache)` and the ingest wall time; `execute_external` with a closure that calls `search(text, None, k, &SearchOptions { explain: true, .. })` and maps hits to `external_id`, while collecting `{query_id, lexical, dense, fused}` (from the explanations' ranks) for `--export-explain`; score; `stage = StageInfo { kind: "hybrid", …, baseline: "guarded" }`; print per-query timing split (lexical / embed / dense / fuse) to stderr; add the `compare a.json b.json` subcommand
- [ ] T039 [US5] Run `cargo nextest run -p xtriever-eval` — **green**; `cargo tree -p xtriever-eval -e normal | grep -E 'candle|tantivy|memmap2|xtriever-pipeline'` prints nothing; the lexical smoke (quickstart Step 4) still exits 0
- [ ] T040 [US5] Produce the SciFact hybrid baseline (quickstart Step 5, `RAYON_NUM_THREADS=4`): `run --dataset scifact --config hybrid-baseline-v1 --cache-dir target/xt-dense-cache --index-dir target/xt-hybrid-index/scifact --out specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.scifact.json --export-run target/hybrid-run-scifact.jsonl --export-explain target/hybrid-explain-scifact.jsonl`; `gen_003_fixtures.py --verify-run` PASS (SC-007); `gen_005_fixtures.py --verify-fusion target/hybrid-explain-scifact.jsonl` PASS; re-run to `/tmp/hybrid-scifact-again.json` ⇒ `diff` identical and `embedded 0`
- [ ] T041 [US5] Produce the NFCorpus and FiQA hybrid baselines the same way into `specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.{nfcorpus,fiqa}.json`, each `--verify-run` and `--verify-fusion` PASS; FiQA under `/usr/bin/time -l` capturing ingest time, per-query timing, `maximum resident set size`, `du -sk target/xt-hybrid-index/fiqa`, `stat -f %z …/ids.json` (SC-010)
- [ ] T042 [US5] Run the six `compare`s (quickstart Step 5) and apply **SC-011**: fused nDCG@10 ≥ the dense baseline (0.645082 / 0.316673 / 0.368671 — the better stage on every dataset today) on at least 2 of 3. Record the verdict and all six tables in `specs/005-hybrid-pipeline/report.md`. A miss is ⛔ — stop, report the numbers and the candidates to investigate (`rrf_k`, depth, a fusion defect), and wait for the human; never adjust the bar. Confirm `beir delta` still refuses the mixed pair (exit 1)
- [ ] T043 [US5] Measure the `Filter::Ids` cost once (research D6/D12): on the SciFact hybrid index, time `search` with `Filter::Ids` of half the corpus vs no filter (a tiny throwaway example or a `#[ignore]` test printing the two timings); record both numbers and the method in `report.md`

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Scaffold removal, docs, report, PR description, the full gate. Completes **PR 4**.

- [ ] T044 [P] Write `specs/005-hybrid-pipeline/report.md`: verdict; nextest summaries; the three hybrid baselines with `--verify-run` and `--verify-fusion`; the six `compare` tables and the SC-011 verdict; observations (FiQA ingest time, index/id-map sizes, peak RSS, per-query split, `Filter::Ids` cost) with method; the FR-027 diff (empty); findings
- [ ] T045 Delete `crates/xtriever-pipeline/src/scaffold.rs` and every `NotImplemented` reference; `./scripts/check-no-stubs.sh` **passes**
- [ ] T046 [P] Write `crates/xtriever-pipeline/src/lib.rs` crate docs: what the pipeline owns, the directory layout, the id boundary, the fusion rule and tie-break, degradation and the caller-supplied clock (why no `Instant`), explain, the `mmap` feature and the dense stage's precondition
- [ ] T047 [P] Write `specs/005-hybrid-pipeline/pr-description.md`: summary, four-commit split with hand-written line counts, the baseline table, **the delta section** (six `compare` tables + SC-011 verdict — the first real one), observations, the "no ADR, no unsafe, no new dependency" line, the standing CI rule (SciFact lexical smoke only)
- [ ] T048 [P] Fill the FiQA hybrid report's `observations` by hand (`index_dir_bytes` = `du` of the hybrid directory, `peak_rss_bytes`, `embed_corpus_ms: 0`, `search_ms`, method naming every command) and re-run `--verify-run` on the edited file
- [ ] T049 Run the full gate from quickstart Step 6: `cargo fmt --all --check`; `RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets`; `RUSTFLAGS="-D warnings" cargo clippy -p xtriever-pipeline --features mmap --all-targets`; `cargo nextest run --workspace`; `cargo nextest run -p xtriever-pipeline --features mmap`; `cargo nextest run -p xtriever-pipeline --run-ignored only`; `cargo deny check`; `cargo check --workspace --target aarch64-apple-ios` / `aarch64-apple-ios-sim` / `aarch64-linux-android`; `cargo check --workspace --target wasm32-unknown-unknown` (best-effort, record the failure point); `./scripts/check-no-stubs.sh`; `./scripts/check-toolchain.sh`; `grep -rn 'unsafe' crates/xtriever-pipeline/src/` (nothing); `grep -rn 'Instant\|SystemTime\|std::thread' crates/xtriever-pipeline/src/` (nothing); the eval purity `cargo tree`; the FR-027 `git diff --stat` (empty). Paste the results into `report.md` and `pr-description.md`. Any failure ⛔ — stop and report

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T002; T003 parallel
- **Oracle & red suite (Phase 2)**: T004 → T005 → T006 → T007 → T008; T009 after T001; T010–T018 after T008 and T009; T019 last → **PR 1**
- **Foundational (Phase 3)**: T020; T021 ‖ T022; T023
- **US1 (Phase 4)**: T024 → T025 → T026 → T027 → T028 → **PR 2**
- **US2 (Phase 5)**: T029 → T030 → T031 (needs US1)
- **US4 (Phase 6)**: T032 → T033 (needs US2)
- **US3 (Phase 7)**: T034 → T035 (needs US4) → **PR 3**
- **US5 (Phase 8)**: T036 ‖ T037 → T038 → T039 → T040 → T041 → T042 → T043 (needs US3)
- **Polish (Phase 9)**: T044 ‖ T046 ‖ T047 ‖ T048 → T045 → T049 → **PR 4**

### Rule 6 stop-points (⛔)

T042 (SC-011: fused below the better stage on ≥ 2 datasets); T049 (any gate failure). The
response is a report, never a looser test or a moved bar.

### Parallel Opportunities

- Phase 2: T010 ‖ T011 ‖ T012 ‖ T013 ‖ T014 ‖ T015 ‖ T016 ‖ T017 ‖ T018 (nine test files)
- Phase 3: T021 ‖ T022
- Phase 8: T036 ‖ T037
- Phase 9: T044 ‖ T046 ‖ T047 ‖ T048

---

## Parallel Example: Phase 2 red suite

```text
after T008 (fixtures) and T009 (scaffold):
  T010 support + fixtures_valid   T011 fusion_golden + fusion_prop   T012 ingest
  T013 persist                    T014 search                        T015 explain
  T016 degrade                    T017 model_roundtrip               T018 eval hybrid_run + report
then T019 (red checkpoint, PR 1)
```

## Implementation Strategy

1. **PR 1** (Phases 1–2): oracle, goldens, scaffold, red suite.
2. **PR 2** (Phases 3–4) — **MVP**: the id boundary, persistence and the four-count check.
3. **PR 3** (Phases 5–7): fusion, search, explain, degradation — the product.
4. **PR 4** (Phases 8–9): the harness, the three fused baselines, the first real delta section,
   the SC-011 verdict, the gate.
