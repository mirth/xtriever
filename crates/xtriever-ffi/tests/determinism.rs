//! T016 — Principle VI: the same corpus, query and config must yield identical results.
//!
//! An invariant test rather than a golden comparison: it indexes the same corpus twice, into two
//! different directories, and requires the rankings to be identical. This is the property the
//! host-versus-device oracle depends on; if it fails on one machine it cannot hold across two.
#![cfg(feature = "spike")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{corpus_documents, load};
use xtriever_ffi::ffi::{SpikeDocument, spike_index, spike_query};

fn index_and_query(query: &str, k: u32) -> Vec<(String, u32)> {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().to_string_lossy().into_owned();
    let documents: Vec<SpikeDocument> = corpus_documents()
        .into_iter()
        .map(|(external_id, text)| SpikeDocument { external_id, text })
        .collect();
    spike_index(path.clone(), documents).expect("spike_index must succeed");
    spike_query(path, query.to_owned(), k)
        .expect("spike_query must succeed")
        .into_iter()
        .map(|h| (h.external_id, h.score.to_bits()))
        .collect()
}

#[test]
fn repeated_indexing_yields_identical_rankings() {
    let corpus = load("corpus.json");
    let query = corpus["query"].as_str().expect("query");

    let first = index_and_query(query, 10);
    let second = index_and_query(query, 10);

    assert!(
        !first.is_empty(),
        "the fixture query must match at least one document"
    );
    assert_eq!(
        first, second,
        "identical input must give identical output, scores included"
    );
}
