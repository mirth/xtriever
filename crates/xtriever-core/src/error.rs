//! One error type for the whole engine (Constitution §VII). Stage crates map backend errors
//! into these variants; `Backend` is the escape hatch for anything else.

use crate::types::{DocId, FieldName};

/// Engine-wide error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Document does not conform to the schema.
    #[error("schema violation: {0}")]
    Schema(String),
    /// Query cannot be executed as written.
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    /// Field is not declared in the schema.
    #[error("unknown field `{0}`")]
    UnknownField(FieldName),
    /// Vector or feature row has the wrong width.
    #[error("dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected width.
        expected: usize,
        /// Actual width.
        actual: usize,
    },
    /// Document id is not in the index.
    #[error("document {0} not found")]
    NotFound(DocId),
    /// Model failed to load or run.
    #[error("model `{model}`: {message}")]
    Model {
        /// Model identity.
        model: String,
        /// Backend message.
        message: String,
    },
    /// Index data is corrupt or was written by an incompatible version.
    #[error("corrupt or incompatible index: {0}")]
    Corrupt(String),
    /// Index was built with a different embedder configuration.
    #[error("embedder fingerprint mismatch: index has `{index}`, embedder is `{current}`")]
    FingerprintMismatch {
        /// Fingerprint stored in the index.
        index: String,
        /// Fingerprint of the embedder being used.
        current: String,
    },
    /// A stage exhausted its budget while running in strict mode.
    #[error("budget exhausted: {0}")]
    BudgetExhausted(String),
    /// Filesystem error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// Backend-specific error with no better mapping.
    #[error("backend error: {0}")]
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

/// The message every refused write and every refused mutation on a read-only index carries —
/// one text across the lexical, dense and pipeline layers, matched by the FFI's read-only
/// mapping and by tests.
pub const READ_ONLY_MESSAGE: &str = "read-only index";

impl Error {
    /// Wrap an arbitrary backend error.
    pub fn backend<E: std::error::Error + Send + Sync + 'static>(e: E) -> Self {
        Self::Backend(Box::new(e))
    }

    /// The refusal of a mutation on an index opened read-only: `Io` with
    /// `ErrorKind::PermissionDenied` and [`READ_ONLY_MESSAGE`].
    #[must_use]
    pub fn read_only() -> Self {
        Self::Io(read_only_io_error())
    }
}

/// [`Error::read_only`] as the bare `std::io::Error`, for layers that speak `io::Error` (a
/// storage backend's directory abstraction).
#[must_use]
pub fn read_only_io_error() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, READ_ONLY_MESSAGE)
}

/// Result alias used throughout Xtriever.
pub type Result<T> = std::result::Result<T, Error>;
