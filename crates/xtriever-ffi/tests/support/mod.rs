//! Shared helpers for the Feature 007 acceptance suite: model directories, the 005 fixture
//! corpus, a fixture index built with the real embedder, bit helpers, and the fixture as wire
//! types (Feature 011's builder; shared since Feature 027).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use xtriever_core::{ChunkInfo, FieldKind as CoreKind, FieldName, Filter, Schema, Value};
use xtriever_dense::{LoadPath, MiniLmEmbedder};
use xtriever_ffi::{
    ChunkInfo as WireChunk, Document, FieldDef, FieldKind, FieldValue, IndexConfig,
};
use xtriever_pipeline::{HybridConfig, HybridIndex, SourceDocument};

/// A path as the wire's `String`.
pub fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The git-ignored embedder directory, overridable with `XTRIEVER_MODEL_DIR`.
pub fn embedder_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_MODEL_DIR").map_or_else(
        || repo_root().join("reference/models/all-MiniLM-L6-v2-q8"),
        PathBuf::from,
    )
}

/// The git-ignored re-rank model directory, overridable with `XTRIEVER_RERANK_MODEL_DIR`.
pub fn reranker_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_RERANK_MODEL_DIR").map_or_else(
        || repo_root().join("reference/models/ms-marco-MiniLM-L-6-v2-q8"),
        PathBuf::from,
    )
}

/// The git-ignored sparse encoder directory (Feature 027), overridable with
/// `XTRIEVER_SPARSE_MODEL_DIR`.
pub fn sparse_encoder_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_SPARSE_MODEL_DIR").map_or_else(
        || repo_root().join("reference/models/opensearch-neural-sparse-encoding-doc-v3-distill"),
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
    build_fixture_index_with(
        dir,
        HybridConfig::new(h.schema.clone(), h.dense_fields.clone()),
        None,
    )
}

/// As [`build_fixture_index`] with the caller's configuration and, for a sparse one, the sparse
/// encoder (Feature 027).
pub fn build_fixture_index_with(
    dir: &Path,
    config: HybridConfig,
    sparse: Option<xtriever_dense::sparse::SparseEncoder>,
) -> HybridIndex {
    let h = fixture_docs();
    let embedder = MiniLmEmbedder::load(&embedder_dir(), LoadPath::Buffered).expect("embedder");
    let mut index = match sparse {
        Some(encoder) => HybridIndex::create_sparse(dir, config, Box::new(embedder), encoder),
        None => HybridIndex::create(dir, config, Box::new(embedder)),
    }
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

/// A writable copy of a model directory for tamper tests: every file in it, whichever pinned
/// artefact it holds (Feature 026: the eight-bit directory has the GGUF, the float
/// configuration and tokenizer, and for the re-ranker the borrowed pooler).
pub fn model_copy(src: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(src).unwrap().filter_map(Result::ok) {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
        }
    }
    dir
}

// ── the fixture as wire types ────────────────────────────────────────────────────────────────

pub fn wire_kind(kind: &CoreKind) -> FieldKind {
    match kind {
        CoreKind::Text(a) => FieldKind::Text {
            analyzer: a.0.clone(),
        },
        CoreKind::Keyword => FieldKind::Keyword,
        CoreKind::U64 => FieldKind::U64,
        CoreKind::I64 => FieldKind::I64,
        CoreKind::F64 => FieldKind::F64,
        CoreKind::Bool => FieldKind::Bool,
        CoreKind::DateMillis => FieldKind::DateMillis,
    }
}

pub fn wire_value(v: &Value) -> FieldValue {
    match v {
        Value::Text(t) => FieldValue::Text(t.clone()),
        Value::Keyword(k) => FieldValue::Keyword(k.clone()),
        Value::U64(n) => FieldValue::U64(*n),
        Value::I64(n) => FieldValue::I64(*n),
        Value::F64(x) => FieldValue::F64(*x),
        Value::Bool(b) => FieldValue::Bool(*b),
        Value::DateMillis(n) => FieldValue::DateMillis(*n),
    }
}

/// The fixture's schema and dense fields as the wire config, pipeline defaults otherwise.
pub fn config(h: &Hybrid) -> IndexConfig {
    IndexConfig {
        fields: h
            .schema
            .fields
            .iter()
            .map(|f| FieldDef {
                name: f.name.to_string(),
                kind: wire_kind(&f.kind),
                indexed: f.indexed,
                stored: f.stored,
                boost: f.boost,
            })
            .collect(),
        dense_fields: h.dense_fields.iter().map(ToString::to_string).collect(),
        candidate_depth: 100,
        rrf_k: 60,
        rerank_depth: 20,
        rerank_mode: None,
        dense_compact_dead_share: None,
        sparse: None,
    }
}

pub fn document(d: &FixtureDoc) -> Document {
    Document {
        external_id: d.external_id.clone(),
        fields: d
            .fields
            .iter()
            .map(|(k, v)| (k.to_string(), wire_value(v)))
            .collect::<HashMap<_, _>>(),
        chunk: d.chunk.as_ref().map(|c| WireChunk {
            parent: c.parent.clone(),
            ordinal: c.ordinal,
            byte_start: c.byte_range.map(|r| r.0),
            byte_end: c.byte_range.map(|r| r.1),
        }),
    }
}
