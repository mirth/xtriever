# Phase 0 Research: The Lexical Stage

**Feature**: `002-lexical-stage` | **Date**: 2026-09-11 | **Plan**: [plan.md](./plan.md)

Every backend item below was read from the pinned source, not from memory (Agent Operating Rule 1).
`T` abbreviates `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tantivy-0.26.2/`, which is
the exact crate Feature 001 built and measured. Line numbers are from that checkout.

Format per decision: **Decision** / **Rationale** / **Alternatives considered** / **Evidence**.

---

## D1. Writer configuration: one indexing worker, one merge worker, minimum arena

**Decision**: Build the writer with `IndexWriterOptions::builder().num_worker_threads(1)
.num_merge_threads(1).memory_budget_per_thread(MEMORY_BUDGET_NUM_BYTES_MIN).build()` via
`Index::writer_with_options`. Never `Index::writer`, which sizes the pool from the host.

**Rationale**: tantivy's own test helper says why one worker matters: *"Using a single thread gives
us a deterministic allocation of DocId"* (`T/src/index/index.rs:596-601`). Each worker builds its
own segment, so worker count determines segment layout, which determines `DocAddress`, which is the
backend's tie-break. Fixing both counts to 1 makes thread count a constant of the crate (FR-004) and
keeps the footprint minimal on mobile. The arena minimum is `MEMORY_BUDGET_NUM_BYTES_MIN =
MARGIN_IN_BYTES * 15` = 15,000,000 bytes (`T/src/indexer/index_writer.rs:29-32`); anything below is
rejected with `InvalidArgument` (`:285-288`). Feature 001 used the same value for the same reason.

**Alternatives considered**: `writer_with_num_threads(1, 15_000_000)` — equivalent but leaves
`num_merge_threads` at its default of 4 (`T/src/indexer/index_writer.rs:64-66`). Rejected: merge
thread count should be pinned too.

**Evidence**: `IndexWriterOptions` fields and defaults `T/src/indexer/index_writer.rs:49-67`;
`writer_with_options` `T/src/index/index.rs:539-556`; lock acquisition on construction `:544-555`.

## D2. The writer is created lazily, on the first mutation

**Decision**: `TantivyIndex` holds `Option<IndexWriter>`. `add`/`delete` create it on first use;
`commit` with no writer is a no-op returning `Ok(())`. The writer lives until the index is dropped.

**Rationale**: Three facts combine. (1) Creating a writer acquires `INDEX_WRITER_LOCK` and fails
with `TantivyError::LockFailure` if another writer holds it, in this or another process
(`T/src/index/index.rs:544-555`). (2) It allocates the 15 MB arena and spawns threads (D1, FR-004).
(3) Dropping it kills the segment updater and joins the workers (`T/src/indexer/index_writer.rs:
807-815`). So an index opened only to serve queries should pay none of that, and two handles on the
same directory should both open successfully — the conflict surfaces only when the second one
*mutates*. That gives FR-031 a clean, testable rule (D14).

**Alternatives considered**: Eager writer at open — simpler code, but every reader pays 15 MB and
three threads, and `open` on a directory another handle is writing would fail outright. Rejected.

## D3. Readers reload manually, after each commit; searches are snapshot-consistent

**Decision**: `index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()`. `commit()`
calls `writer.commit()` then `reader.reload()`. Every read method takes `reader.searcher()` once
and uses that `Searcher` for the whole call.

**Rationale**: `ReloadPolicy::Manual` — *"No change is reflected automatically. You are required to
call `IndexReader::reload()` manually"* (`T/src/reader/mod.rs:22-27`). The alternative
`OnCommitWithDelay` watches `meta.json` on a background thread (`:28-30`) — another thread, and
reads that change *between* two calls with no mutation in between, which would break FR-015's
repeat-call determinism. A `Searcher` is a fixed set of segment readers, so a single call cannot
observe a half-applied commit (FR-030). `IndexReader::reload` `T/src/reader/mod.rs:287`; `searcher`
`:298`.

