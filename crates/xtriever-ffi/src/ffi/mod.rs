//! The uniffi boundary: one object — open / info / search (Feature 007, spec FR-001) and, since
//! Feature 011, create / add / add_embedded / delete / commit / merge / contains — and nothing else.
//!
//! Every function here is a thin delegating shim. All logic lives in `crate::index`, which
//! re-declares `#![deny(unsafe_code)]`. Keeping this module logic-free is what makes ADR-0003's
//! lint relaxation reviewable — a reader can confirm at a glance that it covers wire format and
//! generated scaffolding only.

// See the crate-root comment and ADR-0003: `#[uniffi::export]` and `#[derive(uniffi::Object)]`
// emit `#[unsafe(no_mangle)] pub unsafe extern "C" fn` and `unsafe impl`.
#![allow(unsafe_code)]

pub mod error;
pub mod types;

use std::sync::Arc;

pub use error::XtrieverError;
pub use types::{
    ChunkInfo, Degradation, DegradeReason, Document, FieldDef, FieldKind, FieldValue, Hit,
    HitExplain, IndexConfig, IndexInfo, LoadPath, RerankMode, RerankReport, SearchOptions,
    SearchResponse, SparseInfo, SparseOptionConfig, StageReport,
};

/// An open hybrid index with its models; calls on one handle are serialised by a lock (no
/// ordering guarantee among waiters). `open` gives a **writable** handle whenever the directory's
/// lock can be taken and a read-only one otherwise (an app bundle); `create` (Feature 011) is
/// always writable; on a read-only handle every write returns `Io` "read-only index". The
/// Swift package wraps this as `XtrieverIndex` with the async layer.
#[derive(uniffi::Object)]
pub struct IndexHandle {
    inner: crate::index::Inner,
}

impl std::fmt::Debug for IndexHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexHandle")
            .field("info", &crate::index::info(&self.inner))
            .finish()
    }
}

#[uniffi::export]
impl IndexHandle {
    /// Open a hybrid index read-only with the pinned embedder and, optionally, the pinned
    /// re-ranker. A writable directory is opened with the lexical backend's lock (readers stay
    /// safe from a concurrent writer); a directory that refuses the lock file — an app bundle —
    /// is opened in place, lock-free (Feature 008 D11; the 007 copy-out, report F-001, is gone).
    ///
    /// # Errors
    ///
    /// The pipeline's and the models' errors, lowered one-to-one ([`XtrieverError`]).
    #[uniffi::constructor]
    pub fn open(
        index_dir: String,
        embedder_dir: String,
        reranker_dir: Option<String>,
        load_path: LoadPath,
    ) -> Result<Arc<Self>, XtrieverError> {
        let inner = crate::index::open(
            &index_dir,
            &embedder_dir,
            reranker_dir.as_deref(),
            load_path,
        )?;
        Ok(Arc::new(Self { inner }))
    }

    /// Identity and configuration of the open index.
    pub fn info(&self) -> IndexInfo {
        crate::index::info(&self.inner)
    }

    /// One search. Calls on one handle run one at a time (a lock, not a queue: waiters are not
    /// ordered); the time budget is
    /// measured from the moment the call takes the handle.
    ///
    /// # Errors
    ///
    /// The pipeline's errors, lowered one-to-one; in strict mode a stage failure is `Model`
    /// and a spent budget is `BudgetExhausted`.
    pub fn search(
        &self,
        query: String,
        options: SearchOptions,
    ) -> Result<SearchResponse, XtrieverError> {
        crate::index::search(&self.inner, &query, &options)
    }

    // ── Feature 011: the builder ─────────────────────────────────────────────────────────

    /// Create an empty index at `index_dir` (absent or empty; a non-empty directory is
    /// `Corrupt`) with `config`, the pinned embedder and, optionally, the pinned re-ranker.
    /// The handle is writable at once. Refusals are the engine's: a dense field missing from
    /// the schema or not a text field, or an unknown analyzer, is `Schema`.
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open), plus the engine's creation errors.
    #[uniffi::constructor]
    pub fn create(
        index_dir: String,
        config: IndexConfig,
        embedder_dir: String,
        reranker_dir: Option<String>,
        load_path: LoadPath,
    ) -> Result<Arc<Self>, XtrieverError> {
        let inner = crate::index::create(
            &index_dir,
            config,
            &embedder_dir,
            reranker_dir.as_deref(),
            load_path,
        )?;
        Ok(Arc::new(Self { inner }))
    }

    /// Stage documents, embedded by the index's embedder; a known id replaces. Visible to
    /// `search` and `contains` after `commit`.
    ///
    /// # Errors
    ///
    /// `Schema` (empty id, a value the schema rejects), `Model`, the stages' errors; `Io`
    /// "read-only index" on a handle the process could not lock.
    pub fn add(&self, docs: Vec<Document>) -> Result<(), XtrieverError> {
        crate::index::add(&self.inner, docs)
    }

    /// As [`add`](Self::add) with caller-supplied vectors, one per document in order.
    ///
    /// # Errors
    ///
    /// `Schema` when the counts differ, `DimensionMismatch` for a vector of the wrong width,
    /// otherwise as [`add`](Self::add).
    pub fn add_embedded(
        &self,
        docs: Vec<Document>,
        vectors: Vec<Vec<f32>>,
    ) -> Result<(), XtrieverError> {
        crate::index::add_embedded(&self.inner, docs, vectors)
    }

    /// Stage deletions by external id; unknown ids are ignored. Visible after `commit`.
    ///
    /// # Errors
    ///
    /// The stages' errors; `Io` "read-only index" on a read-only handle.
    pub fn delete(&self, external_ids: Vec<String>) -> Result<(), XtrieverError> {
        crate::index::delete(&self.inner, external_ids)
    }

    /// Commit every staged change (lexical → dense → passages → id map → descriptor); a no-op
    /// when nothing is staged.
    ///
    /// # Errors
    ///
    /// `Io`, the stages' errors; `Io` "read-only index" on a read-only handle.
    pub fn commit(&self) -> Result<(), XtrieverError> {
        crate::index::commit(&self.inner)
    }

    /// Commit, then compact the dense vectors (live rows only, under a new generation —
    /// Feature 024) and merge the lexical stage into one segment — the shape a shipped index
    /// has (Feature 008). Dense scores are identical before and after; across a merge that
    /// physically drops deleted or replaced documents the lexical BM25 statistics move, so
    /// fused hits and scores can change (ADR-0013).
    ///
    /// # Errors
    ///
    /// As [`commit`](Self::commit), plus the dense compaction's and the lexical merge's.
    pub fn merge(&self) -> Result<(), XtrieverError> {
        crate::index::merge(&self.inner)
    }

    /// Whether `external_id` is a committed live document (staged changes do not count).
    pub fn contains(&self, external_id: String) -> bool {
        crate::index::contains(&self.inner, &external_id)
    }
}
