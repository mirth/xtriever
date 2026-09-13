//! The hand-written logic behind the boundary: open, info, search, and the conversions
//! between wire types and pipeline types (research D1, D3, D5). No generated code here, so
//! `unsafe` is denied again (ADR-0003).
//!
//! The clock lives here on purpose: the pipeline reads none (Principle III) and takes an
//! elapsed-time source from its caller; `xtriever-ffi` is that caller and is a leaf crate.
#![deny(unsafe_code)]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use xtriever_dense::MiniLmEmbedder;
use xtriever_pipeline::{HybridIndex, OpenOptions, Response};
use xtriever_rerank::MiniLmCrossEncoder;

use crate::ffi::error::{XtrieverError, poisoned};
use crate::ffi::types::{
    ChunkInfo, Degradation, DegradeReason, Hit, HitExplain, IndexInfo, LoadPath, RerankReport,
    SearchOptions, SearchResponse, StageReport,
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

/// Open read-only: the embedder, then the pipeline directory, then the optional re-ranker.
/// Read-only means the directory too: the pipeline is opened with `read_only: true`, so the
/// lexical backend takes no lock file and an index inside a read-only app bundle opens in place
/// (Feature 008 D11; resolves 007 F-001).
pub(crate) fn open(
    index_dir: &str,
    embedder_dir: &str,
    reranker_dir: Option<&str>,
    load_path: LoadPath,
) -> Result<Inner, XtrieverError> {
    let t = Instant::now();
    let embedder = MiniLmEmbedder::load(embedder_dir.as_ref(), load_path.into())?;
    let embedder_load = t.elapsed();

    let dir = std::path::Path::new(index_dir);
    let mut index = HybridIndex::open_with(
        dir,
        Box::new(embedder),
        OpenOptions {
            mapped: matches!(load_path, LoadPath::Mmap),
            read_only: true,
        },
    )?;

    let reranker_load = match reranker_dir {
        Some(rdir) => {
            let t = Instant::now();
            let reranker = MiniLmCrossEncoder::load(rdir.as_ref(), load_path.into())?;
            index.set_reranker(Some(Box::new(reranker)));
            Some(t.elapsed())
        }
        None => None,
    };
    Ok(Inner {
        index: Mutex::new(index),
        embedder_load,
        reranker_load,
    })
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
        format_version: xtriever_pipeline::FORMAT_VERSION,
        embedder_fingerprint: guard.embedder().fingerprint().to_owned(),
        reranker_model_id: guard.reranker().map(|r| r.model_id().to_owned()),
        candidate_depth: count(config.candidate_depth),
        rerank_depth: count(config.rerank_depth),
        rrf_k: config.rrf_k,
        embedder_load_ms: ms(inner.embedder_load),
        reranker_load_ms: inner.reranker_load.map(ms),
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
        },
        elapsed_ms,
    }
}