**Alternatives considered**: Reload on every search — costs a `meta.json` check per query and
makes a second handle's view change under it (D14). Rejected.

## D4. Field mapping: `xtriever-core` kinds → tantivy options

**Decision** (the table the plan is bound to):

| `FieldKind` | tantivy field | options | why |
|---|---|---|---|
| `Text(id)` | `add_text_field` | `TextOptions::default().set_indexing_options(TextFieldIndexing::default().set_tokenizer(<D5 name>).set_index_option(IndexRecordOption::WithFreqsAndPositions))` + `STORED` if `stored` | positions always on (FR-018); tokenizer from the D5 table |
| `Keyword` | `add_text_field` | `STRING` + `FAST` (`set_fast(None)`) + `STORED` if `stored` | `STRING` = raw tokenizer, `IndexRecordOption::Basic` (`T/src/schema/text_options.rs:264-273`); FAST needed by `ExistsQuery` and fast-field `RangeQuery` |
| `U64`/`I64`/`F64`/`Bool` | `add_u64_field` etc. | `INDEXED | FAST` + `STORED` if `stored` | INDEXED for `TermQuery`/`TermSetQuery` (Eq/In/Ids), FAST for `RangeQuery`/`ExistsQuery` |
| `DateMillis` | **`add_i64_field`** — *not* `add_date_field` | as numeric | see D6 |

`indexed: false` on any kind drops `INDEXED` (and the text indexing options) and keeps `FAST`/`STORED`
only; every query on such a field is `InvalidQuery` (FR-018).

**Evidence**: `SchemaBuilder::add_*` `T/src/schema/schema.rs:50-148`; `TEXT`/`STRING` constants
`T/src/schema/text_options.rs:264-285`; `TextOptions::set_fast`/`set_stored`/`set_indexing_options`
`:129-157`; `TextFieldIndexing::set_tokenizer`/`set_index_option` `:224-250`; `NumericOptions::
set_indexed`/`set_fast`/`set_stored` `T/src/schema/numeric_options.rs:97-128`; flags
`T/src/schema/flags.rs:16-53`.

## D5. Analyzer table: two ids, both mapped to tokenizers tantivy already registers

**Decision**:

| `AnalyzerId` | tantivy tokenizer name | chain | independent oracle |
|---|---|---|---|
| `"standard"` | `"default"` | `SimpleTokenizer → RemoveLongFilter::limit(40) → LowerCaser` | Feature 001's transcription, unchanged |
| `"standard_en"` | `"en_stem"` | same + `Stemmer::new(Language::English)` | 001's transcription + Python `snowballstemmer` (risk R2) |

Any other id → `Error::Schema` at index creation naming the field and the id (FR-006). No
`TokenizerManager::register` call is needed for these two: both are in `TokenizerManager::default()`
(`T/src/tokenizer/tokenizer_manager.rs:55-80`). `Index::tokenizer_for_field(field)`
(`T/src/index/index.rs:418`) returns the `TextAnalyzer` for query-time analysis (D9) and for the
length column (D7).

**Rationale**: Both chains exist in the backend already, so the table adds zero tokenizer code
(Principle I) and `"standard"` reuses 001's verified oracle byte-for-byte. `"standard_en"` is the
smallest step that makes the table a table rather than a single entry, and English stemming is the
one analysis feature a RAG corpus is likely to want first.

**Alternatives considered**: Adding `StopWordFilter` to `standard_en` — would require transcribing
tantivy's embedded stopword list into Python; deferred. Registering custom chains — nothing to gain
while both needed chains ship with the backend. Wiring core's `Analyzer` trait — cut by
clarification (spec Clarifications, Q5).

**Evidence**: `TextAnalyzer::builder`/`token_stream` `T/src/tokenizer/tokenizer.rs:63-68`;
`RemoveLongFilter::limit` `T/src/tokenizer/remove_long.rs:29`; `Stemmer::new`/`Language`
`T/src/tokenizer/stemmer.rs:12-69`; `rust-stemmers 1.2.0` `T/Cargo.toml:335-337`.

