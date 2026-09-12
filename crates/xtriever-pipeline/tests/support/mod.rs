//! Shared helpers for the Feature 005 acceptance suite: fixtures, the table-driven stub embedder,
//! the failing embedder, and direct access to the two stage indexes for the composition check.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use xtriever_core::{
    ChunkInfo, Embedder, Error, FieldName, Filter, Metric, Result, Schema, TextKind, Value, Vector,
};
use xtriever_dense::FlatIndex;
use xtriever_lexical::TantivyIndex;
use xtriever_pipeline::{HybridConfig, HybridIndex, SourceDocument};

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/005")
}

pub fn model_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_MODEL_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/models/all-MiniLM-L6-v2"),
        PathBuf::from,
    )
}

pub fn load_json<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = fixtures_dir().join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

// ── fusion.json ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct Expected {
    pub id: u32,
    pub score: f64,
}

#[derive(Deserialize)]
pub struct FusionCase {
    pub id: String,
    pub lexical: Vec<u32>,
    pub dense: Vec<u32>,
    pub k: usize,
    pub expected: Vec<Expected>,
}

#[derive(Deserialize)]
pub struct FusionGoldens {
    pub rrf_k: u32,
    pub score_abs_tol: f64,
    pub cases: Vec<FusionCase>,
}

pub fn fusion() -> FusionGoldens {
    load_json("fusion.json")
}

// ── hybrid.json ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Clone)]
pub struct FixtureDoc {
    pub external_id: String,
    pub fields: BTreeMap<FieldName, Value>,
    pub chunk: Option<ChunkInfo>,
    pub passage: String,
    pub vector: Vec<f32>,
}

#[derive(Deserialize)]
pub struct DenseExpected {
    pub id: String,
    pub score: f64,
}

#[derive(Deserialize)]
pub struct FixtureQuery {
    pub id: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub filter: Option<Filter>,
    pub expected_dense: Vec<DenseExpected>,
}

#[derive(Deserialize)]
pub struct Hybrid {
    pub dim: usize,
    pub fingerprint: String,
    pub score_abs_tol: f64,
    pub dense_fields: Vec<FieldName>,
    pub schema: Schema,
    pub documents: Vec<FixtureDoc>,
    pub queries: Vec<FixtureQuery>,
}

pub fn hybrid() -> Hybrid {
    load_json("hybrid.json")
}

impl FixtureDoc {
    pub fn source(&self) -> SourceDocument {
        SourceDocument {
            external_id: self.external_id.clone(),
            fields: self.fields.clone(),
            chunk: self.chunk.clone(),
        }
    }
}

// ── stub embedders ──────────────────────────────────────────────────────────────────────────

/// Looks every text up in the fixture (passages and query texts). Unknown text is an error, so a
/// test that forgets a fixture fails loudly. Counts calls so tests can assert a stage was skipped.
pub struct TableEmbedder {
    table: BTreeMap<String, Vec<f32>>,
    fingerprint: String,
    pub calls: Arc<Mutex<usize>>,
}

impl TableEmbedder {
    pub fn from_fixture(h: &Hybrid) -> Self {
        let mut table = BTreeMap::new();
        for d in &h.documents {
            table.insert(d.passage.clone(), d.vector.clone());
        }
        for q in &h.queries {
            table.insert(q.text.clone(), q.vector.clone());
        }
        Self {
            table,
            fingerprint: h.fingerprint.clone(),
            calls: Arc::new(Mutex::new(0)),
        }
    }

    pub fn with_fingerprint(mut self, fp: &str) -> Self {
        self.fingerprint = fp.to_owned();
        self
    }

    /// A handle on the call counter that survives boxing the embedder.
    pub fn counter(&self) -> Arc<Mutex<usize>> {
        Arc::clone(&self.calls)
    }

    /// A table containing one extra text → vector pair.
    pub fn with(mut self, text: &str, vector: Vec<f32>) -> Self {
        self.table.insert(text.to_owned(), vector);
        self
    }
}

impl Embedder for TableEmbedder {
    fn dim(&self) -> usize {
        8
    }
    fn metric(&self) -> Metric {
        Metric::Cosine
    }
    fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    fn max_input_tokens(&self) -> Option<usize> {
        None
    }
    fn embed(&self, texts: &[&str], _: TextKind) -> Result<Vec<Vector>> {
        *self.calls.lock().unwrap() += texts.len();
        texts
            .iter()
            .map(|t| {
                self.table.get(*t).cloned().ok_or_else(|| Error::Model {
                    model: "table".into(),
                    message: format!("no vector for text {t:?}"),
                })
            })
            .collect()
    }
}

/// Always fails; same identity as the table embedder so an index built with one opens with it.
pub struct FailingEmbedder;

impl Embedder for FailingEmbedder {
    fn dim(&self) -> usize {
        8
    }
    fn metric(&self) -> Metric {
        Metric::Cosine
    }
    fn fingerprint(&self) -> &str {
        "table-fp"
    }
    fn max_input_tokens(&self) -> Option<usize> {
        None
    }
    fn embed(&self, _: &[&str], _: TextKind) -> Result<Vec<Vector>> {
        Err(Error::Model {
            model: "failing".into(),
            message: "stub failure".into(),
        })
    }
}

// ── building ────────────────────────────────────────────────────────────────────────────────

pub fn fixture_config(h: &Hybrid) -> HybridConfig {
    HybridConfig::new(h.schema.clone(), h.dense_fields.clone())
}

/// Every fixture document added in order, one commit.
pub fn build_from_fixture(dir: &Path) -> (Hybrid, HybridIndex) {
    let h = hybrid();
    let mut index = HybridIndex::create(
        dir,
        fixture_config(&h),
        Box::new(TableEmbedder::from_fixture(&h)),
    )
    .expect("create");
    let docs: Vec<SourceDocument> = h.documents.iter().map(FixtureDoc::source).collect();
    index.add(&docs).expect("add");
    index.commit().expect("commit");
    (h, index)
}

/// The two stages opened directly, bypassing the pipeline (composition check).
pub fn open_stages(dir: &Path) -> (TantivyIndex, FlatIndex) {
    (
        TantivyIndex::open(&dir.join("lexical")).expect("open lexical"),
        FlatIndex::open(&dir.join("dense")).expect("open dense"),
    )
}

pub fn ids(hits: &[xtriever_pipeline::HybridHit]) -> Vec<String> {
    hits.iter().map(|h| h.external_id.clone()).collect()
}
