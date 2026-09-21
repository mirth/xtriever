//! Feature 026 (spec FR-007): an index built with one artefact's embedder refuses to open with
//! the other's, both ways, as a hard error at open naming both fingerprints. The two
//! fingerprints differ only in the artefact they name (`model_pins.rs`), which is exactly what
//! must make them incompatible: eight-bit weights do not produce the float vectors.
//!
//! The always-on tests use stub embedders carrying the two constant fingerprints, so the rule is
//! checked without a model; the model-backed test does it with the real loaders.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{Embedder, Error, Metric, Result, TextKind, Vector};
use xtriever_dense::model::{FINGERPRINT, FINGERPRINT_Q8};
use xtriever_dense::{FlatIndex, LoadPath, MiniLmEmbedder};

struct Stub(&'static str);

impl Embedder for Stub {
    fn dim(&self) -> usize {
        384
    }
    fn metric(&self) -> Metric {
        Metric::Cosine
    }
    fn fingerprint(&self) -> &str {
        self.0
    }
    fn max_input_tokens(&self) -> Option<usize> {
        Some(256)
    }
    fn embed(&self, texts: &[&str], _kind: TextKind) -> Result<Vec<Vector>> {
        Ok(texts.iter().map(|_| vec![1.0; 384]).collect())
    }
}

fn mismatch(result: Result<FlatIndex>) -> (String, String) {
    match result.map(|_| ()).unwrap_err() {
        Error::FingerprintMismatch { index, current } => (index, current),
        other => panic!("expected FingerprintMismatch, got {other:?}"),
    }
}

#[test]
fn a_float_index_refuses_the_eight_bit_embedder_and_the_reverse() {
    for (built_with, opened_with) in [(FINGERPRINT, FINGERPRINT_Q8), (FINGERPRINT_Q8, FINGERPRINT)]
    {
        let tmp = tempfile::tempdir().unwrap();
        drop(FlatIndex::create(tmp.path(), 384, Metric::Cosine, built_with).unwrap());
        let (index, current) = mismatch(FlatIndex::open_for(tmp.path(), &Stub(opened_with)));
        assert_eq!(index, built_with);
        assert_eq!(current, opened_with);
        // The matching embedder still opens it.
        FlatIndex::open_for(tmp.path(), &Stub(built_with)).unwrap();
    }
}

#[test]
#[ignore = "needs both model directories"]
fn with_the_real_loaders() {
    let float = MiniLmEmbedder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    let eight = MiniLmEmbedder::load(&support::model_dir_q8(), LoadPath::Buffered).unwrap();
    assert_ne!(float.fingerprint(), eight.fingerprint());
    for (builder, opener) in [(&float, &eight), (&eight, &float)] {
        let tmp = tempfile::tempdir().unwrap();
        drop(FlatIndex::create(tmp.path(), 384, Metric::Cosine, builder.fingerprint()).unwrap());
        let (index, current) = mismatch(FlatIndex::open_for(tmp.path(), opener));
        assert_eq!(index, builder.fingerprint());
        assert_eq!(current, opener.fingerprint());
    }
}
