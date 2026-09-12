//! Shared helpers for the Feature 004 acceptance suite: fixture loading, the model directory,
//! float comparisons and the search-golden schema.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use serde::Deserialize;
use xtriever_core::{DocId, DocSet, Metric};

/// `reference/fixtures/004/`, resolved from the crate manifest so it works from any cwd.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/004")
}

/// The git-ignored model directory, overridable with `XTRIEVER_MODEL_DIR` (quickstart Step 1).
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

// ── embeddings.json ─────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct Tolerance {
    pub cosine_min: f64,
    pub max_abs_diff: f64,
    pub unit_norm_abs: f64,
}

#[derive(Deserialize)]
pub struct EmbeddingCase {
    pub id: String,
    pub text: String,
    pub input_ids: Vec<u32>,
    pub attention_mask: Vec<u32>,
    pub n_real_tokens: usize,
    pub vector: Vec<f32>,
}

#[derive(Deserialize)]
pub struct EmbeddingGoldens {
    pub fingerprint: String,
    pub dim: usize,
    pub tolerance: Tolerance,
    pub cases: Vec<EmbeddingCase>,
}

pub fn embeddings() -> EmbeddingGoldens {
    load_json("embeddings.json")
}

// ── search.json / mutations.json ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct Expected {
    pub id: u32,
    pub score: f64,
}

#[derive(Deserialize)]
pub struct SearchCase {
    pub k: usize,
    pub allowed: Option<Vec<u32>>,
    pub expected: Vec<Expected>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub id: String,
    pub vector: Vec<f32>,
    pub cases: Vec<SearchCase>,
}

#[derive(Deserialize)]
pub struct Row {
    pub id: u32,
    pub vector: Vec<f32>,
}

#[derive(Deserialize)]
pub struct DesignedTie {
    pub query: String,
    pub k: usize,
    pub ids: Vec<u32>,
    pub winner: u32,
}

#[derive(Deserialize)]
pub struct SearchSet {
    pub id: String,
    pub dim: usize,
    pub metric: String,
    pub designed_ties: Vec<DesignedTie>,
    pub rows: Vec<Row>,
    pub queries: Vec<SearchQuery>,
}

#[derive(Deserialize)]
pub struct SearchGoldens {
    pub score_abs_tol: f64,
    pub sets: Vec<SearchSet>,
}

pub fn search() -> SearchGoldens {
    load_json("search.json")
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Step {
    Add {
        id: u32,
        vector: Vec<f32>,
    },
    Delete {
        ids: Vec<u32>,
    },
    Commit,
    Reopen,
    Expect {
        len: u64,
        query: Vec<f32>,
        k: usize,
        results: Vec<Expected>,
    },
}

#[derive(Deserialize)]
pub struct Mutations {
    pub dim: usize,
    pub metric: String,
    pub fingerprint: String,
    pub score_abs_tol: f64,
    pub steps: Vec<Step>,
}

pub fn mutations() -> Mutations {
    load_json("mutations.json")
}

pub fn parse_metric(s: &str) -> Metric {
    match s {
        "cosine" => Metric::Cosine,
        "dot" => Metric::Dot,
        "euclidean" => Metric::Euclidean,
        other => panic!("unknown metric {other}"),
    }
}

pub fn doc_set(ids: &[u32]) -> DocSet {
    let mut set = DocSet::new();
    for &id in ids {
        set.insert(DocId(id));
    }
    set
}

// ── float helpers ───────────────────────────────────────────────────────────────────────────

pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b) {
        dot += f64::from(x) * f64::from(y);
        na += f64::from(x) * f64::from(x);
        nb += f64::from(y) * f64::from(y);
    }
    dot / (na.sqrt() * nb.sqrt())
}

pub fn max_abs(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| (f64::from(x) - f64::from(y)).abs())
        .fold(0.0, f64::max)
}

pub fn norm(a: &[f32]) -> f64 {
    a.iter()
        .map(|&x| f64::from(x) * f64::from(x))
        .sum::<f64>()
        .sqrt()
}

pub fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// Compare a hit list against a golden: ids and order exact, scores within `tol`.
pub fn assert_hits(hits: &[xtriever_core::Hit], expected: &[Expected], tol: f64, label: &str) {
    let got: Vec<u32> = hits.iter().map(|h| h.id.0).collect();
    let want: Vec<u32> = expected.iter().map(|e| e.id).collect();
    assert_eq!(got, want, "{label}: ids/order differ");
    for (h, e) in hits.iter().zip(expected) {
        let diff = (f64::from(h.score) - e.score).abs();
        assert!(
            diff <= tol,
            "{label}: score for id {} differs by {diff:e} (got {}, want {})",
            h.id,
            h.score,
            e.score
        );
    }
}
