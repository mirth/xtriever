# Contract: the public surface of `xtriever-lexical`

**Feature**: `002-lexical-stage` | **Date**: 2026-09-11 | **Plan**: [../plan.md](../plan.md)

This is the whole public API of the crate after Feature 002. Anything not listed is `pub(crate)`.
Backend types never appear in a public signature (Principle V: no `DocAddress`, no `tantivy::*`).

```rust
pub struct TantivyIndex { /* private */ }

impl TantivyIndex {
    /// Create a new index at `dir` for `schema`. `dir` must not exist or must be empty.
    pub fn create(dir: &Path, schema: Schema) -> xtriever_core::Result<Self>;

    /// Open an existing index. The schema is read from the descriptor (FR-012).
    pub fn open(dir: &Path) -> xtriever_core::Result<Self>;

    /// Merge all segments into one and make the result visible. Commits pending mutations first.
    pub fn merge(&mut self) -> xtriever_core::Result<()>;
}

impl xtriever_core::LexicalIndex for TantivyIndex { /* all eight methods */ }

/// Analyzer ids this crate recognises in `FieldKind::Text(..)` (FR-006).
pub const ANALYZERS: &[&str] = &["standard", "standard_en"];
```

That is three inherent methods, one trait impl, one constant. No reader type (FR-029), no analyzer
registry (FR-006), no builder.

## Trait method semantics

| method | behaviour | errors |
|---|---|---|
| `schema()` | the created-with schema, from memory | — |
| `add(docs)` | for each: validate against schema; `delete_term(__xt_id)` then `add_document` — replace-by-id within the batch and across commits (FR-008); `chunk` ignored (FR-008b); `stored` values written (FR-008a); length columns written for present text fields | `UnknownField`, `Schema`, `Backend` |
| `delete(ids)` | `delete_term(__xt_id)` per id; unknown ids are a no-op (FR-009) | `Backend` |
| `commit()` | backend commit, then reader reload; no-op if nothing was ever mutated (FR-010) | `Backend` |
| `search(q, filter, k)` | `k == 0` ⇒ `Ok(vec![])`. Translate `q` (D9); resolve `filter` (D10) if present; `TopDocs(k)` optionally wrapped in `FilterCollector` on `__xt_id` (D11); map to `DocId`; re-sort `(score DESC, id ASC)` (D12). Scores are BM25 × field boost × query boost (FR-005/FR-017) | `UnknownField`, `InvalidQuery`, `Backend` |
| `resolve_filter(f)` | leaves through backend queries, algebra through `roaring` (D10); live documents only (FR-020) | `UnknownField`, `InvalidQuery`, `Backend` |
| `term_stats(field, term)` | `None` if the term is absent from every segment (FR-026); otherwise live-only `doc_freq` and `total_term_freq` by walking postings and skipping deleted docs (FR-024, D8) | `UnknownField`, `InvalidQuery` (non-text/keyword field), `Backend` |
| `stats()` | `num_docs` = live count; `avg_field_len[f]` = Σ `__xt_len_f` over live docs ÷ live docs that carry `f` (FR-027, D7); a text field carried by no live doc is absent from the map | `Backend` |

## Contract caveat recorded by this feature (FR-014)

`LexicalIndex::search`'s doc comment in `xtriever-core` currently promises the `DocId` tie-break
without qualification. After this feature it reads, in addition:

> The tie-break governs the **order** of the returned hits, not their **membership**: when a score
> tie spans the `k`-th and `(k+1)`-th positions, which of the tied documents are returned is
> backend-defined and stable. See ADR-0005 (amended 2026-09-11).

This is the only `xtriever-core` change in the feature, it is documentation-only, and it lands in
the same PR as the ADR-0005 amendment that authorises it.

## Analyzer table (FR-006)

| id | chain | oracle |
|---|---|---|
| `standard` | simple tokenizer → drop tokens > 40 bytes → lowercase | Feature 001's Python transcription |
| `standard_en` | `standard` → English Snowball stemmer | 001's transcription + `snowballstemmer` |

Unknown id ⇒ `Error::Schema("field `f`: unknown analyzer `id`; known: standard, standard_en")` at
`create`.

## Error mapping (FR-003)

See research D16. The variants used are exactly `UnknownField`, `Schema`, `InvalidQuery`, `Corrupt`,
`Io`, `Backend`. No variant is added.

## Thread and memory facts a caller should know (FR-004)

- `create`/`open` spawn no threads and allocate no arena.
- The first `add` or `delete` spawns one indexing worker, one segment-updater thread, one merge
  worker and (backend default) one doc-store compression thread, and allocates a 15 MB arena. They
  live until the index is dropped.
- Query execution is single-threaded (backend default executor).
- Wrapping the index in a read-write lock is the supported way to share it; a write lock held
  across `add` + `commit` blocks all queries for that duration (FR-029).
- Two handles on one directory: both open; the second to mutate gets `Error::Backend` (D14).
