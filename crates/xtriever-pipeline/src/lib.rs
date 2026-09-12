//! Xtriever `pipeline` stage: the hybrid index that composes the lexical stage (002) and the
//! dense stage (004) behind one synchronous API — external ids in, fused ranking out, graceful
//! degradation, explanation.
//!
//! Implemented by Feature 005 (`specs/005-hybrid-pipeline/`). The public surface is the one in
//! `specs/005-hybrid-pipeline/contracts/hybrid-pipeline.md`.
//!
//! # Feature 005
//!
//! - [`HybridIndex`] owns a `TantivyIndex` (`<dir>/lexical`), a `FlatIndex` (`<dir>/dense`), a
//!   caller-supplied [`Embedder`](xtriever_core::Embedder) and the id map (`<dir>/ids.json`),
//!   described by `<dir>/xtriever-pipeline.json` ([`FORMAT_VERSION`] 1). Both pipeline files are
//!   only ever replaced by `rename`.
//! - **The id boundary**: documents go in as [`SourceDocument`]s under external string ids; the
//!   pipeline assigns internal `DocId`s in ingestion order and never reuses one after a delete;
//!   neither stage ever sees a string id. Chunk provenance is kept in the id map and returned on
//!   hits, ungrouped.
//! - **Commit order** is lexical → dense → id map → descriptor; `open` refuses any partial
//!   state by comparing four live counts (descriptor, id map, lexical, dense).
//! - **Search**: a filter is resolved once and applied to both stages as one allowed set; each
//!   stage returns `candidate_depth` candidates; reciprocal rank fusion (`Σ 1/(rrf_k + rank)`,
//!   `rrf_k` 60, computed in `f64`, lexical term then dense term) orders them
//!   `(score DESC, DocId ASC)`; [`rrf`] is the exact rule, public for tests and the harness.
//! - **Degradation** (Principle VI): a dense-stage error, or a spent time budget, yields the
//!   lexical list marked as degraded on the [`StageReport`]; strict mode returns the error (or
//!   `BudgetExhausted`) instead. A lexical failure is always an error. Time comes from the
//!   caller's [`SearchOptions::elapsed`] closure, checked between stages — this crate reads no
//!   clock, because `Instant` does not exist on wasm32 (Principle III).
//! - **Explanation**: every hit can carry [`HitExplain`] — both stages' scores and 1-based
//!   ranks (absent where a stage did not retrieve it) and the fused score — under the core's
//!   feature names via [`HitExplain::features`]; requesting it never changes the ranking.
//! - **`mmap` feature** (off by default) forwards to the dense stage's read-only mapping; the
//!   dense crate's precondition on external writers applies. The pipeline itself contains no
//!   `unsafe`.

mod descriptor;
mod error;
mod fusion;
mod ids;
mod index;
mod search;
mod types;

pub use fusion::rrf;
pub use index::HybridIndex;
pub use types::{
    Degradation, DegradeReason, HitExplain, HybridConfig, HybridHit, Response, SearchOptions,
    SourceDocument, StageReport,
};

/// On-disk format version of the pipeline descriptor and id map this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;
