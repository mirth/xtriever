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

// ── Feature 006: the re-rank stage degrades per stage (US3 scenarios 3, 6; FR-013–FR-015) ──

#[test]
fn a_failing_reranker_degrades_to_the_fused_order_by_default_and_errors_in_strict() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0];
    let plain = index
        .search(&q.text, None, 10, &support::rerank_options(0))
        .unwrap();
    index.set_reranker(Some(Box::new(support::FailingReranker)));
    let r = index
        .search(&q.text, None, 10, &support::rerank_options(5))
        .unwrap();
    assert_eq!(r.hits, plain.hits);
    let rr = r.stages.rerank.as_ref().expect("stage report");
    assert_eq!((rr.candidates, rr.scored), (0, 0));
    assert!(
        matches!(&rr.skipped, Some(DegradeReason::StageError(m)) if m.contains("stub rerank failure")),
        "{:?}",
        rr.skipped
    );
    assert!(r.stages.degraded.is_none(), "the dense stage ran");
    let strict = SearchOptions {
        strict: true,
        ..support::rerank_options(5)
    };
    assert!(matches!(
        index.search(&q.text, None, 10, &strict),
        Err(Error::Model { .. })
    ));
}

#[test]
fn a_wrong_length_or_non_finite_result_is_an_error_in_every_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0];
    for reranker in [
        Box::new(support::WrongLengthReranker) as Box<dyn xtriever_core::Reranker>,
        Box::new(support::NanReranker),
    ] {
        let name = reranker.model_id().to_owned();
        index.set_reranker(Some(reranker));
        for strict in [false, true] {
            let opts = SearchOptions {
                strict,
                ..support::rerank_options(5)
            };
            match index.search(&q.text, None, 10, &opts) {
                Err(Error::Model { model, .. }) => assert_eq!(model, name),
                other => panic!("{name} strict={strict}: expected Error::Model, got {other:?}"),
            }
        }
    }
}

#[test]
fn a_spent_budget_at_check_point_c_skips_the_reranker() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    let reranker = support::TableReranker::from_fn(&h, |id| id as f32);
    let calls = reranker.calls();
    index.set_reranker(Some(Box::new(reranker)));
    let clock = || Duration::from_millis(500);
    let opts = SearchOptions {
        budget: Budget {
            max_time: Some(Duration::from_millis(100)),
            max_items: None,
        },
        elapsed: Some(&clock),
        ..support::rerank_options(5)
    };
    let r = index.search(&h.queries[0].text, None, 10, &opts).unwrap();
    assert!(
        r.stages.degraded.is_some(),
        "the dense stage degraded too (005)"
    );
    let rr = r.stages.rerank.as_ref().unwrap();
    assert_eq!((rr.candidates, rr.scored), (0, 0));
    assert!(matches!(
        rr.skipped,
        Some(DegradeReason::BudgetExceeded {
            elapsed_ms: 500,
            limit_ms: 100
        })
    ));
    assert!(
        calls.lock().unwrap().is_empty(),
        "the re-ranker was not called"
    );
    assert!(r.hits.iter().all(|x| x.rerank_score.is_none()));

    let strict = SearchOptions {
        strict: true,
        ..opts
    };
    // Strict: the dense stage's spent budget fires first (check point A); make the clock pass A
    // and B and fail only at C to see the re-rank message.
    let calls_seen = Cell::new(0u32);
    let staged = || {
        let n = calls_seen.get();
        calls_seen.set(n + 1);
        if n < 2 {
            Duration::ZERO
        } else {
            Duration::from_millis(500)
        }
    };
    let strict_c = SearchOptions {
        elapsed: Some(&staged),
        ..strict
    };
    match index.search(&h.queries[0].text, None, 10, &strict_c) {
        Err(Error::BudgetExhausted(m)) => assert!(m.contains("rerank"), "{m}"),
        other => panic!("expected BudgetExhausted, got {other:?}"),
    }
}

#[test]
fn the_reranker_receives_the_remaining_time_or_none_without_a_clock() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    let reranker = support::TableReranker::from_fn(&h, |id| id as f32);
    let calls = reranker.calls();
    index.set_reranker(Some(Box::new(reranker)));
    // A, B, C: 0, 0, 30 ms.
    let n = Cell::new(0u32);
    let clock = || {
        let i = n.get();
        n.set(i + 1);
        if i < 2 {
            Duration::ZERO
        } else {
            Duration::from_millis(30)
        }
    };
    let opts = SearchOptions {
        budget: Budget {
            max_time: Some(Duration::from_millis(100)),
            max_items: None,
        },
        elapsed: Some(&clock),
        ..support::rerank_options(5)
    };
    let r = index.search(&h.queries[0].text, None, 10, &opts).unwrap();
    assert!(r.stages.degraded.is_none());
    assert_eq!(r.stages.rerank.as_ref().unwrap().scored, 5);
    let received = calls.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].max_time, Some(Duration::from_millis(70)));
    drop(received);

    let no_clock = SearchOptions {
        budget: Budget {
            max_time: Some(Duration::from_millis(100)),
            max_items: None,
        },
        elapsed: None,
        ..support::rerank_options(5)
    };
    let r = index
        .search(&h.queries[0].text, None, 10, &no_clock)
        .unwrap();
    assert!(r.stages.time_limit_ignored);
    let received = calls.lock().unwrap();
    assert_eq!(received.len(), 2);
    assert_eq!(received[1].max_time, None);
}

#[test]
fn a_degraded_dense_stage_does_not_skip_the_reranker() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let mut index = HybridIndex::open(tmp.path(), Box::new(support::FailingEmbedder)).unwrap();
    index.set_reranker(Some(Box::new(support::TableReranker::from_fn(&h, |id| {
        id as f32
    }))));
    let r = index
        .search(&h.queries[0].text, None, 10, &support::rerank_options(5))
        .unwrap();
    assert_eq!(r.stages.degraded.as_ref().unwrap().stage, "dense");
    let rr = r.stages.rerank.as_ref().unwrap();
    assert!(rr.scored > 0 && rr.skipped.is_none(), "{rr:?}");
    // The scored prefix is ordered by the stub's score (descending id).
    let (scored, _) = support::rerank_split(&r.hits);
    for w in scored.windows(2) {
        assert!(w[0].1 >= w[1].1);
    }
}
