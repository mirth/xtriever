//! SC-010: two embedders loaded from the same pinned files report the same fingerprint and
//! produce bit-identical vectors. Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{Embedder, TextKind};
use xtriever_dense::model::FINGERPRINT;
use xtriever_dense::{LoadPath, MiniLmEmbedder};

#[test]
#[ignore = "needs the model"]
fn two_loads_same_fingerprint_same_bits() {
    let dir = support::model_dir();
    let a = MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap();
    let b = MiniLmEmbedder::load(&dir, LoadPath::Buffered).unwrap();
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_eq!(a.fingerprint(), FINGERPRINT);
    let texts: Vec<String> = support::embeddings()
        .cases
        .into_iter()
        .map(|c| c.text)
        .collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let va = a.embed(&refs, TextKind::Passage).unwrap();
    let vb = b.embed(&refs, TextKind::Passage).unwrap();
    for (i, (x, y)) in va.iter().zip(&vb).enumerate() {
        assert_eq!(support::bits(x), support::bits(y), "text {i}");
    }
}
