//! Every fixture file must hash to the value recorded in `manifest.json` (FR-033). This test is
//! GREEN at the red checkpoint: it is what makes "fails for the right reason" mechanical.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    generator: String,
    seed: u64,
    files: BTreeMap<String, String>,
}

#[test]
fn every_fixture_matches_manifest() {
    let manifest: Manifest = support::load_json("manifest.json");
    assert_eq!(manifest.generator, "gen_002_fixtures.py");
    assert_eq!(manifest.seed, 2);
    let expected_files = [
        "schema.json",
        "corpus.json",
        "queries.json",
        "filters.json",
        "stats.json",
        "mutations.json",
    ];
    for name in expected_files {
        let recorded = manifest
            .files
            .get(name)
            .unwrap_or_else(|| panic!("manifest lacks {name}"));
        let bytes = std::fs::read(support::fixtures_dir().join(name)).expect("read fixture");
        let actual = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            &actual, recorded,
            "{name}: hash mismatch — a golden was edited by hand?"
        );
    }
    assert_eq!(
        manifest.files.len(),
        expected_files.len(),
        "manifest lists unexpected files"
    );
}

#[test]
fn fixtures_deserialize_into_core_types() {
    let schema = support::fixture_schema();
    assert_eq!(schema.fields.len(), 11);
    let corpus = support::corpus();
    assert_eq!(corpus.documents.len(), 1000);
    assert!(
        corpus
            .documents
            .iter()
            .enumerate()
            .all(|(i, d)| d.id.0 as usize == i),
        "ids must be 0..999 in order"
    );
    let queries = support::queries();
    assert!(queries.queries.len() >= 18);
    let _filters: support::Filters = support::load_json("filters.json");
    let _stats: support::Stats = support::load_json("stats.json");
    let _mutations: support::Mutations = support::load_json("mutations.json");
}
