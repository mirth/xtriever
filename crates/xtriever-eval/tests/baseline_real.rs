//! User Story 3 end to end on SciFact (FR-015, FR-020): needs the dataset cache. Builds the
//! index through `xtriever-lexical` — a dev-dependency, so the library graph stays pure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::LexicalIndex;
use xtriever_eval::dataset::{Dataset, Manifest};
use xtriever_eval::report::{EvalReport, score};
use xtriever_eval::run::{EvalConfig, build, execute};
use xtriever_lexical::TantivyIndex;

fn evaluate_scifact() -> EvalReport {
    let m = Manifest::load(&support::repo_root().join("reference/datasets/beir-manifest.json"))
        .unwrap();
    let ds = Dataset::load(
        &m,
        "scifact",
        &support::repo_root().join("reference/datasets/beir"),
    )
    .unwrap();
    let cfg = EvalConfig::lexical_baseline_v1();
    let (schema, docs, ids) = build(&ds, &cfg).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut index = TantivyIndex::create(&dir.path().join("idx"), schema).unwrap();
    index.add(&docs).unwrap();
    index.commit().unwrap();
    let run = execute(&index, &ids, &ds, &cfg).unwrap();
    score(&run, &ds, "test").unwrap()
}

#[test]
#[ignore = "needs reference/datasets/beir (scripts/fetch-beir.sh); ~seconds"]
fn scifact_baseline_is_reproducible_and_inside_the_published_band() {
    let a = evaluate_scifact();
    let b = evaluate_scifact();
    assert_eq!(
        a, b,
        "two runs on the same commit must be identical (SC-004)"
    );
    assert_eq!(a.scored_queries, 300);
    assert_eq!(a.unjudged_queries, 0);
    // FR-020: within ±0.10 of BEIR's published BM25 nDCG@10 for SciFact (research D5). A miss is a
    // finding to record — the band is not to be widened here.
    let published = 0.665;
    assert!(
        (a.mean_ndcg_10 - published).abs() <= 0.10,
        "SciFact nDCG@10 {} is outside {published} ± 0.10 — record as a finding",
        a.mean_ndcg_10
    );
    println!(
        "scifact lexical-baseline-v1: ndcg@10={} recall@100={} (beir-rounded {} / {})",
        a.mean_ndcg_10, a.mean_recall_100, a.beir_rounded.ndcg_10, a.beir_rounded.recall_100
    );
}
