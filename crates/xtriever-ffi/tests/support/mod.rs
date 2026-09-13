//! Shared helpers for the Feature 007 acceptance suite: model directories, the 005 fixture
//! corpus, a fixture index built with the real embedder, and bit helpers.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use xtriever_core::{ChunkInfo, FieldName, Filter, Schema, Value};
use xtriever_dense::{LoadPath, MiniLmEmbedder};
use xtriever_pipeline::{HybridConfig, HybridIndex, SourceDocument};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The git-ignored embedder directory, overridable with `XTRIEVER_MODEL_DIR`.
pub fn embedder_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_MODEL_DIR").map_or_else(
        || repo_root().join("reference/models/all-MiniLM-L6-v2"),
        PathBuf::from,
    )
}

/// The git-ignored re-rank model directory, overridable with `XTRIEVER_RERANK_MODEL_DIR`.
pub fn reranker_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_RERANK_MODEL_DIR").map_or_else(
        || repo_root().join("reference/models/ms-marco-MiniLM-L-6-v2"),
        PathBuf::from,
    )
}

// ── the 005 fixture corpus ──────────────────────────────────────────────────────────────────

#[derive(Deserialize, Clone)]
pub struct FixtureDoc {
    pub external_id: String,
    pub fields: BTreeMap<FieldName, Value>,
    pub chunk: Option<ChunkInfo>,
    pub passage: String,
}

#[derive(Deserialize)]
pub struct FixtureQuery {
    pub id: String,
    pub text: String,
    pub filter: Option<Filter>,
}

#[derive(Deserialize)]
pub struct Hybrid {
    pub dense_fields: Vec<FieldName>,
    pub schema: Schema,
    pub documents: Vec<FixtureDoc>,
    pub queries: Vec<FixtureQuery>,
}

pub fn fixture_docs() -> Hybrid {
    let path = repo_root().join("reference/fixtures/005/hybrid.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Every fixture document embedded with the real embedder (buffered), one commit.
pub fn build_fixture_index(dir: &Path) -> HybridIndex {
    let h = fixture_docs();
    let embedder = MiniLmEmbedder::load(&embedder_dir(), LoadPath::Buffered).expect("embedder");
    let mut index = HybridIndex::create(
        dir,
        HybridConfig::new(h.schema.clone(), h.dense_fields.clone()),
        Box::new(embedder),
    )
    .expect("create");
    let docs: Vec<SourceDocument> = h
        .documents
        .iter()
        .map(|d| SourceDocument {
            external_id: d.external_id.clone(),
            fields: d.fields.clone(),
            chunk: d.chunk.clone(),
        })
        .collect();
    index.add(&docs).expect("add");
    index.commit().expect("commit");
    index
}

pub fn bits(x: f64) -> u64 {
    x.to_bits()
}

pub fn bits32(x: f32) -> u32 {
    x.to_bits()
}

/// A writable copy of a model directory for tamper tests.
pub fn model_copy(src: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in ["config.json", "tokenizer.json", "model.safetensors"] {
        std::fs::copy(src.join(name), dir.path().join(name)).unwrap();
    }
    dir
}
