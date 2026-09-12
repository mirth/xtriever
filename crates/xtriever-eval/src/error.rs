//! One error type for the harness (Principle VII: `thiserror`, no panics).

use std::path::PathBuf;

/// Anything the harness can fail with.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The manifest is malformed, names an unknown dataset, or a loaded count disagrees with it.
    #[error("manifest: {0}")]
    Manifest(String),
    /// A cached file does not match its pinned size or hash (spec FR-007). Both hashes are named.
    #[error("hash mismatch for {path}: expected {expected}, actual {actual}")]
    HashMismatch {
        /// The file that failed verification.
        path: PathBuf,
        /// `<bytes>/<sha256>` recorded in the manifest.
        expected: String,
        /// `<bytes>/<sha256>` of the file on disk.
        actual: String,
    },
    /// A dataset file could not be parsed.
    #[error("parse error in {path} line {line}: {msg}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// 1-based line number.
        line: usize,
        /// What was wrong.
        msg: String,
    },
    /// The retriever or the configuration rejected the run.
    #[error("run: {0}")]
    Run(String),
    /// Filesystem error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// An error from the retrieval stage under test.
    #[error("retrieval stage: {0}")]
    Core(#[from] xtriever_core::Error),
    /// JSON encoding or decoding of a report or manifest.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result alias for the harness.
pub type Result<T> = std::result::Result<T, Error>;
