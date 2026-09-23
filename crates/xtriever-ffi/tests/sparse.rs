//! Feature 027 (User Story 4): a sparse index on the wire. `IndexHandle::create` with
//! `IndexConfig.sparse` builds one and can add to it at once; `info()` reports the option and
//! the index's own format version; `IndexHandle::open` — no new argument, no encoder — searches
//! it exactly as the pipeline's own handle does. Model-backed: the embedder, the re-ranker and
//! the sparse encoder (`#[ignore]`, release):
//!
//!     cargo nextest run -p xtriever-ffi --release --run-ignored all -E 'binary(sparse)'
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;

use support::{config, document};
use xtriever_dense::model::SPARSE_IDENTITY;
use xtriever_dense::sparse::SparseEncoder;
use xtriever_ffi::{IndexHandle, LoadPath, SearchOptions, SparseInfo, SparseOptionConfig};
use xtriever_pipeline::{
    FORMAT_VERSION, HybridConfig, HybridIndex, SPARSE_FORMAT_VERSION, SourceDocument, SparseOption,
};

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn sparse_option() -> SparseOptionConfig {
    SparseOptionConfig {
        encoder_dir: s(&support::sparse_encoder_dir()),
        scale: 10,
        boost: 1.0,
    }
}

/// The fixture built sparse through the wire, committed, with the re-ranker.
fn build_wire(dir: &std::path::Path, h: &support::Hybrid) -> Arc<IndexHandle> {
    let mut cfg = config(h);
    cfg.sparse = Some(sparse_option());
    let handle = IndexHandle::create(
        s(dir),
        cfg,
        s(&support::embedder_dir()),
        Some(s(&support::reranker_dir())),
        LoadPath::Buffered,
    )
    .unwrap();
    handle
        .add(h.documents.iter().map(document).collect())
        .unwrap();
    handle.commit().unwrap();
    handle
}

/// The same documents through the pipeline's own `create_sparse`, re-ranker attached.
fn build_pipeline(dir: &std::path::Path, h: &support::Hybrid) -> HybridIndex {
    let embedder = xtriever_dense::MiniLmEmbedder::load(
        &support::embedder_dir(),
        xtriever_dense::LoadPath::Buffered,
    )
    .unwrap();
    let encoder = SparseEncoder::load(
        &support::sparse_encoder_dir(),
        xtriever_dense::LoadPath::Buffered,
    )
    .unwrap();
    let config = HybridConfig {
        sparse: Some(SparseOption::default()),
        ..HybridConfig::new(h.schema.clone(), h.dense_fields.clone())
    };
    let mut index = HybridIndex::create_sparse(dir, config, Box::new(embedder), encoder).unwrap();
    let docs: Vec<SourceDocument> = h
        .documents
        .iter()
        .map(|d| SourceDocument {
            external_id: d.external_id.clone(),
            fields: d.fields.clone(),
            chunk: d.chunk.clone(),
        })
        .collect();
    index.add(&docs).unwrap();
    index.commit().unwrap();
    let reranker = xtriever_rerank::MiniLmCrossEncoder::load(
        &support::reranker_dir(),
        xtriever_rerank::LoadPath::Buffered,
    )
    .unwrap();
    index.set_reranker(Some(Box::new(reranker)));
    index
}

fn wire_hits(handle: &IndexHandle, text: &str) -> Vec<(String, u64, Option<u32>)> {
    let opts = SearchOptions {
        k: 10,
        depth: None,
        rerank_depth: None,
        rerank_mode: None,
        max_time_ms: None,
        max_items: None,
        strict: true,
        explain: false,
    };
    handle
        .search(text.to_owned(), opts)
        .unwrap()
        .hits
        .into_iter()
        .map(|h| {
            (
                h.external_id,
                h.score.to_bits(),
                h.rerank_score.map(f32::to_bits),
            )
        })
        .collect()
}

fn pipeline_hits(index: &HybridIndex, text: &str) -> Vec<(String, u64, Option<u32>)> {
    let opts = xtriever_pipeline::SearchOptions {
        strict: true,
        ..xtriever_pipeline::SearchOptions::default()
    };
    index
        .search(text, None, 10, &opts)
        .unwrap()
        .hits
        .into_iter()
        .map(|h| {
            (
                h.external_id,
                h.score.to_bits(),
                h.rerank_score.map(f32::to_bits),
            )
        })
        .collect()
}

#[test]
#[ignore = "needs the sparse encoder and both models"]
fn a_sparse_index_built_on_the_wire_reports_the_option() {
    let h = support::fixture_docs();
    let tmp = tempfile::tempdir().unwrap();
    let info = build_wire(&tmp.path().join("idx"), &h).info();
    assert_eq!(info.format_version, SPARSE_FORMAT_VERSION);
    assert_eq!(
        info.sparse,
        Some(SparseInfo {
            scale: 10,
            boost: 1.0,
            encoder: SPARSE_IDENTITY.to_owned(),
        })
    );
}

/// Built on the wire or by the pipeline, the index answers the same; opened on the wire with
/// no encoder, it answers the same again.
#[test]
#[ignore = "needs the sparse encoder and both models"]
fn the_wire_searches_a_sparse_index_as_the_pipeline_does() {
    let h = support::fixture_docs();
    let tmp = tempfile::tempdir().unwrap();
    let wire_dir = tmp.path().join("wire");
    let pipeline = build_pipeline(&tmp.path().join("pipeline"), &h);
    drop(build_wire(&wire_dir, &h));
    let reopened = IndexHandle::open(
        s(&wire_dir),
        s(&support::embedder_dir()),
        Some(s(&support::reranker_dir())),
        LoadPath::Buffered,
    )
    .unwrap();
    assert_eq!(reopened.info().format_version, SPARSE_FORMAT_VERSION);
    for q in &h.queries {
        assert_eq!(
            wire_hits(&reopened, &q.text),
            pipeline_hits(&pipeline, &q.text),
            "{}: {:?}",
            q.id,
            q.text
        );
    }
}

/// An index without the option is what it was: format version 2, no sparse record.
#[test]
#[ignore = "needs both models"]
fn an_index_without_the_option_reports_none() {
    let h = support::fixture_docs();
    let tmp = tempfile::tempdir().unwrap();
    let handle = IndexHandle::create(
        s(&tmp.path().join("idx")),
        config(&h),
        s(&support::embedder_dir()),
        None,
        LoadPath::Buffered,
    )
    .unwrap();
    let info = handle.info();
    assert_eq!(info.format_version, FORMAT_VERSION);
    assert_eq!(info.sparse, None);
}
