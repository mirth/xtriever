//! US2 scenarios 2–3 at the Rust level — the FFI's clock feeds the pipeline's budget (FR-007;
//! SC-002 partial). Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_ffi::{IndexHandle, LoadPath, SearchOptions, XtrieverError};

fn open(dir: &std::path::Path) -> std::sync::Arc<IndexHandle> {
    IndexHandle::open(
        dir.to_string_lossy().into_owned(),
        support::embedder_dir().to_string_lossy().into_owned(),
        Some(support::reranker_dir().to_string_lossy().into_owned()),
        LoadPath::Buffered,
    )
    .unwrap()
}

fn budgeted(ms: u64, strict: bool) -> SearchOptions {
    SearchOptions {
        k: 20,
        depth: None,
        rerank_depth: Some(20),
        max_time_ms: Some(ms),
        max_items: None,
        strict,
        explain: true,
    }
}

#[test]
#[ignore = "needs both models"]
fn a_short_time_budget_yields_a_partial_rerank_without_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let ffi = open(tmp.path());
    let mut partial = 0;
    for q in &support::fixture_docs().queries {
        let r = ffi.search(q.text.clone(), budgeted(200, false)).unwrap();
        assert!(r.elapsed_ms < 1000, "{}: {} ms", q.id, r.elapsed_ms);
        assert!(!r.stages.time_limit_ignored);
        if let Some(rr) = &r.stages.rerank
            && rr.skipped.is_none()
            && rr.scored > 0
            && rr.scored < rr.candidates
        {
            partial += 1;
            // Scored first, then the rest.
            let scored = r
                .hits
                .iter()
                .take_while(|h| h.rerank_score.is_some())
                .count();
            assert_eq!(scored as u32, rr.scored);
        }
    }
    assert!(
        partial >= 1,
        "no query was partially re-ranked under 200 ms"
    );
}

#[test]
#[ignore = "needs both models"]
fn a_zero_budget_degrades_by_default_and_errors_in_strict_mode() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let ffi = open(tmp.path());
    let q = support::fixture_docs().queries[0].text.clone();
    let r = ffi.search(q.clone(), budgeted(0, false)).unwrap();
    assert!(r.stages.degraded.is_some(), "{:?}", r.stages);
    assert!(
        r.stages
            .rerank
            .as_ref()
            .is_some_and(|rr| rr.skipped.is_some())
    );
    assert!(!r.hits.is_empty(), "the lexical ranking still comes back");
    match ffi.search(q, budgeted(0, true)) {
        Err(XtrieverError::BudgetExhausted { message }) => assert!(!message.is_empty()),
        other => panic!("expected BudgetExhausted, got {other:?}"),
    }
}
