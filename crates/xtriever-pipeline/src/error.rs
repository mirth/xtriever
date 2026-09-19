//! Helpers mapping the pipeline's failures onto `xtriever_core::Error`; no variant is added.

use xtriever_core::Error;

/// A document, id or configuration the pipeline cannot accept.
pub(crate) fn schema_err(message: impl Into<String>) -> Error {
    Error::Schema(message.into())
}

/// A descriptor, id map or stage state this build cannot serve.
pub(crate) fn corrupt(message: impl Into<String>) -> Error {
    Error::Corrupt(message.into())
}

/// Write `bytes` to `<path>.json.tmp`, sync, `rename` over `path`, and sync the directory —
/// `xtriever_core::fs`'s definition, so the descriptor, the id map and the dense manifest are
/// durable the same way (Feature 024).
pub(crate) fn write_atomically(path: &std::path::Path, bytes: &[u8]) -> xtriever_core::Result<()> {
    let tmp = path.with_extension("json.tmp");
    xtriever_core::fs::write_atomically(path, &tmp, bytes)?;
    if let Some(dir) = path.parent() {
        xtriever_core::fs::sync_dir(dir)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn helpers_map_to_the_expected_variants() {
        assert!(matches!(schema_err("x"), Error::Schema(_)));
        assert!(matches!(corrupt("x"), Error::Corrupt(_)));
    }

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_atomically(&path, b"1").unwrap();
        write_atomically(&path, b"22").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"22");
        assert!(!dir.path().join("a.json.tmp").exists());
    }
}
