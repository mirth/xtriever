//! Xtriever `rerank` stage: a candle-backed [`Reranker`](xtriever_core::Reranker) for the pinned
//! `cross-encoder/ms-marco-MiniLM-L-6-v2` cross-encoder.
//!
//! Implemented by Feature 006 (`specs/006-rerank-stage/`). The public surface is the one in
//! `specs/006-rerank-stage/contracts/rerank-stage.md`.
//!
//! # Feature 006
//!
//! - [`MiniLmCrossEncoder`] loads `cross-encoder/ms-marco-MiniLM-L-6-v2` at the revision and
//!   file hashes in [`model::PINNED`] (every file is size- and SHA-256-verified before it is
//!   parsed) and asserts the model's shape from its files. candle-transformers 0.9.2 ships the
//!   BERT encoder without a classification head, so the head — the CLS row through the pooler,
//!   `tanh`, and a 384 → 1 linear layer — is composed from two `candle_nn::Linear` layers exactly
//!   as the reference `BertForSequenceClassification` defines it; the score is the raw logit.
//! - **One pair per forward pass at its own length**: the query and passage are tokenised
//!   together under `longest_first` truncation at 512 tokens, with no padding. A pair's tensor
//!   shapes therefore depend only on the pair, and its score is bit-identical regardless of the
//!   other passages in the call, their order, the call boundaries or the thread count
//!   (`RAYON_NUM_THREADS`, read by candle, never set here) — per CPU architecture, as for the
//!   embedder. An empty passage is encoded as the query alone, because that is what the
//!   reference tokenizer does with a falsy second string.
//! - **Budget** (`Reranker::rerank`): passages are scored in input order until the item limit
//!   is reached or the time limit — checked before every pair, the first included — is
//!   exceeded; every passage not reached is `None`, never an error. This crate is a leaf crate
//!   and reads `std::time::Instant` for that check (Principle III); the pipeline hands it the
//!   *remaining* time of the caller's budget.
//! - **Model identity** ([`model::MODEL_ID`]): `repo@revision;weights=sha256:…;max_tokens=512;
//!   trunc=longest_first;head=cls-pooler-tanh-linear;act=identity;dtype=f32;engine=candle-0.9.2`
//!   — every input whose change would change a score.
//! - **`mmap` feature** (off by default): [`LoadPath::Mmap`] reads the weights through a
//!   read-only memory map — the crate's only hand-written `unsafe` block, in
//!   `bytes::map_readonly`, admitted by constitution v1.3.0 and ADR-0009 and tested bit-for-bit
//!   against the buffered path. The default feature set compiles no `unsafe` at all. Mapping
//!   carries the contract every mmap-backed store has: the caller must ensure no other process
//!   modifies or truncates the mapped file while the handle lives.

mod budget;
mod bytes;
mod error;
mod gguf_header;
pub mod model;
mod quantised_bert;
mod scorer;

pub use budget::rerank_with;
pub use scorer::MiniLmCrossEncoder;

/// How the weight file is brought into memory (spec FR-003).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadPath {
    /// `std::fs::read` into a heap buffer — the default, no `unsafe`.
    Buffered,
    /// Read-only memory map (feature `mmap`, ADR-0009).
    ///
    /// **Precondition the caller owns**: the mapped weights file must not be modified or
    /// truncated by any other process while the mapping lives. This crate never writes it, but
    /// no code can defend a mapping against an external writer — that is the inherent contract
    /// of memory mapping and the reason this path is opt-in rather than the default.
    #[cfg(feature = "mmap")]
    Mmap,
}
