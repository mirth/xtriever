//! The ordering rule against the Python oracle (`pipeline_order.json`, research D8, SC-004).
//! Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::DocId;
use xtriever_pipeline::{order_interpolated, order_reranked};

#[test]
fn every_ordering_case_is_exact() {
    let g = support::order_goldens();
    for name in [
        "full",
        "partial-m-lt-d",
        "none-scored",
        "d-gt-k",
        "d-lt-k",
        "d-ge-len",
        "d-zero",
        "ties-by-id",
        "k-lt-scored",
        "single",
    ] {
        assert!(
            g.cases.iter().any(|c| c.name == name),
            "missing case {name}"
        );
    }
    for c in &g.cases {
        // The fused score is the position (descending), which the rule must carry through.
        let fused: Vec<(DocId, f64)> = c
            .fused
            .iter()
            .enumerate()
            .map(|(i, &id)| (DocId(id), 1.0 - i as f64 * 0.01))
            .collect();
        let scores: Vec<Option<f32>> = c.scores.iter().take(c.d).copied().collect();
        let out = order_reranked(&fused, &scores, c.k);
        let got: Vec<(u32, Option<f32>)> = out.iter().map(|(id, _, s)| (id.0, *s)).collect();
        assert_eq!(got, c.expected, "case {}", c.name);
        for (id, fused_score, _) in &out {
            let pos = c.fused.iter().position(|x| x == &id.0).unwrap();
            assert_eq!(
                *fused_score,
                1.0 - pos as f64 * 0.01,
                "case {}: fused score kept",
                c.name
            );
        }
    }
}

// Feature 015: the interpolating rule against the 006 reference's `interpolate_cases`.
#[test]
fn every_interpolation_case_is_exact() {
    let g = support::order_goldens();
    for name in [
        "both-present",
        "single",
        "constant-ce",
        "constant-fused",
        "ties-by-fused-rank",
        "head-shorter-than-d",
        "k-below-head",
        "alpha-zero-is-fused",
        "alpha-one-is-ce",
        "partial-scores",
        "hand-computed-half",
    ] {
        assert!(
            g.interpolate_cases.iter().any(|c| c.name == name),
            "missing interpolate case {name}"
        );
    }
    for c in &g.interpolate_cases {
        let fused: Vec<(DocId, f64)> = c
            .fused
            .iter()
            .zip(&c.fused_scores)
            .map(|(&id, &s)| (DocId(id), s))
            .collect();
        let scores: Vec<Option<f32>> = c.scores.iter().take(c.d).copied().collect();
        let out = order_interpolated(&fused, &scores, c.k, c.alpha);
        let got: Vec<(u32, Option<f64>)> = out.iter().map(|(id, _, _, cmb)| (id.0, *cmb)).collect();
        let got_ids: Vec<u32> = got.iter().map(|(id, _)| *id).collect();
        let want_ids: Vec<u32> = c.expected.iter().map(|(id, _)| *id).collect();
        assert_eq!(got_ids, want_ids, "case {}", c.name);
        for ((_, g), (_, w)) in got.iter().zip(&c.expected) {
            match (g, w) {
                (Some(g), Some(w)) => {
                    assert!((g - w).abs() <= 1e-12, "case {}: {g} vs {w}", c.name)
                }
                (None, None) => {}
                other => panic!("case {}: combined {other:?}", c.name),
            }
        }
        // The cross-encoder score rides along for the head, the fused score for every hit.
        for (id, fused_score, rerank, combined) in &out {
            let pos = c.fused.iter().position(|x| x == &id.0).unwrap();
            assert_eq!(*fused_score, c.fused_scores[pos], "case {}", c.name);
            assert_eq!(rerank.is_some(), combined.is_some(), "case {}", c.name);
        }
    }
}
