//! US3 scenarios 1, 2, 4, 5, 6 — the pipeline re-ranks its fused candidates under the ordering
//! rule (spec FR-009, FR-011–FR-013; SC-004). Offline, with stub re-rankers over the 005
//! fixture index.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_core::{Budget, DocId};
use xtriever_pipeline::{HybridIndex, RerankReport, Response, SearchOptions};

fn ids(r: &Response) -> Vec<DocId> {
    r.hits.iter().map(|h| h.id).collect()
}

/// Build the fixture index and attach `reranker`; return the plain (no re-ranker) response too.
fn with_reranker(
    dir: &std::path::Path,
    reranker: Box<dyn xtriever_core::Reranker>,
) -> (support::Hybrid, HybridIndex) {
    let (h, mut index) = support::build_from_fixture(dir);
    index.set_reranker(Some(reranker));
    (h, index)
}

#[test]
fn the_first_d_fused_candidates_are_reordered_by_the_reranker_then_the_rest_follow() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0];
    let plain = index
        .search(&q.text, None, 10, &support::rerank_options(0))
        .unwrap();
    assert!(
        plain.hits.len() >= 8,
        "fixture query must have enough candidates"
    );
    let fused = ids(&plain);
    // Reverse the fused order among the first 5: score(id) = its fused position.
    let pos: BTreeMap<u32, f32> = fused
        .iter()
        .enumerate()
        .map(|(i, id)| (id.0, i as f32))
        .collect();
    drop(index);
    let (_, mut index) = support::build_from_fixture(tmp.path().join("b").as_path());
    let reranker = support::TableReranker::from_fn(&h, |id| *pos.get(&id).unwrap_or(&-1.0));
    index.set_reranker(Some(Box::new(reranker)));

    let r = index
        .search(&q.text, None, 10, &support::rerank_options(5))
        .unwrap();
    let mut expected: Vec<DocId> = fused[..5].to_vec();
    expected.reverse();
    expected.extend_from_slice(&fused[5..10]);
    assert_eq!(ids(&r), expected);
    assert_eq!(
        r.stages.rerank,
        Some(RerankReport {
            candidates: 5,
            scored: 5,
            skipped: None
        })
    );
    let (scored, unscored) = support::rerank_split(&r.hits);
    assert_eq!(scored.len(), 5);
    assert_eq!(unscored.len(), r.hits.len() - 5);
    for h in &r.hits {
        assert_eq!(
            h.score,
            plain.hits.iter().find(|p| p.id == h.id).unwrap().score,
            "the fused score keeps its meaning"
        );
    }
    assert!(r.stages.degraded.is_none());
}

#[test]
fn a_partial_result_puts_the_scored_first_then_every_unscored_in_fused_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0];
    let fused = ids(&index
        .search(&q.text, None, 10, &support::rerank_options(0))
        .unwrap());
    drop(index);
    let (_, mut index) = support::build_from_fixture(tmp.path().join("b").as_path());
    // The second candidate outscores the first; the budget reaches only two of five.
    let first = fused[0].0;
    let reranker =
        support::TableReranker::from_fn(&h, |id| if id == first { 1.0 } else { 2.0 }).with_limit(2);
    index.set_reranker(Some(Box::new(reranker)));
    let r = index
        .search(&q.text, None, 10, &support::rerank_options(5))
        .unwrap();
    let mut expected = vec![fused[1], fused[0]];
    expected.extend_from_slice(&fused[2..10]);
    assert_eq!(ids(&r), expected);
    assert_eq!(
        r.stages.rerank,
        Some(RerankReport {
            candidates: 5,
            scored: 2,
            skipped: None
        })
    );
    assert_eq!(support::rerank_split(&r.hits).0.len(), 2);
    assert!(r.hits[2..].iter().all(|h| h.rerank_score.is_none()));
}

