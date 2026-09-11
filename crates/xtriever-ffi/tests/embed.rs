//! T014 — the embedding must match the Python reference within the spec's tolerance (FR-015).
#![cfg(feature = "spike")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{as_f32_vec, cosine, load, max_abs_diff, model_dir};
use xtriever_ffi::ffi::{LoadPath, spike_embed};

#[test]
fn embedding_matches_the_python_reference_within_tolerance() {
    let expected = load("embedding.json");
    let corpus = load("corpus.json");
    let reference = as_f32_vec(&expected["vector"]);

    let got = spike_embed(
        model_dir().to_string_lossy().into_owned(),
        corpus["sentence"].as_str().expect("sentence").to_owned(),
        LoadPath::Buffered,
    )
    .expect("spike_embed must succeed");

    assert_eq!(got.len(), 384, "all-MiniLM-L6-v2 produces 384 dimensions");

    // Tolerance, not equality — two independent BERT implementations will not agree bit-for-bit.
    // The band is only defensible because FR-033 pins 32-bit weights; a quantized variant would
    // need a much looser one and would be a weaker oracle.
    let cos = cosine(&got, &reference);
    let diff = max_abs_diff(&got, &reference);
    let cosine_min = expected["cosine_min"].as_f64().expect("cosine_min");
    let max_diff = expected["max_abs_diff"].as_f64().expect("max_abs_diff");

    assert!(
        cos >= cosine_min,
        "cosine {cos} is below the {cosine_min} floor"
    );
    assert!(diff <= max_diff, "max abs diff {diff} exceeds {max_diff}");
}
