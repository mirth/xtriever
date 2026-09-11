//! T015 — ADR-0002 condition 4: both weight loaders must produce the same embedding.
//!
//! This is the test that makes the `unsafe` mmap path self-checking. If the two paths disagree by
//! even one bit, the mmap path is a finding and gets **deleted**, not accommodated.
#![cfg(feature = "spike")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{load, model_dir};
use xtriever_ffi::ffi::{LoadPath, spike_embed};

#[test]
fn buffered_and_mmapped_loaders_agree_bit_for_bit() {
    let corpus = load("corpus.json");
    let sentence = corpus["sentence"].as_str().expect("sentence").to_owned();
    let dir = model_dir().to_string_lossy().into_owned();

    let buffered = spike_embed(dir.clone(), sentence.clone(), LoadPath::Buffered)
        .expect("buffered load must succeed");
    let mmapped = spike_embed(dir, sentence, LoadPath::Mmapped).expect("mmapped load must succeed");

    assert_eq!(buffered.len(), mmapped.len());
    for (i, (b, m)) in buffered.iter().zip(&mmapped).enumerate() {
        // Bit-exact: the two paths feed identical bytes into identical arithmetic, so any
        // difference is a real defect in the mmap path rather than acceptable float drift.
        assert_eq!(
            b.to_bits(),
            m.to_bits(),
            "dimension {i} differs between load paths"
        );
    }
}
