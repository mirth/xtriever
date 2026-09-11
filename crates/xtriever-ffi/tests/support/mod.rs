//! Shared fixture loading for the Feature 001 acceptance tests.
//!
//! Compiled into several test binaries, each using only part of it, hence `dead_code` is allowed.
//! `missing_docs` is on workspace-wide and applies to test crates too, so every item is documented.
#![allow(dead_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

/// Absolute path to `reference/fixtures/001`.
///
/// Resolved from `CARGO_MANIFEST_DIR` so the tests do not depend on the working directory.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../reference/fixtures/001")
        .canonicalize()
        .expect("reference/fixtures/001 is missing — run reference/gen_001_fixtures.py")
}

/// Absolute path to the gitignored model cache, overridable with `XTRIEVER_MODEL_DIR`.
///
/// The weights are 87.1 MiB and are never committed; they are downloaded and verified by
/// `reference/gen_001_fixtures.py`.
pub fn model_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_MODEL_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../reference/models/001"),
        PathBuf::from,
    )
}

/// Load and parse one fixture file by name.
///
/// # Panics
///
/// Panics if the file is missing or is not valid JSON. Both mean the fixtures were not generated,
/// which is a setup failure rather than a test failure, and must not be silently skipped (FR-028).
pub fn load(name: &str) -> serde_json::Value {
    let path = fixtures_dir().join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e} — run reference/gen_001_fixtures.py",
            path.display()
        )
    });
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()))
}

/// Cosine similarity between two equal-length vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), b.len(), "vectors must have equal length");
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b) {
        dot += f64::from(*x) * f64::from(*y);
        na += f64::from(*x) * f64::from(*x);
        nb += f64::from(*y) * f64::from(*y);
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Largest absolute element-wise difference between two equal-length vectors.
pub fn max_abs_diff(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), b.len(), "vectors must have equal length");
    a.iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x - *y).abs())
        .fold(0.0, f64::max)
}

/// Read a JSON array of numbers as `Vec<f32>`.
pub fn as_f32_vec(value: &serde_json::Value) -> Vec<f32> {
    value
        .as_array()
        .expect("expected a JSON array")
        .iter()
        .map(|v| v.as_f64().expect("expected a number") as f32)
        .collect()
}

/// Read a JSON array of numbers as `Vec<u32>`.
pub fn as_u32_vec(value: &serde_json::Value) -> Vec<u32> {
    value
        .as_array()
        .expect("expected a JSON array")
        .iter()
        .map(|v| {
            u32::try_from(v.as_u64().expect("expected a non-negative integer"))
                .expect("fits in u32")
        })
        .collect()
}

/// Load the corpus as `(external_id, text)` pairs, in insertion order.
///
/// Order is significant: it is what makes tantivy's internal `DocId` assignment deterministic and
/// therefore what makes the golden ranking comparable at all (research D5).
pub fn corpus_documents() -> Vec<(String, String)> {
    load("corpus.json")["documents"]
        .as_array()
        .expect("documents array")
        .iter()
        .map(|d| {
            (
                d["external_id"].as_str().expect("external_id").to_owned(),
                d["text"].as_str().expect("text").to_owned(),
            )
        })
        .collect()
}
