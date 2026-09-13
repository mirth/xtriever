//! The pipeline's public data types (contract `hybrid-pipeline.md`; data-model).

use std::collections::BTreeMap;
use std::time::Duration;

use xtriever_core::{Budget, ChunkInfo, DocId, FeatureName, FieldName, Schema, Value, features};

/// Creation-time configuration of a hybrid index.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridConfig {
    /// The lexical schema; every field is stored in the lexical stage.
    pub schema: Schema,
    /// Text fields joined by a single space, in this order, into the dense passage.
    pub dense_fields: Vec<FieldName>,
    /// Candidates taken from each stage per search unless the caller overrides.
    pub candidate_depth: usize,
    /// Reciprocal rank fusion constant.
    pub rrf_k: u32,
    /// Fused candidates re-scored by an attached re-ranker unless the caller overrides;
    /// 0 = never by default.
    pub rerank_depth: usize,
}

impl HybridConfig {
    /// A configuration with the defaults: candidate depth 100, `rrf_k` 60, re-rank depth 20.
    #[must_use]
    pub fn new(schema: Schema, dense_fields: Vec<FieldName>) -> Self {
        Self {
            schema,
            dense_fields,
            candidate_depth: 100,
            rrf_k: 60,
            rerank_depth: 20,
        }
    }
}

/// A document under its external id.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceDocument {
    /// The caller's id; never seen by the stages.
    pub external_id: String,
    /// Field values, validated by the lexical stage against the schema.
    pub fields: BTreeMap<FieldName, Value>,
    /// Chunk provenance, returned with hits.
    pub chunk: Option<ChunkInfo>,
}

/// Per-search options. `Default` = no override, degrading mode, no budget, no clock, no explain.
#[derive(Clone, Copy, Default)]
pub struct SearchOptions<'a> {
    /// Candidates per stage; `None` = the index's configured depth.
    pub depth: Option<usize>,
    /// Fused candidates handed to the re-ranker; `None` = the index's configured depth,
    /// `Some(0)` = no re-ranking on this call.
    pub rerank_depth: Option<usize>,
    /// Return an ML stage's error instead of degrading.
    pub strict: bool,
    /// Item limit caps the dense depth; the time limit needs `elapsed`.
    pub budget: Budget,
    /// Monotonic time since the call started, supplied by the caller (the crate reads no clock).
    pub elapsed: Option<&'a dyn Fn() -> Duration>,
    /// Attach [`HitExplain`] to every hit.
    pub explain: bool,
}

impl std::fmt::Debug for SearchOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchOptions")
            .field("depth", &self.depth)
            .field("rerank_depth", &self.rerank_depth)
            .field("strict", &self.strict)
            .field("budget", &self.budget)
            .field("elapsed", &self.elapsed.map(|_| "<fn>"))
            .field("explain", &self.explain)
            .finish()
    }
}

/// A search result.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    /// At most `k` hits. Without re-ranking: `(score DESC, DocId ASC)`; degraded: the lexical
    /// list in lexical order. With re-ranking: the hits with a `rerank_score` first, ordered
    /// `(rerank_score DESC, DocId ASC)`, then every other hit in fused order.
    pub hits: Vec<HybridHit>,
    /// What ran, what was skipped and why.
    pub stages: StageReport,
}

/// One hit.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridHit {
    /// The caller's id.
    pub external_id: String,
    /// The internal id (the tie-break key).
    pub id: DocId,
    /// Fused score (RRF, `f64`); the lexical score when degraded. Its meaning does not change
    /// when the hit was re-ranked — the order does (see [`Response::hits`]).
    pub score: f64,
    /// The re-ranker's score where the stage scored this hit.
    pub rerank_score: Option<f32>,
    /// The stored passage text (the text the dense stage embedded).
    pub text: String,
    /// Chunk provenance, if the document is a chunk.
    pub chunk: Option<ChunkInfo>,
    /// Per-stage explanation when requested.
    pub explain: Option<HitExplain>,
}

