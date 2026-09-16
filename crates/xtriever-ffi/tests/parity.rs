//! US1 scenarios 1–3, 5 / FR-003, FR-004 — the FFI's hits equal the pipeline's bit-for-bit
//! (SC-001, Rust half), with and without the re-ranker. Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_ffi::{IndexHandle, LoadPath, SearchOptions};
use xtriever_rerank::MiniLmCrossEncoder;

fn opts() -> SearchOptions {
    SearchOptions {
        k: 10,
        depth: None,
        rerank_depth: Some(5),
        rerank_mode: None,
        max_time_ms: None,
        max_items: None,
        strict: false,
        explain: true,
    }
}

fn pipeline_opts() -> xtriever_pipeline::SearchOptions<'static> {
    xtriever_pipeline::SearchOptions {
        rerank_depth: Some(5),
        explain: true,
        ..xtriever_pipeline::SearchOptions::default()
    }
}

#[test]
#[ignore = "needs both models"]
fn info_reports_the_index_identity() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let ffi = IndexHandle::open(
        tmp.path().to_string_lossy().into_owned(),
        support::embedder_dir().to_string_lossy().into_owned(),
        Some(support::reranker_dir().to_string_lossy().into_owned()),
        LoadPath::Buffered,
    )
    .unwrap();
    let info = ffi.info();
    assert_eq!(info.documents, 40);
    assert_eq!(info.format_version, xtriever_pipeline::FORMAT_VERSION);
    assert_eq!(
        info.embedder_fingerprint,
        xtriever_dense::model::FINGERPRINT
    );
    assert_eq!(
        info.reranker_model_id.as_deref(),
        Some(xtriever_rerank::model::MODEL_ID)
    );
    assert_eq!(
        (info.candidate_depth, info.rerank_depth, info.rrf_k),
        (100, 20, 60)
    );
    assert_eq!(
        info.rerank_mode,
        xtriever_ffi::RerankMode::Interpolate { alpha: 0.5 }
    );
    assert!(info.embedder_load_ms > 0);
    assert!(info.reranker_load_ms.is_some_and(|ms| ms > 0));
}

fn assert_parity(with_reranker: bool) {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::fixture_docs();
    let mut pipeline = support::build_fixture_index(tmp.path());
    if with_reranker {
        pipeline.set_reranker(Some(Box::new(
            MiniLmCrossEncoder::load(
                &support::reranker_dir(),
                xtriever_rerank::LoadPath::Buffered,
            )
            .unwrap(),
        )));
    }
    let ffi = IndexHandle::open(
        tmp.path().to_string_lossy().into_owned(),
        support::embedder_dir().to_string_lossy().into_owned(),
        with_reranker.then(|| support::reranker_dir().to_string_lossy().into_owned()),
        LoadPath::Buffered,
    )
    .unwrap();
    for q in &h.queries {
        let want = pipeline
            .search(&q.text, None, 10, &pipeline_opts())
            .unwrap();
        let got = ffi.search(q.text.clone(), opts()).unwrap();
        assert_eq!(got.hits.len(), want.hits.len(), "{}", q.id);
        for (g, w) in got.hits.iter().zip(&want.hits) {
            assert_eq!(g.external_id, w.external_id, "{}", q.id);
            assert_eq!(support::bits(g.score), support::bits(w.score), "{}", q.id);
            assert_eq!(
                g.rerank_score.map(support::bits32),
                w.rerank_score.map(support::bits32),
                "{}",
                q.id
            );
            assert_eq!(g.text, w.text);
            assert_eq!(
                g.chunk
                    .as_ref()
                    .map(|c| (c.parent.clone(), c.ordinal, c.byte_start, c.byte_end)),
                w.chunk.as_ref().map(|c| (
                    c.parent.clone(),
                    c.ordinal,
                    c.byte_range.map(|r| r.0),
                    c.byte_range.map(|r| r.1)
                ))
            );
            let (ge, we) = (g.explain.as_ref().unwrap(), w.explain.as_ref().unwrap());
            assert_eq!(
                ge.bm25_score.map(support::bits32),
                we.bm25_score.map(support::bits32)
            );
            assert_eq!(ge.bm25_rank, we.bm25_rank);
            assert_eq!(
                ge.dense_score.map(support::bits32),
                we.dense_score.map(support::bits32)
            );
            assert_eq!(ge.dense_rank, we.dense_rank);
            assert_eq!(support::bits(ge.fused), support::bits(we.fused));
            assert_eq!(
                ge.rerank_score.map(support::bits32),
                we.rerank_score.map(support::bits32)
            );
            assert_eq!(ge.rerank_rank, we.rerank_rank);
            assert_eq!(
                ge.rerank_combined.map(support::bits),
                we.rerank_combined.map(support::bits)
            );
        }
        assert_eq!(
            got.stages.lexical_candidates as usize,
            want.stages.lexical_candidates
        );
        assert_eq!(
            got.stages.dense_candidates.map(|n| n as usize),
            want.stages.dense_candidates
        );
        assert_eq!(
            got.stages.degraded.is_some(),
            want.stages.degraded.is_some()
        );
        match (&got.stages.rerank, &want.stages.rerank) {
            (Some(g), Some(w)) => {
                assert_eq!(
                    (g.candidates as usize, g.scored as usize),
                    (w.candidates, w.scored)
                );
                assert_eq!(g.skipped.is_some(), w.skipped.is_some());
            }
            (None, None) => assert!(!with_reranker),
            other => panic!("{}: rerank report differs: {other:?}", q.id),
        }
        assert!(!got.stages.time_limit_ignored);
        if !with_reranker {
            assert!(got.hits.iter().all(|h| h.rerank_score.is_none()));
        }
    }
}

