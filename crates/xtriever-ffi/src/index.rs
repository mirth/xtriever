//! The hand-written logic behind the boundary: open, info, search, and the conversions
//! between wire types and pipeline types (research D1, D3, D5). No generated code here, so
//! `unsafe` is denied again (ADR-0003).
//!
//! The clock lives here on purpose: the pipeline reads none (Principle III) and takes an
//! elapsed-time source from its caller; `xtriever-ffi` is that caller and is a leaf crate.
#![deny(unsafe_code)]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use std::collections::BTreeMap;

use xtriever_core::{AnalyzerId, FieldName, Schema, Value};
use xtriever_dense::MiniLmEmbedder;
use xtriever_dense::sparse::SparseEncoder;
use xtriever_pipeline::{
    HybridConfig, HybridIndex, OpenOptions, Response, SourceDocument, SparseOption,
};
use xtriever_rerank::MiniLmCrossEncoder;

use crate::ffi::error::{XtrieverError, poisoned};
use crate::ffi::types::{
    ChunkInfo, Degradation, DegradeReason, Document, FieldDef, FieldKind, FieldValue, Hit,
    HitExplain, IndexConfig, IndexInfo, LoadPath, RerankMode, RerankReport, SearchOptions,
    SearchResponse, SparseInfo, SparseOptionConfig, StageReport,
};

/// The open index and what it cost to load.
pub(crate) struct Inner {
    /// The lock is the per-handle serialisation (spec FR-006) and makes the handle `Sync`.
    index: Mutex<HybridIndex>,
    embedder_load: Duration,
    reranker_load: Option<Duration>,
}

impl From<LoadPath> for xtriever_dense::LoadPath {
    fn from(p: LoadPath) -> Self {
        match p {
            LoadPath::Buffered => Self::Buffered,
            LoadPath::Mmap => Self::Mmap,
        }
    }
}

impl From<LoadPath> for xtriever_rerank::LoadPath {
    fn from(p: LoadPath) -> Self {
        match p {
            LoadPath::Buffered => Self::Buffered,
            LoadPath::Mmap => Self::Mmap,
        }
    }
}

/// Open an existing index: both models first (nothing is touched if one fails), then the
/// directory — writable when its lock can be taken, read-only otherwise (below).
///
/// The directory is opened **with** the lexical backend's meta lock whenever it can be taken —
/// that lock is what keeps a reader safe from a concurrent writer's garbage collection, and a
/// non-mutating handle does not make a writable directory immutable. Only when the directory
/// itself refuses the lock file (`PermissionDenied`: an app bundle, a read-only mount) is it
/// reopened `read_only`, lock-free — a directory this process cannot write is one no writer of
/// this process's rights can change under it (Feature 008 D11; resolves 007 F-001).
pub(crate) fn open(
    index_dir: &str,
    embedder_dir: &str,
    reranker_dir: Option<&str>,
    load_path: LoadPath,
) -> Result<Inner, XtrieverError> {
    let Models {
        embedder,
        embedder_load,
        reranker,
    } = load_models(embedder_dir, reranker_dir, load_path)?;

    let dir = std::path::Path::new(index_dir);
    let mapped = matches!(load_path, LoadPath::Mmap);
    let index = match HybridIndex::open_with(
        dir,
        Box::new(embedder),
        OpenOptions {
            mapped,
            read_only: false,
        },
    ) {
        Err(xtriever_core::Error::Io(e)) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // The embedder moved into the failed open; load it again (~100 ms mapped).
            let embedder = MiniLmEmbedder::load(embedder_dir.as_ref(), load_path.into())?;
            HybridIndex::open_with(
                dir,
                Box::new(embedder),
                OpenOptions {
                    mapped,
                    read_only: true,
                },
            )?
        }
        other => other?,
    };
    Ok(finish(index, embedder_load, reranker))
}

