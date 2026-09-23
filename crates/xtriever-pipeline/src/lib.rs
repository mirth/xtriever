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
//!
//! # Feature 015
//!
//! - **The re-ranked head is ordered by interpolation by default** ([`RerankMode::Interpolate`]
//!   with α 0.5; ADR-0012): `(1 − α)·minmax(fused score) + α·minmax(cross-encoder score)` over
//!   the scored head, ties by fused position, then the rest in fused order
//!   ([`order_interpolated`] is the exact rule). Feature 014 measured it at 0.7207 / 0.3622 /
//!   0.3910 nDCG@10 on SciFact / NFCorpus / FiQA against replace-order's 0.6954 / 0.3609 /
//!   0.3742, at the same cross-encoder calls. [`RerankMode::Replace`] keeps the Feature 006
//!   rule bit for bit.
//! - **Recorded and overridable**: [`HybridConfig::rerank_mode`] is written to the descriptor
//!   (`rerank_mode`; an index written before this feature has no key and reads as the default —
//!   the format version is unchanged); [`SearchOptions::rerank_mode`] overrides it per call.
//!   An α outside `[0, 1]` is `Error::Schema` at `create` and at `search`.
//! - **Explanation**: [`HitExplain::rerank_combined`] is the score the head was ordered by
//!   (`None` under `Replace`), under this crate's [`RERANK_COMBINED`]; `features()` has eight
//!   entries.
//!
//! # Feature 027
//!
//! - **Sparse lexical expansion, opt-in per index** ([`HybridConfig::sparse`], ADR-0016). A
//!   sparse index is created by [`HybridIndex::create_sparse`] with the pinned document encoder
//!   (`xtriever_dense::sparse::SparseEncoder`, build host only). Each passage's expansion is
//!   written to the reserved lexical field [`SPARSE_FIELD`] as the term `s<id>`, repeated
//!   `round(weight × scale)` times (default scale 10, field boost 1.0). The encoder's tokenizer
//!   and query-side table are copied into `<dir>/sparse/`, so a device searches with nothing
//!   but the index: `search` adds `Term(_sparse, "s<id>")` for each of the query's kept tokens,
//!   beside `Match` over the user's text fields.
//! - **Format**: a sparse index's descriptor is [`SPARSE_FORMAT_VERSION`] 3 with a `sparse`
//!   record (scale, boost, field, encoder identity, the two files' SHA-256s); every other index
//!   stays [`FORMAT_VERSION`] 2, byte for byte. An older engine refuses a sparse index by name.
//! - **Adding**: `add` expands with the attached encoder ([`HybridIndex::set_sparse_encoder`];
//!   none attached is `Error::Model`); `add_embedded` is refused on a sparse index;
//!   [`HybridIndex::add_encoded`] takes caller-supplied vectors and expansions (the evaluation
//!   harness's caches) and writes them exactly as `add` does.
//! - **When to switch it on**: corpora without titles and with vocabulary mismatch between
//!   questions and answers — FiQA gained +0.017 nDCG@10 in the spike; SciFact and NFCorpus did
//!   not. It is not the default (Feature 016's floor).
//!
//! # Feature 010
//!
//! - **The id map's shape**: one map, shared between the committed and the pending view
//!   (`Arc`, copied on the first staged change, rejoined at commit), held as arenas — every id's
//!   bytes once, a span per slot, a hash table over the arena, fixed-width chunk slots with
//!   interned parents — about 52 bytes per slot plus the ids' own bytes, read straight from the
//!   file through a streaming visitor. `ids.json` itself is unchanged and byte-identical
//!   (`specs/010-id-map-memory/contracts/id-map.md`).

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
pub use index::{HybridIndex, dense_passage};
pub use rerank::{RerankMode, order_interpolated, order_reranked};
pub use types::{
    Degradation, DegradeReason, HitExplain, HybridConfig, HybridHit, OpenOptions, RERANK_COMBINED,
    RERANK_RANK, RerankReport, Response, SPARSE_FIELD, SearchOptions, SourceDocument, SparseOption,
    SparseRecord, StageReport,
};

/// On-disk format version of the pipeline descriptor and id map this build reads and writes.
pub const FORMAT_VERSION: u32 = 2;

/// The descriptor's format version for a sparse index (Feature 027, ADR-0016): an index with the
/// option writes 3 so an engine that cannot search its expansions refuses it by name; every
/// other index stays [`FORMAT_VERSION`].
pub const SPARSE_FORMAT_VERSION: u32 = 3;

/// The counting allocator of the unit-test binary (Feature 010, research D7): the id-map
/// accounting tests read what the process actually holds. Integration tests link the library
/// without `cfg(test)` and are unaffected.
#[cfg(test)]
#[global_allocator]
static TEST_ALLOC: peak_alloc::PeakAlloc = peak_alloc::PeakAlloc;

#[cfg(test)]
pub(crate) fn test_alloc() -> &'static peak_alloc::PeakAlloc {
    &TEST_ALLOC
}
