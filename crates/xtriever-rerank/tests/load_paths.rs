//! FR-003 / ADR-0009 condition 3 — the mapped weight loader scores bit-identically to the
//! buffered one over the golden set. Model-backed; only with the `mmap` feature.
#![cfg(feature = "mmap")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

#[test]
#[ignore = "needs the model"]
fn buffered_and_mapped_scores_are_bit_identical() {
    let buffered = MiniLmCrossEncoder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    let mapped = MiniLmCrossEncoder::load(&support::model_dir(), LoadPath::Mmap).unwrap();
    assert_eq!(buffered.load_path(), LoadPath::Buffered);
    assert_eq!(mapped.load_path(), LoadPath::Mmap);
    for (q, p, _) in support::golden_pairs() {
        assert_eq!(
            buffered.score(&q, &p).unwrap().to_bits(),
            mapped.score(&q, &p).unwrap().to_bits(),
            "{q:.30} / {p:.30}"
        );
    }
}
