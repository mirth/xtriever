//! US3 scenarios 1–5 — degradation, strict mode, budgets with a caller-supplied clock
//! (spec FR-014–FR-018; SC-005). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::cell::Cell;
use std::time::Duration;

use xtriever_core::{Budget, Error, Filter, LexicalIndex, LexicalQuery, Value};
use xtriever_pipeline::{DegradeReason, HybridIndex, SearchOptions};

#[test]
fn a_failing_dense_stage_degrades_to_the_lexical_ranking_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let (lexical, _) = support::open_stages(tmp.path());
    let index = HybridIndex::open(tmp.path(), Box::new(support::FailingEmbedder)).unwrap();
    let q = &h.queries[0];
    let r = index
        .search(
            &q.text,
            None,
            100,
            &SearchOptions {
                explain: true,
                ..SearchOptions::default()
            },
        )
        .unwrap();
    let lex = lexical
        .search(&LexicalQuery::Match(None, q.text.clone()), None, 100)
        .unwrap();
    assert_eq!(r.hits.len(), lex.len());
    for (hit, l) in r.hits.iter().zip(&lex) {
        assert_eq!(hit.id, l.id);
        assert_eq!(hit.score, f64::from(l.score));
        let e = hit.explain.as_ref().unwrap();
        assert_eq!(e.bm25_score, Some(l.score));
        assert!(e.dense_score.is_none() && e.dense_rank.is_none());
        assert_eq!(e.fused, hit.score);
    }
    let d = r.stages.degraded.as_ref().expect("degraded");
    assert_eq!(d.stage, "dense");
    assert!(
        matches!(&d.reason, DegradeReason::StageError(m) if m.contains("stub failure")),
        "{:?}",
        d.reason
    );
    assert_eq!(r.stages.dense_candidates, None);
    assert!(!r.stages.time_limit_ignored);
}

#[test]
fn strict_mode_returns_the_dense_stage_error_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let index = HybridIndex::open(tmp.path(), Box::new(support::FailingEmbedder)).unwrap();
    let err = index
        .search(
            &h.queries[0].text,
            None,
            10,
            &SearchOptions {
                strict: true,
                ..SearchOptions::default()
            },
        )
        .unwrap_err();
    match err {
        Error::Model { model, message } => {
            assert_eq!(model, "failing");
            assert_eq!(message, "stub failure");
        }
        other => panic!("{other:?}"),
    }
}

/// Build the fixture index with an embedder whose call counter the test keeps.
fn counted(
    dir: &std::path::Path,
) -> (
    support::Hybrid,
    HybridIndex,
    std::sync::Arc<std::sync::Mutex<usize>>,
) {
    let h = support::hybrid();
    let embedder = support::TableEmbedder::from_fixture(&h);
    let counter = embedder.counter();
    let mut index =
        HybridIndex::create(dir, support::fixture_config(&h), Box::new(embedder)).unwrap();
    let docs: Vec<_> = h
        .documents
        .iter()
        .map(support::FixtureDoc::source)
        .collect();
    index.add(&docs).unwrap();
    index.commit().unwrap();
    (h, index, counter)
}

#[test]
fn a_spent_time_budget_skips_the_dense_stage_before_it_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index, counter) = counted(tmp.path());
    let calls_before = *counter.lock().unwrap();
    let clock = || Duration::from_millis(500);
    let opts = SearchOptions {
        budget: Budget {
            max_time: Some(Duration::from_millis(100)),
            max_items: None,
        },
        elapsed: Some(&clock),
        explain: true,
        ..SearchOptions::default()
    };
    let r = index.search(&h.queries[0].text, None, 10, &opts).unwrap();
    let d = r.stages.degraded.as_ref().unwrap();
    assert_eq!(
        d.reason,
        DegradeReason::BudgetExceeded {
            elapsed_ms: 500,
            limit_ms: 100
        }
    );
    assert_eq!(r.stages.dense_candidates, None);
    assert_eq!(
        *counter.lock().unwrap(),
        calls_before,
        "the dense stage was not called"
    );
    assert!(!r.hits.is_empty());
    assert!(
        r.hits
            .iter()
            .all(|x| x.explain.as_ref().unwrap().dense_rank.is_none())
    );
}

#[test]
fn a_budget_spent_after_the_dense_stage_discards_its_candidates() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index, counter) = counted(tmp.path());
    let calls_before = *counter.lock().unwrap();
    let ticks = Cell::new(0u64);
    let clock = || {
        let t = ticks.get();
        ticks.set(t + 1);
        // First check point (before dense): 0 ms; second (after dense): 500 ms.
        Duration::from_millis(if t == 0 { 0 } else { 500 })
    };
    let opts = SearchOptions {
        budget: Budget {
            max_time: Some(Duration::from_millis(100)),
            max_items: None,
        },
        elapsed: Some(&clock),
        ..SearchOptions::default()
    };
    let r = index.search(&h.queries[0].text, None, 10, &opts).unwrap();
    assert!(matches!(
        r.stages.degraded.as_ref().unwrap().reason,
        DegradeReason::BudgetExceeded {
            elapsed_ms: 500,
            limit_ms: 100
        }
    ));
    assert_eq!(r.stages.dense_candidates, None, "candidates discarded");
    assert!(
        *counter.lock().unwrap() > calls_before,
        "the dense stage did run"
    );
    assert!(ticks.get() >= 2);
}

#[test]
fn strict_mode_with_a_spent_budget_is_budget_exhausted() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let clock = || Duration::from_millis(1);
    let opts = SearchOptions {
        strict: true,
        budget: Budget {
            max_time: Some(Duration::ZERO),
            max_items: None,
        },
        elapsed: Some(&clock),
        ..SearchOptions::default()
    };
    assert!(matches!(
        index
            .search(&h.queries[0].text, None, 10, &opts)
            .unwrap_err(),
        Error::BudgetExhausted(_)
    ));
}

#[test]
fn a_time_limit_without_a_clock_is_ignored_and_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let opts = SearchOptions {
        budget: Budget {
            max_time: Some(Duration::ZERO),
            max_items: None,
        },
        elapsed: None,
        ..SearchOptions::default()
    };
    let r = index.search(&h.queries[0].text, None, 10, &opts).unwrap();
    assert!(r.stages.degraded.is_none());
    assert!(r.stages.time_limit_ignored);
    assert!(r.stages.dense_candidates.is_some());
}

#[test]
fn an_item_budget_caps_the_dense_depth() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let opts = SearchOptions {
        budget: Budget {
            max_time: None,
            max_items: Some(3),
        },
        ..SearchOptions::default()
    };
    let r = index.search(&h.queries[0].text, None, 10, &opts).unwrap();
    assert_eq!(r.stages.dense_candidates, Some(3));
    assert!(
        r.stages.lexical_candidates > 3,
        "the item budget is the dense stage's, not the lexical stage's"
    );
}

#[test]
fn a_lexical_failure_is_an_error_in_every_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let bad = Filter::Eq("no_such_field".into(), Value::Keyword("x".into()));
    for strict in [false, true] {
        let err = index
            .search(
                &h.queries[0].text,
                Some(&bad),
                10,
                &SearchOptions {
                    strict,
                    ..SearchOptions::default()
                },
            )
            .unwrap_err();
        assert!(
            matches!(err, Error::UnknownField(_)),
            "strict={strict}: {err:?}"
        );
    }
}