## D6. `DateMillis` is stored as `i64`, not as a tantivy date field

**Decision**: Map `FieldKind::DateMillis` to an `i64` field holding the millisecond value verbatim.

**Rationale**: A tantivy date field's **inverted-index** precision is hard-coded to seconds:
`DATE_TIME_PRECISION_INDEXED: DateTimePrecision = DateTimePrecision::Seconds`
(`T/src/schema/date_time_options.rs:9`), applied at index time
(`T/src/indexer/segment_writer.rs:245`) and by `Term::from_field_date_for_search`
(`T/src/schema/term.rs:185-191`). The fast-field precision defaults to seconds too
(`DateTimePrecision` `#[default] Seconds`, `tantivy-common-0.11.0/src/datetime.rs:16-19`). So an
`Eq` on a `DateMillis` value through a date field would silently match at second granularity —
exactly the silent coercion FR-007/FR-021 forbid. An `i64` field has no such truncation, and nothing
in `Filter` needs date-specific semantics; `Range`, `Eq`, `In`, `Exists` all work on `i64`.

**Alternatives considered**: `DateOptions::set_precision(Milliseconds)` — fixes the fast field only;
the indexed term stays at seconds. Rejected.

## D7. Hidden columns: `__xt_id` for identity, `__xt_len_<field>` for exact live-only lengths

**Decision**: Two kinds of hidden field, both under a reserved `__xt_` prefix that user schemas may
not use (`Error::Schema` if they do):

- `__xt_id`: `u64`, `INDEXED | FAST`, holding `DocId.0`. Identity for replace/delete
  (`delete_term(Term::from_field_u64(..))`), for `Ids` filters (`TermSetQuery`), for the
  `FilterCollector` predicate (D11), and for mapping every `DocAddress` back to a `DocId`
  (`fast_fields().u64("__xt_id")?.first(doc)`).
- `__xt_len_<name>` for every `Text` field: `u64`, `FAST` only, holding the token count produced by
  the field's analyzer. Written only when the document carries the field.

**Rationale for the length columns**: FR-024/FR-027 require `IndexStats::avg_field_len` to be the
exact live-only average token count. The backend cannot supply that: its per-document fieldnorms are
quantized through a 256-entry table (Feature 001 transcribed it), and its unquantized aggregate
`InvertedIndexReader::total_num_tokens()` *"includ[es] deleted documents"*
(`T/src/index/inverted_index_reader.rs:249-253`). Summing our own column over
`SegmentReader::doc_ids_alive()` (`T/src/index/segment_reader.rs:446`) and dividing by
`Searcher::num_docs()` (live, `T/src/core/searcher.rs:122-129`) gives the exact number. The column
doubles as the `Exists` marker for text fields (D10) and is the `doc.len` feature core already names
for LTR. Cost: one extra pass of the analyzer per text field per document at index time; no
performance budget is set.

**Evidence**: `Term::from_field_u64` `T/src/schema/term.rs:157`; `IndexWriter::delete_term`
semantics — *"only affects documents that were added in previous commits, and documents that were
added previously in the same commit"* (`T/src/indexer/index_writer.rs:672-680`), which is what makes
delete-then-add within one batch a correct replace; `Column::first` `tantivy-columnar/src/column/
mod.rs:88`; `FastFieldReaders::u64` `T/src/fastfield/readers.rs:176`.

## D8. Statistics the backend uses for scoring are deletion-inclusive — FR-025 is now precise

**Finding** (not a decision): `Bm25Weight::for_terms` takes `total_num_tokens`, `total_num_docs`
and `doc_freq` from `Bm25StatisticsProvider` (`T/src/query/bm25.rs:95-129`). The `Searcher`
implementation sums `total_num_tokens()` (deleted included, D7), sums **`max_doc()`** — not
`num_docs()` — for the document count (`T/src/query/bm25.rs:38-45`), and reads `doc_freq` from the
term dictionary, which is not adjusted for deletes (`T/src/index/inverted_index_reader.rs:275-281`).
So every BM25 input is deletion-inclusive until a merge physically drops the documents.

