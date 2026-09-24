//! Wire types crossing to Swift (contract `ffi-surface.md`; data-model 007). Every type here
//! mirrors a pipeline or core type one-to-one; the FFI adds no computation.

// See the crate-root comment and ADR-0003: the uniffi derives emit `unsafe impl`.
#![allow(unsafe_code)]

/// How the re-ranked head is ordered (Feature 015) — `xtriever_pipeline::RerankMode` on the
/// wire. The engine's default is `Interpolate { alpha: 0.5 }`; `Replace` reproduces results
/// from before Feature 015 (ADR-0012).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum RerankMode {
    /// The cross-encoder's order replaces the fused order within the head.
    Replace,
    /// `(1 − alpha)·minmax(fused) + alpha·minmax(cross-encoder)` within the head, ties by fused
    /// rank; `alpha` within `[0, 1]`, refused otherwise (`Schema`).
    Interpolate {
        /// Weight of the cross-encoder term.
        alpha: f64,
    },
}

/// Per-search options — the pipeline's `SearchOptions` on the wire, plus `k`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchOptions {
    /// Hits returned; `0` yields an empty response and runs no stage.
    pub k: u32,
    /// Candidates per stage; `None` = the index's configured depth.
    #[uniffi(default = None)]
    pub depth: Option<u32>,
    /// Fused candidates handed to the re-ranker; `None` = the index's default, `Some(0)` = none.
    #[uniffi(default = None)]
    pub rerank_depth: Option<u32>,
    /// How the re-ranked head is ordered; `None` = the index's recorded mode.
    #[uniffi(default = None)]
    pub rerank_mode: Option<RerankMode>,
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
    /// The combined score the hit was ordered by under `RerankMode::Interpolate`, in `[0, 1]`;
    /// `None` under `Replace` or where the stage did not score the hit (Feature 015).
    pub rerank_combined: Option<f64>,
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
    /// Set when a sparse index's query expansion could not be built (Feature 027) and the text
    /// fields were searched alone; strict mode returns the error instead.
    pub sparse_skipped: Option<DegradeReason>,
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
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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
    /// How the re-ranked head is ordered by default — the index's recorded mode (Feature 015).
    pub rerank_mode: RerankMode,
    /// Reciprocal rank fusion constant.
    pub rrf_k: u32,
    /// The recorded dense compaction share (`None` = compact only on `merge`; Feature 024).
    pub dense_compact_dead_share: Option<f32>,
    /// Wall time the embedder took to load, in milliseconds.
    pub embedder_load_ms: u64,
    /// Wall time the re-ranker took to load, in milliseconds, if one was loaded.
    pub reranker_load_ms: Option<u64>,
    /// The sparse expansion, if the index has the option (Feature 027; its format version is
    /// then 3).
    pub sparse: Option<SparseInfo>,
}

/// How both models' weight files are brought into memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LoadPath {
    /// Heap buffers — the safe default.
    Buffered,
    /// Read-only memory maps (ADR-0007, ADR-0009) for both models' weight files **and** the
    /// dense index's committed rows (`dense/vectors.<g>.bin`, via `HybridIndex::open_mapped`;
    /// Feature 024). The caller
    /// owns the precondition that no other process modifies or truncates any of those files
    /// while the handle lives.
    Mmap,
}

// ── Feature 011: the builder on the wire (research D4) ───────────────────────────────────────

/// The kind of a schema field — `xtriever_core::FieldKind` on the wire.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum FieldKind {
    /// Full text, analyzed with the named analyzer (`"standard"`, `"standard_en"`, …).
    Text {
        /// The analyzer id the lexical stage knows; unknown ids are refused at create.
        analyzer: String,
    },
    /// Exact-match string.
    Keyword,
    /// Unsigned integer.
    U64,
    /// Signed integer.
    I64,
    /// Floating point.
    F64,
    /// Boolean.
    Bool,
    /// Timestamp in Unix milliseconds.
    DateMillis,
}

/// One schema field — `xtriever_core::FieldDef` on the wire.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FieldDef {
    /// Field name, unique within the schema.
    pub name: String,
    /// Field type.
    pub kind: FieldKind,
    /// Searchable (text) or filterable (other kinds).
    #[uniffi(default = true)]
    pub indexed: bool,
    /// Original value stored and returnable with hits.
    #[uniffi(default = false)]
    pub stored: bool,
    /// Query-time weight for text fields (1.0 = neutral).
    #[uniffi(default = 1.0)]
    pub boost: f32,
}

