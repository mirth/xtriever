# Contract: provisional Swift-facing FFI surface

**Feature**: `001-ios-build-spike` | **Date**: 2026-09-10 | **Crate**: `xtriever-ffi`

> ## This is not an API design
>
> FR-031 declares this surface **provisional**. It exists to prove that the three operations' data
> can cross the language boundary, and it must not be depended on by later specs, extended for
> convenience, or defended in review as an API decision. The durable deliverables of this feature are
> the findings report and the Swift harness — not these signatures.
>
> [ADR-0001](../../../docs/adr/0001-pin-candle-0-9-2.md) and
> [ADR-0002](../../../docs/adr/0002-unsafe-mmap-safetensors-measurement.md) were accepted
> 2026-09-10. ADR-0002 is what justifies the `LoadPath` parameter existing at all, and its six
> conditions are binding on the implementation — especially condition 4: if the two load paths
> disagree, the mmap path is a finding and gets deleted.

## Mechanism

uniffi 0.32.1, proc-macro scaffolding — no UDL file and no `build.rs`
(<https://mozilla.github.io/uniffi-rs/latest/tutorial/Rust_scaffolding.html>).

```toml
# crates/xtriever-ffi/Cargo.toml
[lib]
crate-type = ["lib", "cdylib", "staticlib"]   # `lib` is REQUIRED so tests/ can link;
                                              # staticlib is what iOS consumes

[features]
default = []
spike = ["dep:tantivy", "dep:tokenizers", "dep:candle-core", "dep:candle-nn",
         "dep:candle-transformers", "dep:uniffi"]   # dep:uniffi REQUIRED or the feature cannot resolve
cli = ["dep:uniffi", "uniffi/cli"]     # for the bindgen binary only

[[bin]]
name = "uniffi-bindgen"
path = "src/bin/uniffi-bindgen.rs"     # fn main() { uniffi::uniffi_bindgen_swift() }
required-features = ["cli"]
```

`uniffi::setup_scaffolding!()` goes at the top of `lib.rs`.

**Module split (ADR-0003).** uniffi's macros emit `unsafe` that the workspace's
`unsafe_code = "deny"` rejects, so the crate is split: `src/ffi/` (wire types + thin delegating
shims) carries `#![allow(unsafe_code)]` for generated code, and `src/spike/` (all hand-written
logic) re-declares `#![deny(unsafe_code)]`. `SpikeError` and the records therefore live in
`src/ffi/`, not `src/spike/`.

**Feature-gating pitfall (research risk R3) — eliminated by construction.** `#[cfg()]` does **not**
work *inside* an `#[uniffi::export]` block; scaffolding is generated regardless, producing compile
errors. Rather than relying on everyone remembering to put the gate before the attribute, the whole
`ffi` module is gated at its declaration in `lib.rs`:

```rust
#[cfg(feature = "spike")]
pub mod ffi;          // so no #[cfg] ever sits next to a #[uniffi::export]
```

## Types

All types below are verified as crossing to Swift. `Vec<f32>` in particular is exercised by UniFFI's
own CI (`roundtrip_vec_f32` in `bindgen-tests/lib/src/collections.rs`, asserted in
`bindgen-tests/swift/tests/collections.swift`).

```rust
#[derive(uniffi::Record)]
pub struct SpikeDocument {
    pub external_id: String,
    pub text: String,
}

#[derive(uniffi::Record)]
pub struct RankedHit {
    pub external_id: String,   // resolved from a tantivy stored field
    pub score: f32,            // BM25; k1=1.2, b=0.75 are non-configurable in tantivy 0.26.2
    pub segment_ord: u32,      // diagnostic: tantivy ties break on (segment_ord, doc_id)
    pub doc_id: u32,
}

#[derive(uniffi::Record)]
pub struct IndexOutcome {
    pub documents_indexed: u32,
    pub segment_count: u32,    // expected 1; >1 means the tie-break order is not comparable
}

#[derive(uniffi::Enum)]
pub enum LoadPath {
    Buffered,   // safe from_buffered_safetensors — primary
    Mmapped,    // unsafe from_mmaped_safetensors — ADR-0002
}
```

`segment_count` is on the record deliberately: if it is not 1, `ExpectedRanking`'s exact-equality
comparison is not meaningful (research D5) and the harness should say so rather than report a
mismatch.

## Errors

```rust
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SpikeError {
    #[error("index i/o failed at {path}: {message}")]
    IndexIo { path: String, message: String },
    #[error("query could not be parsed: {message}")]
    QueryParse { message: String },
    #[error("model artifact invalid: {message}")]
    Model { message: String },
    #[error("tokenization failed: {message}")]
    Tokenize { message: String },
    #[error("inference failed: {message}")]
    Inference { message: String },

    /// SCAFFOLD — removed when the operation lands (PR 2).
    #[error("{operation} is not implemented in this build")]
    NotImplemented { operation: String },
}
```

CITED requirements (<https://mozilla.github.io/uniffi-rs/latest/types/errors.html>): the error type
must be an `enum` implementing `std::error::Error` — `thiserror` satisfies both, and Principle VII
already mandates `thiserror` for library errors. Variants carry fields (rather than
`#[uniffi(flat_error)]`) so Swift can report *which* artifact or path failed, which is what makes a
`Finding` reproducible.

**Why every operation returns `Result` (FR-008).** A Rust panic inside a *non-throwing* Swift
function becomes an uncatchable fatal error
(<https://mozilla.github.io/uniffi-rs/latest/swift/overview.html>). Declaring `Result<T, E>` is what
makes the Swift function `throws` and keeps failures catchable. This is also why Principle VII's ban
on `unwrap`/`expect`/`panic!` in library code is load-bearing here rather than stylistic: a panic on
this path takes the harness down and loses the measurement.

## Operations

Exactly three (FR-006). No others may be added.

### `spike_index`

```rust
pub fn spike_index(index_dir: String, documents: Vec<SpikeDocument>)
    -> Result<IndexOutcome, SpikeError>
```

Swift: `func spikeIndex(indexDir: String, documents: [SpikeDocument]) throws -> IndexOutcome`

- Creates a tantivy index at `index_dir` (an app-sandbox path) via `Index::create_in_dir`, which the
  `mmap` feature gates.
- Schema: the text field indexed with `IndexRecordOption::WithFreqsAndPositions`; the id field
  `STRING | STORED`.
- **MUST use `Index::writer_with_num_threads(1, budget)`** — not `Index::writer`. This is a
  correctness requirement, not a tuning choice: the multi-threaded writer does not allocate `DocId`s
  reproducibly, which would break FR-014's oracle (research D5). `budget` must be at least
  tantivy's 15 MB per-thread minimum.
- Adds documents in list order, then commits.
- Errors: `IndexIo`.

### `spike_query`

```rust
pub fn spike_query(index_dir: String, query: String, k: u32)
    -> Result<Vec<RankedHit>, SpikeError>
```

Swift: `func spikeQuery(indexDir: String, query: String, k: UInt32) throws -> [RankedHit]`

- Opens the index, builds a `QueryParser` over the text field, and collects
  `TopDocs::with_limit(k).order_by_score()` → `Vec<(Score, DocAddress)>`.
- Resolves each hit's stored `external_id` via `searcher.doc::<TantivyDocument>(addr)`.
- An **empty result is an error, not a pass** — the fixture query is guaranteed to match at least one
  document, so emptiness means analysis dropped the query terms (spec edge case). Return
  `QueryParse`.
- Errors: `IndexIo`, `QueryParse`.

### `spike_embed`

```rust
pub fn spike_embed(model_dir: String, sentence: String, load_path: LoadPath)
    -> Result<Vec<f32>, SpikeError>
```

Swift: `func spikeEmbed(modelDir: String, sentence: String, loadPath: LoadPath) throws -> [Float]`

- Reads `config.json` (into candle's `Config`), `tokenizer.json`, and `model.safetensors` from
  `model_dir` (the app bundle).
- **Asserts `model.safetensors` is exactly 90,868,376 bytes and its SHA-256 matches** before loading
  (FR-016). A mismatch is `Model`, never a warning.
- **Overrides the tokenizer's baked-in truncation and padding to 256 tokens** — `tokenizer.json`
  ships 128, and the reference behaviour is 256, so skipping this makes Rust and Python disagree with
  no iOS involvement (research D6).
- Builds a `VarBuilder` per `load_path` — `from_buffered_safetensors(data, DTYPE, &Device::Cpu)` for
  `Buffered`, or `unsafe { VarBuilder::from_mmaped_safetensors(&[path], DTYPE, &Device::Cpu) }` for
  `Mmapped` (the single ADR-0002 `unsafe` block; note the first argument is a **slice** of paths, not
  one path). `DTYPE` is `candle_transformers::models::bert::DTYPE`, a public constant equal to
  `DType::F32`. Root prefix is `""`: the safetensors keys have no `bert.` prefix, and
  `BertModel::load` applies `vb.pp("embeddings")` / `vb.pp("encoder")` itself, falling back to
  `{model_type}.*` if those are absent.
- **The caller must export `RAYON_NUM_THREADS=1`.** candle's CPU path uses `rayon` unconditionally
  and also spawns raw `std::thread`s; leaving the pool sized to the device's core count adds an
  uncontrolled term to both the footprint measurement and the float summation order (research D15).
  Note the variable name: candle **0.9.2** reads `RAYON_NUM_THREADS`, *not* `CANDLE_NUM_THREADS`
  (which later versions read, and which research D15 originally recorded in error). The crate does
  not set it — a library has no business mutating process-global environment, and in edition 2024
  `std::env::set_var` is `unsafe`. `spike::embed::thread_count()` reports what was in effect.
- `BertModel::forward(&input_ids, &token_type_ids, Some(&attention_mask))` — `token_type_ids` is a
  required positional argument in candle 0.9.2, so a zeros tensor is passed for a single-segment
  sentence.
- Mean-pools the last hidden state weighted by the attention mask, then L2-normalises, yielding 384
  floats.
- Errors: `Model`, `Tokenize`, `Inference`.

**Returning `Vec<f32>` is not free.** `uniffi_core`'s `Lower for Vec<T>` serializes vectors
element-by-element into a `RustBuffer` rather than passing a pointer
(`uniffi_core/src/ffi_converter_impls.rs`: "Vectors are currently always passed by serializing to a
buffer"). For 384 floats that is negligible; for the 1,000-document corpus going *in* it is not, and
it is inside the measured `index` wall time. The report must attribute it as FFI cost, not engine
cost (FR-017's per-operation split is what makes that possible).

## What is deliberately absent

Each omission is a scope boundary, not an oversight.

| Absent | Why |
|---|---|
| Any `xtriever-core` trait implementation | FR-009 and FR-029 — implementing `LexicalIndex`/`Embedder` is the lexical and dense specs' work, not a build spike's |
| Fusion, re-ranking, LTR, pipeline orchestration | FR-029 |
| Async variants | Principle V keeps core APIs synchronous; nothing here needs async |
| Incremental indexing, deletion, index reopening | Not needed to answer the spike's question |
| A tokenize-only entry point | Token parity is asserted host-side against `ExpectedTokens`; adding a fourth operation would breach FR-006 |
| Batch embedding | One sentence is what the spec asks for |
| Configurable BM25 `k1`/`b` | Not possible — private constants in tantivy 0.26.2. Recorded so nobody looks for the knob |
