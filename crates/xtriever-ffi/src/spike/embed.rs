//! `spike_embed` — embed one sentence with all-MiniLM-L6-v2.
//!
//! SCAFFOLD (PR 1a); implementation lands in PR 2 (tasks T029-T032).
//!
//! Three things the implementation must get right, each of which otherwise produces a failure that
//! looks like an iOS problem but is not:
//!
//! 1. Override the tokenizer's baked-in truncation and padding to **256** tokens. The model's
//!    `tokenizer.json` ships 128 while the reference behaviour is 256, so skipping this makes the
//!    Rust and Python results disagree with no iOS involvement (research D6).
//! 2. Pin the thread count to 1. The inference backend uses a work-stealing pool sized to the host's
//!    core count, which perturbs both the memory footprint and float summation order (research D15).
//! 3. Verify the weights by exact byte count and hash before loading, and fail hard on mismatch
//!    (FR-016).
//!
//! This module is where the single ADR-0002 `unsafe` block will live, for the memory-mapped loader.
//! In PR 1a there is none.

use crate::ffi::{LoadPath, SpikeError};

/// A tokenized sentence, padded and truncated to the model's sequence length.
///
/// Internal to the crate: not a uniffi type and not exposed to Swift. It exists so the
/// tokenization-parity oracle can be checked on its own, *before* the embedding comparison —
/// which is what stops a tokenizer disagreement being misfiled as an iOS embedding failure
/// (research D6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokenized {
    /// Token ids, length `max_sequence_length`.
    pub input_ids: Vec<u32>,
    /// 1 for real tokens, 0 for padding.
    pub attention_mask: Vec<u32>,
    /// All zero for a single-segment sentence; required positionally by candle's `forward`.
    pub token_type_ids: Vec<u32>,
}

/// Tokenize `sentence` with the model's tokenizer, overriding truncation and padding to 256.
///
/// # Errors
///
/// Currently always [`SpikeError::NotImplemented`].
pub fn tokenize(_model_dir: &str, _sentence: &str) -> Result<Tokenized, SpikeError> {
    Err(SpikeError::NotImplemented {
        operation: "tokenize".to_owned(),
    })
}

/// Embed `sentence`, returning 384 L2-normalized floats.
///
/// # Errors
///
/// Currently always [`SpikeError::NotImplemented`].
pub fn run(
    _model_dir: &str,
    _sentence: &str,
    _load_path: LoadPath,
) -> Result<Vec<f32>, SpikeError> {
    Err(SpikeError::NotImplemented {
        operation: "spike_embed".to_owned(),
    })
}
