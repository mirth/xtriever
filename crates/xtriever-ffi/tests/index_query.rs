//! T012 — indexing and querying must reproduce the host-minted golden ranking exactly (FR-014).
//!
//! RED until `spike_index`/`spike_query` land (PR 2). Gated on `spike` so CI, which runs
//! `--workspace` without `--all-features`, stays green while these are red.
#![cfg(feature = "spike")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{corpus_documents, load};
use xtriever_ffi::ffi::{SpikeDocument, spike_index, spike_query};

#[test]
fn indexing_the_corpus_produces_one_segment() {
    let dir = tempfile::tempdir().expect("temp dir");
    let documents: Vec<SpikeDocument> = corpus_documents()
        .into_iter()
        .map(|(external_id, text)| SpikeDocument { external_id, text })
        .collect();
    assert_eq!(documents.len(), 1000);

    let outcome = spike_index(dir.path().to_string_lossy().into_owned(), documents)
        .expect("spike_index must succeed");

    assert_eq!(outcome.documents_indexed, 1000);
    // Not a nicety: with more than one segment the collector's tie-break runs on
    // (segment_ord, doc_id) across segments, and the golden ranking stops being comparable at
    // all — a mismatch would then be uninterpretable rather than a finding (research D5).
    assert_eq!(
        outcome.segment_count, 1,
        "the single-threaded writer must produce exactly one segment"
    );
}

#[test]
fn query_matches_the_golden_ranking_exactly() {
    let ranking = load("ranking.json");
    if ranking["status"].as_str() == Some("pending_host_mint") {
        // Still a failure, and deliberately so: FR-013 wants this red until the operations exist.
        // It must never be silently skipped (FR-028) — the panic names the reason.
        panic!(
            "ranking.json is still a placeholder — mint it with \
             `reference/gen_001_fixtures.py --emit-ranking` once spike_index/spike_query land (PR 2)"
        );
    }

    let corpus = load("corpus.json");
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().to_string_lossy().into_owned();
    let documents: Vec<SpikeDocument> = corpus_documents()
        .into_iter()
        .map(|(external_id, text)| SpikeDocument { external_id, text })
        .collect();
    spike_index(path.clone(), documents).expect("spike_index must succeed");

    let k = ranking["k"].as_u64().expect("k") as u32;
    let hits = spike_query(path, corpus["query"].as_str().expect("query").to_owned(), k)
        .expect("spike_query must succeed");

    let expected = ranking["hits"].as_array().expect("hits");
    assert_eq!(
        hits.len(),
        expected.len(),
        "hit count must match the golden exactly"
    );
    for (got, want) in hits.iter().zip(expected) {
        assert_eq!(
            got.external_id,
            want["external_id"].as_str().expect("external_id")
        );
        // Bit-exact, via the recorded f32 bit pattern — comparing decimals would let a lost ULP
        // from JSON round-tripping be written up as an iOS determinism finding.
        let want_bits =
            u32::try_from(want["score_bits"].as_u64().expect("score_bits")).expect("u32");
        assert_eq!(
            got.score.to_bits(),
            want_bits,
            "scores must be bit-identical (Principle VI)"
        );
        assert_eq!(
            u64::from(got.segment_ord),
            want["segment_ord"].as_u64().expect("segment_ord")
        );
        assert_eq!(
            u64::from(got.doc_id),
            want["doc_id"].as_u64().expect("doc_id")
        );
    }
}
