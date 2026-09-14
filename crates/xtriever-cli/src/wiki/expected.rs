//! `wiki expected` — host-side goldens for the device parity check, in exactly the shape
//! `xtriever-ffi/examples/fixture_index.rs --scifact` writes (the device decoder reads both),
//! produced through the same FFI surface the device uses.

use std::path::Path;

use anyhow::Context;
use xtriever_ffi::{IndexHandle, LoadPath, SearchOptions, SearchResponse};

use super::ExpectedArgs;

#[derive(serde::Deserialize)]
struct MeasurementQuery {
    id: String,
    text: String,
}

/// Mirrors `fixture_index.rs::golden_response` — keep the two in step.
fn golden_response(r: &SearchResponse) -> serde_json::Value {
    let hits: Vec<serde_json::Value> = r
        .hits
        .iter()
        .map(|h| {
            let e = h.explain.as_ref();
            serde_json::json!({
                "external_id": h.external_id,
                "score_bits": format!("{:016x}", h.score.to_bits()),
                "rerank_score_bits": h.rerank_score.map(|s| format!("{:08x}", s.to_bits())),
                "rerank_rank": e.and_then(|e| e.rerank_rank),
                "bm25_score_bits": e.and_then(|e| e.bm25_score).map(|s| format!("{:08x}", s.to_bits())),
                "dense_score_bits": e.and_then(|e| e.dense_score).map(|s| format!("{:08x}", s.to_bits())),
            })
        })
        .collect();
    serde_json::json!({
        "hits": hits,
        "stages": {
            "lexical_candidates": r.stages.lexical_candidates,
            "dense_candidates": r.stages.dense_candidates,
            "degraded": r.stages.degraded.is_some(),
            "rerank": r.stages.rerank.as_ref().map(|rr| serde_json::json!({ "candidates": rr.candidates, "scored": rr.scored })),
        },
    })
}

fn options(k: u32, rerank_depth: u32) -> SearchOptions {
    SearchOptions {
        k,
        depth: None,
        rerank_depth: Some(rerank_depth),
        max_time_ms: None,
        max_items: None,
        strict: false,
        explain: true,
    }
}

fn info_json(index: &IndexHandle) -> serde_json::Value {
    let i = index.info();
    serde_json::json!({
        "documents": i.documents,
        "format_version": i.format_version,
        "embedder_fingerprint": i.embedder_fingerprint,
        "reranker_model_id": i.reranker_model_id,
        "candidate_depth": i.candidate_depth,
        "rerank_depth": i.rerank_depth,
        "rrf_k": i.rrf_k,
    })
}

/// Write the goldens for `queries` at depths 0 / 5 / 20, `k = 10`, explained.
///
/// # Errors
///
/// Open/search failures, I/O.
pub fn write_expected(
    index_dir: &Path,
    embedder_dir: &Path,
    reranker_dir: &Path,
    queries: &Path,
    out: &Path,
) -> anyhow::Result<()> {
    let qs: Vec<MeasurementQuery> = serde_json::from_str(
        &std::fs::read_to_string(queries)
            .with_context(|| format!("reading {}", queries.display()))?,
    )?;
    let index = IndexHandle::open(
        index_dir.to_string_lossy().into_owned(),
        embedder_dir.to_string_lossy().into_owned(),
        Some(reranker_dir.to_string_lossy().into_owned()),
        LoadPath::Mmap,
    )?;
    let mut entries = Vec::new();
    for q in &qs {
        let mut per_depth = serde_json::Map::new();
        for depth in [0u32, 5, 20] {
            per_depth.insert(
                depth.to_string(),
                golden_response(&index.search(q.text.clone(), options(10, depth))?),
            );
        }
        entries.push(serde_json::json!({ "id": q.id, "text": q.text, "depths": per_depth }));
        eprintln!("  {} done", q.id);
    }
    let truth = serde_json::json!({
        "generated_by": "xtriever wiki expected (Feature 008)",
        "info": info_json(&index),
        "queries": entries,
    });
    std::fs::write(out, serde_json::to_string_pretty(&truth)? + "\n")
        .with_context(|| format!("writing {}", out.display()))?;
    eprintln!(
        "wiki expected: {} queries × 3 depths at {}",
        qs.len(),
        out.display()
    );
    Ok(())
}

/// Run the command.
///
/// # Errors
///
/// As [`write_expected`].
pub fn run(args: &ExpectedArgs) -> anyhow::Result<()> {
    write_expected(
        &args.index,
        &args.embedder_dir,
        &args.reranker_dir,
        &args.queries,
        &args.out,
    )
}
