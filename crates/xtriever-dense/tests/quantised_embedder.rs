//! Feature 026 (spec FR-005, FR-006, FR-012; contracts/model-artefacts.md): the embedder loads
//! the owner-pinned eight-bit artefact, names it, and refuses what is not it.
//!
//! - the eight-bit directory loads; the embedder reports the same dimension, metric and window
//!   as the float one, the eight-bit fingerprint, and `Precision::EightBit`;
//! - a flipped byte in the artefact is refused by checksum, naming the file and both hashes,
//!   before anything is parsed;
//! - a GGUF whose declared architecture or shape disagrees with the stage is refused by name —
//!   the guard on the pin itself, exercised through `assert_gguf_header` on hand-built headers;
//! - a directory holding both weights files, or neither, is refused naming what was expected;
//! - the eight-bit vectors are unit-length, deterministic, and point where the float reference
//!   points: within a stated cosine of every case in the 004 goldens.
//!
//! Model-backed tests are `#[ignore]`d like the float ones (`model_load.rs`); the header tests
//! need no model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::{Path, PathBuf};

use xtriever_core::{Embedder, Error, Metric, TextKind};
use xtriever_dense::model::{FINGERPRINT, FINGERPRINT_Q8, PINNED, PINNED_Q8, Precision};
use xtriever_dense::{LoadPath, MiniLmEmbedder};

/// Below this cosine against the float reference the artefact would not be the same model.
/// The study measured 0.99941 mean on real embeddings for eight-bit *vectors*; eight-bit
/// *weights* through six blocks accumulate more, so the bound is looser and stated, not tuned.
const MIN_COSINE_TO_FLOAT_REFERENCE: f64 = 0.99;

fn load_q8() -> MiniLmEmbedder {
    MiniLmEmbedder::load(&support::model_dir_q8(), LoadPath::Buffered)
        .expect("load the pinned eight-bit artefact")
}

/// A private copy of the eight-bit directory (config + tokenizer copied, weights hard-linked or
/// copied) so a test can corrupt one file without touching the shared one.
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
            assert_eq!(model, "all-MiniLM-L6-v2");
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

/// A GGUF v3 file with `metadata` and no tensors: magic, version, tensor count, key-value
/// count, then each pair as `string key · u32 type · value` (type 8 = string, 4 = u32).
fn gguf_with(metadata: &[(&str, Meta)]) -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
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

fn header_message(metadata: &[(&str, Meta)]) -> String {
    model_message(
        xtriever_dense::model::assert_gguf_header(&gguf_with(metadata))
            .expect_err("a header disagreeing with the pin must be refused"),
    )
}

#[test]
fn an_artefact_declaring_another_architecture_is_refused_by_name() {
    let mut metadata = pinned_metadata();
    metadata[0] = ("general.architecture", Meta::Str("llama"));
    let msg = header_message(&metadata);
    assert!(msg.contains("architecture"), "{msg}");
    assert!(msg.contains("llama") && msg.contains("bert"), "{msg}");
}

#[test]
fn an_artefact_declaring_another_shape_is_refused_naming_the_field_and_both_values() {
    for (key, wrong, right) in [
        ("bert.block_count", 12u32, "6"),
        ("bert.embedding_length", 768, "384"),
        ("bert.attention.head_count", 16, "12"),
        ("bert.feed_forward_length", 3072, "1536"),
        ("bert.context_length", 128, "512"),
    ] {
        let mut metadata = pinned_metadata();
        for entry in &mut metadata {
            if entry.0 == key {
                entry.1 = Meta::U32(wrong);
            }
        }
        let msg = header_message(&metadata);
        let field = key.rsplit('.').next().unwrap();
        assert!(msg.contains(field), "{key}: {msg}");
        assert!(msg.contains(&wrong.to_string()), "{key}: {msg}");
        assert!(msg.contains(right), "{key}: {msg}");
    }
}

#[test]
fn an_artefact_missing_a_declaration_is_refused_naming_it() {
    let metadata: Vec<(&str, Meta)> = pinned_metadata()
        .into_iter()
        .filter(|(k, _)| *k != "bert.block_count")
        .collect();
    let msg = header_message(&metadata);
    assert!(msg.contains("block_count"), "{msg}");
}

#[test]
fn an_artefact_that_is_not_a_gguf_is_refused_as_such() {
    let msg = model_message(
        xtriever_dense::model::assert_gguf_header(b"not a gguf file at all").unwrap_err(),
    );
    assert!(msg.to_lowercase().contains("gguf"), "{msg}");
}

