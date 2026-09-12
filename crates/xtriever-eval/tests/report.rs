//! User Story 4 — reports, deltas and the smoke verdict (FR-017, FR-019, FR-021, FR-022, FR-024).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_eval::dataset::{Counts, Dataset, Manifest};
use xtriever_eval::report::{EvalReport, LEXICAL_COMMIT, Rounded, StageInfo, delta, score, smoke};
use xtriever_eval::run::Run;

fn mini() -> (tempfile::TempDir, Manifest, Dataset) {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, _) = support::synthetic_dataset(dir.path());
    let ds = Dataset::load(&manifest, "mini", dir.path()).unwrap();
    (dir, manifest, ds)
}

fn report_with(dataset: &str, ndcg: f64, recall: f64) -> EvalReport {
    EvalReport {
        config: "lexical-baseline-v1".into(),
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

// Scenario 1 + FR-017/FR-019: every field, in the contract's key order
#[test]
fn score_fills_every_field_in_stable_key_order() {
    let (_dir, _m, ds) = mini();
    let run = Run {
        config: "lexical-baseline-v1".into(),
        dataset: "mini".into(),
        results: [
            ("q1".to_owned(), vec!["d3".to_owned(), "d1".to_owned()]),
            ("q2".to_owned(), vec!["q2".to_owned(), "d2".to_owned()]),
        ]
        .into_iter()
        .collect(),
        unjudged_queries: 0,
    };
    let r = score(&run, &ds, "abc123").expect("score");
    assert_eq!(r.lexical_commit, LEXICAL_COMMIT);
    assert_eq!(r.harness_commit, "abc123");
    assert_eq!(r.dataset_hashes.len(), 3);
    assert_eq!(r.counts, ds.counts);
    assert_eq!(r.scored_queries, 2);
    assert_eq!(
        r.dropped_identical, 1,
        "the self-id result for q2 is dropped at scoring time"
    );
    // a judged query the run does not contain is reported as skipped, with the reason (FR-017)
    let partial = Run {
        results: run
            .results
            .iter()
            .take(1)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        ..run.clone()
    };
    let p = score(&partial, &ds, "abc123").expect("score");
    assert_eq!((p.scored_queries, p.not_retrieved_queries), (1, 1));
    assert_eq!(
        serde_json::to_value(&p).unwrap()["not_retrieved_queries"],
        1
    );
    assert_eq!(r.per_query.len(), 2);
    assert!((0.0..=1.0).contains(&r.mean_ndcg_10));
    assert_eq!(r.beir_rounded.ndcg_10, (r.mean_ndcg_10 * 1e5).round() / 1e5);
    let json = serde_json::to_string_pretty(&r).unwrap();
    let keys: Vec<&str> = json
        .lines()
        .filter(|l| l.starts_with("  \""))
        .map(|l| l.trim().split('"').nth(1).unwrap())
        .collect();
    assert_eq!(
        keys,
        vec![
            "config",
            "dataset",
            "lexical_commit",
            "harness_commit",
            "dataset_hashes",
            "counts",
            "scored_queries",
            "no_relevant_queries",
            "dropped_identical",
            "unjudged_queries",
            "not_retrieved_queries",
            "mean_ndcg_10",
            "mean_recall_100",
            "beir_rounded",
            "per_query"
        ]
    );
    let back: EvalReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r, "round-trips exactly");
    assert!(EvalReport::to_markdown_table(std::slice::from_ref(&r)).contains("mini"));
}

// Scenario 1 (FR-021) + scenario 5 (FR-022)
#[test]
fn delta_table_and_adr_trigger() {
    let before = [
        report_with("scifact", 0.60, 0.90),
        report_with("nfcorpus", 0.30, 0.25),
        report_with("fiqa", 0.20, 0.50),
    ];
    let after = [
        report_with("scifact", 0.66, 0.91),
        report_with("nfcorpus", 0.29, 0.25),
        report_with("fiqa", 0.19, 0.52),
    ];
    let d = delta(&before, &after).unwrap();
    assert_eq!(d.rows.len(), 6);
    let sf = d
        .rows
        .iter()
        .find(|r| r.dataset == "scifact" && r.metric == "ndcg_10")
        .unwrap();
    assert!((sf.abs - 0.06).abs() < 1e-12);
    assert!((sf.rel.unwrap() - 0.1).abs() < 1e-12);
    assert!(d.adr_trigger, "nDCG@10 fell on 2 of 3 datasets");
    let md = d.to_markdown();
    assert!(md.contains("scifact") && md.contains("ndcg_10") && md.contains("majority"));

    let after_ok = [
        report_with("scifact", 0.66, 0.91),
        report_with("nfcorpus", 0.30, 0.25),
        report_with("fiqa", 0.19, 0.52),
    ];
    assert!(
        !delta(&before, &after_ok).unwrap().adr_trigger,
        "one dataset down is not a majority"
    );

    // FR-022: the majority is of the fixed three-dataset set — one dataset falling is not a
    // majority even when it is the only dataset compared (the SciFact smoke case)
    let one_down = delta(&before[..1], &[report_with("scifact", 0.50, 0.90)]).unwrap();
    assert!(!one_down.adr_trigger, "one of three is not a majority");
    let two_down = delta(
        &before[..2],
        &[
            report_with("scifact", 0.50, 0.90),
            report_with("nfcorpus", 0.20, 0.25),
        ],
    )
    .unwrap();
    assert!(two_down.adr_trigger, "two of three is");
    // a zero baseline reports rel as None, not NaN/inf
    let zero = [report_with("scifact", 0.0, 0.0)];
    let d = delta(&zero, &[report_with("scifact", 0.1, 0.1)]).unwrap();
    assert!(d.rows.iter().all(|r| r.rel.is_none()));
    // datasets missing on one side are skipped, not an error
    assert_eq!(delta(&before, &after[..1]).unwrap().rows.len(), 2);
}

// Scenarios 2 & 3 (FR-024), tolerance 0
#[test]
fn smoke_fails_on_any_decrease_and_passes_otherwise() {
    let base = report_with("scifact", 0.665, 0.908);
    let same = report_with("scifact", 0.665, 0.908);
    let d = smoke(&base, &same).expect("unchanged passes");
    assert!(d.rows.iter().all(|r| r.abs == 0.0));
    let up = report_with("scifact", 0.700, 0.910);
    assert!(smoke(&base, &up).is_ok(), "an increase passes");
    let down = report_with("scifact", 0.664_999, 0.908);
    let f = smoke(&base, &down).expect_err("a decrease fails");
    assert_eq!(f.metric, "ndcg_10");
    assert_eq!((f.baseline, f.current), (0.665, 0.664_999));
    assert!(f.to_string().contains("ndcg_10") && f.to_string().contains("0.665"));
    let down_recall = report_with("scifact", 0.665, 0.907);
    assert_eq!(
        smoke(&base, &down_recall)
            .expect_err("recall decrease fails")
            .metric,
        "recall_100"
    );
}

// Feature 004: the report format is extended additively — every committed lexical report
// round-trips byte-for-byte, `stage` serialises last, and `delta` refuses mixed configurations.
#[test]
fn committed_lexical_reports_round_trip_byte_for_byte() {
    let dir = support::repo_root().join("specs/003-eval-harness/baselines");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let text = std::fs::read_to_string(&path).unwrap();
            let report: EvalReport = serde_json::from_str(&text).unwrap();
            assert!(report.stage.is_none());
            let again = serde_json::to_string_pretty(&report).unwrap() + "\n";
            assert_eq!(again, text, "{} changed under round-trip", path.display());
            seen += 1;
        }
    }
    assert_eq!(seen, 3);
}

