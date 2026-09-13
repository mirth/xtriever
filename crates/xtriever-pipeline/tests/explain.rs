//! US4 scenarios 1–3 — every hit explains itself (spec FR-019–FR-021; SC-006). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{Embedder, Filter, LexicalIndex, LexicalQuery, TextKind, VectorIndex};
use xtriever_pipeline::SearchOptions;

#[test]
fn explanations_reproduce_the_stage_lists_and_feature_names() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let (lexical, dense) = support::open_stages(tmp.path());
    let embedder = support::TableEmbedder::from_fixture(&h);
    let names = [
        "bm25.score",
        "bm25.rank",
        "dense.score",
        "dense.rank",
        "fused.score",
        "rerank.score",
        "rerank.rank",
    ];
    let mut both = 0;
    let mut one_only = 0;
    for q in &h.queries {
        let r = index
            .search(
                &q.text,
                q.filter.as_ref(),
                100,
                &SearchOptions {
                    explain: true,
                    ..SearchOptions::default()
                },
            )
            .unwrap();
        let set = q
            .filter
            .as_ref()
            .map(|f| lexical.resolve_filter(f).unwrap());
        let lex = lexical
            .search(
                &LexicalQuery::Match(None, q.text.clone()),
                set.as_ref()
                    .map(|s| Filter::Ids(s.iter().collect()))
                    .as_ref(),
                100,
            )
            .unwrap();
        let v = embedder
            .embed(&[q.text.as_str()], TextKind::Query)
            .unwrap()
            .remove(0);
        let den = dense.search(&v, set.as_ref(), 100).unwrap();
        for hit in &r.hits {
            let e = hit.explain.as_ref().expect("explain requested");
            assert_eq!(e.fused, hit.score);
            let lp = lex.iter().position(|x| x.id == hit.id);
            let dp = den.iter().position(|x| x.id == hit.id);
            assert_eq!(
                e.bm25_rank,
                lp.map(|p| p as u32 + 1),
                "{} {}",
                q.id,
                hit.external_id
            );
            assert_eq!(
                e.dense_rank,
                dp.map(|p| p as u32 + 1),
                "{} {}",
                q.id,
                hit.external_id
            );
            assert_eq!(
                e.bm25_score.map(f32::to_bits),
                lp.map(|p| lex[p].score.to_bits())
            );
            assert_eq!(
                e.dense_score.map(f32::to_bits),
                dp.map(|p| den[p].score.to_bits())
            );
            let f = e.features();
            assert_eq!(
                f.iter().map(|(n, _)| n.0.as_ref()).collect::<Vec<_>>(),
                names
            );
            match (lp, dp) {
                (Some(_), Some(_)) => {
                    both += 1;
                    assert!(f[..5].iter().all(|(_, v)| !v.is_nan()));
                    assert!(f[5].1.is_nan() && f[6].1.is_nan(), "no re-ranker attached");
                }
                (Some(_), None) => {
                    one_only += 1;
                    assert!(f[2].1.is_nan() && f[3].1.is_nan() && !f[0].1.is_nan());
                }
                (None, Some(_)) => {
                    one_only += 1;
                    assert!(f[0].1.is_nan() && f[1].1.is_nan() && !f[2].1.is_nan());
                }
                (None, None) => panic!("a hit retrieved by neither stage"),
            }
            assert!((f[4].1 - hit.score as f32).abs() <= 1e-6);
        }
    }
    assert!(
        both > 0 && one_only > 0,
        "fixture should exercise both cases: both={both} one_only={one_only}"
    );
}

#[test]
fn explanation_never_changes_the_ranking() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    for q in &h.queries {
        let plain = index
            .search(&q.text, q.filter.as_ref(), 50, &SearchOptions::default())
            .unwrap();
        let explained = index
            .search(
                &q.text,
                q.filter.as_ref(),
                50,
                &SearchOptions {
                    explain: true,
                    ..SearchOptions::default()
                },
            )
            .unwrap();
        assert!(plain.hits.iter().all(|x| x.explain.is_none()));
        let strip = |r: &xtriever_pipeline::Response| {
            r.hits
                .iter()
                .map(|x| {
                    (
                        x.id,
                        x.score.to_bits(),
                        x.external_id.clone(),
                        x.chunk.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(strip(&plain), strip(&explained), "{}", q.id);
        assert_eq!(plain.stages, explained.stages);
        assert!(plain.stages.lexical_candidates > 0 || q.text.is_empty());
    }
}

// ── Feature 006: US4 scenarios 1–3 (FR-016; SC-006) ─────────────────────────────────────────

#[test]
fn rerank_fields_are_present_exactly_where_the_stage_scored() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0];
    index.set_reranker(Some(Box::new(
        support::TableReranker::from_fn(&h, |id| id as f32 * 0.5).with_limit(3),
    )));
    let r = index
        .search(&q.text, None, 10, &support::rerank_options(6))
        .unwrap();
    assert_eq!(r.stages.rerank.as_ref().unwrap().scored, 3);
    for (i, hit) in r.hits.iter().enumerate() {
        let e = hit.explain.as_ref().unwrap();
        let f = e.features();
        assert_eq!(f[5].0.0.as_ref(), "rerank.score");
        assert_eq!(f[6].0.0.as_ref(), "rerank.rank");
        if i < 3 {
            assert_eq!(hit.rerank_score, Some(hit.id.0 as f32 * 0.5));
            assert_eq!(e.rerank_score, hit.rerank_score);
            assert_eq!(e.rerank_rank, Some(i as u32 + 1));
            assert_eq!(f[5].1, hit.id.0 as f32 * 0.5);
            assert_eq!(f[6].1, (i + 1) as f32);
        } else {
            assert!(hit.rerank_score.is_none());
            assert!(e.rerank_score.is_none() && e.rerank_rank.is_none());
            assert!(f[5].1.is_nan() && f[6].1.is_nan());
        }
        assert_eq!(e.fused, hit.score);
    }
    // Explanation never changes hits.
    let bare = index
        .search(
            &q.text,
            None,
            10,
            &SearchOptions {
                explain: false,
                ..support::rerank_options(6)
            },
        )
        .unwrap();
    assert_eq!(bare.hits.len(), r.hits.len());
    for (a, b) in bare.hits.iter().zip(&r.hits) {
        assert_eq!(
            (a.id, a.score, a.rerank_score),
            (b.id, b.score, b.rerank_score)
        );
        assert!(a.explain.is_none());
    }
    assert_eq!(bare.stages, r.stages);
}

#[test]
fn a_skipped_reranker_explains_with_absent_rerank_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    index.set_reranker(Some(Box::new(support::FailingReranker)));
    let r = index
        .search(&h.queries[0].text, None, 10, &support::rerank_options(5))
        .unwrap();
    assert!(r.stages.rerank.as_ref().unwrap().skipped.is_some());
    for hit in &r.hits {
        let e = hit.explain.as_ref().unwrap();
        assert!(e.rerank_score.is_none() && e.rerank_rank.is_none());
        assert!(hit.rerank_score.is_none());
    }
}
