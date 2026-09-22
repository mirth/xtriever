//! The compiled-in pins equal `reference/models/manifest.json`, and the fingerprint equals the
//! oracle's (spec FR-004, research D4/D6). GREEN at the red checkpoint.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde::Deserialize;
use xtriever_dense::model::{FINGERPRINT, FINGERPRINT_Q8, MODEL_NAME, PINNED, PINNED_Q8};

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
    files: Vec<ManifestFile>,
    dim: usize,
    max_tokens: usize,
    f32_tensors: usize,
}

#[test]
fn compiled_pins_match_the_committed_manifest() {
    let path = support::fixtures_dir().join("../../models/manifest.json");
    let m: ModelManifest =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read manifest")).unwrap();
    assert_eq!(PINNED.repository, m.repository);
    assert_eq!(PINNED.revision, m.revision);
    assert_eq!(PINNED.dim, m.dim);
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
    borrows: Borrows,
}

#[derive(Deserialize)]
struct Borrows {
    manifest: String,
    files: Vec<String>,
}

/// Feature 026 (spec FR-005): the compiled-in eight-bit pins equal `manifest-q8.json`, and the
/// borrowed configuration and tokenizer are the float manifest's pins, unchanged.
#[test]
fn compiled_eight_bit_pins_match_the_committed_manifest() {
    let path = support::fixtures_dir().join("../../models/manifest-q8.json");
    let m: ArtefactManifest =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read manifest")).unwrap();
    assert_eq!(PINNED_Q8.repository, m.repository);
    assert_eq!(PINNED_Q8.revision, m.revision);
    // Every default in the tree names this directory; the fetch script fills whatever the
    // manifest says, so the two must agree here.
    assert_eq!(m.local_dir, format!("{MODEL_NAME}-q8"));
    assert_eq!(PINNED_Q8.quantisation, m.quantisation);
    assert_eq!(PINNED_Q8.architecture, m.architecture);
    assert_eq!(PINNED_Q8.blocks, m.blocks);
    assert_eq!(PINNED_Q8.embedding_length, m.hidden);
    assert_eq!(PINNED_Q8.context_length, m.max_tokens);
    assert_eq!(PINNED_Q8.quantised_tensors, m.weight_tensors);
    assert_eq!(m.files.len(), 1, "the artefact supplies weights only");
    let weights = PINNED_Q8.files[2];
    assert_eq!(weights.name, m.files[0].name);
    assert_eq!(weights.bytes, m.files[0].bytes);
    assert_eq!(weights.sha256, m.files[0].sha256);
    assert_eq!(m.borrows.manifest, "manifest.json");
    assert_eq!(m.borrows.files, ["config.json", "tokenizer.json"]);
    assert_eq!(
        PINNED_Q8.files[0], PINNED.files[0],
        "config.json is the float model's"
    );
    assert_eq!(
        PINNED_Q8.files[1], PINNED.files[1],
        "tokenizer.json is the float model's"
    );
}

/// Feature 026 (spec FR-006): the eight-bit fingerprint names the artefact and its precision and
/// shares every other input with the float one, so the two are distinguishable by exactly what
/// differs.
#[test]
fn eight_bit_fingerprint_names_the_artefact_and_differs_only_in_it() {
    assert_ne!(FINGERPRINT_Q8, FINGERPRINT);
    for needle in [
        PINNED_Q8.repository,
        PINNED_Q8.revision,
        PINNED_Q8.files[2].sha256,
        ";dtype=q8_0;",
        ";compute=f32;",
    ] {
        assert!(
            FINGERPRINT_Q8.contains(needle),
            "fingerprint lacks {needle}"
        );
    }
    for shared in [
        ";dim=384;",
        ";pool=mean-mask;",
        ";norm=l2;",
        ";max_tokens=256;",
        ";prefix=none;",
        ";engine=candle-0.9.2",
    ] {
        assert!(
            FINGERPRINT_Q8.contains(shared),
            "fingerprint lacks {shared}"
        );
    }
    assert!(
        !FINGERPRINT_Q8.contains(PINNED.files[2].sha256),
        "names the float weights"
    );
    assert!(!FINGERPRINT_Q8.contains(char::is_whitespace));
}

#[test]
fn fingerprint_matches_the_oracle_and_names_every_input() {
    let goldens = support::embeddings();
    assert_eq!(FINGERPRINT, goldens.fingerprint);
    for needle in [
        PINNED.revision,
        PINNED.files[2].sha256,
        ";dim=384;",
        ";pool=mean-mask;",
        ";norm=l2;",
        ";max_tokens=256;",
        ";dtype=f32;",
        ";prefix=none;",
        ";engine=candle-0.9.2",
    ] {
        assert!(FINGERPRINT.contains(needle), "fingerprint lacks {needle}");
    }
    assert!(!FINGERPRINT.contains(char::is_whitespace));
}
