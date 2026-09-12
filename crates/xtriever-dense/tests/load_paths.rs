//! ADR-0007 condition 3: the buffered and the memory-mapped weight loaders produce bit-identical
//! embeddings over the whole golden set. Model-backed; needs `--features mmap`.
#![cfg(feature = "mmap")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{Embedder, TextKind};
use xtriever_dense::model::FINGERPRINT;
use xtriever_dense::{LoadPath, MiniLmEmbedder};

#[test]
#[ignore = "needs the model"]
fn buffered_and_mapped_loaders_agree_bit_for_bit() {
    let dir = support::model_dir();
    let buffered = MiniLmEmbedder::load(&dir, LoadPath::Buffered).expect("buffered load");
    let mapped = MiniLmEmbedder::load(&dir, LoadPath::Mmap).expect("mapped load");
    assert_eq!(buffered.fingerprint(), FINGERPRINT);
    assert_eq!(mapped.fingerprint(), FINGERPRINT);
    assert_eq!(mapped.load_path(), LoadPath::Mmap);

    let texts: Vec<String> = support::embeddings()
        .cases
        .into_iter()
        .map(|c| c.text)
        .collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let a = buffered.embed(&refs, TextKind::Passage).unwrap();
    let b = mapped.embed(&refs, TextKind::Passage).unwrap();
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        assert_eq!(
            support::bits(x),
            support::bits(y),
            "text {i} differs between load paths"
        );
    }
}
