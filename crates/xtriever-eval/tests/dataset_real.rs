//! User Story 2 with the real cache (FR-006, FR-009): run with `--run-ignored only` after
//! `scripts/fetch-beir.sh`. Counts are the measured ones from research D1.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_eval::dataset::{Dataset, Manifest};

fn load(name: &str) -> Dataset {
    let m = Manifest::load(&support::repo_root().join("reference/datasets/beir-manifest.json"))
        .unwrap();
    Dataset::load(
        &m,
        name,
        &support::repo_root().join("reference/datasets/beir"),
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
#[ignore = "needs reference/datasets/beir (scripts/fetch-beir.sh)"]
fn scifact_counts_and_integrity() {
    let d = load("scifact");
    assert_eq!(
        (
            d.corpus.ids.len(),
            d.queries.queries.len(),
            d.qrels.grades.len(),
            d.qrels.pairs()
        ),
        (5183, 1109, 300, 339)
    );
    assert!(d.dangling.queries.is_empty() && d.dangling.documents.is_empty());
    assert!(d.corpus.titles.iter().all(|t| !t.is_empty()));
}

#[test]
#[ignore = "needs reference/datasets/beir (scripts/fetch-beir.sh)"]
fn nfcorpus_counts_and_graded_judgements() {
    let d = load("nfcorpus");
    assert_eq!(
        (
            d.corpus.ids.len(),
            d.queries.queries.len(),
            d.qrels.grades.len(),
            d.qrels.pairs()
        ),
        (3633, 3237, 323, 12334)
    );
    assert!(
        d.qrels.grades.values().any(|g| g.values().any(|&v| v == 2)),
        "NFCorpus has grade 2"
    );
    assert!(d.dangling.queries.is_empty() && d.dangling.documents.is_empty());
}

#[test]
#[ignore = "needs reference/datasets/beir (scripts/fetch-beir.sh)"]
fn fiqa_counts_empty_titles_and_id_collisions() {
    let d = load("fiqa");
    assert_eq!(
        (
            d.corpus.ids.len(),
            d.queries.queries.len(),
            d.qrels.grades.len(),
            d.qrels.pairs()
        ),
        (57638, 6648, 648, 1706)
    );
    assert_eq!(
        d.corpus.titles.iter().filter(|t| !t.is_empty()).count(),
        0,
        "FiQA titles are all empty"
    );
    let docs: std::collections::BTreeSet<&str> = d.corpus.ids.iter().map(String::as_str).collect();
    let collisions = d
        .qrels
        .grades
        .keys()
        .filter(|q| docs.contains(q.as_str()))
        .count();
    assert_eq!(collisions, 55, "judged query ids that equal a document id");
    assert!(d.dangling.queries.is_empty() && d.dangling.documents.is_empty());
}