#[test]
fn a_depth_beyond_k_can_promote_a_candidate_from_below_k() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0];
    let fused = ids(&index
        .search(&q.text, None, 10, &support::rerank_options(0))
        .unwrap());
    drop(index);
    let (_, mut index) = support::build_from_fixture(tmp.path().join("b").as_path());
    let seventh = fused[6].0;
    let reranker = support::TableReranker::from_fn(&h, |id| if id == seventh { 10.0 } else { 0.0 });
    index.set_reranker(Some(Box::new(reranker)));
    let r = index
        .search(&q.text, None, 3, &support::rerank_options(10))
        .unwrap();
    assert_eq!(r.hits.len(), 3);
    assert_eq!(r.hits[0].id, fused[6]);
    assert_eq!(r.stages.rerank.as_ref().unwrap().candidates, 10);
    // The remaining two are the best-scored (ties by id) of the other nine.
    let mut rest: Vec<DocId> = fused[..10]
        .iter()
        .copied()
        .filter(|d| *d != fused[6])
        .collect();
    rest.sort_by_key(|d| d.0);
    assert_eq!(&ids(&r)[1..], &rest[..2]);
}

#[test]
fn depth_zero_or_no_reranker_gives_the_feature_005_response() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0];
    let base = index
        .search(&q.text, None, 10, &support::rerank_options(0))
        .unwrap();
    assert_eq!(base.stages.rerank, None);
    assert!(base.hits.iter().all(|h| h.rerank_score.is_none()));
    let default_opts = index
        .search(&q.text, None, 10, &SearchOptions::default())
        .unwrap();
    assert_eq!(default_opts.stages.rerank, None, "no re-ranker attached");
    drop(index);
    let (_, mut index) = support::build_from_fixture(tmp.path().join("b").as_path());
    index.set_reranker(Some(Box::new(support::TableReranker::from_fn(&h, |id| {
        -(id as f32)
    }))));
    let d0 = index
        .search(&q.text, None, 10, &support::rerank_options(0))
        .unwrap();
    assert_eq!(d0.hits, base.hits);
    assert_eq!(d0.stages.rerank, None);
    index.set_reranker(None);
    let detached = index
        .search(&q.text, None, 10, &support::rerank_options(5))
        .unwrap();
    assert_eq!(detached.hits, base.hits);
    assert_eq!(detached.stages.rerank, None);
}

#[test]
fn depth_beyond_the_fused_list_ties_by_id_and_determinism() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0];
    let n = index
        .search(&q.text, None, 100, &support::rerank_options(0))
        .unwrap()
        .hits
        .len();
    index.set_reranker(Some(Box::new(support::TableReranker::from_fn(&h, |_| 1.0))));
    let r = index
        .search(&q.text, None, 100, &support::rerank_options(1000))
        .unwrap();
    assert_eq!(r.stages.rerank.as_ref().unwrap().candidates, n);
    assert_eq!(r.stages.rerank.as_ref().unwrap().scored, n);
    let got = ids(&r);
    let mut sorted = got.clone();
    sorted.sort_by_key(|d| d.0);
    assert_eq!(got, sorted, "equal scores order by ascending internal id");
    let again = index
        .search(&q.text, None, 100, &support::rerank_options(1000))
        .unwrap();
    assert_eq!(r, again);
}

#[test]
fn hits_carry_their_passage_text_and_the_stub_receives_the_item_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let reranker = support::TableReranker::from_fn(&support::hybrid(), |id| id as f32);
    let calls = reranker.calls();
    let (h, index) = with_reranker(tmp.path(), Box::new(reranker));
    let q = &h.queries[0];
    let opts = SearchOptions {
        budget: Budget {
            max_items: Some(7),
            max_time: None,
        },
        ..support::rerank_options(5)
    };
    let r = index.search(&q.text, None, 10, &opts).unwrap();
    for hit in &r.hits {
        let doc = h
            .documents
            .iter()
            .find(|d| d.external_id == hit.external_id)
            .unwrap();
        assert_eq!(hit.text, doc.passage, "{}", hit.external_id);
    }
    let received = calls.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].max_items, Some(7));
    assert_eq!(received[0].max_time, None);
}
