# Tasks: The Lexical Stage

**Input**: Design documents from `/specs/002-lexical-stage/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/lexical-index.md](./contracts/lexical-index.md),
[quickstart.md](./quickstart.md)

**Tests**: **Mandatory, not optional.** Principle II (NON-NEGOTIABLE) and spec FR-032 require every
acceptance test to be written and committed **failing** before any implementation. Phase 2 exists
to land the whole suite red; the story phases contain implementation only, each ending with the
task that turns its tests green. Tests must fail at *runtime* for want of an implementation, never
as compile errors (SC-010, and Feature 001's T012a lesson) — hence the `NotImplemented` scaffold in
T014, removed in T060.

**Organization**: Setup → Oracle & red suite → Foundational implementation → one phase per user
story in priority order → Polish. The four-PR split from plan.md is marked at the checkpoints.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US6 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Backend items are cited as research D-numbers; read those
  before implementing — they carry `file:line` citations into the pinned tantivy source (Rule 1).

## Path Conventions

Workspace crate `crates/xtriever-lexical/` (`src/`, `tests/`, `examples/`); Python oracle under
`reference/`; fixtures under `reference/fixtures/002/`; one doc-comment edit in
`crates/xtriever-core/src/traits.rs`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Pin the measured dependency set, extend the Python environment, and make the oracle
refactor provably byte-neutral before anything depends on it.

- [ ] T001 Add dependencies to `crates/xtriever-lexical/Cargo.toml` **with `cargo add`, never by hand**: `cargo add -p xtriever-lexical tantivy --no-default-features --features mmap,stopwords,lz4-compression,stemmer` (must resolve to **0.26.2**, the version Feature 001 measured — research D18), `cargo add -p xtriever-lexical serde --features derive`, `cargo add -p xtriever-lexical serde_json`, `cargo add -p xtriever-lexical --dev proptest tempfile serde_json`; keep `xtriever-core` path dep and `[lints] workspace = true`; update the crate `description`
- [ ] T002 Verify purity and the deny gate after T001: `cargo tree -p xtriever-lexical -e normal --prefix none | grep -Ei '(-sys|^cc |onig|zstd)'` prints nothing, `cargo deny check` passes with **no** change to `deny.toml` (Rule 2), and `Cargo.lock` is committed
- [ ] T003 [P] Create `reference/requirements-002.in` (Feature 001's pins plus `snowballstemmer`) and pin it to `reference/requirements-002.txt`; extend `scripts/setup-reference-venv.sh` to install from `requirements-002.txt` so one venv serves both features (quickstart Step 1)
- [ ] T004 [P] Create `reference/xtref/__init__.py` and `reference/xtref/bm25.py` by **moving** (not copying) from `reference/gen_001_fixtures.py`: `K1`, `B`, `FIELD_NORMS_TABLE`, `fieldnorm_to_id`, `id_to_fieldnorm`, `analyze` (renamed `analyze_standard`), `idf`, and the per-document BM25 scoring; add `analyze_standard_en` = `analyze_standard` followed by `snowballstemmer.stemmer("english")` per token (research D5, D17); keep every constant's source citation comment intact
- [ ] T005 Rewire `reference/gen_001_fixtures.py` to `from xtref.bm25 import …`, then prove the refactor changed nothing: regenerate with `--seed 1 --out /tmp/xt001-check/` and `diff` the `files` map of the two `manifest.json`s — **the diff must be empty**; any difference stops the work (Rule 6, quickstart Step 2)

---

## Phase 2: Oracle & Red Suite (Blocking Prerequisite)

**Purpose**: Land every golden and every acceptance test **red**, with the fixtures proven valid so
"fails for the right reason" is mechanical. This phase is **PR 1**.

**⚠️ CRITICAL**: No story implementation may begin until T024 confirms the suite fails for want of
an implementation and not for a broken fixture.

### Python generator and fixtures

- [ ] T006 Create `reference/gen_002_fixtures.py` scaffold mirroring 001's: interpreter guard (3.12 + venv), thread pinning, `--seed`, `--out`, `write_json`, `sha256_file`, and `manifest.json` emission `{ "generator", "seed", "files": {name: sha256} }` (data-model `Manifest`)
- [ ] T007 In `reference/gen_002_fixtures.py`, emit `schema.json` as the serde form of the 11-field `FixtureSchema` **exactly as tabulated in data-model.md** (`title` `Text("standard")` stored boost **2.0**; `body`; `summary` `Text("standard_en")`; `source`, `tags` `Keyword`; `views` `U64`; `rank` `I64`; `quality` `F64`; `published` `Bool`; `ts` `DateMillis`; `note` `Text("standard")` **`indexed: false`**, stored); the JSON must deserialize into `xtriever_core::Schema` unchanged
- [ ] T008 In `reference/gen_002_fixtures.py`, emit `corpus.json`: exactly **1,000** documents, `id` 0..999 in insertion order, ASCII text, every doc carrying `title, body, source, views, rank, quality, published, ts`, roughly half carrying `summary, tags, note`; plant and **assert** the structures data-model `FixtureCorpus` requires — a `Term` golden with a genuine score tie spanning the k-boundary, a phrase occurring 1× and 2× plus a slop-1 variant, words at Levenshtein 1 and 2 from a query term, shared `title`/`body` vocabulary so the boost is observable; the generator **refuses to emit** if any planted structure is missing
- [ ] T009 In `reference/gen_002_fixtures.py`, emit `filters.json` (`FilterGolden`): one entry per `Filter` variant minimum — `Eq`, `In`, `Range` (closed, half-open both sides, and `(None, None)` ≡ `Exists`), `Exists` (on a Keyword, a numeric, and a **Text** field), `And`, `Or`, `Not`, `Ids` — with `expected_ids` computed in Python over the corpus, sorted; include one filter on `ts` whose bounds differ by **one millisecond** so second-granularity truncation (research D6) would be caught
- [ ] T010 In `reference/gen_002_fixtures.py`, emit `stats.json` (`StatsGolden`): `num_docs = 1000`, `avg_field_len` per text field from **unquantized** token counts over docs carrying the field, and ≥ 10 `terms` entries `{field, term, doc_freq, total_term_freq}` including one absent term (expected `null`) and one `standard_en` stemmed term
- [ ] T011 In `reference/gen_002_fixtures.py`, emit `queries.json` (`QueryGolden`) with `expected: null` placeholders and an `oracle` tag per entry (`python` / `python-membership` / `rust-only`, research D17): ≥ 1 per variant — `Match(Some)`, `Match(None)` (multi-field, boosted), `Phrase` slop 0 and slop 1, `Term` (the k-boundary tie, `k` chosen to split the tie group), `Fuzzy` d=1 and d=2, `Bool` (must+should+must_not, and must_not-only ⇒ empty), `Boost`, plus one entry with a `filter`; and implement `--verify-ranking <dir>` that recomputes each covered entry's ranking in Python (BM25 with quantized fieldnorm and **`max_doc`-based** average, per-field boost, sum across fields; exact-phrase count as tf; Levenshtein membership with score 1.0×boost) and compares ids/order exactly and scores within **1e-5 relative**, membership-only for `python-membership`
- [ ] T012 In `reference/gen_002_fixtures.py`, emit `mutations.json` (`MutationGolden`): a `replace` recipe (id, new document, expected post-commit ranking of a named query), a `delete` list with expected live `num_docs` and per-term live `doc_freq`, and the `history_pair` recipe (add 1,100 then delete 100 ⇒ same live set as the baseline) with expected live-only stats identical to the baseline's
- [ ] T013 Run `python3 reference/gen_002_fixtures.py --seed 2 --out reference/fixtures/002/` and commit the seven files; confirm `git check-attr text eol -- reference/fixtures/002/corpus.json` reports `text: unset` (the existing `.gitattributes` rule must cover the new directory — no edit expected)

### Crate scaffold so tests compile and fail at runtime

- [ ] T014 Create the scaffold in `crates/xtriever-lexical/src/lib.rs` + `src/scaffold.rs`: `pub struct TantivyIndex` with the **exact** public surface from [contracts/lexical-index.md](./contracts/lexical-index.md) (`create`, `open`, `merge`, `impl LexicalIndex`, `pub const ANALYZERS`), every method returning `Err(Error::backend(NotImplemented("<method>")))` where `NotImplemented` is a crate-private `thiserror` struct; **no `todo!`/`unimplemented!`** (workspace lints deny them); `missing_docs` satisfied. This scaffold is deleted in T068
- [ ] T015 Extend `scripts/check-no-stubs.sh` to also scan `crates/xtriever-lexical/src/` for `NotImplemented`, so the scaffold cannot outlive the implementation PRs

### Acceptance tests (all red except `fixtures_valid`)

- [ ] T016 [P] Write `crates/xtriever-lexical/tests/support/mod.rs` (fixture loaders for all seven files, `fixture_schema()`, `tempdir_index()` building a `TantivyIndex` in a `tempfile::tempdir()`, `index_corpus(&mut idx, batches: &[Range<usize>])` committing per batch) and `tests/fixtures_valid.rs` asserting every `manifest.json` hash — this one must be **GREEN** at the red checkpoint (FR-033)
- [ ] T017 [P] Write `crates/xtriever-lexical/tests/index_query.rs` covering **US1 scenarios 1–10**: `schema()` equality; `Match` k=10 returns exactly 10 descending; fewer-than-k; empty result is `Ok(vec![])`; unknown field in a document ⇒ `Error::UnknownField`; every `queries.json` entry compared **exactly** (order, ids, scores); unknown analyzer id ⇒ `Error::Schema` naming field and id at `create`; `title` boost 2.0 doubles `Term` scores and doubles the field's contribution in `Match(None)`; boost on a `Keyword` ⇒ `Error::Schema` at `create`; `ChunkInfo` present vs absent scores identically and no term from it exists; plus `k == 0` ⇒ `Ok(vec![])`, `Phrase` on `Keyword` ⇒ `InvalidQuery`, any query on `note` (unindexed) ⇒ `InvalidQuery`, `Fuzzy` d=3 ⇒ `InvalidQuery`, single-token and zero-token `Phrase` do not panic
- [ ] T018 [P] Write `crates/xtriever-lexical/tests/determinism.rs` covering **US2 scenarios 1–5**: one batch vs four batches ⇒ identical hits for every golden; ties in a multi-segment index come back in ascending `DocId`; repeat call identical; identical before and after `merge()`; the planted k-boundary tie returns exactly the membership recorded in the golden and ascending `DocId` order within it (FR-014)
- [ ] T019 [P] Write `crates/xtriever-lexical/tests/mutation.rs` covering **US3 scenarios 1–5** from `mutations.json`: replace-by-id shows new content once; delete removes from results; deleting an unknown id is `Ok`; `stats().num_docs` counts live only; uncommitted mutations invisible to `search`/`resolve_filter`/`term_stats`/`stats`; same id twice in one uncommitted batch ⇒ last wins; drop without commit then `open` ⇒ committed state only (FR-011)
- [ ] T020 [P] Write `crates/xtriever-lexical/tests/filters.rs` covering **US4 scenarios 1–6** from `filters.json`: every entry's set exact; filtered search = unfiltered results ∩ set with **unchanged scores**; unknown field ⇒ `UnknownField` (not empty); open bounds and inclusivity; `Eq` with wrong `Value` type ⇒ `InvalidQuery`; `Eq` on a `Text` field ⇒ `InvalidQuery`; deleted docs absent; the one-millisecond `ts` filter
- [ ] T021 [P] Write `crates/xtriever-lexical/tests/filter_algebra_prop.rs` (proptest, `ProptestConfig { cases: 1000, .. }`) generating random filter trees over the fixture schema's metadata fields and asserting on the committed corpus: `And`/`Or` commutative and associative, `Not(Not(f)) == f`, `Not(f) == alive − f`, De Morgan both ways, and `search(q, Some(f), k)` ids ⊆ `resolve_filter(f)` for a fixed `q` (FR-023, SC-006)
- [ ] T022 [P] Write `crates/xtriever-lexical/tests/stats.rs` covering **US5 scenarios 1–6** from `stats.json` and `mutations.json`: `term_stats` exact incl. the stemmed term; absent term ⇒ `Ok(None)`; `stats()` exact; after deletes both methods are live-only; `history_pair` stats identical (SC-011); plus one `#[ignore]` test `divergence_measurement` that builds the `history_pair`, runs the named query on both, reads the backend's scoring statistics through `term_stats`/`stats` vs the score deltas, prints a `DivergenceRecord` (data-model) before and after `merge()`, and **asserts nothing about the magnitude** (FR-025, SC-012)
- [ ] T023 [P] Write `crates/xtriever-lexical/tests/concurrency.rs` covering **US6 scenarios 1–5**: `RwLock<TantivyIndex>` queried from 8 threads matches single-threaded results; a writer thread doing add+commit while readers query never yields a mixed state (every reader result equals the before-set or the after-set); two indexes on different dirs don't interfere; a second `open` on a held dir succeeds and its first `add` ⇒ `Error::Backend` while the first handle still works, and succeeds after the first is dropped (research D14); a search during `merge()` from another thread is single-commit-consistent; plus `tests/analyzer_prop.rs` (proptest: random ASCII texts indexed into two fresh indexes give identical `Match` scores) and `tests/roundtrip_prop.rs` (proptest: random valid documents → add → commit → drop → `open` → every `Eq`/`Ids` filter and `stats()` agree with the in-memory model; `note` stored value survives, checked through `open` + a backend-level stored-field read in `support`)
- [ ] T024 Run `cargo nextest run -p xtriever-lexical` (suite in `crates/xtriever-lexical/tests/`) and confirm: `fixtures_valid` **passes**; every other test **fails**; every failure message contains `not implemented` (the scaffold) — none names a missing file, a hash mismatch or a parse error. Commit in this red state (Rule 4, SC-010). **Checkpoint — PR 1.**

