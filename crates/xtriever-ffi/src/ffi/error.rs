//! The error type Swift sees.
//!
//! Every exported operation returns `Result<_, SpikeError>`. That is load-bearing, not stylistic:
//! a Rust panic inside a *non-throwing* Swift function becomes an uncatchable fatal error, so a
//! panic on this path takes the measurement harness down and loses the run. Declaring `Result` is
//! what makes the Swift function `throws`.

// See the crate-root comment and ADR-0003: `#[derive(uniffi::Error)]` emits `unsafe impl`.
#![allow(unsafe_code)]

/// A failure crossing the FFI boundary, lowered to a typed Swift error.
///
/// Variants carry fields rather than being flattened with `#[uniffi(flat_error)]` so that Swift can
/// report *which* path or artifact failed — which is what makes a spike `Finding` reproducible.
//
// DELIBERATE: no `From<xtriever_core::Error>`. `xtriever_core::Error::Backend(Box<dyn Error>)` is
// not lowerable to Swift, and `Error` is `#[non_exhaustive]`, so any `From` would need a catch-all
// arm that silently absorbs variants added later. The spike implements no core trait (FR-009,
// FR-029), so it never produces one. Defining that mapping is the lexical/dense specs' work and
// changes error semantics, which needs an ADR under Principle V. Do not add it here.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
#[non_exhaustive]
pub enum SpikeError {
    /// Creating, opening, or writing the on-disk index failed.
    #[error("index i/o failed at {path}: {message}")]
    IndexIo {
        /// The index directory involved.
        path: String,
        /// The underlying backend message.
        message: String,
    },

    /// The query could not be parsed, or analysis reduced it to nothing.
    #[error("query could not be parsed: {message}")]
    QueryParse {
        /// What went wrong.
        message: String,
    },

    /// A model artifact was missing, the wrong size, or the wrong dtype.
    #[error("model artifact invalid: {message}")]
    Model {
        /// What failed verification.
        message: String,
    },

    /// Tokenization failed.
    #[error("tokenization failed: {message}")]
    Tokenize {
        /// The underlying tokenizer message.
        message: String,
    },

    /// The forward pass failed.
    #[error("inference failed: {message}")]
    Inference {
        /// The underlying inference message.
        message: String,
    },

    /// SCAFFOLD — an operation whose implementation has not landed yet.
    ///
    /// This exists so the acceptance tests can be committed failing (FR-013, Agent Operating
    /// Rule 4) with clean red test *failures* rather than compile errors, and without `todo!` or
    /// `unimplemented!`, which are lint-banned and would abort the Swift process.
    ///
    /// It is temporary. `scripts/check-no-stubs.sh` fails the gate if it survives PR 2, and its
    /// presence on `main` after the three operations land is a defect.
    #[error("{operation} is not implemented in this build")]
    NotImplemented {
        /// The operation that was called.
        operation: String,
    },
}
