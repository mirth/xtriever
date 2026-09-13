//! Build the Swift parity fixture: a hybrid index over the 005 fixture documents with the real
//! models, and the goldens the Swift tests compare against — minted by the FFI itself, so
//! equality on the Swift side is the boundary's correctness (research D8).
//!
//! ```sh
//! cargo run --release -p xtriever-ffi --example fixture_index -- swift/Xtriever/Tests/Fixtures
//! cargo run --release -p xtriever-ffi --example fixture_index -- --scifact <index_dir> <queries.json> <out.json>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;
use xtriever_core::{ChunkInfo, FieldName, Schema, Value};
use xtriever_dense::MiniLmEmbedder;
use xtriever_ffi::{IndexHandle, LoadPath, SearchOptions, SearchResponse};
use xtriever_pipeline::{HybridConfig, HybridIndex, SourceDocument};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn embedder_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_MODEL_DIR").map_or_else(
        || repo_root().join("reference/models/all-MiniLM-L6-v2"),
        PathBuf::from,
    )
}

fn reranker_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_RERANK_MODEL_DIR").map_or_else(
        || repo_root().join("reference/models/ms-marco-MiniLM-L-6-v2"),
        PathBuf::from,
    )
}

#[derive(Deserialize)]
struct FixtureDoc {
    external_id: String,
    fields: BTreeMap<FieldName, Value>,
    chunk: Option<ChunkInfo>,
}

#[derive(Deserialize)]
struct FixtureQuery {
    id: String,
    text: String,
}

#[derive(Deserialize)]
struct Hybrid {
    dense_fields: Vec<FieldName>,
    schema: Schema,
    documents: Vec<FixtureDoc>,
    queries: Vec<FixtureQuery>,
}

fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// The goldens' view of a response: ids and score bits, never floats.
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

fn open(index_dir: &Path, with_reranker: bool) -> anyhow::Result<std::sync::Arc<IndexHandle>> {
    Ok(IndexHandle::open(
        index_dir.to_string_lossy().into_owned(),
        embedder_dir().to_string_lossy().into_owned(),
        with_reranker.then(|| reranker_dir().to_string_lossy().into_owned()),
        LoadPath::Buffered,
    )?)
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

fn build_fixture(out_dir: &Path) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(repo_root().join("reference/fixtures/005/hybrid.json"))?;
    let h: Hybrid = serde_json::from_str(&text)?;
    let index_dir = out_dir.join("index");
    if index_dir.exists() {
        std::fs::remove_dir_all(&index_dir)?;
    }
    let embedder = MiniLmEmbedder::load(&embedder_dir(), xtriever_dense::LoadPath::Buffered)
        .context("loading the embedder")?;
    let mut index = HybridIndex::create(
        &index_dir,
        HybridConfig::new(h.schema.clone(), h.dense_fields.clone()),
        Box::new(embedder),
    )?;
    let docs: Vec<SourceDocument> = h
        .documents
        .iter()
        .map(|d| SourceDocument {
            external_id: d.external_id.clone(),
            fields: d.fields.clone(),
            chunk: d.chunk.clone(),
        })
        .collect();
    index.add(&docs)?;
    index.commit()?;
    drop(index);

    let with = open(&index_dir, true)?;
    let without = open(&index_dir, false)?;
    let mut queries = Vec::new();
    for q in &h.queries {
        queries.push(serde_json::json!({
            "id": q.id,
            "text": q.text,
            "k": 10,
            "rerank_depth": 5,
            "with_reranker": golden_response(&with.search(q.text.clone(), options(10, 5))?),
            "without_reranker": golden_response(&without.search(q.text.clone(), options(10, 5))?),
        }));
    }
    let expected = serde_json::json!({
        "generated_by": format!("xtriever-ffi examples/fixture_index.rs @ {}", git_head()),
        "info": info_json(&with),
        "queries": queries,
    });
    std::fs::write(
        out_dir.join("expected.json"),
        serde_json::to_string_pretty(&expected)? + "\n",
    )?;
    eprintln!(
        "fixture index: {} documents at {}; goldens for {} queries at {}",
        h.documents.len(),
        index_dir.display(),
        h.queries.len(),
        out_dir.join("expected.json").display()
    );
    Ok(())
}

#[derive(Deserialize)]
struct MeasurementQuery {
    id: String,
    text: String,
}

/// Host-side truth for the device parity check (research D9): the same queries at depths 0, 5
/// and 20 through the same FFI on the Mac.
fn scifact_truth(index_dir: &Path, queries: &Path, out: &Path) -> anyhow::Result<()> {
    let qs: Vec<MeasurementQuery> = serde_json::from_str(&std::fs::read_to_string(queries)?)?;
    let index = open(index_dir, true)?;
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
    }
    let truth = serde_json::json!({
        "generated_by": format!("xtriever-ffi examples/fixture_index.rs --scifact @ {}", git_head()),
        "info": info_json(&index),
        "queries": entries,
    });
    std::fs::write(out, serde_json::to_string_pretty(&truth)? + "\n")?;
    eprintln!(
        "scifact truth: {} queries × 3 depths at {}",
        qs.len(),
        out.display()
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [out] => build_fixture(Path::new(out)),
        [flag, index_dir, queries, out] if flag == "--scifact" => {
            scifact_truth(Path::new(index_dir), Path::new(queries), Path::new(out))
        }
        _ => bail!(
            "usage: fixture_index <out_dir> | fixture_index --scifact <index_dir> <queries.json> <out.json>"
        ),
    }
}
