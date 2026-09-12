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
//! - [`FlatIndex`] stores `(DocId, vector)` rows in one `index.bin` per generation
//!   ([`FORMAT_VERSION`] 1) and searches them exactly: scores accumulate in `f64` and are rounded
//!   once; results are ordered `(score DESC, DocId ASC)`, including at the `k`-th rank. `commit`
//!   writes a whole new file and `rename`s it over the old one, so a handle (or a mapping) of
//!   the previous generation is never disturbed.
//! - **Fingerprint** ([`model::FINGERPRINT`]): `repo@revision;weights=sha256:…;dim=384;
//!   pool=mean-mask;norm=l2;max_tokens=256;dtype=f32;prefix=none;engine=candle-0.9.2` — every
//!   input whose change would change the vectors, including the inference engine version. An
//!   index carries it and `FlatIndex::open_for` refuses a different embedder.
//! - **Determinism promise**: same fingerprint, same CPU architecture ⇒ bit-identical vectors,
//!   regardless of batch composition, order, size or thread count (`RAYON_NUM_THREADS`, read by
//!   candle, never set here). Across architectures the SIMD reduction paths differ, so agreement
//!   is within the golden tolerance, not bit-for-bit; the fingerprint deliberately excludes the
//!   architecture so an index built on one machine opens on another.
//! - **`mmap` feature** (off by default): `LoadPath::Mmap` and `FlatIndex::open_mapped*`
//!   read the weights and the index through a read-only memory map. This is the crate's only
//!   hand-written `unsafe` block, in `bytes::map_readonly`, admitted by constitution v1.2.0 and
//!   ADR-0007 and tested bit-for-bit against the buffered path. The default feature set compiles
//!   no `unsafe` at all. Mapping carries the contract every mmap-backed store has: the caller
//!   must ensure no other process modifies or truncates the mapped file while the handle lives.

mod bytes;
mod embedder;
mod error;
mod index;
pub mod model;

pub use embedder::MiniLmEmbedder;
pub use index::FlatIndex;

/// How the weight file (and the vector index file) are brought into memory (spec FR-008).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadPath {
    /// `std::fs::read` into a heap buffer — the default, no `unsafe`.
    Buffered,
    /// Read-only memory map (feature `mmap`, ADR-0007).
    ///
    /// **Precondition the caller owns**: the mapped file (the weights, or an index's
    /// `index.bin`) must not be modified or truncated by any other process while the mapping
    /// lives. This crate never writes either file in place, but no code can defend a mapping
    /// against an external writer — that is the inherent contract of memory mapping and the
    /// reason this path is opt-in rather than the default.
    #[cfg(feature = "mmap")]
    Mmap,
}

/// On-disk vector index format version this build reads and writes (data-model "On disk").
pub const FORMAT_VERSION: u32 = 1;