/// Create an empty index at `index_dir` (absent or empty) with `config`, the embedder and the
/// optional re-ranker loaded as `open` loads them (Feature 011 US4). Every requested model is
/// loaded **before** the directory is touched, so a wrong model path leaves nothing behind
/// and a retry with the right one succeeds (review round 1 #1). The directory is created
/// writable: the handle can `add`, `delete` and `commit` at once. With `config.sparse`
/// (Feature 027) the sparse encoder is one of those models, loaded first like the others, and
/// the index is created sparse with it attached; an omitted scale or boost is the engine's
/// default.
pub(crate) fn create(
    index_dir: &str,
    config: IndexConfig,
    embedder_dir: &str,
    reranker_dir: Option<&str>,
    load_path: LoadPath,
) -> Result<Inner, XtrieverError> {
    let Models {
        embedder,
        embedder_load,
        reranker,
    } = load_models(embedder_dir, reranker_dir, load_path)?;
    let (mut config, sparse) = pipeline_config(config);
    let dir = std::path::Path::new(index_dir);
    let index = match sparse {
        None => HybridIndex::create(dir, config, Box::new(embedder))?,
        Some(sparse) => {
            let encoder = SparseEncoder::load(sparse.encoder_dir.as_ref(), load_path.into())?;
            let defaults = SparseOption::default();
            config.sparse = Some(SparseOption {
                scale: sparse.scale.unwrap_or(defaults.scale),
                boost: sparse.boost.unwrap_or(defaults.boost),
            });
            HybridIndex::create_sparse(dir, config, Box::new(embedder), encoder)?
        }
    };
    Ok(finish(index, embedder_load, reranker))
}

/// The embedder with its load time and, if asked for, the re-ranker with its own.
struct Models {
    embedder: MiniLmEmbedder,
    embedder_load: Duration,
    reranker: Option<(MiniLmCrossEncoder, Duration)>,
}

/// Both models, timed, before any directory is opened or created — a model failure mutates
/// nothing.
fn load_models(
    embedder_dir: &str,
    reranker_dir: Option<&str>,
    load_path: LoadPath,
) -> Result<Models, XtrieverError> {
    let t = Instant::now();
    let embedder = MiniLmEmbedder::load(embedder_dir.as_ref(), load_path.into())?;
    let embedder_load = t.elapsed();
    let reranker = match reranker_dir {
        Some(rdir) => {
            let t = Instant::now();
            let reranker = MiniLmCrossEncoder::load(rdir.as_ref(), load_path.into())?;
            Some((reranker, t.elapsed()))
        }
        None => None,
    };
    Ok(Models {
        embedder,
        embedder_load,
        reranker,
    })
}

/// Attach the re-ranker, if one was loaded, and wrap the index for the boundary.
fn finish(
    mut index: HybridIndex,
    embedder_load: Duration,
    reranker: Option<(MiniLmCrossEncoder, Duration)>,
) -> Inner {
    let reranker_load = reranker.map(|(reranker, load)| {
        index.set_reranker(Some(Box::new(reranker)));
        load
    });
    Inner {
        index: Mutex::new(index),
        embedder_load,
        reranker_load,
    }
}

// ── Feature 011: the builder — wire → core/pipeline, and the write operations ────────────────

impl From<FieldKind> for xtriever_core::FieldKind {
    fn from(k: FieldKind) -> Self {
        match k {
            FieldKind::Text { analyzer } => Self::Text(AnalyzerId(analyzer)),
            FieldKind::Keyword => Self::Keyword,
            FieldKind::U64 => Self::U64,
            FieldKind::I64 => Self::I64,
            FieldKind::F64 => Self::F64,
            FieldKind::Bool => Self::Bool,
            FieldKind::DateMillis => Self::DateMillis,
        }
    }
}

impl From<FieldDef> for xtriever_core::FieldDef {
    fn from(f: FieldDef) -> Self {
        Self {
            name: FieldName::from(f.name.as_str()),
            kind: f.kind.into(),
            indexed: f.indexed,
            stored: f.stored,
            boost: f.boost,
        }
    }
}

/// The pipeline's configuration from the wire's, and the sparse part apart from it: the option
/// needs its encoder loaded, which only `create` does, so the conversion hands it back rather
/// than drop it (a private function, not a `From`, so no caller can lose it by accident).
fn pipeline_config(c: IndexConfig) -> (HybridConfig, Option<SparseOptionConfig>) {
    let to_usize = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
    let sparse = c.sparse;
    (
        HybridConfig {
            schema: Schema {
                fields: c.fields.into_iter().map(Into::into).collect(),
            },
            dense_fields: c
                .dense_fields
                .iter()
                .map(|n| FieldName::from(n.as_str()))
                .collect(),
            candidate_depth: to_usize(c.candidate_depth),
            rrf_k: c.rrf_k,
            rerank_depth: to_usize(c.rerank_depth),
            rerank_mode: c.rerank_mode.map_or_else(Default::default, Into::into),
            dense_compact_dead_share: c.dense_compact_dead_share,
            // Set by `create` from the returned sparse part, with the encoder it loads.
            sparse: None,
        },
        sparse,
    )
}

impl From<RerankMode> for xtriever_pipeline::RerankMode {
    fn from(m: RerankMode) -> Self {
        match m {
            RerankMode::Replace => Self::Replace,
            RerankMode::Interpolate { alpha } => Self::Interpolate { alpha },
        }
    }
}

