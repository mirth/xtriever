# Data Model: Optional Sparse Lexical Expansion (Feature 027)

## SparseOption (creation-time, per index)

Part of `HybridConfig` as `sparse: Option<SparseOption>`; `None` is the default and changes
nothing (FR-002). The encoder is not part of it: `create_sparse` takes the loaded encoder
(verified at load, FR-003), and an opened index needs none (contract `sparse-option.md`).

| field | type | default | rule |
|---|---|---|---|
| `scale` | u32 | 10 | `1..=1000` (`sparse::MAX_SCALE`); a weight becomes `round(weight × scale)` occurrences (FR-004) |
| `boost` | f32 | 1.0 | finite, > 0; the `_sparse` field's boost (FR-005) |

## SparseRecord (in the descriptor, format version 3)

What the index remembers; written once at creation, never changed.

| field | type | meaning |
|---|---|---|
| `scale` | u32 | as created |
| `boost` | f32 | as created |
| `field` | string | `"_sparse"` — the reserved lexical field |
| `encoder` | string | `repository@revision;weights=sha256:…;activation=log1p_log1p_relu;max_tokens=512;engine=candle-0.9.2` |
| `tokenizer_sha256` | string | of `<index>/sparse/tokenizer.json` |
| `table_sha256` | string | of `<index>/sparse/query-table.json` |

Validation at open: the descriptor's version is 3 exactly when `sparse` is present; both files
exist and hash to the recorded values; the schema holds `_sparse` as a text field under `standard`
with the recorded boost. Any failure: `Error::Corrupt` naming what differs (FR-006, FR-009).

## Index layout (additions only)

```text
<index>/
├── xtriever-pipeline.json      # format_version 3 and a "sparse" record when the option is on
├── lexical/                    # the schema gains the text field `_sparse`
└── sparse/                     # only when the option is on
    ├── tokenizer.json          # the encoder's, byte for byte (pinned in manifest-sparse-doc-v3.json)
    └── query-table.json        # the encoder's idf.json, byte for byte
```

## DocumentExpansion (per document, transient)

`Vec<(token id: u32, weight: f32)>`, strictly ascending by token id, every id below 30,522
(`sparse::VOCABULARY_SIZE`) and not a special token (`sparse::SPECIAL_IDS`: 0, 100–103), every
weight finite, > 0 and ≤ 4.5 (`sparse::MAX_WEIGHT`: `ln(1 + ln(1 + f32::MAX))` ≈ 4.4967 is the most the encoder can
produce), special tokens absent. `Expansion::validate` checks this; an expansion that fails it —
from a cache or a caller — is refused (`Error::Schema`), never repaired. Becomes the `_sparse` field value: for each entry, the term `s<id>` repeated
`round(weight × scale)` times (half away from zero), entries of zero occurrences dropped, terms
in ascending id order separated by single spaces. An expansion with no occurrences is the empty
string; the document is still indexed and found by its text.

## QueryTerms (per query, transient)

The query's token ids from the stored tokenizer (no truncation, no added special tokens), kept
if their table entry is positive and they are not special tokens, de-duplicated, ascending.

## PinnedSparseEncoder (compiled in, `xtriever-dense`)

Repository, revision and every file's name, size and SHA-256 — mirrors
`reference/models/manifest-sparse-doc-v3.json` and is pinned against it by a test, as the
embedder's and re-ranker's pins are. Files: `config.json`, `tokenizer.json`, `model.safetensors`,
`idf.json`.

## State and lifecycle

```text
create_sparse(dir, config{sparse: Some(opt)}, embedder, encoder)
   → copy tokenizer.json + idf.json into <dir>/sparse/ (checked against the pins)
   → schema + `_sparse` → descriptor v3 with SparseRecord
open(dir, embedder)                    → verify <dir>/sparse/* → SparseQuery ready (no encoder)
set_sparse_encoder(Some(encoder))      → add() may encode; without it add() refuses
add(docs)            → dense embed + sparse encode each passage → `_sparse` text → stage
add_embedded(docs)   → caller's vectors; expanded by the attached encoder, as add (PR C review)
add_encoded(docs)    → caller-supplied vectors and expansions (the evaluation cache)
search(text)         → Bool{Match per user text field, Term(_sparse, s<id>) per query term}
search_lexical(q)    → unchanged: the caller's query, no expansion added
```
