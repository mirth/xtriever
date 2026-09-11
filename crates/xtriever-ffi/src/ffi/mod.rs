//! The uniffi boundary: exactly three exported operations (FR-006), and nothing else.
//!
//! Every function here is a thin delegating shim. All logic lives in `crate::spike`, which
//! re-declares `#![deny(unsafe_code)]`. Keeping this module logic-free is what makes ADR-0003's
//! lint relaxation reviewable — a reader can confirm at a glance that it covers wire format and
//! generated scaffolding only.

// See the crate-root comment and ADR-0003: `#[uniffi::export]` emits
// `#[unsafe(no_mangle)] pub unsafe extern "C" fn`.
#![allow(unsafe_code)]

pub mod error;
pub mod types;

pub use error::SpikeError;
pub use types::{IndexOutcome, LoadPath, RankedHit, SpikeDocument};

/// Index a corpus into a tantivy index at `index_dir`.
///
/// Documents are added in list order with a **single** writer thread. That is a correctness
/// requirement, not tuning: the multi-threaded writer does not allocate `DocId`s reproducibly, so
/// the host-produced golden ranking would not be comparable to the device's (research D5).
///
/// # Errors
///
/// [`SpikeError::IndexIo`] if the directory cannot be created, written, or committed.
#[uniffi::export]
pub fn spike_index(
    index_dir: String,
    documents: Vec<SpikeDocument>,
) -> Result<IndexOutcome, SpikeError> {
    crate::spike::index::run(&index_dir, &documents)
}

/// Run one keyword query against the index at `index_dir` and return the top `k` hits.
///
/// # Errors
///
/// [`SpikeError::IndexIo`] if the index cannot be opened; [`SpikeError::QueryParse`] if the query
/// cannot be parsed **or** matches nothing — the fixture query is guaranteed to match at least one
/// document, so an empty ranking means analysis dropped the query terms and is a failure, not a
/// vacuous pass.
#[uniffi::export]
pub fn spike_query(index_dir: String, query: String, k: u32) -> Result<Vec<RankedHit>, SpikeError> {
    crate::spike::query::run(&index_dir, &query, k)
}

/// Embed one sentence with all-MiniLM-L6-v2, returning 384 L2-normalized floats.
///
/// `load_path` selects the weight loader so the two can be compared; see [`LoadPath`].
///
/// # Errors
///
/// [`SpikeError::Model`] if an artifact is missing or fails its size/hash/dtype check,
/// [`SpikeError::Tokenize`] if encoding fails, [`SpikeError::Inference`] if the forward pass fails.
#[uniffi::export]
pub fn spike_embed(
    model_dir: String,
    sentence: String,
    load_path: LoadPath,
) -> Result<Vec<f32>, SpikeError> {
    crate::spike::embed::run(&model_dir, &sentence, load_path)
}
