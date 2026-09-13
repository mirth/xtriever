//! `wiki::manifest` — the pinned snapshot manifest and file verification (data-model
//! "Snapshot manifest"; spec FR-001).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use xtriever_cli::wiki::manifest::{Manifest, verify_file};

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn the_committed_manifest_parses_with_its_pins_and_rules() {
    let m = Manifest::load(&repo().join("reference/datasets/wiki-manifest.json")).unwrap();
    assert_eq!(m.edition, "simple");
    assert_eq!(m.snapshot_date, "2023-11-01");
    assert_eq!(m.parquet.bytes, 156_885_218);
    assert_eq!(
        m.parquet.sha256,
        "31bded16768a47c286becd292079122f5d7d4397a17b87d4250a00ccd581e6f0"
    );
    assert_eq!(m.parquet.rows, 241_787);
    assert_eq!(m.jsonl.file, "simple.jsonl");
    assert_eq!(m.jsonl.lines, 241_787);
    assert_eq!(m.jsonl.sha256.len(), 64, "the JSONL must be pinned");
    assert_eq!(m.exclusions.len(), 3);
    assert_eq!(m.licence.name, "CC BY-SA 4.0");
}

#[test]
fn an_unknown_rule_kind_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(repo().join("reference/datasets/wiki-manifest.json"))
        .unwrap()
        .replace("\"title_suffix\"", "\"title_regex\"");
    let path = dir.path().join("m.json");
    std::fs::write(&path, text).unwrap();
    let err = format!("{:#}", Manifest::load(&path).unwrap_err());
    assert!(err.contains("title_regex"), "{err}");
}

#[test]
fn verify_file_names_the_path_and_both_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.bin");
    std::fs::write(&path, b"hello").unwrap();
    let good = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    verify_file(&path, 5, good).unwrap();
    let err = verify_file(&path, 5, &"0".repeat(64))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("f.bin") && err.contains(good) && err.contains(&"0".repeat(64)),
        "{err}"
    );
    let err = verify_file(&path, 6, good).unwrap_err().to_string();
    assert!(err.contains("6") && err.contains("5"), "{err}");
}
