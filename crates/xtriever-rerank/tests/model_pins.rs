//! The compiled-in pins equal `reference/models/manifest-rerank.json`, and the identity string
//! equals the oracle's (spec FR-002, FR-004; research D2). GREEN at the red checkpoint.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde::Deserialize;
use xtriever_rerank::model::{MODEL_ID, MODEL_ID_Q8, MODEL_NAME, PINNED, PINNED_Q8};

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

#[derive(Deserialize)]
struct ArtefactManifest {
    repository: String,
    revision: String,
    local_dir: String,
    files: Vec<ManifestFile>,
    quantisation: String,
    architecture: String,
    blocks: usize,
    hidden: usize,
    max_tokens: usize,
    weight_tensors: usize,
    classifier_tensors: Vec<String>,
    borrows: Borrows,
}

#[derive(Deserialize)]
struct Borrows {
    manifest: String,
    files: Vec<String>,
    tensors: BorrowedTensors,
}

#[derive(Deserialize)]
struct BorrowedTensors {
    file: String,
    from: String,
    names: Vec<String>,
    bytes: u64,
    sha256: String,
}

/// Feature 026 (spec FR-005, FR-008): the compiled-in eight-bit pins equal
/// `manifest-rerank-q8.json`, the borrowed configuration and tokenizer are the float manifest's
/// pins, and the classification head the loader must find is the one the manifest names.
#[test]
fn compiled_eight_bit_pins_match_the_committed_manifest() {
    let path = support::fixtures_dir().join("../../models/manifest-rerank-q8.json");
    let m: ArtefactManifest =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read manifest")).unwrap();
    assert_eq!(PINNED_Q8.repository, m.repository);
    assert_eq!(PINNED_Q8.revision, m.revision);
    assert_eq!(m.local_dir, format!("{MODEL_NAME}-q8"));
    assert_eq!(PINNED_Q8.quantisation, m.quantisation);
    assert_eq!(PINNED_Q8.architecture, m.architecture);
    assert_eq!(PINNED_Q8.blocks, m.blocks);
    assert_eq!(PINNED_Q8.embedding_length, m.hidden);
    assert_eq!(PINNED_Q8.context_length, m.max_tokens);
    assert_eq!(PINNED_Q8.quantised_tensors, m.weight_tensors);
    assert_eq!(PINNED_Q8.classifier_tensors.to_vec(), m.classifier_tensors);
    assert_eq!(m.files.len(), 1, "the artefact supplies weights only");
    let weights = PINNED_Q8.files[2];
    assert_eq!(weights.name, m.files[0].name);
    assert_eq!(weights.bytes, m.files[0].bytes);
    assert_eq!(weights.sha256, m.files[0].sha256);
    assert_eq!(m.borrows.manifest, "manifest-rerank.json");
    assert_eq!(m.borrows.files, ["config.json", "tokenizer.json"]);
    assert_eq!(PINNED_Q8.files[0], PINNED.files[0]);
    assert_eq!(PINNED_Q8.files[1], PINNED.files[1]);
    // The borrowed pooler: cut from the float weights, pinned like any other file.
    let pooler = PINNED_Q8.files[3];
    assert_eq!(pooler.name, m.borrows.tensors.file);
    assert_eq!(pooler.bytes, m.borrows.tensors.bytes);
    assert_eq!(pooler.sha256, m.borrows.tensors.sha256);
    assert_eq!(m.borrows.tensors.from, PINNED.files[2].name);
    assert_eq!(
        m.borrows.tensors.names,
        ["bert.pooler.dense.bias", "bert.pooler.dense.weight"]
    );
    assert!(
        MODEL_ID_Q8.contains(pooler.sha256),
        "the identity names the pooler it scored with"
    );
}

/// Feature 026 (spec FR-006): the eight-bit identity names the artefact and its precision and
/// shares every other input with the float one.
#[test]
fn eight_bit_identity_names_the_artefact_and_differs_only_in_it() {
    assert_ne!(MODEL_ID_Q8, MODEL_ID);
    for needle in [
        PINNED_Q8.repository,
        PINNED_Q8.revision,
        PINNED_Q8.files[2].sha256,
        ";dtype=q8_0;",
        ";compute=f16;",
    ] {
        assert!(MODEL_ID_Q8.contains(needle), "identity lacks {needle}");
    }
    for shared in [
        ";max_tokens=512;",
        ";trunc=longest_first;",
        ";head=cls-pooler-tanh-linear;",
        ";act=identity;",
        ";engine=candle-0.9.2",
    ] {
        assert!(MODEL_ID_Q8.contains(shared), "identity lacks {shared}");
    }
    assert!(!MODEL_ID_Q8.contains(PINNED.files[2].sha256));
}
