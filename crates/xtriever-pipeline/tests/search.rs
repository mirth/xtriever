//! US2 scenarios 1, 3, 5, 6 — one fused ranking (spec FR-008, FR-011–FR-013; SC-003, SC-004).
//! The composition check runs the two stages DIRECTLY on the sub-indexes and fuses with the
//! crate's `rrf` (itself verified against the Python oracle in `fusion_golden.rs`). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{DocId, Embedder, Filter, LexicalIndex, LexicalQuery, TextKind, VectorIndex};
use xtriever_pipeline::{SearchOptions, rrf};

fn explained() -> SearchOptions<'static> {
    SearchOptions {
        explain: true,
        ..SearchOptions::default()
    }
}

#[test]
fn pipeline_search_equals_rrf_of_the_stages_searched_directly() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let (lexical, dense) = support::open_stages(tmp.path());
    let embedder = support::TableEmbedder::from_fixture(&h);
    let ids = |r: &xtriever_pipeline::Response| r.hits.iter().map(|x| x.id).collect::<Vec<_>>();
    for q in &h.queries {
        let r = index
            .search(&q.text, q.filter.as_ref(), 100, &explained())
            .unwrap();
        assert!(r.stages.degraded.is_none(), "{}: {:?}", q.id, r.stages);
        let set = q
            .filter
            .as_ref()
            .map(|f| lexical.resolve_filter(f).unwrap());
        let lex_filter = set.as_ref().map(|s| Filter::Ids(s.iter().collect()));
        let lex = lexical
            .search(
                &LexicalQuery::Match(None, q.text.clone()),
                lex_filter.as_ref(),
                100,
            )
            .unwrap();
        let v = embedder
            .embed(&[q.text.as_str()], TextKind::Query)
            .unwrap()
            .remove(0);
        let den = dense.search(&v, set.as_ref(), 100).unwrap();
        let expected = rrf(&lex, &den, 60, 100);
        assert_eq!(
            ids(&r),
            expected.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            "{}: order",
            q.id
        );
        for (hit, (_, score)) in r.hits.iter().zip(&expected) {
            assert_eq!(
                hit.score.to_bits(),
                score.to_bits(),
                "{}: fused score bits",
                q.id
            );
        }
        assert_eq!(r.stages.lexical_candidates, lex.len());
        assert_eq!(r.stages.dense_candidates, Some(den.len()));
        // The explanation's dense ranks reproduce the fixture's dense oracle (the stage saw the
        // right vectors, through the pipeline).
        let want: Vec<&str> = q.expected_dense.iter().map(|e| e.id.as_str()).collect();
        let mut got: Vec<(u32, &str)> = r
            .hits
            .iter()
            .filter_map(|x| {
                x.explain
                    .as_ref()
                    .and_then(|e| e.dense_rank)
                    .map(|rk| (rk, x.external_id.as_str()))
            })
            .collect();
        got.sort_unstable();
        let got_ids: Vec<&str> = got.iter().map(|(_, id)| *id).collect();
        assert_eq!(
            got_ids,
            want[..got_ids.len()],
            "{}: dense ranks vs oracle",
            q.id
        );
        assert!(got_ids.len() == want.len().min(100));
        for x in &r.hits {
            if let Some(e) = &x.explain
                && let Some(ds) = e.dense_score
            {
                let oracle = q
                    .expected_dense
                    .iter()
                    .find(|d| d.id == x.external_id)
                    .unwrap()
                    .score;
                assert!(
                    (f64::from(ds) - oracle).abs() <= h.score_abs_tol,
                    "{}: {} dense score",
                    q.id,
                    x.external_id
                );
            }
        }
    }
}

