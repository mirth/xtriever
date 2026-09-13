//! The error type Swift sees: one case per `xtriever_core::Error` variant, each carrying the
//! engine's message (research D4). Every export returns `Result<_, XtrieverError>`, which
//! uniffi lowers to a Swift `throws` — a Rust error never becomes an uncatchable fatal error.

// See the crate-root comment and ADR-0003: `#[derive(uniffi::Error)]` emits `unsafe impl`.
#![allow(unsafe_code)]

/// A failure crossing the FFI boundary, lowered to a typed Swift error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum XtrieverError {
    /// Document or configuration does not conform to the schema.
    #[error("schema violation: {message}")]
    Schema {
        /// The engine's message.
        message: String,
    },
    /// Query cannot be executed as written.
    #[error("invalid query: {message}")]
    InvalidQuery {
        /// The engine's message.
        message: String,
    },
    /// Field is not declared in the schema.
    #[error("unknown field `{field}`")]
    UnknownField {
        /// The field name.
        field: String,
    },
    /// Vector or feature row has the wrong width.
    #[error("dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected width.
        expected: u64,
        /// Actual width.
        actual: u64,
    },
    /// Document id is not in the index.
    #[error("document {id} not found")]
    NotFound {
        /// The internal id.
        id: u32,
    },
    /// Model failed to load or run.
    #[error("model `{model}`: {message}")]
    Model {
        /// Model identity.
        model: String,
        /// Backend message.
        message: String,
    },
    /// Index data is corrupt or was written by an incompatible version.
    #[error("corrupt or incompatible index: {message}")]
    Corrupt {
        /// The engine's message.
        message: String,
    },
    /// Index was built with a different embedder configuration.
    #[error("embedder fingerprint mismatch: index has `{index}`, embedder is `{current}`")]
    FingerprintMismatch {
        /// Fingerprint stored in the index.
        index: String,
        /// Fingerprint of the embedder being used.
        current: String,
    },
    /// A stage exhausted its budget while running in strict mode.
    #[error("budget exhausted: {message}")]
    BudgetExhausted {
        /// The engine's message.
        message: String,
    },
    /// Filesystem error.
    #[error("i/o error: {message}")]
    Io {
        /// The engine's message.
        message: String,
    },
    /// Backend-specific error with no better mapping — also where any core variant added after
    /// this mapping lands, with its text (the core enum is `#[non_exhaustive]`).
    #[error("backend error: {message}")]
    Backend {
        /// The engine's message.
        message: String,
    },
}

impl From<xtriever_core::Error> for XtrieverError {
    fn from(e: xtriever_core::Error) -> Self {
        use xtriever_core::Error as E;
        match e {
            E::Schema(message) => Self::Schema { message },
            E::InvalidQuery(message) => Self::InvalidQuery { message },
            E::UnknownField(field) => Self::UnknownField {
                field: field.to_string(),
            },
            E::DimensionMismatch { expected, actual } => Self::DimensionMismatch {
                expected: expected as u64,
                actual: actual as u64,
            },
            E::NotFound(id) => Self::NotFound { id: id.0 },
            E::Model { model, message } => Self::Model { model, message },
            E::Corrupt(message) => Self::Corrupt { message },
            E::FingerprintMismatch { index, current } => {
                Self::FingerprintMismatch { index, current }
            }
            E::BudgetExhausted(message) => Self::BudgetExhausted { message },
            E::Io(io) => Self::Io {
                message: io.to_string(),
            },
            E::Backend(inner) => Self::Backend {
                message: inner.to_string(),
            },
            // `xtriever_core::Error` is `#[non_exhaustive]`: a variant added later arrives here
            // with its text rather than being lost. `tests/errors.rs` enumerates every current
            // variant, so a new one shows up as a test to extend.
            other => Self::Backend {
                message: other.to_string(),
            },
        }
    }
}

/// The index lock was poisoned by a panic on another thread.
pub(crate) fn poisoned() -> XtrieverError {
    XtrieverError::Backend {
        message: "index lock poisoned".into(),
    }
}