---

## Phase 3: Foundational Implementation (Blocking Prerequisite)

**Purpose**: The pieces every story needs and no story owns — errors, schema mapping, the
descriptor, and an index that can be created and reopened. Replaces the scaffold module by module.

- [ ] T025 Implement `crates/xtriever-lexical/src/error.rs`: `fn map(e: tantivy::TantivyError) -> xtriever_core::Error` per research **D16** (`LockFailure` and all others ⇒ `Error::backend(e)`; `IoError` ⇒ `Error::Io`), plus helpers `schema_err(field, msg)`, `invalid_query(field, msg)`, `unknown_field(name)` that build the core variants with the field name in the message; unit tests for the mapping in the same file
- [ ] T026 Implement the analyzer table in `crates/xtriever-lexical/src/schema.rs`: `pub const ANALYZERS: &[&str] = &["standard", "standard_en"]` and `fn tokenizer_name(id: &AnalyzerId) -> Option<&'static str>` mapping `standard → "default"`, `standard_en → "en_stem"` (research D5 — both already registered in the backend's default `TokenizerManager`; **register nothing**)
- [ ] T027 Implement `FieldMap`/`MappedField` in `crates/xtriever-lexical/src/schema.rs` per data-model: build the backend `Schema` from a core `Schema` using the **D4 table verbatim** (`Text` ⇒ `TextOptions` with `WithFreqsAndPositions` + tokenizer name; `Keyword` ⇒ `STRING | FAST`; numerics/`Bool` ⇒ `INDEXED | FAST`; **`DateMillis` ⇒ `add_i64_field`**, research D6; `stored` ⇒ `| STORED`; `indexed: false` drops indexing); add hidden `__xt_id` (u64, `INDEXED | FAST`) and one `__xt_len_<name>` (u64, `FAST`) per `Text` field (D7); `text_fields` in schema order. Validation, all `Error::Schema`: field names unique; **none starts with `__xt_`**; `Text(id)` ⇒ `id` in the table; **`boost != 1.0` ⇒ kind is `Text`**
- [ ] T028 Implement `Descriptor` in `crates/xtriever-lexical/src/schema.rs`: `{ format_version: u32 = 1, schema: Schema }` serde struct; `write(dir)` to `<dir>/xtriever-lexical.json` (write-then-rename); `read(dir)` ⇒ `Error::Corrupt("not an xtriever-lexical index: missing descriptor")` if absent, `Corrupt` on version ≠ 1 or JSON failure (research D13)
- [ ] T029 Implement `TantivyIndex::create` and `open` in `crates/xtriever-lexical/src/index.rs`: struct fields per data-model (`schema`, `fields`, `index`, `reader`, `writer: Option<_>`); `create(dir, schema)` ⇒ dir absent or empty else `Error::Io`, `FieldMap::build`, `Index::create_in_dir`, write descriptor, build reader with `ReloadPolicy::Manual` (D3); `open(dir)` ⇒ read descriptor **first**, rebuild `FieldMap`, `Index::open_in_dir`, compare the backend schema to the rebuilt one field-by-field ⇒ `Corrupt` on mismatch; `schema()` returns `&self.schema`. **Do not create a writer here** (D2)
- [ ] T030 Wire `crates/xtriever-lexical/src/lib.rs` to the real modules (`error`, `schema`, `index`, and empty `query`, `filter`, `search`, `stats` modules for later phases), `pub use index::TantivyIndex`, `pub use schema::ANALYZERS`, crate-level docs stating the thread/memory facts from the contract's last section; delete the scaffold's `create`/`open`/`schema` paths (remaining methods still return `NotImplemented`)
- [ ] T031 Run `cargo nextest run -p xtriever-lexical` and confirm the `create`-time tests in `index_query.rs` (schema equality, unknown analyzer, boost on keyword, reserved prefix) and the descriptor cases in `roundtrip_prop.rs` (`open` after `create`, wrong-version file ⇒ `Corrupt`) are **green**; everything else still red for `not implemented`

**Checkpoint**: an index can be created, described on disk, and reopened. Story work can begin.

---

## Phase 4: User Story 1 — Documents go in and come back ranked (Priority: P1) 🎯 MVP

**Goal**: `add` + `commit` + `search` for all six `LexicalQuery` shapes with exact goldens.

**Independent Test**: `tests/index_query.rs` green against `queries.json` after the goldens are
minted (T041); no other story's tests needed.

- [ ] T032 [US1] Implement `LexicalIndex::add` in `crates/xtriever-lexical/src/index.rs`: lazily create the writer with `IndexWriterOptions::builder().num_worker_threads(1).num_merge_threads(1).memory_budget_per_thread(MEMORY_BUDGET_NUM_BYTES_MIN).build()` via `writer_with_options` (research **D1/D2**; map `LockFailure` through `error::map`); for each document validate every field against `FieldMap` (`UnknownField`; `Value` variant ≠ `FieldKind` ⇒ `Schema` naming the field — **silent coercion and silent dropping are both forbidden**, FR-007); `delete_term(Term::from_field_u64(__xt_id, id))` **then** `add_document` (D7 — this is what makes replace-by-id work within a batch and across commits); write `__xt_id`; for each present `Text` field also write `__xt_len_<f>` = token count from `index.tokenizer_for_field(f)?.token_stream(text)`; `DateMillis` ⇒ `add_i64`; **ignore `document.chunk` entirely** (FR-008b)
- [ ] T033 [US1] Implement `LexicalIndex::commit` in `crates/xtriever-lexical/src/index.rs`: `Ok(())` if `writer` is `None`; else `writer.commit()` then `reader.reload()` (D3), mapping errors
- [ ] T034 [US1] Implement `crates/xtriever-lexical/src/search.rs`: `fn top_k(searcher, query, k, filter: Option<Arc<RoaringBitmap>>) -> Result<Vec<Hit>>` — `k == 0 ⇒ Ok(vec![])` **before any collector is built** (`TopDocs::with_limit(0)` panics, D11); unfiltered ⇒ `TopDocs::with_limit(k).order_by_score()`; filtered ⇒ `FilterCollector::new("__xt_id".into(), move |xid: u64| bitmap.contains(xid as u32), TopDocs…)` (D11); map every `DocAddress` to `DocId` via `segment_reader.fast_fields().u64("__xt_id")?.first(doc)`; re-sort with `sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)))` (D12, ADR-0005)
- [ ] T035 [P] [US1] Implement `Match` and `Term` in `crates/xtriever-lexical/src/query.rs` per research **D9**: `translate(&LexicalQuery, &FieldMap, &Index) -> Result<Box<dyn Query>>`; field checks per data-model `TranslatedQuery` table (`UnknownField`; not `indexed` ⇒ `InvalidQuery`; `Match` needs `Text`; `Term` needs `Text` or `Keyword`); `Match(Some)` ⇒ analyze with `tokenizer_for_field`, `BooleanQuery` of `Should` `TermQuery(term, WithFreqs)`, zero tokens ⇒ `EmptyQuery`, wrap in `BoostQuery(boost)` iff `boost != 1.0`; `Match(None)` ⇒ the same per indexed `Text` field in `text_fields` order, each under its own boost, all `Should` of one outer `BooleanQuery` (FR-017 sum); `Term` ⇒ verbatim `Term::from_field_text`
- [ ] T036 [P] [US1] Implement `Phrase` in `crates/xtriever-lexical/src/query.rs`: `Text` only (`Keyword` ⇒ `InvalidQuery`, FR-018); analyze; **≥ 2 tokens ⇒ `PhraseQuery::new_with_offset_and_slop(terms_with_offsets, slop)`; exactly 1 ⇒ `TermQuery`; 0 ⇒ `EmptyQuery`** — the backend constructor panics below two terms (D9); field boost wrapping as in T035
- [ ] T037 [P] [US1] Implement `Fuzzy` in `crates/xtriever-lexical/src/query.rs`: `distance > 2 ⇒ InvalidQuery` up front (backend limit, D9); `FuzzyTermQuery::new(term, distance, false)` — **`false` = transposition costs two** (spec FR-017, strict Levenshtein); `Text` or `Keyword`; field boost wrapping
- [ ] T038 [US1] Implement `Bool` and `Boost` in `crates/xtriever-lexical/src/query.rs`: `Bool` ⇒ `BooleanQuery::new` with `Occur::{Must,Should,MustNot}` over recursively translated clauses (empty or `MustNot`-only matches nothing — document it on the fn); `Boost(q, b)` ⇒ `BoostQuery::new(translate(q)?, b)`
- [ ] T039 [US1] Implement `LexicalIndex::search` in `crates/xtriever-lexical/src/index.rs`: take one `searcher` for the call; `translate` the query; if `filter` is `Some`, call `resolve_filter` (returns `NotImplemented` until Phase 7 — leave the call in place so US4 only has to fill in `filter.rs`); hand off to `search::top_k`
- [ ] T040 [US1] Write `crates/xtriever-lexical/examples/gen_ranking.rs`: `--fixtures <dir>` builds the index in a tempdir from `corpus.json` + `schema.json` in **one batch**, runs every `queries.json` entry, and rewrites `expected` in place (pretty-printed, stable key order), refusing to overwrite an entry whose `expected` is non-null unless `--force` (mirrors 001's minter)
- [ ] T041 [US1] Mint the goldens: `cargo run -p xtriever-lexical --example gen_ranking -- --fixtures reference/fixtures/002/`, then `python3 reference/gen_002_fixtures.py --verify-ranking reference/fixtures/002/` must report every `python` entry within 1e-5 and every `python-membership` entry with identical membership — **a disagreement is a finding to record in `report.md`, not a reason to touch the tolerance** (Rule 6); regenerate `manifest.json` hashes for `queries.json` and commit
- [ ] T042 [US1] Run `cargo nextest run -p xtriever-lexical --test index_query --test fixtures_valid --test analyzer_prop` (files under `crates/xtriever-lexical/tests/`) — all **green**. **Checkpoint — Story 1 delivers ranked retrieval (MVP). PR 2 = T025–T035, T039; PR 3 = T036–T038, T040–T042 plus Phase 7.**

---

## Phase 5: User Story 2 — Ties, segments and repeated runs behave identically (Priority: P1)

**Goal**: The ADR-0005 contract holds across segment layouts, before and after merges, and at the
k-boundary as decided.

**Independent Test**: `tests/determinism.rs` green — builds the corpus one way and four ways and
compares.

- [ ] T043 [US2] Implement `TantivyIndex::merge` in `crates/xtriever-lexical/src/index.rs` per research **D15**: ensure a writer exists (lazy create), `commit` if pending, collect `index.searchable_segment_ids()?`, if more than one call `writer.merge(&ids).wait()?`, then `writer.commit()` and `reader.reload()`; no-op with `Ok(())` on 0 or 1 segments
- [ ] T044 [US2] Add a `#[doc]` note on `search` in `crates/xtriever-lexical/src/index.rs` stating the FR-014 k-boundary rule in the words of [contracts/lexical-index.md](./contracts/lexical-index.md) ("Contract caveat") — the crate-side half of the ADR-0005 amendment; the core-side half is T069
- [ ] T045 [US2] Run `cargo nextest run -p xtriever-lexical --test determinism` — all five scenarios **green**, including the k-boundary membership assertion. If the four-batch build differs from the one-batch build on any golden, that is a Principle VI finding: record it in `report.md` with the differing hits and stop (Rule 6)

**Checkpoint**: multi-segment ranking is proven identical to single-segment ranking.

---

## Phase 6: User Story 3 — Documents can be replaced and removed (Priority: P2)

**Goal**: `delete` works, replace-by-id is observable, statistics count live docs, uncommitted
state is invisible.

**Independent Test**: `tests/mutation.rs` green from `mutations.json`.

- [ ] T046 [US3] Implement `LexicalIndex::delete` in `crates/xtriever-lexical/src/index.rs`: lazily create the writer; `delete_term(Term::from_field_u64(__xt_id, id.0 as u64))` per id; unknown ids are silently a no-op (FR-009, and the backend's `delete_term` semantics in D7)
- [ ] T047 [US3] Confirm FR-010/FR-011 in `crates/xtriever-lexical/src/index.rs` by inspection and doc comment: no read path touches `writer`; `Drop` is the backend's (uncommitted ops discarded, lock released) — add a crate-level doc sentence saying so
- [ ] T048 [US3] Run `cargo nextest run -p xtriever-lexical --test mutation --test roundtrip_prop` — all **green** (`stats().num_docs` in scenario 4 uses the live count the backend's `Searcher::num_docs()` already provides; the full `stats()` lands in Phase 8, so if `mutation.rs` needs it, gate that one assertion on Phase 8 with a clear `// Phase 8` comment rather than a stub)

**Checkpoint**: Stories 1–3 green; the index is mutable and durable.

---

## Phase 7: User Story 4 — Filters restrict results and can be shared with other stages (Priority: P2)

**Goal**: Every `Filter` variant resolves to a `DocSet`; filtered search leaves scores untouched;
the algebra holds under property testing.

**Independent Test**: `tests/filters.rs` and `tests/filter_algebra_prop.rs` green.

- [ ] T049 [US4] Implement leaf evaluation in `crates/xtriever-lexical/src/filter.rs` per research **D10**: `fn resolve(f: &Filter, fields: &FieldMap, searcher: &Searcher) -> Result<DocSet>`; type rules from data-model ("Filter leaf evaluation" table — `Text` supports **`Exists` only**; `Value` variant must match `FieldKind` else `InvalidQuery` naming field and both types); `Eq` ⇒ `TermQuery(…, Basic)`; `In`/`Ids` ⇒ `TermSetQuery`; `Range` ⇒ `RangeQuery::new(Bound::Included/Unbounded …)`, with **`(None, None)` rewritten to `Exists`** (the backend panics with no bound); `Exists` ⇒ `ExistsQuery::new(name, false)` where a `Text` field's name is its `__xt_len_<f>` column; each leaf runs through `searcher.search(&q, &DocSetCollector)` and maps `DocAddress → DocId` via `__xt_id`
- [ ] T050 [US4] Implement combinators in `crates/xtriever-lexical/src/filter.rs`: `And` ⇒ fold `intersection` (empty `And` ⇒ alive set); `Or` ⇒ fold `union` (empty ⇒ empty set); `Not(f)` ⇒ `alive.difference(&resolve(f))` where `alive = resolve(Exists(__xt_id))` computed once per call; all via `xtriever_core::DocSet` ops
- [ ] T051 [US4] Implement `LexicalIndex::resolve_filter` in `crates/xtriever-lexical/src/index.rs` (one searcher, delegate to `filter::resolve`) and complete the `Some(filter)` branch of `search` from T039: `Arc::new(set.as_bitmap().clone())` into `search::top_k`
- [ ] T052 [US4] Run `cargo nextest run -p xtriever-lexical --test filters --test filter_algebra_prop` (files under `crates/xtriever-lexical/tests/`) — **green**, with the proptest run reporting ≥ 1,000 cases per property (SC-006). **Checkpoint — PR 3.**

---

## Phase 8: User Story 5 — Corpus statistics are available for scoring (Priority: P3)

**Goal**: Live-only `term_stats` and `stats`, plus the FR-025 divergence measured and recorded.

**Independent Test**: `tests/stats.rs` green; the `#[ignore]` measurement prints a
`DivergenceRecord`.

- [ ] T053 [US5] Implement `term_stats` in `crates/xtriever-lexical/src/stats.rs` per research **D7/D8**: field must be `Text` or `Keyword` (else `InvalidQuery`); for each segment reader `inverted_index(field)?.read_postings(&term, IndexRecordOption::WithFreqs)?`; if `None` in every segment ⇒ `Ok(None)`; else walk postings with `advance()` until `TERMINATED`, skipping docs where `alive_bitset().is_some_and(|b| b.is_deleted(doc))`, summing `doc_freq += 1` and `total_term_freq += term_freq()`; if the live sum is zero the term is *seen zero times* — return `Some(TermStats { 0, 0 })`, which is distinct from `None` (FR-026)
- [ ] T054 [US5] Implement `stats` in `crates/xtriever-lexical/src/stats.rs`: `num_docs = searcher.num_docs()`; for each `Text` field, over every segment's `doc_ids_alive()` read `fast_fields().u64("__xt_len_<f>")?.first(doc)`, summing values and counting docs that have one; `avg_field_len[f] = sum / count` as `f32`; a field with `count == 0` is **absent** from the map (contract table)
- [ ] T055 [US5] Implement `LexicalIndex::term_stats` and `stats` in `crates/xtriever-lexical/src/index.rs` delegating to `stats.rs` with one searcher each
- [ ] T056 [US5] Run `cargo nextest run -p xtriever-lexical --test stats` — **green** (SC-011); then `cargo nextest run -p xtriever-lexical --test stats -- --ignored` and paste the printed `DivergenceRecord` (before and after `merge()`) into `specs/002-lexical-stage/report.md` under "FR-025 divergence" with `verdict` `agree`/`diverge` exactly as observed — the expected outcome is `diverge` before merge and `agree` after (research D8); **never adjust either number** (Rule 6). If it constrains the `Ranker`, open an ADR stub and say so in the report

**Checkpoint**: Stories 1–5 green; the one predicted Principle VI divergence is on record.

---

## Phase 9: User Story 6 — One index can be shared across threads under a lock (Priority: P3)

**Goal**: The exclusive-writer decision is sound in practice, and two-handle behaviour is exactly
what research D14 defines.

**Independent Test**: `tests/concurrency.rs` green.

- [ ] T057 [US6] Add a compile-time assertion `const _: () = { fn assert_send_sync<T: Send + Sync>() {} let _ = assert_send_sync::<TantivyIndex>; };` (or the `static_assertions`-free equivalent) in `crates/xtriever-lexical/src/index.rs`, and audit that no `Rc`, `RefCell`, `thread_local!` or mutable `static` exists in `src/` (FR-030)
- [ ] T058 [US6] Document sharing on `TantivyIndex` in `crates/xtriever-lexical/src/index.rs`: the `RwLock` recipe, that a write lock across `add`+`commit` blocks queries (FR-029), and the D14 two-handle table verbatim
- [ ] T059 [US6] Run `cargo nextest run -p xtriever-lexical --test concurrency` (files under `crates/xtriever-lexical/tests/`) — **green** (SC-013). If the second-handle `add` does **not** fail with `Error::Backend` while the first holds the lock, or a reader ever observes a mixed state, stop and record the finding

**Checkpoint**: all six stories green on the host.

---

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Retire the scaffold, land the contract change with its ADR, record the baseline and
findings, and run the full gate. This phase completes **PR 4**.

- [ ] T060 Delete `crates/xtriever-lexical/src/scaffold.rs` and every `NotImplemented` reference; run `./scripts/check-no-stubs.sh` — must print PASS for both crates
- [ ] T061 Apply the ADR-0005 amendment to `crates/xtriever-core/src/traits.rs`: add the "Contract caveat" sentence from [contracts/lexical-index.md](./contracts/lexical-index.md) to the doc comments of **both** `LexicalIndex::search` and `VectorIndex::search`, citing `docs/adr/0005-tie-breaking-contract.md` (amended 2026-09-12). **Doc comment only** — `cargo public-api`-style diff or a manual check confirms no signature changed (FR-002, FR-014, Principle V)
- [ ] T062 [P] Write `specs/002-lexical-stage/report.md`: the ADR-0006 **baseline commit hash** (condition 1), the FR-025 `DivergenceRecord` from T056, the T041 verify-ranking summary, any findings from T045/T056/T059, the known-cost notes (O(df) `term_stats`, second analyzer pass, 15 MB arena), and a per-SC table SC-001…SC-013 naming the test that proves each
- [ ] T063 [P] Re-validate `specs/002-lexical-stage/checklists/requirements.md` against the shipped behaviour and append an "Iteration 3 — implementation" note listing any requirement whose wording had to be interpreted (there should be none; if there are, say which)
- [ ] T064 [P] Update `crates/xtriever-lexical/Cargo.toml` `description` and `src/lib.rs` crate docs to their final wording; confirm `cargo doc -p xtriever-lexical --no-deps` builds with zero `missing_docs` warnings
- [ ] T065 Run the full local gate from [quickstart.md](./quickstart.md) Step 6: `cargo fmt --all --check`; `RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets`; `cargo nextest run --workspace`; `cargo deny check`; `cargo check --workspace --target aarch64-apple-ios`; `… aarch64-apple-ios-sim`; `… aarch64-linux-android`; `./scripts/check-no-stubs.sh`; the `cargo tree` purity grep — all must pass (Rule 5)
- [ ] T066 Run `cargo check --workspace --target wasm32-unknown-unknown`, record the first error verbatim in `report.md` as the tracked wasm32 blocker (expected: `mmap`/threads via tantivy; research R5) — best-effort, non-blocking
- [ ] T067 Write the PR description for PR 4 (and retro-fit PRs 1–3 if still open) per quickstart Step 7: nextest summary, the `DivergenceRecord` table, and the line **"eval delta: N/A — ADR-0006"** — its presence is ADR-0006 condition 1 and must not be omitted
- [ ] T068 Final review pass of `crates/xtriever-lexical/src/` against Rule 7: no generics over backends, no macros beyond `thiserror`'s derive, no trait beyond the core impl; and against Principle VII: `grep -rn 'unwrap()\|expect(\|panic!\|todo!\|unimplemented!' crates/xtriever-lexical/src/` returns nothing

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: none. T003/T004 parallel; T005 needs T004; T002 needs T001.
- **Phase 2 (Oracle & red suite)**: needs Phase 1. T006 → T007–T012 (sequential edits of one
  script; T007 first since the corpus needs the schema) → T013. T014 → T015. T016–T023 parallel
  after T013 + T014. T024 last. **Blocks everything below.**
- **Phase 3 (Foundational impl)**: needs T024. T025, T026 parallel; T027 needs T026; T028
  parallel with T027; T029 needs T027 + T028; T030 needs T029; T031 last. **Blocks all stories.**
- **Phase 4 (US1)**: needs Phase 3. T032 → T033 → T034; T035/T036/T037 parallel after T034; T038
  after T035; T039 after T034 + T038; T040 after T039; T041 after T040; T042 last.
- **Phase 5 (US2)**: needs US1 (T042). T043 → T044 → T045.
- **Phase 6 (US3)**: needs US1 (add/commit). Independent of US2. T046 → T047 → T048.
- **Phase 7 (US4)**: needs US1 (search plumbing T039). Independent of US2/US3. T049 → T050 → T051
  → T052.
- **Phase 8 (US5)**: needs US1 (length columns written in T032) and US2 (`merge` for the
  measurement, T043). T053/T054 parallel → T055 → T056.
- **Phase 9 (US6)**: needs US1 and US2 (`merge` in scenario 5). T057/T058 parallel → T059.
- **Phase 10 (Polish)**: needs all stories. T060 first; T061–T064 parallel; T065 → T066 → T067 →
  T068.

### User Story Dependencies

| story | depends on | why |
|---|---|---|
| US1 | Phase 3 | create/open, schema, errors |
| US2 | US1 | needs `search` and `add`/`commit` to have anything to compare |
| US3 | US1 | needs `add`/`commit`; `delete` is its own |
| US4 | US1 | `search`'s filter branch (T039) is the hook it fills |
| US5 | US1, US2 | length columns from `add`; `merge` for the FR-025 measurement |
| US6 | US1, US2 | `merge` during search is scenario 5 |

US2, US3 and US4 are mutually independent after US1 and can proceed in parallel.

### Parallel Opportunities

- **Phase 2**: T003 ∥ T004; T016–T023 are eight independent test files — fully parallel.
- **Phase 3**: T025 ∥ T026; T027 ∥ T028.
- **Phase 4**: T035 ∥ T036 ∥ T037 (three query shapes, same file but disjoint functions — treat as
  parallel only if merged carefully; otherwise sequential in that order).
- **After US1**: US2 ∥ US3 ∥ US4.
- **Phase 8**: T053 ∥ T054. **Phase 9**: T057 ∥ T058. **Phase 10**: T061–T064.

---

## Parallel Example: Phase 2 red suite

```bash
# After T013 (fixtures) and T014 (scaffold), launch all eight test files together:
Task: "Write crates/xtriever-lexical/tests/support/mod.rs + tests/fixtures_valid.rs"   # T016
Task: "Write crates/xtriever-lexical/tests/index_query.rs"                             # T017
Task: "Write crates/xtriever-lexical/tests/determinism.rs"                             # T018
Task: "Write crates/xtriever-lexical/tests/mutation.rs"                                # T019
Task: "Write crates/xtriever-lexical/tests/filters.rs"                                 # T020
Task: "Write crates/xtriever-lexical/tests/filter_algebra_prop.rs"                     # T021
Task: "Write crates/xtriever-lexical/tests/stats.rs"                                   # T022
Task: "Write crates/xtriever-lexical/tests/concurrency.rs + analyzer_prop + roundtrip" # T023
# Then T024: confirm red for the right reason, commit.
```

## Parallel Example: after User Story 1

```bash
Task: "US2 — merge() + determinism green"        # T043–T045
Task: "US3 — delete() + mutation green"          # T046–T048
Task: "US4 — filter.rs + filters green"          # T049–T052
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 → Phase 2 (**PR 1**, red) → Phase 3.
2. Phase 4: US1. Mint the goldens (T041), turn `index_query.rs` green (T042).
3. **STOP and VALIDATE**: `cargo nextest run -p xtriever-lexical --test index_query` green and the
   Python cross-check agrees. That is a working BM25 stage over all six query shapes — demonstrable
   on its own.

### Incremental Delivery

1. PR 1: oracle refactor proven neutral, fixtures, every test red.
2. PR 2: Phase 3 + US1 core (`Match`/`Term`) — first green goldens.
3. PR 3: remaining query shapes + US4 filters — Stories 1 and 4 complete.
4. PR 4: US2, US3, US5, US6 + polish — all 13 SCs, both ADR obligations discharged, full gate.

Each PR stays under ~800 hand-written lines (Rule 3); fixture JSON is generated data and counted
separately.

### Where this plan pre-commits to stopping (Rule 6)

- T005: any diff in the 001 manifest after the oracle refactor.
- T041: any Python/Rust disagreement on a covered query shape.
- T045: any one-batch vs four-batch difference.
- T056: the divergence is *recorded*, whichever way it comes out — never tuned.
- T059: any torn read or an unexpected lock outcome.

---

## Notes

- Read the cited research decision before implementing any task that names one; the `file:line`
  citations into tantivy 0.26.2 are the Rule 1 evidence and also where the three panicking
  constructors are documented.
- The scaffold (T014) exists only so Phase 2 ends in runtime failures. T060 removes it and
  `check-no-stubs.sh` enforces that it is gone.
- Every PR description carries "eval delta: N/A — ADR-0006" (T067). Its absence is a gate failure,
  not an oversight.
- Commit after each task or logical group; stop at any checkpoint to validate the story
  independently.
