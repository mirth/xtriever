//! `spike_index` — build a tantivy index from a document corpus.
//!
//! SCAFFOLD (PR 1a). The contract is fixed here so the acceptance tests can be committed failing
//! (FR-013); the implementation lands in PR 2 (tasks T027).
//!
//! When implemented, this MUST use a single writer thread and add documents in list order —
//! `Index::writer_with_num_threads(1, budget)`, never `Index::writer`. The multi-threaded writer
//! does not allocate `DocId`s reproducibly, and the collector breaks score ties by ascending
//! `DocAddress = (segment_ord, doc_id)`, so a multi-threaded build makes the host-produced golden
//! ranking incomparable to the device's for reasons that have nothing to do with iOS
//! (research D5).

use crate::ffi::{IndexOutcome, SpikeDocument, SpikeError};

/// Index `documents` into a fresh index at `index_dir`.
///
/// # Errors
///
/// Currently always [`SpikeError::NotImplemented`].
pub fn run(_index_dir: &str, _documents: &[SpikeDocument]) -> Result<IndexOutcome, SpikeError> {
    Err(SpikeError::NotImplemented {
        operation: "spike_index".to_owned(),
    })
}
