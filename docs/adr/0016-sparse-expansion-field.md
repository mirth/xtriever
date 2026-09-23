# ADR-0016: Sparse lexical expansion — a reserved field, a stored query side, descriptor format version 3

- **Status**: Proposed — awaiting the repository owner's review at PR B's checkpoint
- **Date**: 2026-09-23
- **Deciders**: mirth (repository owner)
- **Spec**: [027-sparse-lexical-expansion](../../specs/027-sparse-lexical-expansion/spec.md) FR-002–FR-009
- **Blocks**: Principle V row in [plan.md](../../specs/027-sparse-lexical-expansion/plan.md)

## Context

Principle V: "the on-disk format … unchanged — or the change has human review plus an ADR".
Feature 027 lets an installation opt an index into **sparse lexical expansion**. Each document
is expanded by a pinned learned-sparse encoder
(`opensearch-neural-sparse-encoding-doc-v3-distill`, 268 MB) into weighted vocabulary entries,
and those entries are scored by BM25 beside the document's own text. The spike measured
+0.0174 nDCG@10 on FiQA and a three-dataset mean of +0.0041 at the re-ranked stage. That is
below Feature 016's +0.005 floor for a default stage, so the owner made it **opt-in per index**
(spec Clarification Q1).

An expansion has to live somewhere in the index, and a query has to find it. Three things
change on disk, and each must be something an index built without the option never sees:

1. where the expansion is stored;
2. what a device needs in order to search it, given that it never has the encoder;
3. how an engine that cannot search it refuses it.

Features 015 and 024 added descriptor fields **without** a version bump, because an older engine
could safely ignore them: a missing `rerank_mode` reads as the default, and a missing
compaction share reads as "compact on merge". A sparse index is different. An older engine
would open it, run `Match(None, text)` over every indexed text field, `_sparse` included, send
the query's analysed words at `s<id>` terms that none of them can match, and never send the
expansion terms at all. Nothing would fail, and the results would be quietly wrong.

## Decision

**1. A reserved lexical field, `_sparse`.** A sparse index's lexical schema is the user's
schema plus one field:

- text under the unstemmed `standard` analyzer, indexed, not stored;
- its boost is the option's `boost` (default 1.0).

Each kept entry `(token id, weight)` of a document's expansion becomes the term `s<id>`,
repeated `round(weight × scale)` times (in f64, halves away from zero, default scale 10).
Entries that round to zero are dropped, and terms are in ascending id order
(`xtriever_dense::sparse::field_text`). `s<id>` survives `standard` as one token, where a
wordpiece such as `##ing` would not. BM25 reads the repetition as term frequency, which is how
Feature 012's `bm25x` and the spike expressed weights. `create_sparse` refuses a user field
named `_sparse`. On an index without the option the name is unreserved, so nothing changes
there.

**2. The query side is stored in the index.** At creation the pipeline copies the encoder's
`tokenizer.json` and its `idf.json`, each byte for byte and checked against its pin. They go to
`<index>/sparse/tokenizer.json` and `<index>/sparse/query-table.json`, and the descriptor records
both SHA-256s. `open` verifies both files and refuses a difference as `Error::Corrupt`, naming
the file and both hashes. The query rule needs no model: take the query's distinct token ids
that are not special tokens and have a positive table entry, and search for
`Term(_sparse, "s<id>")` for each, beside the user's text fields spelled out as `Match(Some(f))`.
A device therefore searches a sparse index with nothing but the index, which costs 1.6 MB.
The packagers already stage the index directory, and nothing ever stages the encoder.

**3. Descriptor format version 3, for sparse indexes only.** `xtriever-pipeline.json` of a
sparse index is `format_version: 3` and carries a record:

```json
"sparse": {
  "scale": 10, "boost": 1.0, "field": "_sparse",
  "encoder": "opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill@babf71f3…;weights=sha256:…;activation=log1p_log1p_relu;max_tokens=512;engine=candle-0.9.2",
  "tokenizer_sha256": "…", "table_sha256": "…"
}
```

**Every other index stays version 2, byte for byte**: the key is omitted, not written as `null`.
A reader accepts 2 without the record and 3 with it, and refuses 3 without it or 2 with it,
naming the version. The id map (`ids.json`) and the passage store keep their own versions, and
the dense and lexical stages are unchanged. An engine older than this feature reads version 3
and gives its existing refusal ("this build reads 2; rebuild the index"). That is the point of
the bump: a refusal by name instead of silently wrong results.

**The encoder is not part of the configuration.** `HybridConfig.sparse` is
`Option<SparseOption { scale, boost }>`, which is exactly what the descriptor records, so an
opened index reports it. `HybridIndex::create_sparse(dir, config, embedder, encoder)` takes the
loaded encoder and attaches it, and `set_sparse_encoder` attaches it to an opened index for
further additions. Adding to a sparse index without an attached encoder is `Error::Model`.
`add_embedded` has no expansion to write and is refused. `add_encoded` takes caller-supplied
expansions and validates them.

## Consequences

- **Nothing changes without the option.** Descriptor bytes, schema, query (`Match(None, text)`)
  and every existing test are the same (FR-002).
- **Size.** At scale 10 the lexical stage grows by the expansion's postings. The spike measured
  2.2–3.6×, and the feature records each dataset's size (SC-005: at most 4×).
- **Search cost** is one extra `TermQuery` per kept query token. The query side is a
  tokenizer pass and a table lookup, with no model call.
- **Build cost** is the encoder: about five times the embedder per document on CPU. It is
  recorded, not budgeted (Principle IV; research D12).
- **An index's expansion is fixed at creation.** The scale, boost and encoder are recorded, and
  changing any of them means rebuilding. A different encoder has a different identity.
- **`search_lexical` is unchanged.** It sends the caller's query as built, so a caller who wants
  the expansion adds the `_sparse` terms. `explain`'s BM25 score is the whole lexical score,
  expansion included.

## Alternatives considered

- **A trait for sparse encoders in `xtriever-core`.** This would be a contract change the spec
  does not authorise (Rule 2). The pipeline can hold the concrete types, because it already
  depends on `xtriever-dense`.
- **A separate sparse posting list with its own scorer** (Feature 016's design: dot product of
  document weights and query weights, fused as a third list). This means a new stage, a new
  on-disk structure and a new fusion input. The spike found BM25 over the repeated terms beside
  the text field better on FiQA and simpler, and it reuses tantivy (Principle I).
- **Store the weights as a float field and score them with a custom query.** tantivy scores
  term frequency, and a custom scorer means a hand-built retrieval component (Principle I).
- **Keep the descriptor at version 2 with an optional record** (as Features 015 and 024 did).
  An older engine would then search a sparse index silently wrongly (see Context).
- **Use the dense stage's tokenizer as the query side, with a recorded identity** (the spec's
  first wording of FR-006). The two tokenizers share a vocabulary today (checked 2026-09-23),
  but keeping them in step would be a second artefact and a second refusal path. The copy costs
  0.7 MB per sparse index.
- **Name the field in the user namespace, e.g. `sparse`.** It could collide with a user field.
  The lexical crate reserves only `__xt_`, and a leading underscore marks `_sparse` as the
  engine's without using that prefix.