/// Per-stage scores and 1-based ranks; `None` where a stage did not retrieve the document.
#[derive(Debug, Clone, PartialEq)]
pub struct HitExplain {
    /// BM25 score from the lexical stage.
    pub bm25_score: Option<f32>,
    /// 1-based rank in the lexical candidate list.
    pub bm25_rank: Option<u32>,
    /// Similarity from the dense stage.
    pub dense_score: Option<f32>,
    /// 1-based rank in the dense candidate list.
    pub dense_rank: Option<u32>,
    /// The fused score.
    pub fused: f64,
    /// The re-ranker's score, where it scored this hit.
    pub rerank_score: Option<f32>,
    /// 1-based position among the re-ranked hits, where scored.
    pub rerank_rank: Option<u32>,
}

impl HitExplain {
    /// The explanation under the core's well-known feature names (plus the pipeline's
    /// [`RERANK_RANK`]), `NaN` for absent values.
    #[must_use]
    pub fn features(&self) -> [(FeatureName, f32); 7] {
        let opt = |v: Option<f32>| v.unwrap_or(f32::NAN);
        let rank = |r: Option<u32>| r.map_or(f32::NAN, |r| r as f32);
        [
            (
                FeatureName::from_static(features::BM25_SCORE),
                opt(self.bm25_score),
            ),
            (
                FeatureName::from_static(features::BM25_RANK),
                rank(self.bm25_rank),
            ),
            (
                FeatureName::from_static(features::DENSE_SCORE),
                opt(self.dense_score),
            ),
            (
                FeatureName::from_static(features::DENSE_RANK),
                rank(self.dense_rank),
            ),
            (
                FeatureName::from_static(features::FUSED_SCORE),
                self.fused as f32,
            ),
            (
                FeatureName::from_static(features::RERANK_SCORE),
                opt(self.rerank_score),
            ),
            (
                FeatureName::from_static(RERANK_RANK),
                rank(self.rerank_rank),
            ),
        ]
    }
}

/// 1-based position among the re-ranked hits — the pipeline's own feature name, beside the
/// core's `rerank.score`.
pub const RERANK_RANK: &str = "rerank.rank";

/// What the stages did for one search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageReport {
    /// Candidates the lexical stage returned.
    pub lexical_candidates: usize,
    /// Candidates the dense stage returned; `None` when it did not run — degraded (see
    /// `degraded`), or short-circuited by `k == 0` / an empty resolved filter.
    pub dense_candidates: Option<usize>,
    /// Set when the dense stage was skipped and the fused list is the lexical list.
    pub degraded: Option<Degradation>,
    /// What the re-ranker did; `None` when the stage did not run at all (no re-ranker
    /// attached, depth 0, or nothing to re-rank).
    pub rerank: Option<RerankReport>,
    /// A time limit was set but no time source was supplied, so it was ignored.
    pub time_limit_ignored: bool,
}

/// The re-rank stage's outcome for one search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RerankReport {
    /// Passages handed to the re-ranker (at most the re-rank depth).
    pub candidates: usize,
    /// Passages the re-ranker scored within its budget.
    pub scored: usize,
    /// Set when the stage was skipped for cause — an error, or a budget spent before it ran —
    /// and the response is the fused order.
    pub skipped: Option<DegradeReason>,
}

/// A skipped ML stage and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Degradation {
    /// The stage skipped (`"dense"`).
    pub stage: &'static str,
    /// Why.
    pub reason: DegradeReason,
}

/// Why a stage was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DegradeReason {
    /// The stage returned an error (its message).
    StageError(String),
    /// The caller's time budget was spent.
    BudgetExceeded {
        /// Elapsed milliseconds when the check fired.
        elapsed_ms: u64,
        /// The limit in milliseconds.
        limit_ms: u64,
    },
}
