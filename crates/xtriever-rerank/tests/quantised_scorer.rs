//! Feature 026 (spec FR-005, FR-006, FR-008, FR-012; contracts/model-artefacts.md): the
//! cross-encoder loads the owner-pinned eight-bit artefact, names it, verifies its
//! classification head, and refuses what is not it.
//!
//! - the eight-bit directory loads and reports `MODEL_ID_Q8` and `Precision::EightBit`;
//! - a flipped byte is refused by checksum before parsing;
//! - a GGUF whose declared architecture or shape disagrees is refused by name, and one
//!   **without `classifier.weight` and `classifier.bias`** is refused (FR-008): a cross-encoder
//!   without its head produces embeddings that look like relevance scores;
//! - a directory holding both weights files, or neither, is refused naming both;
//! - the eight-bit scores order the 006 golden pairs as the float reference orders them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::{Path, PathBuf};

use xtriever_core::{Error, Reranker};
use xtriever_rerank::model::{MODEL_ID, MODEL_ID_Q8, PINNED, PINNED_Q8, Precision};
use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

fn load_q8() -> MiniLmCrossEncoder {
    MiniLmCrossEncoder::load(&support::model_dir_q8(), LoadPath::Buffered)
        .expect("load the pinned eight-bit artefact")
}

fn private_copy_q8(dir: &Path) -> PathBuf {
    let src = support::model_dir_q8();
    for f in PINNED_Q8.files {
        let from = src.join(f.name);
        let to = dir.join(f.name);
        if f.name.ends_with(".gguf") {
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
            assert_eq!(model, "ms-marco-MiniLM-L-6-v2");
            message
        }
        other => panic!("expected Error::Model, got {other:?}"),
    }
}

// ── hand-built GGUF headers, for the guard on the pin ──────────────────────────────────────

