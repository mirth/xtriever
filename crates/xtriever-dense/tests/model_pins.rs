//! The compiled-in pins equal `reference/models/manifest.json`, and the fingerprint equals the
//! oracle's (spec FR-004, research D4/D6). GREEN at the red checkpoint.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde::Deserialize;
use xtriever_dense::model::{FINGERPRINT, PINNED};

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