**Consequence**: FR-015's "different mutation history" case *will* diverge, and FR-025's measurement
has a closed-form prediction: the divergence is exactly the difference between (N, df, Σtokens) over
all docs and over live docs. The measurement in the report should show both numbers side by side.
This also means `term_stats` (live-only per FR-024) will not equal the df the scorer used on an index
with pending deletes — which is the FR-025 finding, recorded and not adjusted.

**Also**: `TermInfo` carries `doc_freq` but no total term frequency (`T/src/postings/term_info.rs:
9-16`), so `total_term_freq` is computed by walking postings and summing `Postings::term_freq()`
(`T/src/postings/postings.rs:13-15`) over alive docs — O(df) per call, acceptable without a budget.

## D9. Query translation

**Decision** — one translation per `LexicalQuery` variant, all returning `Box<dyn Query>`:

| variant | translation | notes |
|---|---|---|
| `Match(Some(f), text)` | analyze `text` with `tokenizer_for_field(f)`; `BooleanQuery::new(vec![(Occur::Should, TermQuery)…])`; wrap in `BoostQuery` if `boost ≠ 1.0` | `TermQuery::new(term, IndexRecordOption::WithFreqs)` (`T/src/query/term_query/term_query.rs:73`); zero tokens ⇒ `EmptyQuery` |
| `Match(None, text)` | the above for every indexed `Text` field, each field's clauses under that field's boost, all as `Should` of one outer `BooleanQuery` | FR-017 sum semantics hold because `BooleanQuery` uses `SumCombiner` (`T/src/query/boolean_query/boolean_query.rs:157-169`) |
| `Phrase(f, text, slop)` | analyze; **≥2 terms** ⇒ `PhraseQuery::new_with_offset_and_slop(terms, slop)`; exactly 1 ⇒ `TermQuery`; 0 ⇒ `EmptyQuery` | `PhraseQuery::new*` **panics** on fewer than two terms (`T/src/query/phrase_query/phrase_query.rs:48-52`) — the guard is mandatory, not stylistic. Score = BM25 with tf = phrase count (`phrase_scorer.rs:578-582`) |
| `Term(f, s)` | `TermQuery::new(Term::from_field_text(f, s), WithFreqs)` verbatim | on `Keyword` or `Text` only; other kinds ⇒ `InvalidQuery` |
| `Fuzzy(f, s, d)` | `FuzzyTermQuery::new(term, d, false)` | `false` = transposition costs two (spec FR-017, strict Levenshtein). `d > 2` ⇒ backend `InvalidArgument` (`T/src/query/fuzzy_query.rs:118-126`); we reject `d > 2` up front as `InvalidQuery`. **Score is constant `1.0 × boost`** — `AutomatonWeight::scorer` returns a `ConstScorer` (`T/src/query/automaton_weight.rs:87-110`) — so the Fuzzy oracle is set membership |
| `Bool{must,should,must_not}` | `BooleanQuery::new` with `Occur::{Must,Should,MustNot}` | only `MustNot` clauses, or no clauses ⇒ matches nothing (backend semantics, `boolean_query.rs:10-12`); documented, tested |
| `Boost(q, b)` | `BoostQuery::new(translate(q), b)` (`T/src/query/boost_query.rs:20`) | composes multiplicatively with field boost |

`Match`/`Phrase` on a non-`Text` field, `Phrase` on `Keyword`, any query on `indexed: false` ⇒
`Error::InvalidQuery` naming the field, before the backend is called (FR-018).

## D10. Filter evaluation: leaves through the backend, algebra through `roaring`

