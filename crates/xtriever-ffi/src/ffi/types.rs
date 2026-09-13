//! Wire types crossing to Swift (contract `ffi-surface.md`; data-model 007). Every type here
//! mirrors a pipeline or core type one-to-one; the FFI adds no computation.

// See the crate-root comment and ADR-0003: the uniffi derives emit `unsafe impl`.
#![allow(unsafe_code)]

/// Per-search options — the pipeline's `SearchOptions` on the wire, plus `k`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SearchOptions {
    /// Hits returned; `0` yields an empty response and runs no stage.
    pub k: u32,
    /// Candidates per stage; `None` = the index's configured depth.
    #[uniffi(default = None)]
    pub depth: Option<u32>,
    /// Fused candidates handed to the re-ranker; `None` = the index's default, `Some(0)` = none.
    #[uniffi(default = None)]
    pub rerank_depth: Option<u32>,
    /// Time budget in milliseconds, measured by the FFI layer from the start of the call.
    #[uniffi(default = None)]
    pub max_time_ms: Option<u64>,
    /// Item budget: caps the dense depth and the re-ranked count.
    #[uniffi(default = None)]
    pub max_items: Option<u32>,
    /// Return an ML stage's error instead of degrading.
    #[uniffi(default = false)]
    pub strict: bool,
    /// Attach [`HitExplain`] to every hit.
    #[uniffi(default = false)]
    pub explain: bool,
}

/// A search result.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchResponse {
    /// At most `k` hits in the pipeline's order (re-ranked first, then fused order).
    pub hits: Vec<Hit>,
    /// What ran, what was skipped and why.
    pub stages: StageReport,
    /// Wall time of the call inside the FFI layer, lock wait excluded, in milliseconds.
    pub elapsed_ms: u64,
}

/// One hit.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Hit {
    /// The caller's document id.
    pub external_id: String,
    /// The stored passage text.
    pub text: String,
    /// Fused score (RRF); the lexical score when degraded. Its meaning does not change when the
    /// hit was re-ranked — the order does.
    pub score: f64,
    /// The re-ranker's score where the stage scored this hit.
    pub rerank_score: Option<f32>,
    /// Chunk provenance, if the document is a chunk.
    pub chunk: Option<ChunkInfo>,
    /// Per-stage explanation when requested.
    pub explain: Option<HitExplain>,
}

/// Chunk provenance (the core's `ChunkInfo`, byte range flattened).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChunkInfo {
    /// External id of the parent document.
    pub parent: String,
    /// 0-based position within the parent.
    pub ordinal: u32,
    /// Byte range start within the parent text, if known.
    pub byte_start: Option<u64>,
    /// Byte range end within the parent text, if known.
    pub byte_end: Option<u64>,
}

/// Per-stage scores and 1-based ranks; `None` where a stage did not see the hit.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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

/// What the stages did for one search.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StageReport {
    /// Candidates the lexical stage returned.
    pub lexical_candidates: u32,
    /// Candidates the dense stage returned; `None` when it did not run.
    pub dense_candidates: Option<u32>,
    /// Set when the dense stage was skipped and the fused list is the lexical list.
    pub degraded: Option<Degradation>,
    /// What the re-ranker did; `None` when the stage did not run at all.
    pub rerank: Option<RerankReport>,
    /// A time limit was set but no time source was supplied (never through this FFI).
    pub time_limit_ignored: bool,
}

/// The re-rank stage's outcome.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RerankReport {
    /// Passages handed to the re-ranker.
    pub candidates: u32,
    /// Passages scored within the budget.
    pub scored: u32,
    /// Set when the stage was skipped for cause.
    pub skipped: Option<DegradeReason>,
}

/// A skipped ML stage and why.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Degradation {
    /// The stage skipped (`"dense"`).
    pub stage: String,
    /// Why.
    pub reason: DegradeReason,
}

/// Why a stage was skipped.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DegradeReason {
    /// The stage returned an error (its message).
    StageError {
        /// The engine's message.
        message: String,
    },
    /// The caller's time budget was spent.
    BudgetExceeded {
        /// Elapsed milliseconds when the check fired.
        elapsed_ms: u64,
        /// The limit in milliseconds.
        limit_ms: u64,
    },
}

/// Identity and configuration of an open index.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IndexInfo {
    /// Committed live documents.
    pub documents: u64,
    /// The pipeline's on-disk format version.
    pub format_version: u32,
    /// The embedder's fingerprint (also stored in the index).
    pub embedder_fingerprint: String,
    /// The attached re-ranker's identity, if any.
    pub reranker_model_id: Option<String>,
    /// Candidates per stage by default.
    pub candidate_depth: u32,
    /// Re-rank depth by default.
    pub rerank_depth: u32,
    /// Reciprocal rank fusion constant.
    pub rrf_k: u32,
    /// Wall time the embedder took to load, in milliseconds.
    pub embedder_load_ms: u64,
    /// Wall time the re-ranker took to load, in milliseconds, if one was loaded.
    pub reranker_load_ms: Option<u64>,
}

/// How both models' weight files are brought into memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LoadPath {
    /// Heap buffers — the safe default.
    Buffered,
    /// Read-only memory maps (ADR-0007, ADR-0009). The caller owns the precondition that no
    /// other process modifies or truncates the weight files while the index is open.
    Mmap,
}
