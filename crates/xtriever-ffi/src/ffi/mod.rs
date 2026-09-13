//! The uniffi boundary: one object, three operations (spec FR-001), and nothing else.
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
    ChunkInfo, Degradation, DegradeReason, Hit, HitExplain, IndexInfo, LoadPath, RerankReport,
    SearchOptions, SearchResponse, StageReport,
};

/// A read-only open hybrid index with its models; searches are serialised per handle. The
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
    /// re-ranker. Nothing is ever written to `index_dir`.
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

    /// One search. Calls on one handle run one at a time, in call order; the time budget is
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
}
