//! US5 scenarios 1–2 at the library level (offline) — the re-ranked configuration and the
//! additive report keys (spec 006 FR-017, FR-018).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_eval::dataset::Counts;
use xtriever_eval::report::{
    EvalReport, LEXICAL_COMMIT, Observations, Rounded, StageInfo, compare,
};
use xtriever_eval::run::{HybridConfig, RerankConfig, RerankMode};

fn report_with(config: &str, dataset: &str, ndcg: f64, recall: f64) -> EvalReport {
    EvalReport {
        config: config.into(),
        dataset: dataset.into(),
        lexical_commit: LEXICAL_COMMIT.into(),
        harness_commit: "deadbeef".into(),
        dataset_hashes: BTreeMap::new(),
        counts: Counts {
            documents: 0,
            queries: 0,
            judged_queries: 0,
            judgement_pairs: 0,
        },
        scored_queries: 1,
        no_relevant_queries: 0,
        dropped_identical: 0,
        unjudged_queries: 0,
        not_retrieved_queries: 0,
        mean_ndcg_10: ndcg,
        mean_recall_100: recall,
        beir_rounded: Rounded {
            ndcg_10: (ndcg * 1e5).round() / 1e5,
            recall_100: (recall * 1e5).round() / 1e5,
        },
        per_query: BTreeMap::new(),
        observations: None,
        stage: None,
    }
}

fn stage(kind: &str) -> StageInfo {
    StageInfo {
        kind: kind.into(),
        embedder_fingerprint: "fp".into(),
        load_path: "buffered".into(),
        thread_count: 4,
        baseline: "guarded".into(),
        reranker_model_id: None,
        rerank_depth: None,
        rerank_mode: None,
        sparse: None,
    }
}

#[test]
fn hybrid_rerank_v1_is_the_fused_recipe_at_depth_20_and_validates() {
    let cfg = RerankConfig::hybrid_rerank_v1();
    assert_eq!(cfg.name, "hybrid-rerank-v1");
    assert_eq!(cfg.rerank_depth, 20);
    assert_eq!(cfg.hybrid, HybridConfig::hybrid_baseline_v1());
    cfg.validate().unwrap();
    let mut zero = cfg.clone();
    zero.rerank_depth = 0;
    assert!(zero.validate().is_err());
    let mut deep = cfg.clone();
    deep.rerank_depth = cfg.hybrid.k + 1;
    assert!(
        deep.validate().is_err(),
        "depth beyond k re-ranks what cannot be returned"
    );
    let mut bad_hybrid = cfg.clone();
    bad_hybrid.hybrid.k = 10;
    assert!(bad_hybrid.validate().is_err());
}

#[test]
fn the_new_stage_keys_round_trip_and_are_omitted_when_absent() {
    let mut r = report_with("hybrid-rerank-v1", "scifact", 0.7, 0.9);
    r.stage = Some(StageInfo {
        kind: "hybrid-rerank".into(),
        reranker_model_id: Some("cross-encoder/x@rev".into()),
        rerank_depth: Some(20),
        rerank_mode: None,
        sparse: None,
        ..stage("hybrid-rerank")
    });
    let json = serde_json::to_string_pretty(&r).unwrap();
    assert!(json.contains("\"reranker_model_id\": \"cross-encoder/x@rev\""));
    assert!(json.contains("\"rerank_depth\": 20"));
    let back: EvalReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);

    let mut plain = report_with("hybrid-baseline-v1", "scifact", 0.7, 0.9);
    plain.stage = Some(stage("hybrid"));
    let json = serde_json::to_string_pretty(&plain).unwrap();
    assert!(!json.contains("reranker_model_id") && !json.contains("rerank_depth"));
    let keys: Vec<&str> = json
        .lines()
        .filter(|l| l.starts_with("  \""))
        .map(|l| l.trim().split('"').nth(1).unwrap())
        .collect();
    assert_eq!(keys.last(), Some(&"stage"));
}

#[test]
fn the_new_observation_keys_round_trip_and_are_omitted_when_absent() {
    let mut r = report_with("hybrid-rerank-v1", "fiqa", 0.4, 0.7);
    r.observations = Some(Observations {
        index_dir_bytes: 1,
        peak_rss_bytes: 2,
        method: "m".into(),
        embed_corpus_ms: Some(0),
        search_ms: Some(3),
        model_bytes_buffered: None,
        model_bytes_mmapped: None,
        rerank_ms: Some(4),
        rerank_pairs: Some(5),
        rerank_model_bytes_buffered: Some(6),
        rerank_model_bytes_mmapped: Some(7),
    });
    let json = serde_json::to_string_pretty(&r).unwrap();
    for key in [
        "rerank_ms",
        "rerank_pairs",
        "rerank_model_bytes_buffered",
        "rerank_model_bytes_mmapped",
    ] {
        assert!(json.contains(&format!("\"{key}\":")), "{key}");
    }
    assert!(!json.contains("\"model_bytes_buffered\":"));
    let back: EvalReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
    let mut none = r.clone();
    none.observations = Some(Observations {
        rerank_ms: None,
        rerank_pairs: None,
        rerank_model_bytes_buffered: None,
        rerank_model_bytes_mmapped: None,
        ..r.observations.clone().unwrap()
    });
    let json = serde_json::to_string_pretty(&none).unwrap();
    assert!(!json.contains("rerank_"));
}

