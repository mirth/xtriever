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
//!
//! # Feature 006
//!
//! - **Format version 2** ([`FORMAT_VERSION`], ADR-0008): every hybrid index stores its passage
//!   text — the dense passage, exactly what the embedder saw — in `<dir>/passages.bin`, written
//!   whole at commit (lexical → dense → passages → id map → descriptor) and read per hit, never
//!   held in memory whole. Every [`HybridHit`] carries its `text`. `open` refuses any other
//!   format version naming both, and checks a fifth count: the store's slot count against the
//!   id map's length. The descriptor gains `rerank_depth`.
//! - **Re-ranking**: an optional [`Reranker`](xtriever_core::Reranker) attached per handle
//!   (`set_reranker`, never persisted) re-scores the first `rerank_depth` fused candidates
//!   (default 20, overridable per call) after a third budget check point; the fused list is
//!   built to `max(k, rerank_depth)` so a candidate just below `k` can be promoted. The
//!   response is the scored hits first, `(rerank_score DESC, DocId ASC)`, then every unscored
//!   hit — not reached by the budget, or beyond the depth — in fused order ([`order_reranked`]
//!   is the exact rule). `HybridHit::score` keeps its fused meaning; `rerank_score` is a
//!   separate field. The re-ranker receives the *remaining* time of the caller's budget.
//! - **Degradation, per stage**: a re-ranker error or a budget spent at the check point keeps
//!   the fused order and records the reason in [`StageReport::rerank`]; strict mode returns the
//!   error. A partial result is the contract working, not a degradation. A wrong-length or
//!   non-finite result is `Error::Model` in every mode. A degraded dense stage does not skip the
//!   re-ranker.
//! - **Explanation**: [`HitExplain`] gains `rerank_score` and `rerank_rank` (1-based position
//!   among the re-ranked hits) under the core's `rerank.score` and this crate's [`RERANK_RANK`].

mod descriptor;
mod error;
mod fusion;
mod ids;
mod index;
mod passages;
mod rerank;
mod search;
mod types;

pub use fusion::rrf;
pub use index::HybridIndex;
pub use rerank::order_reranked;
pub use types::{
    Degradation, DegradeReason, HitExplain, HybridConfig, HybridHit, OpenOptions, RERANK_RANK,
    RerankReport, Response, SearchOptions, SourceDocument, StageReport,
};

/// On-disk format version of the pipeline descriptor and id map this build reads and writes.
pub const FORMAT_VERSION: u32 = 2;
