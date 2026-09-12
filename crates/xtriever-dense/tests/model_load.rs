//! US1 scenarios 1–2 and the edge cases around loading (spec FR-002, FR-003). Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::{Path, PathBuf};

use xtriever_core::{Embedder, Error, Metric};
use xtriever_dense::model::{FINGERPRINT, PINNED};
use xtriever_dense::{LoadPath, MiniLmEmbedder};

fn load() -> MiniLmEmbedder {
    MiniLmEmbedder::load(&support::model_dir(), LoadPath::Buffered).expect("load pinned model")
}

/// A private copy of the model directory (config + tokenizer copied, weights hard-linked) so a
/// test can corrupt one file without touching the shared cache.
fn private_copy(dir: &Path) -> PathBuf {
    let src = support::model_dir();
    for f in PINNED.files {
        let from = src.join(f.name);
        let to = dir.join(f.name);
        if f.name == "model.safetensors" {
            if std::fs::hard_link(&from, &to).is_err() {
                std::fs::copy(&from, &to).unwrap();
            }
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
    dir.to_path_buf()
}

fn model_message(e: Error) -> String {
    match e {
        Error::Model { model, message } => {
            assert_eq!(model, "all-MiniLM-L6-v2");
            message
        }
        other => panic!("expected Error::Model, got {other:?}"),
    }
}

#[test]
#[ignore = "needs reference/models/all-MiniLM-L6-v2 (scripts/fetch-model.sh)"]
fn loaded_embedder_reports_the_pinned_identity() {
    let e = load();
    assert_eq!(e.dim(), 384);
    assert_eq!(e.metric(), Metric::Cosine);
    assert_eq!(e.max_input_tokens(), Some(256));
    assert_eq!(e.fingerprint(), FINGERPRINT);
    assert_eq!(e.load_path(), LoadPath::Buffered);
}

#[test]
#[ignore = "needs the model"]
fn a_flipped_byte_fails_naming_the_file_and_both_hashes() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy(tmp.path());
    let path = dir.join("config.json");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[10] ^= 0x01;
    std::fs::write(&path, bytes).unwrap();

    let msg = model_message(MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(msg.contains("config.json"), "{msg}");
    assert!(
        msg.contains(PINNED.files[0].sha256),
        "expected hash missing: {msg}"
    );
    assert!(msg.contains("sha256"), "{msg}");
    assert!(
        !msg.to_lowercase().contains("parse"),
        "must fail on the hash, not a parse: {msg}"
    );
}

#[test]
#[ignore = "needs the model"]
fn a_truncated_file_fails_naming_the_file_and_both_sizes() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy(tmp.path());
    let path = dir.join("tokenizer.json");
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() - 7]).unwrap();

    let msg = model_message(MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(msg.contains("tokenizer.json"), "{msg}");
    assert!(msg.contains("466247"), "expected size missing: {msg}");
    assert!(
        msg.contains(&(bytes.len() - 7).to_string()),
        "actual size missing: {msg}"
    );
}

#[test]
#[ignore = "needs the model"]
fn a_missing_tokenizer_fails_naming_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy(tmp.path());
    std::fs::remove_file(dir.join("tokenizer.json")).unwrap();
    let msg = model_message(MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(msg.contains("tokenizer.json"), "{msg}");
}

#[test]
#[ignore = "needs the model"]
fn a_missing_weights_file_fails_naming_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy(tmp.path());
    std::fs::remove_file(dir.join("model.safetensors")).unwrap();
    let msg = model_message(MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(msg.contains("model.safetensors"), "{msg}");
}

#[test]
#[ignore = "needs the model"]
fn verify_files_alone_accepts_the_pinned_directory() {
    xtriever_dense::model::verify_files(&support::model_dir()).expect("pinned files verify");
}
