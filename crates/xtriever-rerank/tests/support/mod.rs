//! Shared helpers for the Feature 006 acceptance suite: fixture loading, the model directory,
//! the golden schema and a tamperable copy of the model.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// `reference/fixtures/006/`, resolved from the crate manifest so it works from any cwd.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/006")
}

/// The git-ignored eight-bit model directory (Feature 026), overridable with
/// `XTRIEVER_RERANK_MODEL_DIR_Q8`, staged by
/// `scripts/fetch-model.sh --manifest reference/models/manifest-rerank-q8.json`.
pub fn model_dir_q8() -> PathBuf {
    std::env::var_os("XTRIEVER_RERANK_MODEL_DIR_Q8").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference/models/ms-marco-MiniLM-L-6-v2-q8")
        },
        PathBuf::from,
    )
}

/// The git-ignored float model directory, overridable with `XTRIEVER_RERANK_MODEL_DIR`.
pub fn model_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_RERANK_MODEL_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference/models/ms-marco-MiniLM-L-6-v2")
        },
        PathBuf::from,
    )
}

pub fn load_json<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = fixtures_dir().join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

// ── rerank.json ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct GoldenPassage {
    pub text: String,
    pub input_ids: Vec<u32>,
    pub token_type_ids: Vec<u32>,
    pub truncated: bool,
    pub score: f32,
}

#[derive(Deserialize)]
pub struct GoldenQuery {
    pub name: String,
    pub query: String,
    pub passages: Vec<GoldenPassage>,
    pub order: Vec<usize>,
}

#[derive(Deserialize)]
pub struct RerankGoldens {
    pub model_id: String,
    pub max_tokens: usize,
    pub tolerance_abs: f32,
    pub min_gap: f32,
    pub queries: Vec<GoldenQuery>,
}

pub fn goldens() -> RerankGoldens {
    load_json("rerank.json")
}

/// Every `(query, passage)` pair of the golden set, flattened in fixture order.
pub fn golden_pairs() -> Vec<(String, String, f32)> {
    goldens()
        .queries
        .into_iter()
        .flat_map(|q| {
            q.passages
                .into_iter()
                .map(move |p| (q.query.clone(), p.text, p.score))
        })
        .collect()
}

/// A writable copy of the model directory for tamper tests.
pub fn model_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in ["config.json", "tokenizer.json", "model.safetensors"] {
        std::fs::copy(model_dir().join(name), dir.path().join(name)).unwrap();
    }
    dir
}