#[test]
fn filtered_search_is_the_fusion_of_both_stages_restricted_to_the_same_set() {
    // FR-012: the filter is resolved once and BOTH stages are restricted to that set before
    // ranking; the result is the fusion of the two restricted candidate lists. (It is not, in
    // general, the unrestricted fusion filtered afterwards — ranks shift when candidates are
    // removed — which is why the spec's SC-004 was corrected at implementation time.)
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let (lexical, dense) = support::open_stages(tmp.path());
    let embedder = support::TableEmbedder::from_fixture(&h);
    let mut checked = 0;
    for q in h.queries.iter().filter(|q| q.filter.is_some()) {
        let set = lexical.resolve_filter(q.filter.as_ref().unwrap()).unwrap();
        let filtered = index
            .search(&q.text, q.filter.as_ref(), 100, &explained())
            .unwrap();
        assert!(
            filtered.hits.iter().all(|x| set.contains(x.id)),
            "{}: hit outside the set",
            q.id
        );
        let lex = lexical
            .search(
                &LexicalQuery::Match(None, q.text.clone()),
                Some(&Filter::Ids(set.iter().collect())),
                100,
            )
            .unwrap();
        let v = embedder
            .embed(&[q.text.as_str()], TextKind::Query)
            .unwrap()
            .remove(0);
        let den = dense.search(&v, Some(&set), 100).unwrap();
        let expected: Vec<DocId> = rrf(&lex, &den, 60, 100)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            filtered.hits.iter().map(|x| x.id).collect::<Vec<_>>(),
            expected,
            "{}",
            q.id
        );
        // The filter never changes a stage's score for a document it returns either way.
        let unrestricted = index.search(&q.text, None, 1000, &explained()).unwrap();
        for x in &filtered.hits {
            if let Some(u) = unrestricted.hits.iter().find(|u| u.id == x.id) {
                let (a, b) = (x.explain.as_ref().unwrap(), u.explain.as_ref().unwrap());
                assert_eq!(
                    a.bm25_score.map(f32::to_bits),
                    b.bm25_score.map(f32::to_bits),
                    "{}: {} bm25 score changed under the filter",
                    q.id,
                    x.external_id
                );
                assert_eq!(
                    a.dense_score.map(f32::to_bits),
                    b.dense_score.map(f32::to_bits),
                    "{}: {} dense score changed under the filter",
                    q.id,
                    x.external_id
                );
            }
        }
        checked += 1;
    }
    assert!(checked >= 3);
}

#[test]
fn k_zero_empty_filter_and_empty_query_edge_cases() {
    let tmp = tempfile::tempdir().unwrap();
    let (_h, index) = support::build_from_fixture(tmp.path());
    let r = index.search("zephyr", None, 0, &explained()).unwrap();
    assert!(r.hits.is_empty());
    assert_eq!(r.stages.lexical_candidates, 0);
    assert_eq!(r.stages.dense_candidates, None);
    let r = index.search("zephyr", None, 3, &explained()).unwrap();
    assert_eq!(r.hits.len(), 3);
    let none = Filter::Eq("source".into(), xtriever_core::Value::Keyword("zzz".into()));
    let r = index
        .search("zephyr", Some(&none), 10, &explained())
        .unwrap();
    assert!(r.hits.is_empty());
    assert_eq!(
        (r.stages.lexical_candidates, r.stages.dense_candidates),
        (0, Some(0))
    );
    // Empty query: lexical matches nothing, the dense stage still ranks (the table knows "").
    let r = index.search("", None, 5, &explained()).unwrap();
    assert_eq!(r.hits.len(), 5);
    assert_eq!(r.stages.lexical_candidates, 0);
    assert!(
        r.hits
            .iter()
            .all(|x| x.explain.as_ref().unwrap().bm25_rank.is_none())
    );
    // Depth smaller than k bounds the result.
    let r = index
        .search(
            "zephyr",
            None,
            10,
            &SearchOptions {
                depth: Some(2),
                ..explained()
            },
        )
        .unwrap();
    assert!(r.hits.len() <= 4);
    assert!(r.stages.lexical_candidates <= 2 && r.stages.dense_candidates.unwrap() <= 2);
}

#[test]
fn equal_fused_scores_are_ordered_by_ascending_internal_id() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let mut found = false;
    for q in &h.queries {
        let r = index
            .search(&q.text, q.filter.as_ref(), 100, &explained())
            .unwrap();
        for w in r.hits.windows(2) {
            assert!(
                w[0].score > w[1].score || (w[0].score == w[1].score && w[0].id < w[1].id),
                "{}: order",
                q.id
            );
            if w[0].score == w[1].score {
                found = true;
            }
        }
    }
    // Equal rank pairs across the two lists tie exactly; at least one such pair exists in the
    // fixture's eight queries (disjoint lexical/dense heads).
    assert!(
        found,
        "no exact fused tie in any fixture query; the fixture should contain one"
    );
    let _ = DocId(0);
}
