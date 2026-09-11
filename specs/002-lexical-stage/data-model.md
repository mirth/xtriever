# Phase 1 Data Model: The Lexical Stage

**Feature**: `002-lexical-stage` | **Date**: 2026-09-11 | **Plan**: [plan.md](./plan.md)

Three groups: **runtime entities** (what the crate holds in memory), **on-disk entities** (what an
index directory contains), and **fixture entities** (committed goldens). Core types (`Schema`,
`Document`, `Filter`, `LexicalQuery`, `Hit`, `DocSet`, `TermStats`, `IndexStats`, `Error`) are
*not* redefined here — they are `xtriever-core`'s and unchanged (FR-002).

---

## Runtime entities

### `TantivyIndex`

The one public type (FR-029). Implements `xtriever_core::LexicalIndex`.

| field | type | rules |
|---|---|---|
| `schema` | `xtriever_core::Schema` | the schema the index was created with; returned by `schema()` without a backend call |
| `fields` | `FieldMap` | derived from `schema` at create/open; never mutated |
| `index` | backend `Index` | opened over a memory-mapped directory (research D13) |
| `reader` | backend `IndexReader` | `ReloadPolicy::Manual`; reloaded only by `commit` and `merge` (D3) |
| `writer` | `Option<backend IndexWriter>` | `None` until the first `add`/`delete` (D2); one indexing worker, one merge worker, 15 MB arena (D1) |

**Invariants**

- `Send + Sync` holds structurally (the backend's `Index`, `IndexReader`, `IndexWriter` are all
  `Send + Sync`); no `Rc`, `RefCell`, thread-locals or statics (FR-030).
- A read method never touches `writer`; a mutating method never touches `reader` except `commit`
  and `merge`, which reload it after the backend commit returns.
- Every `Searcher` is taken once per call and dropped at the end of that call.

**State**

```
Created/Opened ──add/delete──► Dirty ──commit──► Clean (== Created/Opened)
                                 │
                                 └──drop──► uncommitted mutations discarded (FR-011)
```

`merge` is valid in any state; on `Dirty` it commits first (the backend requires committed segments
to merge).

### `FieldMap`

Derived, immutable mapping from the core schema to backend fields.

| field | type | rules |
|---|---|---|
| `by_name` | `BTreeMap<FieldName, MappedField>` | one entry per user field |
| `id_field` | backend `Field` | the hidden `__xt_id` (u64, INDEXED \| FAST) |
| `text_fields` | `Vec<FieldName>` | indexed `Text` fields in schema order — the iteration order for `Match(None, …)` |

### `MappedField`

| field | type | rules |
|---|---|---|
| `field` | backend `Field` | |
| `kind` | `FieldKind` | copied from the `FieldDef`; drives value-type checks (D16) |
| `indexed`, `stored` | `bool` | |
| `boost` | `f32` | `1.0` for non-text (a non-neutral value on non-text is a creation error, FR-005) |
| `analyzer` | `Option<&'static str>` | backend tokenizer name from the analyzer table (D5); `Some` iff `kind` is `Text` |
| `len_field` | `Option<backend Field>` | the hidden `__xt_len_<name>` column; `Some` iff `kind` is `Text` |

**Validation at construction** (all `Error::Schema` unless noted):

1. Field names are unique and none starts with `__xt_`.
2. `Text(id)` ⇒ `id` is in the analyzer table.
3. `boost != 1.0` ⇒ `kind` is `Text`.
4. `Bool`, `F64` etc. accept `boost == 1.0` only.

### `TranslatedQuery`

Not a struct — a `Box<dyn backend Query>` produced by `translate(&LexicalQuery, &FieldMap)` (D9).
Listed here because its validation rules are the spec's FR-018:

| input | check | error |
|---|---|---|
| any field name | in `by_name` | `UnknownField` |
| any field | `indexed` | `InvalidQuery` |
| `Match`, `Phrase` | kind is `Text` | `InvalidQuery` |
| `Term`, `Fuzzy` | kind is `Text` or `Keyword` | `InvalidQuery` |
| `Fuzzy` | distance ≤ 2 | `InvalidQuery` |

### Filter leaf evaluation

`resolve(&Filter, &FieldMap, &Searcher) -> Result<DocSet>` (D10). Value-type table:

| `FieldKind` | accepted `Value` in `Eq`/`In`/`Range` |
|---|---|
| `Keyword` | `Keyword(_)` |
| `U64` | `U64(_)` |
| `I64` | `I64(_)` |
| `F64` | `F64(_)` |
| `Bool` | `Bool(_)` (`Range` on bool is accepted; it is a well-defined ordered type) |
| `DateMillis` | `DateMillis(_)` |
| `Text(_)` | none — `Exists` only |

Anything else ⇒ `InvalidQuery` naming the field and both types.

---

## On-disk entities

### Index directory

```
<dir>/
├── xtriever-lexical.json     # Descriptor — read FIRST on open (D13)
├── meta.json                 # backend
├── .managed.json             # backend
├── *.idx *.pos *.term *.store *.fast *.fieldnorm  # backend segment files
└── .tantivy-writer.lock      # backend; present only while a writer is held
```

### `Descriptor` (`xtriever-lexical.json`)

