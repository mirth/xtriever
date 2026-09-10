//! `spike_query` — run one keyword query and return the top `k` hits.
//!
//! SCAFFOLD (PR 1a); implementation lands in PR 2 (task T028).
//!
//! When implemented, an **empty result set is an error, not a pass**. The fixture query is
//! constructed to match at least one document, so emptiness means analysis dropped the query terms.

use crate::ffi::{RankedHit, SpikeError};

/// Search `index_dir` for `query`, returning at most `k` hits, highest score first.
///
/// # Errors
///
/// Currently always [`SpikeError::NotImplemented`].
pub fn run(_index_dir: &str, _query: &str, _k: u32) -> Result<Vec<RankedHit>, SpikeError> {
    Err(SpikeError::NotImplemented {
        operation: "spike_query".to_owned(),
    })
}
