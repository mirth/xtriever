//! FR-001 / FR-007 at the conversion level — wire options in, pipeline options out; pipeline
//! response in, wire response out. Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use xtriever_core::{ChunkInfo, DocId};
use xtriever_ffi::{DegradeReason, SearchOptions, from_response, to_pipeline_options};
use xtriever_pipeline::{
    Degradation, DegradeReason as PipelineReason, HitExplain, HybridHit, RerankReport, Response,
    StageReport,
};

fn wire(k: u32) -> SearchOptions {
    SearchOptions {
        k,
        depth: None,
        rerank_depth: None,
        max_time_ms: None,
        max_items: None,
        strict: false,
        explain: false,
    }
}

#[test]
fn options_map_field_by_field_and_attach_the_clock_only_with_a_time_limit() {
    let clock = || Duration::from_millis(7);
    let none = to_pipeline_options(&wire(10), Some(&clock));
    assert_eq!(none.depth, None);
    assert_eq!(none.rerank_depth, None);
    assert_eq!(none.budget.max_time, None);
    assert_eq!(none.budget.max_items, None);
    assert!(!none.strict && !none.explain);
    assert!(none.elapsed.is_none(), "no time limit ⇒ no clock");

    let full = SearchOptions {
        k: 5,
        depth: Some(50),
        rerank_depth: Some(3),
        max_time_ms: Some(200),
        max_items: Some(4),
        strict: true,
        explain: true,
    };
    let p = to_pipeline_options(&full, Some(&clock));
    assert_eq!(p.depth, Some(50));
    assert_eq!(p.rerank_depth, Some(3));
    assert_eq!(p.budget.max_time, Some(Duration::from_millis(200)));
    assert_eq!(p.budget.max_items, Some(4));
    assert!(p.strict && p.explain);
    assert_eq!(p.elapsed.map(|f| f()), Some(Duration::from_millis(7)));
    assert_eq!(
        to_pipeline_options(
            &SearchOptions {
                rerank_depth: Some(0),
                ..wire(1)
            },
            None
        )
        .rerank_depth,
        Some(0)
    );
}

#[test]
fn responses_map_field_by_field() {
    let response = Response {
        hits: vec![
            HybridHit {
                external_id: "d1".into(),
                id: DocId(3),
                score: 0.0327,
                rerank_score: Some(4.5),
                text: "passage one".into(),
                chunk: Some(ChunkInfo {
                    parent: "p".into(),
                    ordinal: 2,
                    byte_range: Some((10, 20)),
                }),
                explain: Some(HitExplain {
                    bm25_score: Some(1.5),
                    bm25_rank: Some(1),
                    dense_score: None,
                    dense_rank: None,
                    fused: 0.0327,
                    rerank_score: Some(4.5),
                    rerank_rank: Some(1),
                }),
            },
            HybridHit {
                external_id: "d2".into(),
                id: DocId(9),
                score: 0.016,
                rerank_score: None,
                text: String::new(),
                chunk: None,
                explain: None,
            },
        ],
        stages: StageReport {
            lexical_candidates: 12,
            dense_candidates: None,
            degraded: Some(Degradation {
                stage: "dense",
                reason: PipelineReason::BudgetExceeded {
                    elapsed_ms: 500,
                    limit_ms: 100,
                },
            }),
            rerank: Some(RerankReport {
                candidates: 5,
                scored: 1,
                skipped: Some(PipelineReason::StageError("boom".into())),
            }),
            time_limit_ignored: false,
        },
    };
    let r = from_response(response, 42);
    assert_eq!(r.elapsed_ms, 42);
    assert_eq!(r.hits.len(), 2);
    let h = &r.hits[0];
    assert_eq!(h.external_id, "d1");
    assert_eq!(h.text, "passage one");
    assert_eq!(h.score.to_bits(), 0.0327f64.to_bits());
    assert_eq!(h.rerank_score, Some(4.5));
    let c = h.chunk.as_ref().unwrap();
    assert_eq!(
        (c.parent.as_str(), c.ordinal, c.byte_start, c.byte_end),
        ("p", 2, Some(10), Some(20))
    );
    let e = h.explain.as_ref().unwrap();
    assert_eq!(
        (e.bm25_score, e.bm25_rank, e.dense_score, e.dense_rank),
        (Some(1.5), Some(1), None, None)
    );
    assert_eq!((e.rerank_score, e.rerank_rank), (Some(4.5), Some(1)));
    assert_eq!(e.fused.to_bits(), 0.0327f64.to_bits());
    assert!(
        r.hits[1].chunk.is_none()
            && r.hits[1].explain.is_none()
            && r.hits[1].rerank_score.is_none()
    );
    let s = &r.stages;
    assert_eq!(s.lexical_candidates, 12);
    assert_eq!(s.dense_candidates, None);
    let d = s.degraded.as_ref().unwrap();
    assert_eq!(d.stage, "dense");
    assert_eq!(
        d.reason,
        DegradeReason::BudgetExceeded {
            elapsed_ms: 500,
            limit_ms: 100
        }
    );
    let rr = s.rerank.as_ref().unwrap();
    assert_eq!((rr.candidates, rr.scored), (5, 1));
    assert_eq!(
        rr.skipped,
        Some(DegradeReason::StageError {
            message: "boom".into()
        })
    );
    assert!(!s.time_limit_ignored);
}