#[test]
fn the_005_baselines_still_round_trip_byte_for_byte() {
    let dir = support::repo_root().join("specs/005-hybrid-pipeline/baselines");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let text = std::fs::read_to_string(&path).unwrap();
            let report: EvalReport = serde_json::from_str(&text).unwrap();
            let stage = report.stage.as_ref().unwrap();
            assert_eq!(stage.kind, "hybrid");
            assert!(stage.reranker_model_id.is_none() && stage.rerank_depth.is_none());
            let again = serde_json::to_string_pretty(&report).unwrap() + "\n";
            assert_eq!(again, text, "{} changed under round-trip", path.display());
            seen += 1;
        }
    }
    assert_eq!(seen, 3);
}

#[test]
fn compare_hybrid_against_rerank_yields_both_metrics_and_both_names() {
    let a = [report_with("hybrid-baseline-v1", "scifact", 0.68, 0.94)];
    let b = [report_with("hybrid-rerank-v1", "scifact", 0.70, 0.94)];
    let c = compare(&a, &b);
    assert_eq!(
        (c.a_config.as_str(), c.b_config.as_str()),
        ("hybrid-baseline-v1", "hybrid-rerank-v1")
    );
    assert_eq!(c.rows.len(), 2);
    assert!((c.rows[0].abs - 0.02).abs() < 1e-12);
    assert_eq!(c.rows[1].abs, 0.0);
    let md = c.to_markdown();
    assert!(md.contains("hybrid-baseline-v1 → hybrid-rerank-v1"));
    assert!(!md.contains("ADR"));
}

#[test]
fn the_006_baselines_round_trip_byte_for_byte_and_carry_the_stage() {
    let dir = support::repo_root().join("specs/006-rerank-stage/baselines");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let text = std::fs::read_to_string(&path).unwrap();
            let report: EvalReport = serde_json::from_str(&text).unwrap();
            let stage = report.stage.as_ref().unwrap();
            assert_eq!(stage.kind, "hybrid-rerank");
            assert_eq!(stage.rerank_depth, Some(20));
            assert!(
                stage
                    .reranker_model_id
                    .as_deref()
                    .unwrap()
                    .starts_with("cross-encoder/")
            );
            let again = serde_json::to_string_pretty(&report).unwrap() + "\n";
            assert_eq!(again, text, "{} changed under round-trip", path.display());
            seen += 1;
        }
    }
    assert_eq!(seen, 3);
}

// ── Feature 015: hybrid-rerank-v3, the interpolating mode ────────────────────────────────────

#[test]
fn hybrid_rerank_v3_is_v2_hybrid_at_depth_20_interpolated() {
    let v3 = RerankConfig::hybrid_rerank_v3();
    assert_eq!(v3.name, "hybrid-rerank-v3");
    assert_eq!(v3.hybrid, HybridConfig::hybrid_baseline_v2());
    assert_eq!(v3.rerank_depth, 20);
    assert_eq!(v3.mode, RerankMode::Interpolate { alpha: 0.5 });
    v3.validate().unwrap();
    assert_eq!(RerankConfig::hybrid_rerank_v1().mode, RerankMode::Replace);
    assert_eq!(RerankConfig::hybrid_rerank_v2().mode, RerankMode::Replace);
    // The configuration serialises its mode, so a report says which rule produced it.
    let json = serde_json::to_string(&v3).unwrap();
    assert!(
        json.contains("\"mode\":{\"interpolate\":{\"alpha\":0.5}}"),
        "{json}"
    );
    let v2 = serde_json::to_string(&RerankConfig::hybrid_rerank_v2()).unwrap();
    assert!(v2.contains("\"mode\":\"replace\""), "{v2}");
    let back: RerankConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back, v3);
    // The report records the mode beside the depth (review round 1 #3); absent → omitted.
    let mut r = report_with("hybrid-rerank-v3", "scifact", 0.72, 0.955);
    r.stage = Some(StageInfo {
        rerank_depth: Some(20),
        rerank_mode: Some(RerankMode::Interpolate { alpha: 0.5 }),
        ..stage("hybrid-rerank")
    });
    let json = serde_json::to_string_pretty(&r).unwrap();
    assert!(
        json.contains("\"rerank_mode\": {\n      \"interpolate\": {\n        \"alpha\": 0.5"),
        "{json}"
    );
    let back: EvalReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
}
