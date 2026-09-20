//! Every fixture file must hash to the value recorded in `manifest.json`. GREEN at the red
//! checkpoint (spec FR-029).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    files: BTreeMap<String, String>,
    generator_sha256: String,
    /// Feature 026: the generator that recomputed the expectations under format 3, pinned like
    /// the one that minted the rows (review round 3, finding 5).
    rescored_by: String,
    rescored_by_sha256: String,
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn every_fixture_matches_manifest() {
    let manifest: Manifest = support::load_json("manifest.json");
    for name in ["fusion.json", "hybrid.json"] {
        let recorded = manifest
            .files
            .get(name)
            .unwrap_or_else(|| panic!("manifest lacks {name}"));
        let bytes = std::fs::read(support::fixtures_dir().join(name)).expect("read fixture");
        assert_eq!(
            &hex(&Sha256::digest(&bytes)),
            recorded,
            "{name} does not match manifest.json"
        );
    }
    let generator = support::fixtures_dir().join("../../gen_005_fixtures.py");
    let actual = hex(&Sha256::digest(
        std::fs::read(generator).expect("read generator"),
    ));
    assert_eq!(
        actual, manifest.generator_sha256,
        "gen_005_fixtures.py changed; regenerate or --refresh-manifest"
    );
    assert_eq!(manifest.rescored_by, "reference/gen_026_fixtures.py");
    let rescorer = support::fixtures_dir().join("../../gen_026_fixtures.py");
    let actual = hex(&Sha256::digest(
        std::fs::read(rescorer).expect("read rescorer"),
    ));
    assert_eq!(
        actual, manifest.rescored_by_sha256,
        "gen_026_fixtures.py changed; run it with --write"
    );
}

#[test]
fn goldens_have_the_required_shape() {
    let f = support::fusion();
    assert_eq!(f.rrf_k, 60);
    assert_eq!(f.score_abs_tol, 1e-9);
    for id in [
        "both_lists",
        "lexical_only",
        "dense_only",
        "disjoint",
        "identical",
        "reversed",
        "tie_at_k",
        "depth_below_k",
        "k_zero",
        "both_empty",
    ] {
        assert!(
            f.cases.iter().any(|c| c.id == id),
            "missing fusion case {id}"
        );
    }
    let h = support::hybrid();
    assert_eq!(h.dim, 8);
    assert_eq!(h.fingerprint, "table-fp");
    assert!(h.documents.len() >= 30);
    assert!(h.documents.iter().filter(|d| d.chunk.is_some()).count() >= 6);
    assert!(h.queries.iter().any(|q| q.filter.is_some()));
    assert!(
        h.queries.iter().any(|q| q.text.is_empty()),
        "an empty query is an edge case"
    );
    for d in &h.documents {
        assert_eq!(d.vector.len(), 8, "{}", d.external_id);
    }
}
