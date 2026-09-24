//! FR-001 / FR-007 at the conversion level — wire options in, pipeline options out; pipeline
//! response in, wire response out. Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use xtriever_core::{ChunkInfo, DocId};
mod support;

use xtriever_ffi::{
    DegradeReason, IndexConfig, IndexHandle, LoadPath, RerankMode, SearchOptions, XtrieverError,
    from_response, to_pipeline_options,
};
use xtriever_pipeline::{
    Degradation, DegradeReason as PipelineReason, HitExplain, HybridHit, RerankReport, Response,
    StageReport,
};

fn wire(k: u32) -> SearchOptions {
    SearchOptions {
        k,
        depth: None,
        rerank_depth: None,
        rerank_mode: None,
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
        rerank_mode: Some(RerankMode::Interpolate { alpha: 0.25 }),
        max_time_ms: Some(200),
        max_items: Some(4),
        strict: true,
        explain: true,
    };
    let p = to_pipeline_options(&full, Some(&clock));
    assert_eq!(p.depth, Some(50));
    assert_eq!(p.rerank_depth, Some(3));
    assert_eq!(
        p.rerank_mode,
        Some(xtriever_pipeline::RerankMode::Interpolate { alpha: 0.25 })
    );
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
                    rerank_combined: Some(0.75),
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
            sparse_skipped: Some(PipelineReason::StageError("cannot tokenise".into())),
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
    assert_eq!(
        e.rerank_combined,
        Some(0.75),
        "Feature 015: the combined score crosses the wire"
    );
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
    // Feature 027: a skipped sparse expansion reaches the bindings.
    assert_eq!(
        s.sparse_skipped,
        Some(DegradeReason::StageError {
            message: "cannot tokenise".into()
        })
    );
}

// ── Feature 015: the re-rank mode on the wire ────────────────────────────────────────────────

#[test]
fn rerank_mode_wire_defaults() {
    let cfg = IndexConfig {
        fields: vec![],
        dense_fields: vec![],
        candidate_depth: 100,
        rrf_k: 60,
        rerank_depth: 20,
        rerank_mode: None,
        dense_compact_dead_share: None,
        sparse: None,
    };
    assert_eq!(
        cfg.rerank_mode, None,
        "None = the engine's default at build"
    );
    assert_eq!(
        wire(1).rerank_mode,
        None,
        "None = the index's mode at search"
    );
    let replace = to_pipeline_options(
        &SearchOptions {
            rerank_mode: Some(RerankMode::Replace),
            ..wire(1)
        },
        None,
    );
    assert_eq!(
        replace.rerank_mode,
        Some(xtriever_pipeline::RerankMode::Replace)
    );
}

/// The pre-015 re-ranked order of the first golden query (query `q0` "zephyr", k 10, depth 5):
/// what `Replace` must still produce — the re-ranker's order over the top five, then the fused
/// order. Minted with the eight-bit artefacts (Feature 026): the head is the float era's, and
/// the tail is the fused tail the re-minted `expected.json` records, where the eight-bit
/// embedder places `d038` before `d008`.
const Q0_REPLACE_ORDER: [&str; 10] = [
    "d016", "d011", "d031", "d026", "d001", "d032", "d020", "d038", "d008", "d030",
];

#[test]
#[ignore = "needs both models"]
fn rerank_mode_defaults_and_override() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let ffi = IndexHandle::open(
        tmp.path().to_string_lossy().into_owned(),
        support::embedder_dir().to_string_lossy().into_owned(),
        Some(support::reranker_dir().to_string_lossy().into_owned()),
        LoadPath::Buffered,
    )
    .unwrap();
    assert_eq!(
        ffi.info().rerank_mode,
        RerankMode::Interpolate { alpha: 0.5 },
        "the default, recorded"
    );
    let base = SearchOptions {
        rerank_depth: Some(5),
        explain: true,
        ..wire(10)
    };
    let replaced = ffi
        .search(
            "zephyr".into(),
            SearchOptions {
                rerank_mode: Some(RerankMode::Replace),
                ..base.clone()
            },
        )
        .unwrap();
    let ids: Vec<&str> = replaced
        .hits
        .iter()
        .map(|h| h.external_id.as_str())
        .collect();
    assert_eq!(ids, Q0_REPLACE_ORDER, "the pre-015 order under Replace");
    assert!(replaced.hits.iter().all(|h| {
        h.explain
            .as_ref()
            .is_some_and(|e| e.rerank_combined.is_none())
    }));
    let interpolated = ffi.search("zephyr".into(), base.clone()).unwrap();
    let head: Vec<&str> = interpolated.hits[..5]
        .iter()
        .map(|h| h.external_id.as_str())
        .collect();
    assert_eq!(
        interpolated
            .hits
            .iter()
            .filter(|h| h.explain.as_ref().unwrap().rerank_combined.is_some())
            .count(),
        5
    );
    assert_eq!(
        &interpolated.hits[5..]
            .iter()
            .map(|h| h.external_id.as_str())
            .collect::<Vec<_>>()[..],
        &Q0_REPLACE_ORDER[5..],
        "the tail is the fused order under both modes"
    );
    let _ = head;
    match ffi.search(
        "zephyr".into(),
        SearchOptions {
            rerank_mode: Some(RerankMode::Interpolate { alpha: 2.0 }),
            ..base
        },
    ) {
        Err(XtrieverError::Schema { .. }) => {}
        other => panic!("{other:?}"),
    }
}
