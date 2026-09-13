//! The ordering rule against the Python oracle (`pipeline_order.json`, research D8, SC-004).
//! Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::DocId;
use xtriever_pipeline::order_reranked;

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
