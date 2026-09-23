# Contract: the sparse option in the engine (PR A, PR B)

## `xtriever-dense::sparse` (PR A)

```rust
/// The pinned document encoder — build host only.
pub struct SparseEncoder { /* private */ }
impl SparseEncoder {
    /// Verify every pinned file (size, SHA-256) then load. `Error::Model` names the file and both
    /// values on a mismatch; nothing is parsed before verification.
    pub fn load(dir: &Path, load_path: LoadPath) -> Result<Self>;
    /// One document, alone, truncated to 512 tokens: its kept `(token id, weight)` entries,
    /// ascending by id, weights > 0, special tokens excluded (research D3).
    pub fn encode(&self, text: &str) -> Result<Expansion>;
    /// The token ids the encoder sees, for the tokenization-parity test.
    pub fn token_ids(&self, text: &str) -> Result<Vec<u32>>;
    /// The identity recorded in a sparse index (data-model `SparseRecord.encoder`).
    pub fn identity(&self) -> &str;
}

/// One document's expansion; `truncated` says the document ran past the encoder's window, so
/// a build can count them (spec User Story 1, scenario 1).
pub struct Expansion { pub entries: Vec<(u32, f32)>, pub truncated: bool }
impl Expansion {
    /// Ids in the vocabulary, not special, strictly ascending; weights finite, > 0 and
    /// ≤ MAX_WEIGHT. `Error::Schema` otherwise.
    pub fn validate(&self) -> Result<()>;
}
pub const SPECIAL_IDS: [u32; 5] = [0, 100, 101, 102, 103];   // the pinned tokenizer's; load refuses another
pub const VOCABULARY_SIZE: u32 = 30_522;
pub const MAX_SCALE: u32 = 1_000;
pub const MAX_WEIGHT: f32 = 4.5;   // ≥ ln(1 + ln(1 + f32::MAX)), the encoder's largest weight

/// The query side — every installation that searches a sparse index.
pub struct SparseQuery { /* private */ }
impl SparseQuery {
    /// From a tokenizer and a query-side table, each verified against an expected SHA-256.
    pub fn open(tokenizer: &Path, table: &Path, tokenizer_sha256: &str, table_sha256: &str)
        -> Result<Self>;
    /// Distinct kept token ids, ascending (data-model `QueryTerms`).
    pub fn terms(&self, text: &str) -> Result<Vec<u32>>;
}

/// `s<id>` repeated `round(weight × scale)` times — the one rule both the pipeline and the
/// evaluation cache use (research D4). `Error::Schema` for a scale outside `1..=MAX_SCALE` or
/// an expansion that fails `validate`, so no input makes the text unboundedly long.
pub fn field_text(expansion: &Expansion, scale: u32) -> Result<String>;
```

Errors: `Error::Model { model: "opensearch-neural-sparse-encoding-doc-v3-distill", .. }` for the
encoder — including a forward pass that produces a non-finite logit; `Error::Corrupt` for a
stored tokenizer or table whose hash differs; `Error::Schema` for an expansion or scale
`field_text` refuses. `load` reads each file once and parses the bytes it verified.

## `xtriever-pipeline` (PR B)

```rust
pub struct SparseOption { pub scale: u32, pub boost: f32 }        // Default: 10, 1.0
impl HybridConfig { pub sparse: Option<SparseOption> /* new field, default None */ }
pub const SPARSE_FIELD: &str = "_sparse";
pub const SPARSE_FORMAT_VERSION: u32 = 3;                        // FORMAT_VERSION stays 2

impl HybridIndex {
    /// Create a sparse index: `config.sparse` set, the loaded encoder given and attached.
    pub fn create_sparse(dir: &Path, config: HybridConfig, embedder: Box<dyn Embedder>,
                         encoder: SparseEncoder) -> Result<Self>;
    /// Attach the encoder so `add` can expand documents; `None` detaches it.
    pub fn set_sparse_encoder(&mut self, encoder: Option<SparseEncoder>);
    /// The recorded option, if this index has one.
    pub fn sparse(&self) -> Option<&SparseRecord>;
    /// The descriptor's format version: 3 for a sparse index, 2 otherwise.
    pub fn format_version(&self) -> u32;
    /// Documents this handle's `add` truncated to the encoder's window.
    pub fn sparse_truncated(&self) -> u64;
    /// Caller-supplied vectors and expansions, for a sparse index built from caches.
    pub fn add_encoded(&mut self, docs: &[(SourceDocument, Vec<f32>, Expansion)]) -> Result<()>;
}

impl SparseEncoder {   // xtriever-dense, added in PR B
    /// Copy the pinned tokenizer.json and idf.json (as query-table.json) into `dest`, each
    /// checked against its pin; returns their SHA-256s.
    pub fn write_query_side(&self, dest: &Path) -> Result<(String, String)>;
}
```

**Why the encoder is not in `HybridConfig`** (a change from the plan's first draft, which had
`SparseOption { encoder_dir, .. }`): `open` rebuilds the configuration from the descriptor, and
an encoder directory is neither part of the index nor present on a device. The option holds
what the index records; the encoder is an object the build host passes to `create_sparse` or
`set_sparse_encoder`. The FFI keeps `encoder_dir` in its own `IndexConfig` (PR C), loads the
encoder from it and calls `create_sparse`.

Behaviour on a sparse index:

| call | behaviour |
|---|---|
| `create` | validates the option as `create_sparse` does, then refuses it, naming `create_sparse` (it has no encoder) |
| `create_sparse` | refuses a missing option, a user schema field named `_sparse`, a scale outside `1..=1000`, a non-finite or non-positive boost; copies the query side (`write_query_side`); writes descriptor version 3; attaches the encoder |
| `open*` | verifies `<dir>/sparse/*` against the recorded hashes; version 3 without a record, or 2 with one, is `Corrupt` |
| `add` | refuses with `Error::Model` naming the missing encoder unless one is attached; refuses a document that supplies `_sparse` itself |
| `add_embedded` | refuses: no expansion to write (use `add` or `add_encoded`) |
| `add_encoded` | refuses an expansion `field_text` refuses; on an index without the option, refuses altogether |
| `search` | the query of research D7 |
| `search_lexical` | unchanged |

On an index without the option, every call behaves exactly as before (FR-002), and the
descriptor stays version 2.

## Descriptor (ADR-0016)

`format_version: 3` and

```json
"sparse": {
  "scale": 10,
  "boost": 1.0,
  "field": "_sparse",
  "encoder": "opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill@babf71f3…;weights=sha256:…;activation=log1p_log1p_relu;max_tokens=512;engine=candle-0.9.2",
  "tokenizer_sha256": "…",
  "table_sha256": "…"
}
```

An engine before Feature 027 reads version 3 as "this build reads 2; rebuild the index" — the
existing refusal, by name.
