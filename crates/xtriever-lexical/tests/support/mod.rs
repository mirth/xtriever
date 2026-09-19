//! Shared helpers for the Feature 002 acceptance suite: fixture loading and index construction.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tempfile::TempDir;
use xtriever_core::{DocId, Document, Filter, Hit, LexicalIndex, LexicalQuery, Schema};
use xtriever_lexical::TantivyIndex;

/// `reference/fixtures/002/`, resolved from the crate manifest so it works from any cwd.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/002")
}

pub fn load_json<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = fixtures_dir().join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[derive(Deserialize)]
pub struct Corpus {
    pub seed: u64,
    pub documents: Vec<Document>,
}

#[derive(Deserialize, Clone)]
pub struct QueryEntry {
    pub name: String,
    pub query: LexicalQuery,
    pub filter: Option<Filter>,
    pub k: usize,
    pub oracle: String,
    #[serde(default)]
    pub tie_ok: bool,
    pub expected: Option<Vec<Hit>>,
}

#[derive(Deserialize)]
pub struct Queries {
    pub score_rel_tol: f64,
    pub queries: Vec<QueryEntry>,
}

#[derive(Deserialize)]
pub struct FilterEntry {
    pub name: String,
    pub filter: Filter,
    pub expected_ids: Vec<u32>,
}

#[derive(Deserialize)]
pub struct Filters {
    pub filters: Vec<FilterEntry>,
}

#[derive(Deserialize)]
pub struct TermEntry {
    pub field: String,
    pub term: String,
    pub doc_freq: Option<u64>,
    pub total_term_freq: Option<u64>,
}

#[derive(Deserialize)]
pub struct Stats {
    pub num_docs: u64,
    pub avg_field_len: BTreeMap<String, f32>,
    pub terms: Vec<TermEntry>,
}

#[derive(Deserialize)]
pub struct PresentIn {
    pub query: LexicalQuery,
    pub expected_ids: Vec<u32>,
}

#[derive(Deserialize)]
pub struct Replace {
    pub id: u32,
    pub new_document: Document,
    pub absent_from: String,
    pub present_in: PresentIn,
}

#[derive(Deserialize)]
pub struct Delete {
    pub ids: Vec<u32>,
    pub expected_num_docs: u64,
    pub expected_term_stats: Vec<TermEntry>,
    pub expected_avg_field_len: BTreeMap<String, f32>,
}

#[derive(Deserialize)]
pub struct HistoryPair {
    pub extra_documents: Vec<Document>,
    pub delete_ids: Vec<u32>,
    pub divergence_query: String,
}

#[derive(Deserialize)]
pub struct Mutations {
    pub replace: Replace,
    pub delete: Delete,
    pub history_pair: HistoryPair,
}

pub fn fixture_schema() -> Schema {
    load_json("schema.json")
}

pub fn corpus() -> Corpus {
    load_json("corpus.json")
}

pub fn queries() -> Queries {
    load_json("queries.json")
}

pub fn query(name: &str) -> QueryEntry {
    queries()
        .queries
        .into_iter()
        .find(|q| q.name == name)
        .unwrap_or_else(|| panic!("no query golden named {name}"))
}

/// A `TantivyIndex` in a temp directory that lives as long as the value.
pub struct TestIndex {
    pub dir: TempDir,
    pub index: TantivyIndex,
}

impl TestIndex {
    pub fn create(schema: Schema) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let index = TantivyIndex::create(&dir.path().join("idx"), schema).expect("create index");
        Self { dir, index }
    }

    pub fn create_fixture() -> Self {
        Self::create(fixture_schema())
    }

    pub fn path(&self) -> PathBuf {
        self.dir.path().join("idx")
    }

    /// Reopen the same directory through `TantivyIndex::open`, dropping the current handle first.
    pub fn reopen(self) -> Self {
        let TestIndex { dir, index } = self;
        drop(index);
        let index = TantivyIndex::open(&dir.path().join("idx")).expect("open index");
        Self { dir, index }
    }
}

/// Add `docs` in `batches` roughly equal batches, committing after each.
pub fn index_in_batches(index: &mut TantivyIndex, docs: &[Document], batches: usize) {
    let per = docs.len().div_ceil(batches.max(1));
    for chunk in docs.chunks(per.max(1)) {
        index.add(chunk).expect("add");
        index.commit().expect("commit");
    }
}

/// The fixture corpus indexed in one batch — the layout the goldens were minted from.
pub fn fixture_index() -> TestIndex {
    let mut t = TestIndex::create_fixture();
    index_in_batches(&mut t.index, &corpus().documents, 1);
    t
}

/// The ids of `(id, score bits)` pairs.
pub fn ids_of(pairs: &[(u32, u32)]) -> Vec<u32> {
    pairs.iter().map(|p| p.0).collect()
}

pub fn ids(hits: &[Hit]) -> Vec<u32> {
    hits.iter().map(|h| h.id.0).collect()
}

/// Exact comparison: order, ids and scores bit-for-bit (spec Assumptions: own goldens are exact).
pub fn assert_hits_exact(name: &str, actual: &[Hit], expected: &[Hit]) {
    assert_eq!(ids(actual), ids(expected), "{name}: ids/order differ");
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            a.score.to_bits() == e.score.to_bits(),
            "{name}: score for id {} differs: {} vs golden {}",
            a.id,
            a.score,
            e.score
        );
    }
}

pub fn doc_id(n: u32) -> DocId {
    DocId(n)
}
