//! The sparse document encoder and its query side (Feature 027, research D1–D4, D7).
//!
//! RED-CHECKPOINT STUB: the public items of `contracts/sparse-option.md` with no behaviour.

use std::path::Path;

use xtriever_core::Result;

use crate::LoadPath;
use crate::error::sparse_err;

/// One document's expansion: its kept `(token id, weight)` entries, ascending by id, every
/// weight above zero, special tokens absent (research D3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Expansion {
    /// `(token id, weight)`, ascending by id.
    pub entries: Vec<(u32, f32)>,
    /// The document ran past the encoder's window and was truncated to it.
    pub truncated: bool,
}

/// The pinned document encoder — build host only.
#[derive(Debug)]
pub struct SparseEncoder {}

impl SparseEncoder {
    /// Verify every pinned file, then load.
    ///
    /// # Errors
    ///
    /// `Error::Model` naming the file and both values on a mismatch.
    pub fn load(_dir: &Path, _load_path: LoadPath) -> Result<Self> {
        Err(sparse_err("not implemented"))
    }

    /// One document, alone, truncated to the encoder's window.
    ///
    /// # Errors
    ///
    /// `Error::Model` if tokenisation or the forward pass fails.
    pub fn encode(&self, _text: &str) -> Result<Expansion> {
        Err(sparse_err("not implemented"))
    }

    /// The token ids the encoder sees for `text`, for the tokenization-parity test.
    ///
    /// # Errors
    ///
    /// `Error::Model` if tokenisation fails.
    pub fn token_ids(&self, _text: &str) -> Result<Vec<u32>> {
        Err(sparse_err("not implemented"))
    }

    /// The identity recorded in a sparse index.
    #[must_use]
    pub fn identity(&self) -> &str {
        ""
    }
}

/// The query side — every installation that searches a sparse index.
#[derive(Debug)]
pub struct SparseQuery {}

impl SparseQuery {
    /// From a tokenizer and a query-side table, each verified against an expected SHA-256.
    ///
    /// # Errors
    ///
    /// `Error::Corrupt` naming the file and both hashes.
    pub fn open(
        _tokenizer: &Path,
        _table: &Path,
        _tokenizer_sha256: &str,
        _table_sha256: &str,
    ) -> Result<Self> {
        Err(sparse_err("not implemented"))
    }

    /// Distinct kept token ids, ascending.
    ///
    /// # Errors
    ///
    /// `Error::Model` if tokenisation fails.
    pub fn terms(&self, _text: &str) -> Result<Vec<u32>> {
        Err(sparse_err("not implemented"))
    }
}

/// `s<id>` repeated `round(weight × scale)` times.
#[must_use]
pub fn field_text(_expansion: &Expansion, _scale: u32) -> String {
    String::new()
}
