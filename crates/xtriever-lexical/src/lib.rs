//! Xtriever lexical stage — BM25 retrieval, metadata filters and corpus statistics over tantivy.
//!
//! One public type, [`TantivyIndex`], implements [`xtriever_core::LexicalIndex`]; the contract is
//! `specs/002-lexical-stage/contracts/lexical-index.md`.
//!
//! # Threads and memory
//!
//! `create`/`open` spawn no threads and allocate no arena. The first `add` or `delete` creates the
//! backend writer: one indexing worker, one segment-updater thread, one merge worker and the
//! backend's doc-store compression thread, plus a 15 MB arena, all living until the index is
//! dropped. Query execution is single-threaded. The crate spawns no threads of its own.
//!
//! # Sharing
//!
//! Mutation takes `&mut self` and there is no interior locking (spec FR-028). Wrap the index in a
//! `RwLock` to share it; a write lock held across `add` + `commit` blocks queries for its duration.
//! Two handles may open the same directory: both read; the second to mutate gets
//! `Error::Backend` while the first holds the writer lock, and each handle sees only its own
//! commits (research D14).

mod error;
mod filter;
mod index;
mod query;
mod schema;
mod search;
mod stats;

pub use index::TantivyIndex;
pub use schema::ANALYZERS;