| field | type | rules |
|---|---|---|
| `format_version` | `u32` | `1`. Any other value ⇒ `Error::Corrupt` at open. Bumped only with an ADR (Principle V: on-disk format) |
| `schema` | `xtriever_core::Schema` (serde) | must equal the schema the backend index was built from, field by field, including `AnalyzerId`s — the lexical analogue of the embedder fingerprint |

Written atomically after the backend directory is created; never rewritten.

### Hidden backend fields

| name | type | options | present on |
|---|---|---|---|
| `__xt_id` | u64 | INDEXED \| FAST | every document |
| `__xt_len_<field>` | u64 | FAST | documents carrying that text field |

The `__xt_` prefix is reserved (FieldMap validation 1).

---

## Fixture entities (`reference/fixtures/002/`)

All generated by `reference/gen_002_fixtures.py` from a fixed seed; `manifest.json` records a
SHA-256 per file and `tests/fixtures_valid.rs` asserts every hash before any other test runs
(FR-033). `.gitattributes` already exempts `reference/fixtures/**` from EOL translation.

### `FixtureSchema` (`schema.json`)

The `xtriever_core::Schema` used by every 002 golden, serialized by the same serde derive the
descriptor uses. Fields:

| name | kind | indexed | stored | boost |
|---|---|---|---|---|
| `title` | `Text("standard")` | yes | yes | 2.0 |
| `body` | `Text("standard")` | yes | no | 1.0 |
| `summary` | `Text("standard_en")` | yes | no | 1.0 |
| `source` | `Keyword` | yes | yes | 1.0 |
| `tags` | `Keyword` | yes | no | 1.0 |
| `views` | `U64` | yes | no | 1.0 |
| `rank` | `I64` | yes | no | 1.0 |
| `quality` | `F64` | yes | no | 1.0 |
| `published` | `Bool` | yes | no | 1.0 |
| `ts` | `DateMillis` | yes | no | 1.0 |
| `note` | `Text("standard")` | **no** | yes | 1.0 |

`note` exists so "query on an unindexed field" (FR-018) and "stored round-trip" (FR-008a) have a
target. `summary` exists so two analyzers coexist in one schema (edge case). `title` carries the
boost so scenario 8 has a target.

### `FixtureCorpus` (`corpus.json`)

| field | rules |
|---|---|
| `seed` | integer; regeneration is byte-identical |
| `documents` | exactly 1,000 `Document`s in insertion order; `id` = 0..999 ascending; every document has `title`, `body`, `source`, `views`, `rank`, `quality`, `published`, `ts`; roughly half have `summary`, `tags`, `note` (so `Exists` is non-trivial) |
| planted structure | at least one term with exact ties at the k-boundary for the `Term` golden (FR-014 scenario); at least one phrase occurring 1×, 2× and with a slop-1 variant; at least one word within Levenshtein 1 and one within 2 of a query term; `title` and `body` share terms so field boost is observable |

Text is ASCII, as in 001, so analyzer behaviour is not entangled with Unicode on the first run.

### `QueryGolden` (`queries.json`)

One entry per named query; ≥ 1 per `LexicalQuery` variant (SC-002).

| field | rules |
|---|---|
| `name` | unique |
| `query` | the `LexicalQuery`, serde |
| `filter` | optional `Filter`, serde |
| `k` | integer |
| `expected` | ordered `[{id, score}]`, **exact** at test time; minted by the Rust example and cross-checked against Python within 1e-5 relative where D17 says Python covers the shape |
| `oracle` | `"python"` \| `"python-membership"` \| `"rust-only"` — which check minted it (D17) |

### `FilterGolden` (`filters.json`)

| field | rules |
|---|---|
| `name`, `filter` | as above |
| `expected_ids` | sorted list of `DocId`, exact, computed in Python over the corpus |

### `StatsGolden` (`stats.json`)

| field | rules |
|---|---|
| `num_docs` | 1000 |
| `avg_field_len` | per text field, float, computed from unquantized token counts in Python |
| `terms` | `[{field, term, doc_freq, total_term_freq}]` for ≥ 10 terms including one absent term (expected `null`) |

### `MutationGolden` (`mutations.json`)

Scripts for Story 3, Story 5 scenario 5, and FR-025:

| field | rules |
|---|---|
| `replace` | `{id, new_document}` and the expected post-commit result of a named query |
| `delete` | list of ids; expected `num_docs` and per-term `doc_freq` after commit (live-only) |
| `history_pair` | the superset-then-delete recipe for FR-015/SC-011; expected live-only stats (identical to the baseline's) |

### `Manifest` (`manifest.json`)

`{ "generator": "gen_002_fixtures.py", "seed": N, "files": { "<name>": "<sha256>" } }`.

---

## Report entities (`specs/002-lexical-stage/report.md`, written during implementation)

### `DivergenceRecord` (FR-015, FR-025)

| field | rules |
|---|---|
| `scenario` | `"history-pair"` |
| `query` | the golden query name |
| `live_stats` | `{N, df, sum_tokens}` per term/field from `term_stats`/`stats` |
| `scorer_stats` | the same three from the backend's `max_doc`, term-dictionary `doc_freq`, `total_num_tokens` (D8) |
| `score_delta_max` | max absolute score difference between the two histories, before merge |
| `after_merge_delta_max` | the same after `merge()` — expected `0.0` |
| `verdict` | `agree` \| `diverge` — never adjusted to agree (Rule 6) |
