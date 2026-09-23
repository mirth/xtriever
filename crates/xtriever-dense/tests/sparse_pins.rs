//! Feature 027: the compiled-in sparse encoder pins equal
//! `reference/models/manifest-sparse-doc-v3.json`, and the identity a sparse index records names
//! every input of the expansion. GREEN at the red checkpoint.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde::Deserialize;
use xtriever_dense::model::{PINNED_SPARSE, SPARSE_IDENTITY, SPARSE_MODEL_NAME};

#[derive(Deserialize)]
struct ManifestFile {
    name: String,
    bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct SparseManifest {
    repository: String,
    revision: String,
    local_dir: String,
    activation: String,
    files: Vec<ManifestFile>,
}

#[test]
fn compiled_sparse_pins_match_the_committed_manifest() {
    let path = support::fixtures_dir().join("../../models/manifest-sparse-doc-v3.json");
    let m: SparseManifest =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read manifest")).unwrap();
    assert_eq!(PINNED_SPARSE.repository, m.repository);
    assert_eq!(PINNED_SPARSE.revision, m.revision);
    assert_eq!(PINNED_SPARSE.activation, m.activation);
    // The error names the directory the fetch script fills.
    assert_eq!(m.local_dir, SPARSE_MODEL_NAME);
    assert_eq!(PINNED_SPARSE.files.len(), m.files.len());
    for (p, f) in PINNED_SPARSE.files.iter().zip(&m.files) {
        assert_eq!(p.name, f.name);
        assert_eq!(p.bytes, f.bytes, "{}", f.name);
        assert_eq!(p.sha256, f.sha256, "{}", f.name);
    }
    assert_eq!(
        PINNED_SPARSE.files.map(|f| f.name),
        [
            "config.json",
            "tokenizer.json",
            "model.safetensors",
            "idf.json"
        ]
    );
}

#[test]
fn sparse_identity_names_every_input() {
    for needle in [
        PINNED_SPARSE.repository,
        PINNED_SPARSE.revision,
        PINNED_SPARSE.files[2].sha256,
        ";activation=log1p_log1p_relu;",
        ";max_tokens=512;",
        ";engine=candle-0.9.2",
    ] {
        assert!(SPARSE_IDENTITY.contains(needle), "identity lacks {needle}");
    }
    assert_eq!(PINNED_SPARSE.max_tokens, 512);
    assert!(!SPARSE_IDENTITY.contains(char::is_whitespace));
}
