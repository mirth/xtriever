//! Feature 010 (contract §1, research D6): `ids.json` is byte-identical to the file the
//! pre-change code wrote. `reference/fixtures/010/ids-golden.json` was produced by the `IdMap`
//! at commit `1d45490` from `ids-golden-script.json` (see the README there); both tests here
//! are green at the red commit by construction — they exist so the change cannot alter a byte.
//! The full-file write-back oracle (the 008 index's `ids.json`) needs `IdMap` itself and lives
//! in the crate's unit tests (`ids::tests::full_file_write_back_is_byte_identical`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use xtriever_core::{ChunkInfo, FieldName, Value};
use xtriever_pipeline::{HybridIndex, SourceDocument};

#[derive(Deserialize)]
struct Script {
    operations: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    op: String,
    external: String,
    #[serde(default)]
    chunk: Option<ChunkInfo>,
}

fn fixtures_010() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/010")
}

fn script() -> Script {
    serde_json::from_str(
        &std::fs::read_to_string(fixtures_010().join("ids-golden-script.json")).unwrap(),
    )
    .unwrap()
}

fn golden() -> Vec<u8> {
    std::fs::read(fixtures_010().join("ids-golden.json")).unwrap()
}

fn doc(external: &str, chunk: Option<ChunkInfo>) -> (SourceDocument, Vec<f32>) {
    let mut fields = BTreeMap::new();
    fields.insert(
        FieldName::from("title"),
        Value::Text(format!("Title {external}")),
    );
    fields.insert(
        FieldName::from("text"),
        Value::Text(format!("passage of {external}")),
    );
    fields.insert(FieldName::from("source"), Value::Keyword("golden".into()));
    (
        SourceDocument {
            external_id: external.to_owned(),
            fields,
            chunk,
        },
        vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    )
}

/// Replays the script through the public API, committing every seven operations and at the
/// end; returns the index directory and the set of ids the script leaves live.
fn replay(dir: &Path) -> (HybridIndex, BTreeSet<String>) {
    let h = support::hybrid();
    let mut index = HybridIndex::create(
        dir,
        support::fixture_config(&h),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    let mut live = BTreeSet::new();
    for (i, op) in script().operations.iter().enumerate() {
        match op.op.as_str() {
            "assign" => {
                index
                    .add_embedded(&[doc(&op.external, op.chunk.clone())])
                    .unwrap();
                live.insert(op.external.clone());
            }
            "remove" => {
                index.delete(&[op.external.as_str()]).unwrap();
                live.remove(&op.external);
            }
            other => panic!("unknown op {other}"),
        }
        if i % 7 == 6 {
            index.commit().unwrap();
        }
    }
    index.commit().unwrap();
    (index, live)
}

#[test]
fn golden_reproduces_from_replay() {
    let tmp = tempfile::tempdir().unwrap();
    let (index, live) = replay(tmp.path());
    assert_eq!(index.len(), live.len() as u64);
    let written = std::fs::read(tmp.path().join("ids.json")).unwrap();
    assert_eq!(
        written,
        golden(),
        "ids.json differs from the golden:\n{}\nvs\n{}",
        String::from_utf8_lossy(&written),
        String::from_utf8_lossy(&golden())
    );
}

#[test]
fn golden_reproduces_from_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let (index, live) = replay(tmp.path());
    drop(index);
    let h = support::hybrid();
    let embedder = || Box::new(support::TableEmbedder::from_fixture(&h));

    // Reopen, stage nothing, commit (a no-op): the file is untouched.
    let mut index = HybridIndex::open(tmp.path(), embedder()).unwrap();
    index.commit().unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("ids.json")).unwrap(),
        golden()
    );
    for ext in &live {
        assert!(index.contains(ext), "{ext} live after reopen");
    }
    assert!(!index.contains("d004") && !index.contains("p3#1") && !index.contains("nope"));

    // One more removal, commit, reopen: everything else unchanged, the slot reserved.
    let before = index.len();
    index.delete(&["p1#2"]).unwrap();
    index.commit().unwrap();
    drop(index);
    let index = HybridIndex::open(tmp.path(), embedder()).unwrap();
    assert_eq!(index.len(), before - 1);
    for ext in live.iter().filter(|e| e.as_str() != "p1#2") {
        assert!(index.contains(ext), "{ext} still live");
    }
    assert!(!index.contains("p1#2"));
    // The slot count (deleted included) is unchanged: the id space never shrinks.
    let text = std::fs::read_to_string(tmp.path().join("ids.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["external"].as_array().unwrap().len(), 25);
    assert_eq!(
        v["external"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e.is_null())
            .count(),
        3
    );
}
