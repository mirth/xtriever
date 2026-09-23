//! Xtriever `dense` stage: a candle-backed [`Embedder`](xtriever_core::Embedder) for the pinned
//! `all-MiniLM-L6-v2` model and a flat, exact [`VectorIndex`](xtriever_core::VectorIndex).
//!
//! Implemented by Feature 004 (`specs/004-dense-stage/`). The public surface is the one in
//! `specs/004-dense-stage/contracts/dense-stage.md`.
//!
//! # Feature 004
//!
//! - [`MiniLmEmbedder`] loads `sentence-transformers/all-MiniLM-L6-v2` at the revision and file
//!   hashes in [`model::PINNED`] (every file is size- and SHA-256-verified before it is parsed),
//!   asserts the model's shape from its files, and embeds **one text at a time at a fixed 256
//!   tokens** with attention-mask-weighted mean pooling and L2 normalisation. 384 dimensions,
//!   [`Metric::Cosine`](xtriever_core::Metric::Cosine).
//! - [`FlatIndex`] stores `(DocId, norm, scale, codes)` rows — each vector as one signed byte
//!   per dimension and one scale, 396 bytes at dimension 384 ([`FORMAT_VERSION`] 3, Feature
//!   026, ADR-0015) — in an append-only `vectors.<g>.bin` described by an atomically replaced
//!   `manifest.bin` (Feature 024, ADR-0013), and searches them exactly for what is stored: the
//!   query is quantised with the same scheme, dot products accumulate in `i32` and are scaled
//!   and rounded once, cosine divides by the norms of the two quantised vectors, and Euclidean
//!   recovers the row to floats. The floats that were added are not kept: `vector(id)` returns
//!   the row as it recovers, within half a quantisation step per component. Results are
//!   ordered `(score DESC, DocId ASC)`, including at the `k`-th rank. `commit`
//!   appends the new rows and marks deleted or replaced rows dead in the manifest's tombstone
//!   set — it never modifies a committed byte; `compact` (run by the pipeline's `merge`, and
//!   performed by `commit` itself when the index's `dense_compact_dead_share` would be
//!   exceeded) rewrites the live rows under a new generation and switches the manifest by
//!   rename. A handle (or a mapping) of the previous state is never disturbed
//!   by either.
//! - **Fingerprint** ([`model::FINGERPRINT`]): `repo@revision;weights=sha256:…;dim=384;
//!   pool=mean-mask;norm=l2;max_tokens=256;dtype=f32;prefix=none;engine=candle-0.9.2` — every
//!   input whose change would change the vectors, including the inference engine version. An
//!   index carries it and `FlatIndex::open_for` refuses a different embedder.
//!
//! # Feature 026
//!
//! - **Two artefacts, one model.** [`MiniLmEmbedder::load`] takes a directory holding either
//!   the float `model.safetensors` ([`model::PINNED`]) or the owner-pinned eight-bit GGUF
//!   ([`model::PINNED_Q8`], `all-MiniLM-L6-v2.Q8_0.gguf`, 25 MB against 91) beside the float
//!   model's `config.json` and `tokenizer.json`; a directory holding both, or neither, is
//!   refused naming both. The GGUF's header is asserted against the pin (architecture, shape,
//!   tensor count, layer-norm epsilon) after its bytes are verified, and its weight matrices are
//!   expanded to `f32` once at load and multiplied by the float kernel (ADR-0015) — the mode is fixed in
//!   code, never read from the environment. [`MiniLmEmbedder::precision`] says which artefact
//!   loaded.
//! - **The eight-bit fingerprint** ([`model::FINGERPRINT_Q8`]) names the artefact's repository,
//!   revision and hash and ends `dtype=q8_0;compute=f32;…` — so an index built with it is
//!   refused by a float embedder and vice versa, by exactly what differs.
//! - **Format 3 rows.** Every vector is stored as eight-bit codes with a per-vector scale
//!   whatever embedder produced it; the artefact's precision and the row format are independent
//!   choices that this feature made together, and the three-dataset gate in
//!   `specs/026-eight-bit-precision/runs/` measured both at once.
//! - **Determinism promise**: same fingerprint, same CPU architecture ⇒ bit-identical vectors,
//!   regardless of batch composition, order, size or thread count (`RAYON_NUM_THREADS`, read by
//!   candle, never set here). Across architectures the SIMD reduction paths differ, so agreement
//!   is within the golden tolerance, not bit-for-bit; the fingerprint deliberately excludes the
//!   architecture so an index built on one machine opens on another.
//!
//! # Feature 027
//!
//! - **The sparse document encoder** ([`sparse::SparseEncoder`], build host only) expands a
//!   document into weighted vocabulary entries with the pinned
//!   `opensearch-neural-sparse-encoding-doc-v3-distill` ([`model::PINNED_SPARSE`], 268 MB,
//!   verified by size and SHA-256 before anything is parsed). One document at a time, truncated
//!   to 512 tokens: the masked-LM logits' maximum over positions through
//!   `log1p(log1p(relu(·)))`, special tokens zeroed. The DistilBERT forward pass is written
//!   out over `candle_nn`'s layers with **exact GELU** — candle 0.9.2's own DistilBERT uses the
//!   tanh approximation, which the model's reference does not — and holds every weight within
//!   1e-4 of the PyTorch reference (`reference/gen_027_fixtures.py`; measured 1.6e-5), with
//!   bit-identical expansions across thread counts on one architecture.
//! - **The query side** ([`sparse::SparseQuery`]) needs no model: a query's distinct token ids
//!   that are not special tokens and have a positive entry in the encoder's `idf.json`. It is
//!   built from a tokenizer and table verified against expected hashes, so a sparse index can
//!   carry its own copies (research D6).
//! - [`sparse::field_text`] turns an expansion into the lexical field value a sparse index
//!   stores: the term `s<id>` repeated `round(weight × scale)` times.
//! - **`mmap` feature** (off by default): `LoadPath::Mmap` and `FlatIndex::open_mapped*`
//!   read the weights and the index through a read-only memory map. This is the crate's only
//!   hand-written `unsafe` block, in `bytes::map_readonly`, admitted by constitution v1.2.0 and
//!   ADR-0007 and tested bit-for-bit against the buffered path. The default feature set compiles
//!   no `unsafe` at all. Mapping carries the contract every mmap-backed store has: the caller
//!   must ensure no other process modifies or truncates the mapped file while the handle lives.

