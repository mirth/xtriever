//! US2 scenarios 2 and 4 — the fusion oracle (spec FR-009/FR-010, SC-001). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{DocId, Hit};
use xtriever_pipeline::rrf;

fn hits(ids: &[u32]) -> Vec<Hit> {
    // RRF ignores scores; give them a shape that would betray any accidental use.
    ids.iter()
        .enumerate()
        .map(|(i, &id)| Hit {
            id: DocId(id),
            score: 100.0 - i as f32,
        })
        .collect()
}

#[test]
fn every_fusion_golden_matches_exactly() {
    let g = support::fusion();
    for case in &g.cases {
        let fused = rrf(&hits(&case.lexical), &hits(&case.dense), g.rrf_k, case.k);
        let got: Vec<u32> = fused.iter().map(|(id, _)| id.0).collect();
        let want: Vec<u32> = case.expected.iter().map(|e| e.id).collect();
        assert_eq!(got, want, "{}: ids/order", case.id);
        for ((_, s), e) in fused.iter().zip(&case.expected) {
            assert!(
                (s - e.score).abs() <= g.score_abs_tol,
                "{}: id {} score {s} vs {}",
                case.id,
                e.id,
                e.score
            );
        }
    }
}

#[test]
fn one_list_only_documents_still_appear() {
    let g = support::fusion();
    let case = g.cases.iter().find(|c| c.id == "disjoint").unwrap();
    let fused = rrf(&hits(&case.lexical), &hits(&case.dense), g.rrf_k, case.k);
    let got: Vec<u32> = fused.iter().map(|(id, _)| id.0).collect();
    // 6 + 6 disjoint ids, k = 10: both lists contribute, interleaved by rank (the test's first
    // draft wrongly expected all 12 — a test bug, the golden is right).
    assert_eq!(got.len(), case.k.min(case.lexical.len() + case.dense.len()));
    assert!(got.iter().filter(|id| case.lexical.contains(id)).count() >= 5);
    assert!(got.iter().filter(|id| case.dense.contains(id)).count() >= 5);
    // Equal ranks in different lists tie exactly and break by ascending id.
    assert_eq!(
        got[0],
        *case.lexical.first().min(case.dense.first()).unwrap()
    );
}

#[test]
fn tie_at_k_goes_to_the_lower_id() {
    let g = support::fusion();
    let case = g.cases.iter().find(|c| c.id == "tie_at_k").unwrap();
    let fused = rrf(&hits(&case.lexical), &hits(&case.dense), g.rrf_k, case.k);
    assert_eq!(fused.len(), case.k);
    let with_one_more = rrf(
        &hits(&case.lexical),
        &hits(&case.dense),
        g.rrf_k,
        case.k + 1,
    );
    assert_eq!(
        with_one_more[case.k - 1].1.to_bits(),
        with_one_more[case.k].1.to_bits(),
        "the k-th and (k+1)-th are an exact tie"
    );
    assert!(with_one_more[case.k - 1].0 < with_one_more[case.k].0);
}
