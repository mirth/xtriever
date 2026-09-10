//! `xtriever-core` — the contract every Xtriever retrieval stage is built against.
//!
//! Pure Rust, `std`-only, no I/O, no threads, no async (Constitution §III, §V).
//! Stage crates implement the traits in [`traits`]; the pipeline composes them.

#![forbid(unsafe_code)]

pub mod error;
pub mod traits;
pub mod types;

pub use error::{Error, Result};
pub use traits::{Analyzer, Embedder, LexicalIndex, Ranker, Reranker, VectorIndex};
pub use types::*;
