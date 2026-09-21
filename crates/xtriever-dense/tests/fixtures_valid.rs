//! Every fixture file must hash to the value recorded in `manifest.json`. GREEN at the red
//! checkpoint: it is what makes "fails for the right reason" mechanical (spec FR-028).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    files: BTreeMap<String, String>,
    generator_sha256: String,
    /// Feature 026: the module the generator scores with (dense format 3's scheme), pinned like
    /// the generator itself, so neither can change without the goldens being regenerated.
    scheme: String,
    scheme_sha256: String,
}

#[test]
fn every_fixture_matches_manifest() {
    let manifest: Manifest = support::load_json("manifest.json");
    for name in ["embeddings.json", "search.json", "mutations.json"] {
        let recorded = manifest
            .files
            .get(name)
            .unwrap_or_else(|| panic!("manifest lacks {name}"));
        let bytes = std::fs::read(support::fixtures_dir().join(name)).expect("read fixture");
        let actual = hex(&Sha256::digest(&bytes));
        assert_eq!(&actual, recorded, "{name} does not match manifest.json");
    }
    // The generator that minted them is pinned too.
    let generator = support::fixtures_dir().join("../../gen_004_fixtures.py");
    let actual = hex(&Sha256::digest(
        std::fs::read(generator).expect("read generator"),
    ));
    assert_eq!(
        actual, manifest.generator_sha256,
        "gen_004_fixtures.py changed; regenerate or --refresh-manifest"
    );
    assert_eq!(manifest.scheme, "reference/dense_format3.py");
    let scheme = support::fixtures_dir().join("../../dense_format3.py");
    let actual = hex(&Sha256::digest(std::fs::read(scheme).expect("read scheme")));
    assert_eq!(
        actual, manifest.scheme_sha256,
        "dense_format3.py changed; regenerate the goldens"
    );
}

#[test]
fn goldens_have_the_required_shape() {
    let e = support::embeddings();
    assert_eq!(e.dim, 384);
    assert!(e.cases.len() >= 12);
    for id in [
        "short",
        "long_over_256",
        "empty",
        "whitespace_only",
        "oov_unicode",
        "punctuation_only",
        "beir_like_title_text",
        "duplicate_of_short",
    ] {
        let c = e
            .cases
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("missing case {id}"));
        assert_eq!(c.vector.len(), 384, "{id}");
        assert_eq!(c.input_ids.len(), 256, "{id}");
        assert_eq!(c.attention_mask.len(), 256, "{id}");
        assert!(
            (support::norm(&c.vector) - 1.0).abs() <= e.tolerance.unit_norm_abs,
            "{id} not unit"
        );
    }
    let long = e.cases.iter().find(|c| c.id == "long_over_256").unwrap();
    assert_eq!(long.n_real_tokens, 256);
    let empty = e.cases.iter().find(|c| c.id == "empty").unwrap();
    assert_eq!(empty.n_real_tokens, 2);

    let s = support::search();
    assert_eq!(s.score_abs_tol, 1e-6);
    for set in &s.sets {
        assert!(!set.designed_ties.is_empty(), "{}: no designed tie", set.id);
        for tie in &set.designed_ties {
            let [a, b] = tie.ids[..] else {
                panic!("tie has two ids")
            };
            let ra = &set.rows.iter().find(|r| r.id == a).unwrap().vector;
            let rb = &set.rows.iter().find(|r| r.id == b).unwrap().vector;
            assert_eq!(
                support::bits(ra),
                support::bits(rb),
                "{}: designed tie rows differ",
                set.id
            );
            assert_eq!(tie.winner, a.min(b));
            // A case exists at exactly that k for that query, with `allowed: null`.
            let q = set.queries.iter().find(|q| q.id == tie.query).unwrap();
            let case = q
                .cases
                .iter()
                .find(|c| c.k == tie.k && c.allowed.is_none())
                .unwrap();
            assert_eq!(
                case.expected.last().map(|e| e.id),
                Some(tie.winner),
                "{}: the winner sits at rank k",
                set.id
            );
        }
    }
    let m = support::mutations();
    assert_eq!(m.dim, 8);
    assert_eq!(m.fingerprint, "test-fp");
    assert!(m.steps.iter().any(|s| matches!(s, support::Step::Reopen)));
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
