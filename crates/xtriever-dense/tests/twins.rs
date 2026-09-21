//! Feature 026: the BERT-over-GGUF encoder and the GGUF header reader are duplicated in
//! `xtriever-dense` and `xtriever-rerank`, because the downward dependency rule (Principle V)
//! gives the two stage crates no shared home below `core` and `core` stays engine-free. A copy
//! that drifts is worse than a copy: this test fails the moment the two differ by a byte, so a
//! fix lands in both or in neither (review, PR B round 1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

#[test]
fn the_encoder_and_header_reader_are_byte_identical_in_both_stage_crates() {
    let dense = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let rerank = Path::new(env!("CARGO_MANIFEST_DIR")).join("../xtriever-rerank/src");
    for file in ["quantised_bert.rs", "gguf_header.rs"] {
        let a = std::fs::read(dense.join(file)).expect("dense copy");
        let b = std::fs::read(rerank.join(file)).expect("rerank copy");
        assert!(
            a == b,
            "src/{file} differs between xtriever-dense and xtriever-rerank; apply the change to both"
        );
    }
}
