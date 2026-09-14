//! Backend error → `xtriever_core::Error` mapping (spec FR-003, research D16).
//!
//! Six existing variants cover every failure this crate produces; nothing is added.
//! `Backend` is core's documented escape hatch, and it is where `LockFailure` lands (D14).

use xtriever_core::{Error, FieldName};

/// Map any backend error. I/O errors keep their variant so callers can match on them — including
/// a lock the *directory* refused (`LockFailure(LockError::IoError)`, e.g. `PermissionDenied` on
/// a read-only location; Feature 008 D11), which is an I/O fact about the directory. A lock that
/// is merely busy (`LockBusy`: another writer holds it) stays `Backend` (D14).
pub(crate) fn map(e: tantivy::TantivyError) -> Error {
    match e {
        tantivy::TantivyError::IoError(arc) => Error::Io(std::io::Error::new(arc.kind(), arc)),
        tantivy::TantivyError::LockFailure(
            tantivy::directory::error::LockError::IoError(arc),
            _,
        ) => Error::Io(std::io::Error::new(arc.kind(), arc)),
        other => Error::backend(other),
    }
}

/// A document, schema or analyzer problem: the *schema* is what is wrong (or violated).
pub(crate) fn schema_err(msg: impl Into<String>) -> Error {
    Error::Schema(msg.into())
}

/// A query or filter that cannot be executed as written; the schema itself is fine.
pub(crate) fn invalid_query(msg: impl Into<String>) -> Error {
    Error::InvalidQuery(msg.into())
}

pub(crate) fn unknown_field(name: &FieldName) -> Error {
    Error::UnknownField(name.clone())
}

pub(crate) fn corrupt(msg: impl Into<String>) -> Error {
    Error::Corrupt(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_keep_the_io_variant() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let e = map(tantivy::TantivyError::IoError(std::sync::Arc::new(io)));
        assert!(matches!(e, Error::Io(ref inner) if inner.kind() == std::io::ErrorKind::NotFound));
    }

    #[test]
    fn everything_else_is_backend() {
        let e = map(tantivy::TantivyError::InvalidArgument("x".into()));
        assert!(matches!(e, Error::Backend(_)));
        let e = map(tantivy::TantivyError::SchemaError("y".into()));
        assert!(matches!(e, Error::Backend(_)));
    }
}