impl From<xtriever_pipeline::RerankMode> for RerankMode {
    fn from(m: xtriever_pipeline::RerankMode) -> Self {
        match m {
            xtriever_pipeline::RerankMode::Replace => Self::Replace,
            xtriever_pipeline::RerankMode::Interpolate { alpha } => Self::Interpolate { alpha },
        }
    }
}

impl From<FieldValue> for Value {
    fn from(v: FieldValue) -> Self {
        match v {
            FieldValue::Text(t) => Self::Text(t),
            FieldValue::Keyword(k) => Self::Keyword(k),
            FieldValue::U64(n) => Self::U64(n),
            FieldValue::I64(n) => Self::I64(n),
            FieldValue::F64(x) => Self::F64(x),
            FieldValue::Bool(b) => Self::Bool(b),
            FieldValue::DateMillis(n) => Self::DateMillis(n),
        }
    }
}

impl From<Document> for SourceDocument {
    fn from(d: Document) -> Self {
        Self {
            external_id: d.external_id,
            fields: d
                .fields
                .into_iter()
                .map(|(k, v)| (FieldName::from(k.as_str()), Value::from(v)))
                .collect::<BTreeMap<_, _>>(),
            chunk: d.chunk.map(|c| xtriever_core::ChunkInfo {
                parent: c.parent,
                ordinal: c.ordinal,
                byte_range: c.byte_start.zip(c.byte_end),
            }),
        }
    }
}

/// Take the handle for a write; a poisoned lock is the same error as for a search.
fn write_guard(inner: &Inner) -> Result<std::sync::MutexGuard<'_, HybridIndex>, XtrieverError> {
    inner.index.lock().map_err(|_| poisoned())
}

/// Stage documents, embedded by the index's embedder (`HybridIndex::add`).
pub(crate) fn add(inner: &Inner, docs: Vec<Document>) -> Result<(), XtrieverError> {
    let docs: Vec<SourceDocument> = docs.into_iter().map(Into::into).collect();
    write_guard(inner)?.add(&docs)?;
    Ok(())
}

/// Stage documents with caller-supplied vectors (`HybridIndex::add_embedded`); one vector per
/// document, or `Schema`.
pub(crate) fn add_embedded(
    inner: &Inner,
    docs: Vec<Document>,
    vectors: Vec<Vec<f32>>,
) -> Result<(), XtrieverError> {
    if docs.len() != vectors.len() {
        return Err(XtrieverError::Schema {
            message: format!("{} documents but {} vectors", docs.len(), vectors.len()),
        });
    }
    let pairs: Vec<(SourceDocument, Vec<f32>)> =
        docs.into_iter().map(Into::into).zip(vectors).collect();
    write_guard(inner)?.add_embedded(&pairs)?;
    Ok(())
}

/// Stage deletions by external id; unknown ids are ignored (`HybridIndex::delete`).
pub(crate) fn delete(inner: &Inner, external_ids: Vec<String>) -> Result<(), XtrieverError> {
    let ids: Vec<&str> = external_ids.iter().map(String::as_str).collect();
    write_guard(inner)?.delete(&ids)?;
    Ok(())
}

/// Commit the staged changes (`HybridIndex::commit`); a no-op when nothing is staged.
pub(crate) fn commit(inner: &Inner) -> Result<(), XtrieverError> {
    write_guard(inner)?.commit()?;
    Ok(())
}

/// Commit, compact the dense vectors and merge the lexical stage into one segment
/// (`HybridIndex::merge`).
pub(crate) fn merge(inner: &Inner) -> Result<(), XtrieverError> {
    write_guard(inner)?.merge()?;
    Ok(())
}

