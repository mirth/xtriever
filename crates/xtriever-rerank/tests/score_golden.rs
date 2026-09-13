//! US1 scenarios 2, 4, 5 — tokenization parity, the reference score within tolerance, and the
//! reference order per query (spec FR-005, FR-006, SC-001). Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

fn load() -> MiniLmCrossEncoder {
    MiniLmCrossEncoder::load(&support::model_dir(), LoadPath::Buffered).expect("load pinned model")
}

#[test]
#[ignore = "needs the model"]
fn tokenization_matches_the_reference_on_every_pair() {
    let r = load();
    for q in &support::goldens().queries {
        for (i, p) in q.passages.iter().enumerate() {
            let (ids, types) = r.tokenize_for_test(&q.query, &p.text).unwrap();
            assert_eq!(ids, p.input_ids, "{} passage {i}: input_ids", q.name);
            assert_eq!(
                types, p.token_type_ids,
                "{} passage {i}: token_type_ids",
                q.name
            );
        }
    }
}

#[test]
#[ignore = "needs the model"]
fn every_golden_pair_scores_within_tolerance() {
    let r = load();
    let g = support::goldens();
    let mut worst = 0.0f32;
    for q in &g.queries {
        for (i, p) in q.passages.iter().enumerate() {
            let s = r.score(&q.query, &p.text).unwrap();
            let diff = (s - p.score).abs();
            worst = worst.max(diff);
            assert!(
                diff <= g.tolerance_abs,
                "{} passage {i}: {s} vs reference {} (|Δ| = {diff})",
                q.name,
                p.score
            );
        }
    }
    eprintln!("worst |Δ| over the golden set: {worst:.3e}");
}

#[test]
#[ignore = "needs the model"]
fn every_golden_query_order_is_reproduced_exactly() {
    let r = load();
    for q in &support::goldens().queries {
        let scores: Vec<f32> = q
            .passages
            .iter()
            .map(|p| r.score(&q.query, &p.text).unwrap())
            .collect();
        let mut order: Vec<usize> = (0..scores.len()).collect();
        order.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap().then(a.cmp(&b)));
        assert_eq!(order, q.order, "{}: scores {scores:?}", q.name);
    }
}

#[test]
#[ignore = "needs the model"]
fn the_edge_cases_are_present_and_behave_as_the_reference() {
    let r = load();
    let g = support::goldens();
    let find = |name: &str| g.queries.iter().find(|q| q.name == name).unwrap();

    let over = find("over-length");
    let long = over.passages.iter().find(|p| p.truncated).unwrap();
    let (ids, _) = r.tokenize_for_test(&over.query, &long.text).unwrap();
    assert_eq!(ids.len(), 512);

    let empty = find("empty-passage");
    let (ids, types) = r.tokenize_for_test(&empty.query, "").unwrap();
    assert_eq!(ids.first(), Some(&101));
    assert_eq!(ids.last(), Some(&102));
    assert_eq!(ids.iter().filter(|&&id| id == 102).count(), 1);
    assert!(types.iter().all(|&t| t == 0));
    assert!(r.score(&empty.query, "").unwrap().is_finite());

    let eq = find("empty-query");
    assert!(r.score("", &eq.passages[0].text).unwrap().is_finite());
    let both = find("both-empty");
    assert_eq!(r.tokenize_for_test("", "").unwrap().0, vec![101, 102]);
    assert!(r.score("", "").unwrap().is_finite());
    assert!(!both.passages.is_empty());
}
