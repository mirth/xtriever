//! The golden file must hash to the manifest's value; GREEN at the red checkpoint.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    generator: String,
    seed: u64,
    files: BTreeMap<String, String>,
}

#[test]
fn goldens_match_manifest() {
    let m: Manifest = support::load_json(&support::fixtures_dir().join("manifest.json"));
    assert_eq!(m.generator, "gen_003_fixtures.py");
    assert_eq!(m.seed, 3);
    let bytes = std::fs::read(support::fixtures_dir().join("metrics.json")).unwrap();
    assert_eq!(
        support::sha256_hex(&bytes),
        m.files["metrics.json"],
        "metrics.json was edited by hand?"
    );
    let g = support::goldens();
    assert_eq!(g.tolerance, 1e-6);
    assert_eq!(g.k, 100);
    let names: Vec<&str> = g.cases.iter().map(|c| c.name.as_str()).collect();
    for required in [
        "graded",
        "no_relevant",
        "fewer_than_cutoff",
        "empty_results",
        "unjudged_query",
        "not_retrieved",
        "duplicate_ids",
        "ties_at_cutoff",
        "identical_ids",
        "grade_zero",
        "big_mean",
    ] {
        assert!(names.contains(&required), "missing golden case {required}");
    }
}

#[test]
fn dataset_manifest_parses_and_names_the_three_datasets() {
    let m = xtriever_eval::dataset::Manifest::load(
        &support::repo_root().join("reference/datasets/beir-manifest.json"),
    )
    .expect("manifest loads");
    for name in ["scifact", "nfcorpus", "fiqa"] {
        let d = m.dataset(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(d.files.len(), 3);
    }
    assert!(m.dataset("msmarco").is_err());
}