enum Meta {
    Str(&'static str),
    U32(u32),
}

/// A GGUF v3 file with `metadata` and `tensors` (name, dims, ggml type 0 = F32, offset 0), the
/// header only: enough for a header check, never for a load.
fn gguf_with(metadata: &[(&str, Meta)], tensors: &[(&str, &[u64])]) -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&(tensors.len() as u64).to_le_bytes());
    out.extend_from_slice(&(metadata.len() as u64).to_le_bytes());
    let put_str = |out: &mut Vec<u8>, s: &str| {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    };
    for (key, value) in metadata {
        put_str(&mut out, key);
        match value {
            Meta::Str(s) => {
                out.extend_from_slice(&8u32.to_le_bytes());
                put_str(&mut out, s);
            }
            Meta::U32(n) => {
                out.extend_from_slice(&4u32.to_le_bytes());
                out.extend_from_slice(&n.to_le_bytes());
            }
        }
    }
    for (name, dims) in tensors {
        put_str(&mut out, name);
        out.extend_from_slice(&(dims.len() as u32).to_le_bytes());
        for d in *dims {
            out.extend_from_slice(&d.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // F32
        out.extend_from_slice(&0u64.to_le_bytes()); // offset
    }
    out
}

/// The pinned artefact's declarations, as `gguf_dump` read them on 2026-09-20.
fn pinned_metadata() -> Vec<(&'static str, Meta)> {
    vec![
        ("general.architecture", Meta::Str("bert")),
        ("bert.block_count", Meta::U32(6)),
        ("bert.context_length", Meta::U32(512)),
        ("bert.embedding_length", Meta::U32(384)),
        ("bert.feed_forward_length", Meta::U32(1536)),
        ("bert.attention.head_count", Meta::U32(12)),
        ("general.file_type", Meta::U32(7)),
    ]
}

fn head() -> Vec<(&'static str, &'static [u64])> {
    vec![("classifier.weight", &[384]), ("classifier.bias", &[1])]
}

fn header_error(metadata: &[(&str, Meta)], tensors: &[(&str, &[u64])]) -> String {
    model_message(
        xtriever_rerank::model::assert_gguf_header(&gguf_with(metadata, tensors))
            .expect_err("a header disagreeing with the pin must be refused"),
    )
}

#[test]
fn an_artefact_without_the_classification_head_is_refused() {
    // Both tensors gone: an encoder, not a cross-encoder (FR-008).
    let msg = header_error(&pinned_metadata(), &[]);
    assert!(msg.contains("classifier.weight"), "{msg}");
    // One of the two gone.
    let msg = header_error(&pinned_metadata(), &[("classifier.weight", &[384])]);
    assert!(msg.contains("classifier.bias"), "{msg}");
}

#[test]
fn an_artefact_declaring_another_architecture_or_shape_is_refused_by_name() {
    let mut metadata = pinned_metadata();
    metadata[0] = ("general.architecture", Meta::Str("llama"));
    let msg = header_error(&metadata, &head());
    assert!(
        msg.contains("architecture") && msg.contains("llama"),
        "{msg}"
    );
    for (key, wrong, right) in [
        ("bert.block_count", 12u32, "6"),
        ("bert.embedding_length", 768, "384"),
        ("bert.attention.head_count", 16, "12"),
        ("bert.context_length", 128, "512"),
    ] {
        let mut metadata = pinned_metadata();
        for entry in &mut metadata {
            if entry.0 == key {
                entry.1 = Meta::U32(wrong);
            }
        }
        let msg = header_error(&metadata, &head());
        let field = key.rsplit('.').next().unwrap();
        assert!(
            msg.contains(field) && msg.contains(&wrong.to_string()) && msg.contains(right),
            "{key}: {msg}"
        );
    }
}

#[test]
fn an_artefact_that_is_not_a_gguf_is_refused_as_such() {
    let msg = model_message(
        xtriever_rerank::model::assert_gguf_header(b"not a gguf file at all").unwrap_err(),
    );
    assert!(msg.to_lowercase().contains("gguf"), "{msg}");
}

// ── model-backed ───────────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "needs reference/models/ms-marco-MiniLM-L-6-v2-q8 (scripts/fetch-model.sh --manifest reference/models/manifest-rerank-q8.json)"]
fn the_eight_bit_artefact_loads_and_names_itself() {
    let r = load_q8();
    assert_eq!(r.precision(), Precision::EightBit);
    assert_eq!(r.model_id(), MODEL_ID_Q8);
    assert_ne!(r.model_id(), MODEL_ID);
    assert!(r.model_id().contains(PINNED_Q8.files[2].sha256));
    assert!(r.model_id().contains("dtype=q8_0"));
}

#[test]
#[ignore = "needs both model directories"]
fn the_float_directory_still_loads_as_float() {
    let r = MiniLmCrossEncoder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    assert_eq!(r.precision(), Precision::Float);
    assert_eq!(r.model_id(), MODEL_ID);
}

#[test]
#[ignore = "needs the eight-bit model"]
fn a_flipped_byte_in_the_artefact_is_refused_by_checksum_before_parsing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy_q8(tmp.path());
    let path = dir.join(PINNED_Q8.files[2].name);
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes.len() / 2;
    bytes[at] ^= 0x01;
    std::fs::write(&path, bytes).unwrap();
    let msg = model_message(MiniLmCrossEncoder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(msg.contains(PINNED_Q8.files[2].name), "{msg}");
    assert!(msg.contains(PINNED_Q8.files[2].sha256), "{msg}");
    assert!(
        !msg.to_lowercase().contains("tensor"),
        "must fail on the hash: {msg}"
    );
}

#[test]
#[ignore = "needs the eight-bit model"]
fn a_directory_with_both_or_neither_weights_file_is_refused_naming_both() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy_q8(tmp.path());
    std::fs::write(dir.join(PINNED.files[2].name), b"not really").unwrap();
    let msg = model_message(MiniLmCrossEncoder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(
        msg.contains(PINNED.files[2].name) && msg.contains(PINNED_Q8.files[2].name),
        "{msg}"
    );
    std::fs::remove_file(dir.join(PINNED.files[2].name)).unwrap();
    std::fs::remove_file(dir.join(PINNED_Q8.files[2].name)).unwrap();
    let msg = model_message(MiniLmCrossEncoder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(
        msg.contains(PINNED.files[2].name) && msg.contains(PINNED_Q8.files[2].name),
        "{msg}"
    );
}

#[test]
#[ignore = "needs both model directories"]
fn eight_bit_scores_order_the_golden_pairs_as_the_float_reference_does() {
    let r = load_q8();
    let goldens = support::goldens();
    let mut agree = 0usize;
    let mut total = 0usize;
    for case in &goldens.queries {
        // Score every passage of the query with the eight-bit head and compare the resulting
        // order with the reference scores' order, pair by pair.
        let scores: Vec<f32> = case
            .passages
            .iter()
            .map(|p| r.score(&case.query, &p.text).unwrap())
            .collect();
        for i in 0..scores.len() {
            for j in (i + 1)..scores.len() {
                total += 1;
                let reference = case.passages[i]
                    .score
                    .partial_cmp(&case.passages[j].score)
                    .unwrap();
                let eight = scores[i].partial_cmp(&scores[j]).unwrap();
                if reference == eight {
                    agree += 1;
                }
            }
        }
    }
    let share = agree as f64 / total as f64;
    eprintln!("eight-bit re-ranker: {agree}/{total} golden pair orders agree ({share:.4})");
    assert!(
        share >= 0.95,
        "{share} of pair orders agree with the float reference"
    );
}
