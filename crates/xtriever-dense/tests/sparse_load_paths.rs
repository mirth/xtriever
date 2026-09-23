//! ADR-0007 condition 3 for the sparse encoder (Feature 027): the buffered and the memory-mapped
//! weight loaders produce bit-identical expansions over every fixture document. The mapped path
//! goes through `model::read_pinned` — the map is verified, then parsed — and the crate's one
//! `unsafe` block, so it is tested against the safe path here. Model-backed; needs
//! `--features mmap`:
//!
//!     cargo nextest run -p xtriever-dense --features mmap --run-ignored all -E 'binary(sparse_load_paths)'
#![cfg(feature = "mmap")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use serde::Deserialize;
use xtriever_dense::LoadPath;
use xtriever_dense::model::SPARSE_IDENTITY;
use xtriever_dense::sparse::SparseEncoder;

/// The git-ignored encoder directory, overridable with `XTRIEVER_SPARSE_MODEL_DIR`.
fn encoder_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_SPARSE_MODEL_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference/models/opensearch-neural-sparse-encoding-doc-v3-distill")
        },
        PathBuf::from,
    )
}

#[derive(Deserialize)]
struct Documents {
    documents: Vec<Document>,
}

#[derive(Deserialize)]
struct Document {
    id: String,
    text: String,
}

#[test]
#[ignore = "needs the sparse encoder"]
fn buffered_and_mapped_loaders_agree_bit_for_bit() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/027/documents.json");
    let documents: Documents =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

    let dir = encoder_dir();
    let buffered = SparseEncoder::load(&dir, LoadPath::Buffered).expect("buffered load");
    let mapped = SparseEncoder::load(&dir, LoadPath::Mmap).expect("mapped load");
    assert_eq!(buffered.identity(), SPARSE_IDENTITY);
    assert_eq!(mapped.identity(), SPARSE_IDENTITY);

    for doc in &documents.documents {
        let a = buffered.encode(&doc.text).unwrap();
        let b = mapped.encode(&doc.text).unwrap();
        assert_eq!(a.truncated, b.truncated, "{}", doc.id);
        assert_eq!(a.entries.len(), b.entries.len(), "{}", doc.id);
        for (&(ia, wa), &(ib, wb)) in a.entries.iter().zip(&b.entries) {
            assert_eq!(ia, ib, "{}", doc.id);
            assert_eq!(wa.to_bits(), wb.to_bits(), "{} token {ia}", doc.id);
        }
    }
}