#[test]
fn stage_is_the_last_key_and_round_trips() {
    let mut r = report_with("scifact", 0.5, 0.8);
    r.config = "dense-baseline-v1".into();
    r.stage = Some(StageInfo {
        kind: "dense".into(),
        embedder_fingerprint: "fp".into(),
        load_path: "buffered".into(),
        thread_count: 4,
        baseline: "absolute".into(),
    });
    let json = serde_json::to_string_pretty(&r).unwrap();
    let keys: Vec<&str> = json
        .lines()
        .filter(|l| l.starts_with("  \""))
        .map(|l| l.trim().split('"').nth(1).unwrap())
        .collect();
    assert_eq!(keys.last(), Some(&"stage"));
    assert_eq!(keys[keys.len() - 2], "per_query");
    let back: EvalReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
}

#[test]
fn delta_refuses_reports_of_different_configurations() {
    let lexical = report_with("scifact", 0.6, 0.9);
    let mut dense = report_with("scifact", 0.5, 0.8);
    dense.config = "dense-baseline-v1".into();
    let err = delta(std::slice::from_ref(&lexical), std::slice::from_ref(&dense)).unwrap_err();
    assert!(
        err.to_string().contains("different configurations"),
        "{err}"
    );
    assert!(
        err.to_string().contains("lexical-baseline-v1")
            && err.to_string().contains("dense-baseline-v1"),
        "{err}"
    );
    let same = delta(std::slice::from_ref(&dense), std::slice::from_ref(&dense)).unwrap();
    assert!(same.rows.iter().all(|r| r.abs == 0.0));
}