mod bytes;
mod embedder;
mod error;
mod gguf_header;
mod index;
pub mod model;
#[doc(hidden)]
pub mod quantise;
mod quantised_bert;
pub mod sparse;

pub use embedder::MiniLmEmbedder;
pub use index::{DenseStats, FlatIndex, validate_compaction_threshold};

/// How the weight file (and the vector index file) are brought into memory (spec FR-008).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadPath {
    /// `std::fs::read` into a heap buffer — the default, no `unsafe`.
    Buffered,
    /// Read-only memory map (feature `mmap`, ADR-0007).
    ///
    /// **Precondition the caller owns**: the mapped file (the weights, or an index's row file
    /// `vectors.<g>.bin`) must not be modified or truncated by any other process while the
    /// mapping lives. This crate never modifies a mapped byte — a row file is only extended
    /// beyond every mapping's end or replaced by rename (ADR-0013) — but no code can defend a
    /// mapping against an external writer; that is the inherent contract of memory mapping and
    /// the reason this path is opt-in rather than the default.
    #[cfg(feature = "mmap")]
    Mmap,
}

/// On-disk vector index format version this build reads and writes: 3, eight-bit rows
/// (Feature 026, ADR-0015) in the version-2 directory (Feature 024, ADR-0013). Versions 1 and
/// 2 are refused at open by name, with the instruction to rebuild.
pub const FORMAT_VERSION: u32 = 3;
