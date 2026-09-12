//! US1 scenarios 3, 5, 6 — the embedding oracle (spec FR-006, SC-001). Model-backed.
//!
//! Tokenization is checked BEFORE the vectors so a tokenizer disagreement is reported as such,
//! not as an embedding failure (001 research D6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{Embedder, TextKind};
use xtriever_dense::{LoadPath, MiniLmEmbedder};

fn load() -> MiniLmEmbedder {
    MiniLmEmbedder::load(&support::model_dir(), LoadPath::Buffered).expect("load pinned model")
}

#[test]
#[ignore = "needs the model"]
fn tokenization_matches_the_reference_for_every_case() {
    let e = load();
    for case in support::embeddings().cases {
        let (ids, mask) = e.tokenize_for_test(&case.text).expect("tokenize");
        assert_eq!(ids, case.input_ids, "{}: input_ids", case.id);
        assert_eq!(mask, case.attention_mask, "{}: attention_mask", case.id);
        assert_eq!(
            mask.iter().sum::<u32>() as usize,
            case.n_real_tokens,
            "{}",
            case.id
        );
    }
}

#[test]
#[ignore = "needs the model"]
fn every_golden_embeds_within_tolerance() {
    let e = load();
    let g = support::embeddings();
    let mut seen = Vec::new();
    for case in &g.cases {
        let out = e
            .embed(&[case.text.as_str()], TextKind::Passage)
            .expect("embed");
        assert_eq!(out.len(), 1, "{}", case.id);
        let v = &out[0];
        assert_eq!(v.len(), g.dim, "{}: dim", case.id);
        let norm = support::norm(v);
        assert!(
            (norm - 1.0).abs() <= g.tolerance.unit_norm_abs,
            "{}: norm {norm}",
            case.id
        );
        let cos = support::cosine(v, &case.vector);
        let mad = support::max_abs(v, &case.vector);
        assert!(
            cos >= g.tolerance.cosine_min,
            "{}: cosine {cos} < {}",
            case.id,
            g.tolerance.cosine_min
        );
        assert!(
            mad <= g.tolerance.max_abs_diff,
            "{}: max abs diff {mad:e} > {}",
            case.id,
            g.tolerance.max_abs_diff
        );
        seen.push(case.id.clone());
    }
    // The spec's named edge cases must have been exercised, not merely present in the file.
    for id in ["long_over_256", "empty", "whitespace_only", "oov_unicode"] {
        assert!(seen.iter().any(|s| s == id), "case {id} was not exercised");
    }
}

#[test]
#[ignore = "needs the model"]
fn over_length_text_is_truncated_like_the_reference() {
    let e = load();
    let g = support::embeddings();
    let long = g.cases.iter().find(|c| c.id == "long_over_256").unwrap();
    let (_, mask) = e.tokenize_for_test(&long.text).unwrap();
    assert_eq!(
        mask.iter().sum::<u32>(),
        256,
        "truncated to exactly the maximum"
    );
    let v = &e.embed(&[long.text.as_str()], TextKind::Passage).unwrap()[0];
    assert!(support::cosine(v, &long.vector) >= g.tolerance.cosine_min);
}