**Decision**: `resolve_filter` evaluates each **leaf** with a backend query collected via
`DocSetCollector` (`T/src/collector/docset_collector.rs:9-12`, fruit `HashSet<DocAddress>`) and
mapped to `DocId` through `__xt_id`; combinators are `roaring` set operations on `xtriever-core`'s
`DocSet` (`intersection`/`union`/`difference`), with `Not(f)` = `alive − resolve(f)` where `alive`
is `resolve(Exists("__xt_id"))`.

| leaf | backend query |
|---|---|
| `Eq(f, v)` | `TermQuery(Term::from_field_{u64,i64,f64,bool,text}(..), Basic)` |
| `In(f, vs)` / `Ids(ids)` | `TermSetQuery::new(terms)` (`T/src/query/set_query.rs:19`) |
| `Range(f, lo, hi)` | `RangeQuery::new(Bound<Term>, Bound<Term>)` (`T/src/query/range_query/range_query.rs:80`), inclusive bounds; fast-field path since every filterable field is FAST (`:106-108`, valid types `mod.rs:13-21`) |
| `Range(f, None, None)` | rewritten to `Exists(f)` — `RangeQuery` **panics** with no bound set (`range_query.rs:96-100`) |
| `Exists(f)` | `ExistsQuery::new(name, false)` (`T/src/query/exist_query.rs:64`) — requires FAST; for `Text` fields the name is `__xt_len_<f>` (D7) |

Type rules: `Eq`/`In`/`Range` on a `Text` field ⇒ `InvalidQuery` (filters are metadata predicates;
`Exists` is the only filter a text field supports). A `Value` variant that does not match the
`FieldKind` ⇒ `InvalidQuery` naming the field (FR-021).

**Rationale**: The algebra lives in one place, in plain set operations, which is exactly what
FR-023's property tests exercise; the backend is asked only questions it answers natively. Principle
I names `roaring` for doc sets. Deleted documents are excluded because every leaf query runs through
the `Searcher`, which applies alive bitsets.

**Alternatives considered**: Translating the whole `Filter` into one backend `BooleanQuery` — puts
the algebra in the backend and needs `AllQuery`+`MustNot` gymnastics for `Not`. Rejected. Scanning
fast-field columns directly in Rust — reimplements what `TermQuery`/`RangeQuery` do. Rejected.

## D11. Filtered search: `FilterCollector` over `__xt_id` wrapping `TopDocs`

**Decision**: `search(q, Some(filter), k)` resolves the filter to a `DocSet` (D10), wraps the bitmap
in an `Arc`, and searches with `FilterCollector::new("__xt_id".into(), move |xid: u64|
bitmap.contains(xid as u32), TopDocs::with_limit(k).order_by_score())`. Unfiltered search uses the
inner `TopDocs` directly. Both paths then map `DocAddress → DocId` and re-sort (D12).

**Rationale**: `FilterCollector` *"filters docs using a fast field value and a predicate. Only the
documents … for which the predicate returns `true` will be passed on to the next collector"*
(`T/src/collector/filter_collector_wrapper.rs:18-24`, ctor `:85-92`, predicate bound `Fn(T) -> bool +
Send + Sync + Clone + 'static` `:79-83`). Scores are untouched by construction (FR-022), the
k-boundary behaviour is identical filtered and unfiltered because the same `TopDocs` decides it
(FR-014), and there is no custom collector to get wrong (Principle I, Rule 7). `TopDocs` tie-break
is ascending `DocAddress` (`T/src/collector/top_score_collector.rs:26-28`), which is what ADR-0005
re-sorts.

**Trap**: `TopDocs::with_limit(0)` **panics** (`top_score_collector.rs:92-94`). `k == 0` returns
`Ok(vec![])` before any collector is built.

