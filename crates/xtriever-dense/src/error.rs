//! Helpers mapping this crate's failures onto `xtriever_core::Error` (research D9). No variant is
//! added: `Model`, `DimensionMismatch`, `Schema`, `InvalidQuery`, `Corrupt`, `FingerprintMismatch`
//! and `Io` cover everything the dense stage can fail with.

use xtriever_core::Error;

use crate::model::{MODEL_NAME, SPARSE_MODEL_NAME};

/// A model file, assertion, tokenizer, weight or forward-pass failure.
pub(crate) fn model_err(message: impl Into<String>) -> Error {
    Error::Model {
        model: MODEL_NAME.to_owned(),
        message: message.into(),
    }
}

/// A sparse encoder file, assertion, tokenizer, weight or forward-pass failure (Feature 027).
pub(crate) fn sparse_err(message: impl Into<String>) -> Error {
    Error::Model {
        model: SPARSE_MODEL_NAME.to_owned(),
        message: message.into(),
    }
}

/// A vector offered to `add` that the index cannot store (non-finite, zero-norm under cosine).
pub(crate) fn schema_err(message: impl Into<String>) -> Error {
    Error::Schema(message.into())
}

/// A query vector that cannot be scored (non-finite, zero-norm under cosine).
pub(crate) fn invalid_query(message: impl Into<String>) -> Error {
    Error::InvalidQuery(message.into())
}

/// An index file that is not one this build reads.
pub(crate) fn corrupt(message: impl Into<String>) -> Error {
    Error::Corrupt(message.into())
}

/// A vector or query of the wrong width.
pub(crate) fn dim_mismatch(expected: usize, actual: usize) -> Error {
    Error::DimensionMismatch { expected, actual }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn model_err_names_the_model() {
        let e = model_err("boom");
        assert_eq!(e.to_string(), "model `all-MiniLM-L6-v2`: boom");
    }

    #[test]
    fn dim_mismatch_carries_both_widths() {
        match dim_mismatch(384, 3) {
            Error::DimensionMismatch { expected, actual } => {
                assert_eq!((expected, actual), (384, 3))
            }
            _ => unreachable!(),
        }
    }
}
