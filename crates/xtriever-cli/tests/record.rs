//! `wiki::record` — canonical JSON equals Python's, the identity is stable and sensitive.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use xtriever_cli::wiki::manifest::Manifest;
use xtriever_cli::wiki::record::{CorpusIdentity, canonical_json, now_rfc3339};

#[test]
fn canonical_json_matches_python_json_dumps() {
    let v: serde_json::Value =
        serde_json::from_str(r#"{"b": [1, 2.5, "é", null, true], "a": {"z": "x\ny", "y": {}}}"#)
            .unwrap();
    // python3 -c 'import json; print(json.dumps(json.loads(...), sort_keys=True, separators=(",", ":"), ensure_ascii=False))'
    assert_eq!(
        canonical_json(&v),
        r#"{"a":{"y":{},"z":"x\ny"},"b":[1,2.5,"é",null,true]}"#
    );
}

#[test]
fn the_identity_is_a_pure_function_of_its_inputs() {
    let m = Manifest::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/datasets/wiki-manifest.json"),
    )
    .unwrap();
    let a = CorpusIdentity::new(&m, "fp-1", None);
    let b = CorpusIdentity::new(&m, "fp-1", None);
    assert_eq!(a.corpus_identity, b.corpus_identity);
    assert_eq!(a.corpus_identity.len(), 64);
    assert_ne!(
        a.corpus_identity,
        CorpusIdentity::new(&m, "fp-2", None).corpus_identity,
        "embedder"
    );
    assert_ne!(
        a.corpus_identity,
        CorpusIdentity::new(&m, "fp-1", Some(2000)).corpus_identity,
        "partial"
    );
    let mut m2 = m.clone();
    m2.exclusions.pop();
    assert_ne!(
        a.corpus_identity,
        CorpusIdentity::new(&m2, "fp-1", None).corpus_identity,
        "rules"
    );
    let mut m3 = m.clone();
    m3.jsonl.sha256 = "0".repeat(64);
    assert_ne!(
        a.corpus_identity,
        CorpusIdentity::new(&m3, "fp-1", None).corpus_identity,
        "snapshot"
    );
    // Counts are not part of it.
    let mut c = a.clone();
    c.counts.passages = 7;
    assert_eq!(
        serde_json::to_value(&c).unwrap()["corpus_identity"],
        serde_json::to_value(&a).unwrap()["corpus_identity"]
    );
}

#[test]
fn rfc3339_now_has_the_shape() {
    let s = now_rfc3339();
    assert_eq!(s.len(), 20, "{s}");
    assert!(
        s.starts_with("20") && s.ends_with('Z') && &s[10..11] == "T",
        "{s}"
    );
}