/// What an index is created with — the pipeline's `HybridConfig` on the wire, with its
/// defaults (candidate depth 100, RRF k 60, re-rank depth 20).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct IndexConfig {
    /// The lexical schema.
    pub fields: Vec<FieldDef>,
    /// Text fields joined by one space, in this order, into the dense passage; each must be a
    /// `Text` field of `fields` (refused otherwise).
    pub dense_fields: Vec<String>,
    /// Candidates taken from each stage per search unless the caller overrides.
    #[uniffi(default = 100)]
    pub candidate_depth: u32,
    /// Reciprocal rank fusion constant.
    #[uniffi(default = 60)]
    pub rrf_k: u32,
    /// Fused candidates re-scored by an attached re-ranker unless the caller overrides.
    #[uniffi(default = 20)]
    pub rerank_depth: u32,
    /// How the re-ranked head is ordered unless the caller overrides; `None` = the engine's
    /// default (`Interpolate { alpha: 0.5 }`), recorded in the index (Feature 015).
    #[uniffi(default = None)]
    pub rerank_mode: Option<RerankMode>,
    /// Compact the dense vectors within a commit that would leave more than this share of
    /// them dead (`0.0..=1.0`); `None` = compact only on `merge` (Feature 024).
    #[uniffi(default = None)]
    pub dense_compact_dead_share: Option<f32>,
    /// Sparse lexical expansion (Feature 027): `None` (the default) changes nothing; set, the
    /// index is created sparse with the encoder at `encoder_dir` (build host only) attached, so
    /// **the handle `create` returns** can `add` and `add_embedded` (each passage is expanded).
    /// A sparse index opened later needs nothing to be searched, but cannot be added to: the
    /// encoder is not part of the index, and `add` / `add_embedded` there are `Model` errors.
    /// Build a sparse index in one go, from the handle `create` returned. **Use it for
    /// corpora whose questions are worded unlike their answers (FiQA-shaped: no titles), and
    /// search it with the re-ranker**: re-ranked, FiQA gained +0.017 nDCG@10 and SciFact and
    /// NFCorpus held within 0.005; without re-ranking SciFact and NFCorpus lost 0.006–0.007
    /// (ADR-0017).
    #[uniffi(default = None)]
    pub sparse: Option<SparseOptionConfig>,
}

/// How a sparse index writes and scores its expansions, and where its encoder is (Feature 027).
/// Pair the option with the re-ranker, and use it for FiQA-shaped corpora (ADR-0017).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SparseOptionConfig {
    /// The pinned sparse document encoder's directory
    /// (`opensearch-neural-sparse-encoding-doc-v3-distill`).
    pub encoder_dir: String,
    /// A weight becomes `round(weight × scale)` occurrences of its term; `1..=1000`. `None` =
    /// the engine's default (`SparseOption::default()`, 10) — taken from the engine, never
    /// restated here.
    #[uniffi(default = None)]
    pub scale: Option<u32>,
    /// The `_sparse` field's boost; finite and above zero. `None` = the engine's default (1.0).
    #[uniffi(default = None)]
    pub boost: Option<f32>,
}

/// What a sparse index records about its expansion (Feature 027).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SparseInfo {
    /// As created.
    pub scale: u32,
    /// As created.
    pub boost: f32,
    /// The encoder's identity.
    pub encoder: String,
}

/// A field's value — `xtriever_core::Value` on the wire. The kind must match the field's.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum FieldValue {
    /// Analyzed full text.
    Text(String),
    /// Exact-match string.
    Keyword(String),
    /// Unsigned integer.
    U64(u64),
    /// Signed integer.
    I64(i64),
    /// Floating point.
    F64(f64),
    /// Boolean.
    Bool(bool),
    /// Timestamp in Unix milliseconds.
    DateMillis(i64),
}

/// A document to add — the pipeline's `SourceDocument` on the wire.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Document {
    /// The caller's id; a known id replaces, an empty one is refused.
    pub external_id: String,
    /// Field values by field name.
    pub fields: std::collections::HashMap<String, FieldValue>,
    /// Chunk provenance, if the document is a chunk of a larger one.
    #[uniffi(default = None)]
    pub chunk: Option<ChunkInfo>,
}
