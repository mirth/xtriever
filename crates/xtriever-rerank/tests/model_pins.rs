//! The compiled-in pins equal `reference/models/manifest-rerank.json`, and the identity string
//! equals the oracle's (spec FR-002, FR-004; research D2). GREEN at the red checkpoint.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde::Deserialize;
use xtriever_rerank::model::{MODEL_ID, MODEL_NAME, PINNED};

#[derive(Deserialize)]
struct ManifestFile {
    name: String,
    bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct ModelManifest {
    repository: String,
    revision: String,
    local_dir: String,
    files: Vec<ManifestFile>,
    hidden: usize,
    max_tokens: usize,
    f32_tensors: usize,
}

#[test]
fn compiled_pins_match_the_committed_manifest() {
    let path = support::fixtures_dir().join("../../models/manifest-rerank.json");
    let m: ModelManifest =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read manifest")).unwrap();
    assert_eq!(PINNED.repository, m.repository);
    assert_eq!(PINNED.revision, m.revision);
    assert_eq!(m.local_dir, MODEL_NAME);
    assert_eq!(PINNED.hidden, m.hidden);
    assert_eq!(PINNED.max_tokens, m.max_tokens);
    assert_eq!(PINNED.f32_tensors, m.f32_tensors);
    assert_eq!(PINNED.files.len(), m.files.len());
    for (p, f) in PINNED.files.iter().zip(&m.files) {
        assert_eq!(p.name, f.name);
        assert_eq!(p.bytes, f.bytes, "{}", f.name);
        assert_eq!(p.sha256, f.sha256, "{}", f.name);
    }
    assert_eq!(
        PINNED.files.map(|f| f.name),
        ["config.json", "tokenizer.json", "model.safetensors"]
    );
}

#[test]
fn model_id_matches_the_oracle_and_names_every_input() {
    let goldens = support::goldens();
    assert_eq!(MODEL_ID, goldens.model_id);
    for needle in [
        PINNED.repository,
        PINNED.revision,
        PINNED.files[2].sha256,
        ";max_tokens=512;",
        ";trunc=longest_first;",
        ";head=cls-pooler-tanh-linear;",
        ";act=identity;",
        ";dtype=f32;",
        ";engine=candle-0.9.2",
    ] {
        assert!(MODEL_ID.contains(needle), "identity lacks {needle}");
    }
}