/// Whether `external_id` is a committed live document (`HybridIndex::contains`).
pub(crate) fn contains(inner: &Inner, external_id: &str) -> bool {
    inner
        .index
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(external_id)
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Counts are far below `u32::MAX` (they are bounded by `k` and the candidate depth), but a
/// saturating conversion states that rather than relying on it.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

pub(crate) fn info(inner: &Inner) -> IndexInfo {
    // A poisoned lock still holds a readable index; `info` is infallible on the wire, so the
    // inner value is taken either way.
    let guard = inner
        .index
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let config = guard.config();
    IndexInfo {
        documents: guard.len(),
        // The index's own: 3 for a sparse index (Feature 027), 2 otherwise.
        format_version: guard.format_version(),
        embedder_fingerprint: guard.embedder().fingerprint().to_owned(),
        reranker_model_id: guard.reranker().map(|r| r.model_id().to_owned()),
        candidate_depth: count(config.candidate_depth),
        rerank_depth: count(config.rerank_depth),
        rerank_mode: config.rerank_mode.into(),
        rrf_k: config.rrf_k,
        dense_compact_dead_share: config.dense_compact_dead_share,
        embedder_load_ms: ms(inner.embedder_load),
        reranker_load_ms: inner.reranker_load.map(ms),
        sparse: guard.sparse().map(|r| SparseInfo {
            scale: r.scale,
            boost: r.boost,
            encoder: r.encoder.clone(),
        }),
    }
}

/// One search: start the clock, take the lock, run the pipeline, convert (research D3).
///
/// The clock starts before the lock: FR-007 measures the budget "from the moment the call
/// starts", so a caller that contends on the handle spends its budget while it waits. The
/// Swift wrapper's queue wait happens before this call and is deliberately not counted (see
/// the package's docs).
pub(crate) fn search(
    inner: &Inner,
    query: &str,
    options: &SearchOptions,
) -> Result<SearchResponse, XtrieverError> {
    let start = Instant::now();
    let guard = inner.index.lock().map_err(|_| poisoned())?;
    let elapsed = || start.elapsed();
    let clock: Option<&dyn Fn() -> Duration> = options.max_time_ms.map(|_| &elapsed as _);
    let opts = to_pipeline_options(options, clock);
    let k = usize::try_from(options.k).unwrap_or(usize::MAX);
    let response = guard.search(query, None, k, &opts)?;
    Ok(from_response(response, ms(start.elapsed())))
}

/// Wire options → pipeline options; `elapsed` is attached exactly when a time budget is set.
/// Public for the offline conversion test only.
#[doc(hidden)]
#[must_use]
pub fn to_pipeline_options<'a>(
    options: &SearchOptions,
    elapsed: Option<&'a dyn Fn() -> Duration>,
) -> xtriever_pipeline::SearchOptions<'a> {
    let to_usize = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
    xtriever_pipeline::SearchOptions {
        depth: options.depth.map(to_usize),
        rerank_depth: options.rerank_depth.map(to_usize),
        rerank_mode: options.rerank_mode.map(Into::into),
        strict: options.strict,
        budget: xtriever_core::Budget {
            max_time: options.max_time_ms.map(Duration::from_millis),
            max_items: options.max_items.map(to_usize),
        },
        elapsed: options.max_time_ms.and(elapsed),
        explain: options.explain,
    }
}

fn reason(r: xtriever_pipeline::DegradeReason) -> DegradeReason {
    match r {
        xtriever_pipeline::DegradeReason::StageError(message) => {
            DegradeReason::StageError { message }
        }
        xtriever_pipeline::DegradeReason::BudgetExceeded {
            elapsed_ms,
            limit_ms,
        } => DegradeReason::BudgetExceeded {
            elapsed_ms,
            limit_ms,
        },
    }
}

/// Pipeline response → wire response, field by field. Public for the offline conversion test.
#[doc(hidden)]
#[must_use]
pub fn from_response(response: Response, elapsed_ms: u64) -> SearchResponse {
    let hits = response
        .hits
        .into_iter()
        .map(|h| Hit {
            external_id: h.external_id,
            text: h.text,
            score: h.score,
            rerank_score: h.rerank_score,
            chunk: h.chunk.map(|c| ChunkInfo {
                parent: c.parent,
                ordinal: c.ordinal,
                byte_start: c.byte_range.map(|r| r.0),
                byte_end: c.byte_range.map(|r| r.1),
            }),
            explain: h.explain.map(|e| HitExplain {
                bm25_score: e.bm25_score,
                bm25_rank: e.bm25_rank,
                dense_score: e.dense_score,
                dense_rank: e.dense_rank,
                fused: e.fused,
                rerank_score: e.rerank_score,
                rerank_rank: e.rerank_rank,
                rerank_combined: e.rerank_combined,
            }),
        })
        .collect();
    let s = response.stages;
    SearchResponse {
        hits,
        stages: StageReport {
            lexical_candidates: count(s.lexical_candidates),
            dense_candidates: s.dense_candidates.map(count),
            degraded: s.degraded.map(|d| Degradation {
                stage: d.stage.to_owned(),
                reason: reason(d.reason),
            }),
            rerank: s.rerank.map(|r| RerankReport {
                candidates: count(r.candidates),
                scored: count(r.scored),
                skipped: r.skipped.map(reason),
            }),
            time_limit_ignored: s.time_limit_ignored,
            sparse_skipped: s.sparse_skipped.map(reason),
        },
        elapsed_ms,
    }
}
