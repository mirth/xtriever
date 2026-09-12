//! User Story 1 — the metrics agree with pytrec_eval on every golden (FR-001–FR-004).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_eval::dataset::Qrels;
use xtriever_eval::metrics::{ndcg_at, recall_at, score_queries};

fn qrels(case: &support::Case) -> Qrels {
    Qrels {
        grades: case.qrels.clone(),
    }
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

// Scenario 1: every golden, per-query and mean, within tolerance
#[test]
fn every_golden_case_agrees_with_the_reference() {
    let g = support::goldens();
    for case in &g.cases {
        let m = score_queries(&case.run, &qrels(case));
        assert_eq!(
            m.scored_queries, case.expected.scored_queries,
            "{}: scored_queries",
            case.name
        );
        assert_eq!(
            m.dropped_identical, case.expected.dropped_identical,
            "{}: dropped_identical",
            case.name
        );
        assert_eq!(
            m.per_query.len() as u32,
            case.expected.scored_queries,
            "{}: per_query size",
            case.name
        );
        for (q, e) in &case.expected.per_query {
            let (n, r) = m
                .per_query
                .get(q)
                .unwrap_or_else(|| panic!("{}: query {q} not scored", case.name));
            assert!(
                close(*n, e.ndcg_10, g.tolerance),
                "{}: {q} ndcg {n} vs {}",
                case.name,
                e.ndcg_10
            );
            assert!(
                close(*r, e.recall_100, g.tolerance),
                "{}: {q} recall {r} vs {}",
                case.name,
                e.recall_100
            );
        }
        assert!(
            close(m.mean_ndcg_10, case.expected.mean_ndcg_10, g.tolerance),
            "{}: mean ndcg {} vs {}",
            case.name,
            m.mean_ndcg_10,
            case.expected.mean_ndcg_10
        );
        assert!(
            close(
                m.mean_recall_100,
                case.expected.mean_recall_100,
                g.tolerance
            ),
            "{}: mean recall",
            case.name
        );
    }
}

// Scenario 2 + 6 (FR-003): the documented treatments, checked by count
#[test]
fn documented_treatments_are_counted() {
    let c = support::case("no_relevant");
    let m = score_queries(&c.run, &qrels(&c));
    assert_eq!(m.no_relevant_queries, 1);
    assert_eq!(
        m.scored_queries, 2,
        "a no-relevant query is scored (0.0) and counted"
    );

    let c = support::case("unjudged_query");
    let m = score_queries(&c.run, &qrels(&c));
    assert_eq!(m.unjudged_queries, 1);
    assert_eq!(m.scored_queries, 1, "an unjudged query is ignored");

    let c = support::case("not_retrieved");
    let m = score_queries(&c.run, &qrels(&c));
    assert_eq!(m.not_retrieved_queries, 1);
    assert_eq!(
        m.scored_queries, 1,
        "a judged query absent from the run is excluded from the mean"
    );

    let c = support::case("empty_results");
    let m = score_queries(&c.run, &qrels(&c));
    assert_eq!(
        m.scored_queries, 2,
        "an empty list is scored 0.0 and counted"
    );
    assert_eq!(m.per_query["q1"], (0.0, 0.0));
}

// Scenario 3: fewer results than the cutoff
#[test]
fn fewer_results_than_cutoff_scores_what_was_returned() {
    let grades: BTreeMap<String, u32> = [("d1", 1), ("d2", 1), ("d3", 1)]
        .into_iter()
        .map(|(d, g)| (d.to_owned(), g))
        .collect();
    assert!(close(
        recall_at(&["d2", "d7", "d1"], &grades, 100),
        2.0 / 3.0,
        1e-12
    ));
    let c = support::case("fewer_than_cutoff");
    assert!(close(
        ndcg_at(&["d2", "d7", "d1"], &grades, 10),
        c.expected.per_query["q1"].ndcg_10,
        1e-9
    ));
}

// Scenario 4: graded relevance uses linear gain
#[test]
fn graded_relevance_uses_linear_gain() {
    let grades: BTreeMap<String, u32> = [("d1", 2), ("d2", 1), ("d3", 0)]
        .into_iter()
        .map(|(d, g)| (d.to_owned(), g))
        .collect();
    let n = ndcg_at(&["d2", "d1", "d3", "d4"], &grades, 10);
    let dcg = 1.0 / 2f64.log2() + 2.0 / 3f64.log2();
    let idcg = 2.0 / 2f64.log2() + 1.0 / 3f64.log2();
    assert!(close(n, dcg / idcg, 1e-12), "{n}");
    assert!(
        close(n, 0.859_718_699_852_197_2, 1e-9),
        "matches the pytrec_eval probe value"
    );
}

// duplicate ids count once, at their first rank
#[test]
fn duplicate_ids_count_once_at_first_rank() {
    let grades: BTreeMap<String, u32> = [("d1", 1), ("d2", 1)]
        .into_iter()
        .map(|(d, g)| (d.to_owned(), g))
        .collect();
    let with_dupes = ndcg_at(&["d3", "d1", "d1", "d1", "d2"], &grades, 10);
    let deduped = ndcg_at(&["d3", "d1", "d2"], &grades, 10);
    assert_eq!(with_dupes, deduped);
    assert!(close(
        recall_at(&["d1", "d1", "d1"], &grades, 100),
        0.5,
        1e-12
    ));
}

// ties are scored in the order given — the harness never re-sorts
#[test]
fn order_given_is_order_scored() {
    let grades: BTreeMap<String, u32> = [("d1", 1)]
        .into_iter()
        .map(|(d, g)| (d.to_owned(), g))
        .collect();
    assert!(ndcg_at(&["d1", "d9"], &grades, 10) > ndcg_at(&["d9", "d1"], &grades, 10));
    // and the cutoff is exact: rank 10 counts, rank 11 does not (distinct filler ids — a
    // repeated filler would collapse to one rank under the dedupe rule)
    let filler: Vec<String> = (0..9).map(|i| format!("x{i}")).collect();
    let mut ranked: Vec<&str> = filler.iter().map(String::as_str).collect();
    ranked.push("d1");
    assert!(ndcg_at(&ranked, &grades, 10) > 0.0);
    ranked.insert(0, "y");
    assert_eq!(ndcg_at(&ranked, &grades, 10), 0.0);
}

// no relevant document ⇒ 0.0, not NaN
#[test]
fn no_relevant_document_is_zero_not_nan() {
    let grades: BTreeMap<String, u32> = [("d9", 0)]
        .into_iter()
        .map(|(d, g)| (d.to_owned(), g))
        .collect();
    assert_eq!(ndcg_at(&["d1"], &grades, 10), 0.0);
    assert_eq!(recall_at(&["d1"], &grades, 100), 0.0);
    assert_eq!(ndcg_at(&[], &grades, 10), 0.0);
}
