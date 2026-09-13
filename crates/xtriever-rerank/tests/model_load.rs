//! US1 scenario 1 — every pinned file is verified before use; a violation names the file and
//! both values (spec FR-002, FR-003). Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::io::{Seek, SeekFrom, Write};

use xtriever_core::{Error, Reranker};
use xtriever_rerank::model::{MODEL_ID, PINNED};
use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

fn model_message(err: Error) -> String {
    match err {
        Error::Model { model, message } => {
            assert_eq!(model, "ms-marco-MiniLM-L-6-v2");
            message
        }
        other => panic!("expected Error::Model, got {other:?}"),
    }
}

#[test]
#[ignore = "needs the model"]
fn the_pinned_model_loads_and_reports_its_identity() {
    let r = MiniLmCrossEncoder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    assert_eq!(r.model_id(), MODEL_ID);
    assert_eq!(r.load_path(), LoadPath::Buffered);
}

#[test]
#[ignore = "needs the model"]
fn a_size_mismatch_names_the_file_and_both_sizes() {
    let dir = support::model_copy();
    let path = dir.path().join("model.safetensors");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"\0")
        .unwrap();
    let msg = model_message(MiniLmCrossEncoder::load(dir.path(), LoadPath::Buffered).unwrap_err());
    assert!(msg.contains("model.safetensors"), "{msg}");
    assert!(msg.contains(&PINNED.files[2].bytes.to_string()), "{msg}");
    assert!(
        msg.contains(&(PINNED.files[2].bytes + 1).to_string()),
        "{msg}"
    );
}

#[test]
#[ignore = "needs the model"]
fn a_content_mismatch_names_the_file_and_both_hashes() {
    let dir = support::model_copy();
    let path = dir.path().join("model.safetensors");
    let mut f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    // Flip a byte deep in the tensor data, keeping the size.
    f.seek(SeekFrom::Start(PINNED.files[2].bytes / 2)).unwrap();
    f.write_all(b"\xff").unwrap();
    drop(f);
    let msg = model_message(MiniLmCrossEncoder::load(dir.path(), LoadPath::Buffered).unwrap_err());
    assert!(msg.contains("model.safetensors"), "{msg}");
    assert!(msg.contains(PINNED.files[2].sha256), "{msg}");
    assert!(msg.contains("sha256"), "{msg}");
    // Two 64-hex-digit hashes are present: the pinned one and the actual one.
    let hashes: Vec<&str> = msg
        .split(|c: char| !c.is_ascii_hexdigit())
        .filter(|s| s.len() == 64)
        .collect();
    assert_eq!(hashes.len(), 2, "{msg}");
    assert_ne!(hashes[0], hashes[1]);
}

#[test]
#[ignore = "needs the model"]
fn a_config_edit_is_refused_by_the_hash_check_first() {
    let dir = support::model_copy();
    let path = dir.path().join("config.json");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace("\"hidden_size\": 384", "\"hidden_size\": 768"),
    )
    .unwrap();
    let msg = model_message(MiniLmCrossEncoder::load(dir.path(), LoadPath::Buffered).unwrap_err());
    assert!(msg.contains("config.json"), "{msg}");
    assert!(
        msg.contains(PINNED.files[0].sha256) || msg.contains("bytes"),
        "{msg}"
    );
}

#[test]
#[ignore = "needs the model"]
fn a_missing_file_is_named() {
    let dir = support::model_copy();
    std::fs::remove_file(dir.path().join("tokenizer.json")).unwrap();
    let msg = model_message(MiniLmCrossEncoder::load(dir.path(), LoadPath::Buffered).unwrap_err());
    assert!(msg.contains("tokenizer.json"), "{msg}");
}
