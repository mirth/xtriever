//! Every fixture file must hash to the value recorded in `manifest.json`. GREEN at the red
//! checkpoint: it is what makes "fails for the right reason" mechanical (spec FR-024).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    files: BTreeMap<String, String>,
    generator_sha256: String,
}

#[test]
fn every_fixture_matches_manifest() {
    let manifest: Manifest = support::load_json("manifest.json");
    for name in ["rerank.json", "pipeline_order.json"] {
        let recorded = manifest
            .files
            .get(name)
            .unwrap_or_else(|| panic!("manifest lacks {name}"));
        let bytes = std::fs::read(support::fixtures_dir().join(name)).expect("read fixture");
        let actual = hex(&Sha256::digest(&bytes));
        assert_eq!(&actual, recorded, "{name} does not match manifest.json");
    }
    let generator = support::fixtures_dir().join("../../gen_006_fixtures.py");
    let actual = hex(&Sha256::digest(
        std::fs::read(generator).expect("read generator"),
    ));
    assert_eq!(
        actual, manifest.generator_sha256,
        "gen_006_fixtures.py changed; regenerate or --refresh-manifest"
    );
}

#[test]
fn goldens_have_the_required_shape() {
    let g = support::goldens();
    assert_eq!(g.max_tokens, 512);
    assert!((g.tolerance_abs - 1e-3).abs() < 1e-9);
    assert!((g.min_gap - 1e-2).abs() < 1e-9);
    assert!(g.queries.len() >= 8);
    for name in [
        "over-length",
        "empty-passage",
        "empty-query",
        "both-empty",
        "near-tie",
    ] {
        assert!(
            g.queries.iter().any(|q| q.name == name),
            "missing case {name}"
        );
    }
    for q in &g.queries {
        assert_eq!(q.order.len(), q.passages.len(), "{}", q.name);
        // `order` is the descending-score permutation and every adjacent gap clears min_gap.
        let scores: Vec<f32> = q.order.iter().map(|&i| q.passages[i].score).collect();
        for w in scores.windows(2) {
            assert!(w[0] - w[1] >= g.min_gap, "{}: gap {}", q.name, w[0] - w[1]);
        }
        for p in &q.passages {
            assert_eq!(p.input_ids.len(), p.token_type_ids.len(), "{}", q.name);
            assert!(p.input_ids.len() <= 512, "{}", q.name);
            assert_eq!(p.truncated, p.input_ids.len() == 512, "{}", q.name);
            assert!(p.score.is_finite(), "{}", q.name);
        }
    }
    let over = g.queries.iter().find(|q| q.name == "over-length").unwrap();
    assert!(over.passages.iter().any(|p| p.truncated));
    let empty = g
        .queries
        .iter()
        .find(|q| q.name == "empty-passage")
        .unwrap();
    let e = empty.passages.iter().find(|p| p.text.is_empty()).unwrap();
    assert_eq!(
        e.input_ids.iter().filter(|&&id| id == 102).count(),
        1,
        "an empty passage encodes as a single sequence (research D4)"
    );
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