// ── model-backed ───────────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "needs reference/models/all-MiniLM-L6-v2-q8 (scripts/fetch-model.sh --manifest reference/models/manifest-q8.json)"]
fn the_eight_bit_artefact_loads_and_names_itself() {
    let e = load_q8();
    assert_eq!(e.dim(), 384);
    assert_eq!(e.metric(), Metric::Cosine);
    assert_eq!(e.max_input_tokens(), Some(256));
    assert_eq!(e.precision(), Precision::EightBit);
    assert_eq!(e.fingerprint(), FINGERPRINT_Q8);
    assert_ne!(e.fingerprint(), FINGERPRINT);
    assert!(e.fingerprint().contains(PINNED_Q8.files[2].sha256));
    assert!(e.fingerprint().contains("dtype=q8_0"));
    assert_eq!(e.load_path(), LoadPath::Buffered);
}

#[test]
#[ignore = "needs both model directories"]
fn the_float_directory_still_loads_as_float() {
    let e = MiniLmEmbedder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    assert_eq!(e.precision(), Precision::Float);
    assert_eq!(e.fingerprint(), FINGERPRINT);
}

#[test]
#[ignore = "needs the eight-bit model"]
fn a_flipped_byte_in_the_artefact_is_refused_by_checksum_before_parsing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy_q8(tmp.path());
    let path = dir.join(PINNED_Q8.files[2].name);
    let mut bytes = std::fs::read(&path).unwrap();
    // Deep in the tensor data, where nothing but the hash would notice.
    let at = bytes.len() / 2;
    bytes[at] ^= 0x01;
    std::fs::write(&path, bytes).unwrap();
    let msg = model_message(MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(msg.contains(PINNED_Q8.files[2].name), "{msg}");
    assert!(
        msg.contains(PINNED_Q8.files[2].sha256),
        "expected hash missing: {msg}"
    );
    assert!(msg.contains("sha256"), "{msg}");
    assert!(
        !msg.to_lowercase().contains("parse") && !msg.to_lowercase().contains("tensor"),
        "must fail on the hash, not on the contents: {msg}"
    );
}

#[test]
#[ignore = "needs the eight-bit model"]
fn a_directory_with_both_weights_files_is_refused_as_ambiguous() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy_q8(tmp.path());
    std::fs::write(dir.join(PINNED.files[2].name), b"not really").unwrap();
    let msg = model_message(MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(msg.contains(PINNED.files[2].name), "{msg}");
    assert!(msg.contains(PINNED_Q8.files[2].name), "{msg}");
}

#[test]
#[ignore = "needs the eight-bit model"]
fn a_directory_with_neither_weights_file_names_both() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = private_copy_q8(tmp.path());
    std::fs::remove_file(dir.join(PINNED_Q8.files[2].name)).unwrap();
    let msg = model_message(MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap_err());
    assert!(msg.contains(PINNED.files[2].name), "{msg}");
    assert!(msg.contains(PINNED_Q8.files[2].name), "{msg}");
}

#[test]
#[ignore = "needs the eight-bit model"]
fn verify_files_q8_alone_accepts_the_pinned_directory() {
    xtriever_dense::model::verify_files_q8(&support::model_dir_q8()).expect("pinned files verify");
}

#[test]
#[ignore = "needs the eight-bit model"]
fn eight_bit_vectors_are_unit_length_deterministic_and_point_where_the_reference_points() {
    let e = load_q8();
    let goldens = support::embeddings();
    let mut worst = 1.0f64;
    for case in &goldens.cases {
        let a = e
            .embed(&[case.text.as_str()], TextKind::Passage)
            .unwrap()
            .remove(0);
        let b = e
            .embed(&[case.text.as_str()], TextKind::Passage)
            .unwrap()
            .remove(0);
        assert_eq!(
            support::bits(&a),
            support::bits(&b),
            "{}: not deterministic",
            case.id
        );
        assert_eq!(a.len(), 384, "{}", case.id);
        assert!(
            (support::norm(&a) - 1.0).abs() < goldens.tolerance.unit_norm_abs,
            "{}: norm {}",
            case.id,
            support::norm(&a)
        );
        let cosine = support::cosine(&a, &case.vector);
        worst = worst.min(cosine);
        assert!(
            cosine >= MIN_COSINE_TO_FLOAT_REFERENCE,
            "{}: cosine {cosine} against the float reference",
            case.id
        );
    }
    eprintln!(
        "eight-bit against the float reference: worst cosine {worst:.5} over {} cases",
        goldens.cases.len()
    );
}