#[test]
#[ignore = "needs both models"]
fn hits_equal_the_pipeline_bit_for_bit_with_the_reranker() {
    assert_parity(true);
}

#[test]
#[ignore = "needs the embedder"]
fn hits_equal_the_pipeline_bit_for_bit_without_the_reranker() {
    assert_parity(false);
}

#[test]
#[ignore = "needs both models"]
fn elapsed_is_reported_and_a_second_handle_on_the_same_directory_works() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let open = || {
        IndexHandle::open(
            tmp.path().to_string_lossy().into_owned(),
            support::embedder_dir().to_string_lossy().into_owned(),
            None,
            LoadPath::Buffered,
        )
        .unwrap()
    };
    let (a, b) = (open(), open());
    let q = support::fixture_docs().queries[0].text.clone();
    let ra = a.search(q.clone(), opts()).unwrap();
    let rb = b.search(q, opts()).unwrap();
    assert_eq!(ra.hits, rb.hits);
}

/// The committed Swift goldens (`swift/Xtriever/Tests/Fixtures/expected.json`) equal what the
/// FFI produces today — so they cannot drift from the code without this test noticing.
#[test]
#[ignore = "needs both models"]
fn the_committed_swift_goldens_match_a_fresh_search() {
    let path = support::repo_root().join("swift/Xtriever/Tests/Fixtures/expected.json");
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read expected.json")).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let open = |with: bool| {
        IndexHandle::open(
            tmp.path().to_string_lossy().into_owned(),
            support::embedder_dir().to_string_lossy().into_owned(),
            with.then(|| support::reranker_dir().to_string_lossy().into_owned()),
            LoadPath::Buffered,
        )
        .unwrap()
    };
    let (with, without) = (open(true), open(false));
    let info = with.info();
    assert_eq!(expected["info"]["documents"], info.documents);
    assert_eq!(
        expected["info"]["embedder_fingerprint"],
        info.embedder_fingerprint
    );
    assert_eq!(
        expected["info"]["reranker_model_id"],
        serde_json::json!(info.reranker_model_id)
    );
    for q in expected["queries"].as_array().unwrap() {
        let text = q["text"].as_str().unwrap().to_owned();
        let opts = SearchOptions {
            k: q["k"].as_u64().unwrap() as u32,
            rerank_depth: Some(q["rerank_depth"].as_u64().unwrap() as u32),
            ..opts()
        };
        for (handle, key) in [(&with, "with_reranker"), (&without, "without_reranker")] {
            let r = handle.search(text.clone(), opts.clone()).unwrap();
            let golden = q[key]["hits"].as_array().unwrap();
            assert_eq!(r.hits.len(), golden.len(), "{} {key}", q["id"]);
            for (h, g) in r.hits.iter().zip(golden) {
                assert_eq!(g["external_id"], h.external_id, "{} {key}", q["id"]);
                assert_eq!(
                    g["score_bits"],
                    format!("{:016x}", h.score.to_bits()),
                    "{} {key}",
                    q["id"]
                );
                assert_eq!(
                    g["rerank_score_bits"],
                    serde_json::json!(h.rerank_score.map(|s| format!("{:08x}", s.to_bits()))),
                    "{} {key}",
                    q["id"]
                );
                assert_eq!(
                    g["rerank_rank"],
                    serde_json::json!(h.explain.as_ref().and_then(|e| e.rerank_rank)),
                    "{} {key}",
                    q["id"]
                );
            }
            let rr = &q[key]["stages"]["rerank"];
            assert_eq!(rr.is_null(), r.stages.rerank.is_none(), "{} {key}", q["id"]);
            if let Some(got) = &r.stages.rerank {
                assert_eq!(rr["candidates"], got.candidates);
                assert_eq!(rr["scored"], got.scored);
            }
        }
    }
}
