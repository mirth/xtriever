//! BM25 parity against an independent Python implementation (Principle II, report.md D-001).
//!
//! Distinct from `index_query.rs`, which checks host-versus-device *determinism*. This one answers
//! a different question — "is our BM25 the BM25?" — against a Python transcription of tantivy's
//! documented formula, and so compares ids-and-order exactly but scores within a tolerance:
//! Python accumulates in float64 while tantivy accumulates in f32 against a precomputed cache, and
//! demanding bit equality across those would be a fake oracle.
#![cfg(feature = "spike")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{corpus_documents, load};
use xtriever_ffi::ffi::{SpikeDocument, spike_index, spike_query};

#[test]
fn bm25_scores_match_the_independent_python_reference() {
    let reference = load("bm25_reference.json");
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().to_string_lossy().into_owned();
    let documents: Vec<SpikeDocument> = corpus_documents()
        .into_iter()
        .map(|(external_id, text)| SpikeDocument { external_id, text })
        .collect();
    spike_index(path.clone(), documents).expect("spike_index must succeed");

    let k = reference["k"].as_u64().expect("k") as u32;
    let query = reference["query"].as_str().expect("query").to_owned();
    let hits = spike_query(path, query, k).expect("spike_query must succeed");

    let expected = reference["hits"].as_array().expect("hits");
    let tol = reference["score_rel_tol"].as_f64().expect("score_rel_tol");

    assert_eq!(
        hits.len(),
        expected.len(),
        "hit count must match the reference"
    );
    for (rank, (got, want)) in hits.iter().zip(expected).enumerate() {
        // Order and identity: exact. The generator refuses to emit a fixture whose adjacent top-k
        // scores are closer than `tol`, so the ordering is meaningfully determined.
        assert_eq!(
            got.external_id,
            want["external_id"].as_str().expect("external_id"),
            "rank {rank} identifies a different document than the reference"
        );
        let want_score = want["score"].as_f64().expect("score");
        let rel = (f64::from(got.score) - want_score).abs() / want_score.abs();
        assert!(
            rel <= tol,
            "rank {rank} ({}) scored {} but the reference says {want_score} \
             (relative difference {rel:.3e} > {tol:.0e}) — field_len {}, dequantized {}, tf {}",
            got.external_id,
            got.score,
            want["field_len"],
            want["field_len_dequantized"],
            want["term_freq"]
        );
    }
}
