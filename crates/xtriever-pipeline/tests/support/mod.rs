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
    let index = build_from_fixture_with(dir, &h, fixture_config(&h));
    (h, index)
}

/// As [`build_from_fixture`], with the caller's configuration and fixture (Feature 024).
pub fn build_from_fixture_with(dir: &Path, h: &Hybrid, config: HybridConfig) -> HybridIndex {
    let mut index = HybridIndex::create(dir, config, fixture_embedder(h)).expect("create");
    let docs: Vec<SourceDocument> = h.documents.iter().map(FixtureDoc::source).collect();
    index.add(&docs).expect("add");
    index.commit().expect("commit");
    index
}

/// The fixture's table embedder, boxed for `HybridIndex`.
pub fn fixture_embedder(h: &Hybrid) -> Box<dyn xtriever_core::Embedder> {
    Box::new(TableEmbedder::from_fixture(h))
}

/// A query's fused hits as `(external id, score bits)` — the "same result" snapshot the merge
/// tests compare (`k = 10`, default options).
pub fn fused_bits(index: &HybridIndex, text: &str) -> Vec<(String, u64)> {
    index
        .search(text, None, 10, &xtriever_pipeline::SearchOptions::default())
        .expect("search")
        .hits
        .iter()
        .map(|h| (h.external_id.clone(), h.score.to_bits()))
        .collect()
}

/// The dense stage's committed state, read through a read-only handle that alters nothing in
/// a directory a `HybridIndex` may still hold (the stage's own `stats()`, not a hand-parsed
/// manifest).
pub fn dense_stats(dir: &Path) -> xtriever_dense::DenseStats {
    FlatIndex::open_read_only(&dir.join("dense"))
        .expect("open dense read-only")
        .stats()
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

// ── stub re-rankers (Feature 006, research D13) ─────────────────────────────────────────────

use xtriever_core::{Budget, DocId, Passage, Reranker};
use xtriever_pipeline::SearchOptions;

/// Scores by internal id from a table; `None` after `limit` passages; records every `Budget`
/// it receives. Unknown ids are an error so a test that forgets an id fails loudly.
pub struct TableReranker {
    pub scores: BTreeMap<u32, f32>,
    pub limit: Option<usize>,
    pub calls: Arc<Mutex<Vec<Budget>>>,
}

impl TableReranker {
    /// A score for every fixture document: `score(id) = f(id)`.
    pub fn from_fn(h: &Hybrid, f: impl Fn(u32) -> f32) -> Self {
        let scores = (0..h.documents.len() as u32).map(|i| (i, f(i))).collect();
        Self {
            scores,
            limit: None,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn calls(&self) -> Arc<Mutex<Vec<Budget>>> {
        Arc::clone(&self.calls)
    }
}

impl Reranker for TableReranker {
    fn model_id(&self) -> &str {
        "table-reranker"
    }
    fn rerank(
        &self,
        _query: &str,
        passages: &[Passage<'_>],
        budget: &Budget,
    ) -> Result<Vec<Option<f32>>> {
        self.calls.lock().unwrap().push(*budget);
        passages
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if self.limit.is_some_and(|n| i >= n) {
                    return Ok(None);
                }
                self.scores
                    .get(&p.id.0)
                    .copied()
                    .map(Some)
                    .ok_or_else(|| Error::Model {
                        model: "table-reranker".into(),
                        message: format!("no score for id {}", p.id),
                    })
            })
            .collect()
    }
}

/// Always fails.
pub struct FailingReranker;

impl Reranker for FailingReranker {
    fn model_id(&self) -> &str {
        "failing-reranker"
    }
    fn rerank(&self, _: &str, _: &[Passage<'_>], _: &Budget) -> Result<Vec<Option<f32>>> {
        Err(Error::Model {
            model: "failing-reranker".into(),
            message: "stub rerank failure".into(),
        })
    }
}

/// Returns one entry too few — a defect.
pub struct WrongLengthReranker;

impl Reranker for WrongLengthReranker {
    fn model_id(&self) -> &str {
        "wrong-length-reranker"
    }
    fn rerank(&self, _: &str, p: &[Passage<'_>], _: &Budget) -> Result<Vec<Option<f32>>> {
        Ok(vec![Some(1.0); p.len().saturating_sub(1)])
    }
}

/// Returns `NaN` for the first passage — a defect.
pub struct NanReranker;

impl Reranker for NanReranker {
    fn model_id(&self) -> &str {
        "nan-reranker"
    }
    fn rerank(&self, _: &str, p: &[Passage<'_>], _: &Budget) -> Result<Vec<Option<f32>>> {
        let mut out = vec![Some(0.0); p.len()];
        if let Some(first) = out.first_mut() {
            *first = Some(f32::NAN);
        }
        Ok(out)
    }
}

/// Options that re-rank the first `d` fused candidates, with explanation, under the index's
/// recorded mode (the Feature 015 default, `Interpolate { alpha: 0.5 }`).
pub fn rerank_options(d: usize) -> SearchOptions<'static> {
    SearchOptions {
        rerank_depth: Some(d),
        explain: true,
        ..SearchOptions::default()
    }
}

/// `rerank_options` under the Feature 006 rule (`RerankMode::Replace`), which the 006 tests
/// specify — selected explicitly since Feature 015 made interpolation the default.
pub fn replace_options(d: usize) -> SearchOptions<'static> {
    SearchOptions {
        rerank_mode: Some(xtriever_pipeline::RerankMode::Replace),
        ..rerank_options(d)
    }
}

/// Hits with a re-rank score, then without, as `(id, rerank_score)`.
pub fn rerank_split(hits: &[xtriever_pipeline::HybridHit]) -> (Vec<(DocId, f32)>, Vec<DocId>) {
    let scored = hits
        .iter()
        .filter_map(|h| h.rerank_score.map(|s| (h.id, s)))
        .collect();
    let unscored = hits
        .iter()
        .filter(|h| h.rerank_score.is_none())
        .map(|h| h.id)
        .collect();
    (scored, unscored)
}

// ── pipeline_order.json (Feature 006) ───────────────────────────────────────────────────────

pub fn order_fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/006")
}

#[derive(Deserialize)]
pub struct OrderCase {
    pub name: String,
    pub fused: Vec<u32>,
    pub scores: Vec<Option<f32>>,
    pub d: usize,
    pub k: usize,
    pub expected: Vec<(u32, Option<f32>)>,
}

/// Feature 015: an interpolating-rule case (`interpolate_cases`).
#[derive(Deserialize)]
pub struct InterpolateCase {
    pub name: String,
    pub fused: Vec<u32>,
    pub fused_scores: Vec<f64>,
    pub scores: Vec<Option<f32>>,
    pub d: usize,
    pub k: usize,
    pub alpha: f64,
    pub expected: Vec<(u32, Option<f64>)>,
}

#[derive(Deserialize)]
pub struct OrderGoldens {
    pub cases: Vec<OrderCase>,
    #[serde(default)]
    pub interpolate_cases: Vec<InterpolateCase>,
}

pub fn order_goldens() -> OrderGoldens {
    let path = order_fixtures_dir().join("pipeline_order.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}