**Alternatives considered**: `BooleanQuery[Must(q), Must(ConstScoreQuery(filter, 0.0))]` — keeps
scores but needs the whole filter as a backend query (D10's rejected path) and a second evaluation
route to test for agreement. Rejected.

## D12. Re-sort after collection; accept the k-boundary

**Decision**: After `TopDocs` returns `Vec<(Score, DocAddress)>`, map each address to `DocId` via
`__xt_id` and `sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)))`. No over-fetch
(FR-014). `f32::total_cmp` avoids the `PartialOrd` `unwrap` and orders NaN deterministically.

**Rationale**: ADR-0005 as accepted; the k-boundary is the spec's clarification Q1.

## D13. On-disk descriptor for FR-012

**Decision**: `create(dir, schema)` requires `dir` to be empty or absent, creates the backend index
with `Index::create_in_dir` (`T/src/index/index.rs:333-340`, `MmapDirectory` under the `mmap`
feature), then writes `<dir>/xtriever-lexical.json` = `{ "format_version": 1, "schema": <serde
Schema> }`. `open(dir)` reads the descriptor first: missing ⇒ `Error::Corrupt("not an
xtriever-lexical index")`; `format_version ≠ 1` ⇒ `Corrupt`; then `Index::open_in_dir` (`:464-467`)
and a check that the backend schema equals the one rebuilt from the descriptor ⇒ `Corrupt` on
mismatch. `schema()` returns the descriptor's `Schema` — it needs no backend call.

**Rationale**: The constitution's fingerprint rule (VI) is about hard errors at open, never silent
reinterpretation. The descriptor is plain `serde_json`; `Schema` already derives `Serialize/
Deserialize` in core. The analyzer id is inside the schema, so an analyzer change is a schema
mismatch — the lexical analogue of the embedder fingerprint.

## D14. Two handles on one directory — the defined behaviour (FR-031)

**Decision**:

| situation | outcome |
|---|---|
| second `open` while another handle exists | succeeds (readers only) |
| second handle **mutates** while the first holds the writer | `Error::Backend(LockFailure)` from D2's lazy writer creation; the first handle is unaffected |
| second handle reads after the first commits | sees its own last reload, i.e. the state at its own open/commit — **not** the other handle's commit (D3) |
| first handle dropped, then second mutates | succeeds — `IndexWriter::drop` releases the lock |

Sharing a directory between handles is *defined* but *not recommended*; the doc comment says so.

## D15. Merge control

**Decision**: One inherent method `TantivyIndex::merge(&mut self) -> Result<()>` merging all
searchable segments into one via `IndexWriter::merge(&segment_ids).wait()`
(`T/src/indexer/index_writer.rs:528-539`; `FutureResult::wait` `T/src/future_result.rs:52`) then
reloading the reader. Background merges stay on (default `LogMergePolicy`).

**Rationale**: FR-015 scenario 4 and FR-025 need "before and after a merge" to be a controlled step,
not a race against the merge thread. It is also a legitimate operational compaction. It is the only
public method beyond the trait and two constructors, and it is additive (Principle V unchanged).

**Alternatives considered**: `NoMergePolicy` + manual merges only — deterministic layout, but
segments grow unbounded in production. Rejected. No control — "after merge" untestable. Rejected.

## D16. Error mapping (FR-003)

| condition | `xtriever_core::Error` |
|---|---|
| field not in schema (document, query, filter, `term_stats`) | `UnknownField(name)` |
| value type ≠ field kind in a **document**; unknown analyzer id; boost ≠ 1.0 on non-text; reserved `__xt_` name; duplicate field name | `Schema(msg)` |
| value type ≠ field kind in a **filter**; `Match`/`Phrase` on non-text; `Phrase` on `Keyword`; query on `indexed: false`; `Fuzzy` distance > 2; `Eq`/`In`/`Range` on `Text` | `InvalidQuery(msg)` |
| descriptor missing / wrong version / schema mismatch | `Corrupt(msg)` |
| `std::io::Error` | `Io` (via `From`) |
| `TantivyError::LockFailure` and every other backend error | `Backend(Box<TantivyError>)` |

`Error` is `#[non_exhaustive]` with `Backend` as *"the escape hatch for anything else"*
(`crates/xtriever-core/src/error.rs:1-2`), so no new variant is needed (FR-002/FR-003).

## D17. Independent oracle: extend Feature 001's Python, split into a shared module

**Decision**: Move the analyzer transcription, `FIELD_NORMS_TABLE`, `idf` and BM25 scoring out of
`reference/gen_001_fixtures.py` into `reference/xtref/bm25.py`; 001's script imports it and its
fixtures are regenerated and byte-compared against the committed manifest as the refactor's own
test. `reference/gen_002_fixtures.py` builds the 002 corpus, goldens and manifest.

Oracle coverage, per query shape (SC-005 is scoped to "shapes the reference covers"):

| shape | covered by Python | how |
|---|---|---|
| `Match` (single field, multi-field, boosts) | yes, score ± 1e-5 relative | sum of per-term, per-field BM25 with quantized fieldnorm and `max_doc`-based avg |
| `Term` | yes | single-term BM25 |
| `Phrase`, slop = 0 | yes | tf = exact phrase occurrence count |
| `Phrase`, slop > 0 | **membership only** | backend's slop algorithm (`phrase_scorer.rs:145+`) is not transcribed; scores are Rust-minted goldens, exact on repeat |
| `Fuzzy` | yes, exact | Levenshtein (transposition = 2) membership; score 1.0 × boost |
| `Bool`, `Boost` | yes where clauses are | sum / product of covered clauses |
| filters | yes, exact sets | evaluated over the corpus in Python |
| `term_stats`, `stats` | yes, exact | counted over the corpus |

Ranking goldens are minted by a Rust example (`gen_ranking`, as in 001) and cross-checked against
Python at mint time; the committed goldens are compared exactly at test time.

## D18. Dependencies (added with `cargo add`, never from memory)

| crate | why | purity |
|---|---|---|
| `tantivy` — `default-features = false`, features `mmap`, `stopwords`, `lz4-compression`, `stemmer` | the backend, Feature 001's measured set (`T/Cargo.toml:54-73`: `default` minus `columnar-zstd-compression`) | zero C/C++ (001 verdict matrix) |
| `roaring` (workspace) | already core's `DocSet` | pure |
| `serde`, `serde_json` | descriptor (D13) | pure |
| `thiserror` (workspace) | already core's error | pure |
| dev: `proptest`, `tempfile`, `serde_json` | FR-023/FR-034 properties; on-disk tests; fixture loading | pure |

`cargo deny check` bans stay as they are; no `deny.toml` change (Rule 2).

---

## Risks

| # | risk | mitigation |
|---|---|---|
| R1 | Python `snowballstemmer` and `rust-stemmers 1.2.0` disagree on some English word | both derive from Snowball; any disagreement is a finding on the `standard_en` golden, not a tolerance change (Rule 6). Corpus vocabulary is fixed, so the check is exhaustive |
| R2 | Float summation order across fields/terms differs between Python and the backend | 1e-5 relative is the tolerance 001 justified; 001 saw 8.3e-08 |
| R3 | `FilterCollector` requires the fast column to be present in every segment | `__xt_id` is written for every document, so the column always exists |
| R4 | `LogMergePolicy` merges between two "identical" queries in a test, changing `DocAddress` but not results | that is precisely FR-015's claim; if results differ, it is a finding |
| R5 | wasm32 `cargo check` fails (`mmap`, threads) | best-effort target per the constitution; tracked, not blocking |
| R6 | `docstore_compress_dedicated_thread` defaults to `true` (`T/src/index/index_meta.rs:218-223`) — one more backend thread | documented under FR-004; not disabled (doc-hidden setting) |
| R7 | A stale `INDEX_WRITER_LOCK` after a crash blocks all writers (`T/src/index/index.rs:566-571`) | `Backend(LockFailure)` surfaces with the backend's own message; deletion is operator action, documented |
| R8 | `analyze`-twice cost for length columns is visible on large corpora | no budget is set; recorded as the known cost of exact statistics |
