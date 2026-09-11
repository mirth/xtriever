//! T013 — the Rust token sequence must equal the Python reference exactly (FR-015).
//!
//! Checked **before** the embedding comparison, and able to fail independently of it. The model's
//! `tokenizer.json` bakes in truncation and padding at 128 while the reference behaviour is 256,
//! so without the override Rust and Python disagree with no iOS involvement at all (research D6).
//! This test is what keeps that failure mode from being misfiled as an embedding bug.
#![cfg(feature = "spike")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{as_u32_vec, load, model_dir};
use xtriever_ffi::spike::embed::tokenize;

#[test]
fn tokenization_matches_the_python_reference_exactly() {
    let expected = load("tokens.json");
    let sentence = expected["sentence"].as_str().expect("sentence");

    let got = tokenize(&model_dir().to_string_lossy(), sentence).expect("tokenize must succeed");

    // Exact equality, no tolerance: token ids are integers and any difference is a real
    // configuration divergence, not float noise.
    assert_eq!(
        got.input_ids,
        as_u32_vec(&expected["input_ids"]),
        "input_ids must match exactly"
    );
    assert_eq!(got.attention_mask, as_u32_vec(&expected["attention_mask"]));
    assert_eq!(got.token_type_ids, as_u32_vec(&expected["token_type_ids"]));
    assert_eq!(
        got.input_ids.len(),
        256,
        "must be padded/truncated to 256, not tokenizer.json's 128"
    );
}
