//! Feature 011 US4 — the builder on the wire: the 005 fixture's documents through the new
//! wire types (`IndexConfig`, `FieldDef`, `FieldKind`, `Document`, `FieldValue`) and exports
//! (`create`, `add`, `add_embedded`, `delete`, `commit`, `merge`, `contains`) produce an index
//! whose hits equal the committed goldens bit for bit (SC-007); staged changes stay invisible
//! until commit; every refusal is the engine's. Model-backed (`#[ignore]`, release, `-j 1`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::HashMap;

use xtriever_core::{FieldKind as CoreKind, Value};
use xtriever_ffi::{
    ChunkInfo, Document, FieldDef, FieldKind, FieldValue, IndexConfig, IndexHandle, LoadPath,
    SearchOptions, XtrieverError,
};

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn wire_kind(kind: &CoreKind) -> FieldKind {
    match kind {
        CoreKind::Text(a) => FieldKind::Text {
            analyzer: a.0.clone(),
        },
        CoreKind::Keyword => FieldKind::Keyword,
        CoreKind::U64 => FieldKind::U64,
        CoreKind::I64 => FieldKind::I64,
        CoreKind::F64 => FieldKind::F64,
        CoreKind::Bool => FieldKind::Bool,
        CoreKind::DateMillis => FieldKind::DateMillis,
    }
}

fn wire_value(v: &Value) -> FieldValue {
    match v {
        Value::Text(t) => FieldValue::Text(t.clone()),
        Value::Keyword(k) => FieldValue::Keyword(k.clone()),
        Value::U64(n) => FieldValue::U64(*n),
        Value::I64(n) => FieldValue::I64(*n),
        Value::F64(x) => FieldValue::F64(*x),
        Value::Bool(b) => FieldValue::Bool(*b),
        Value::DateMillis(n) => FieldValue::DateMillis(*n),
    }
}

/// The fixture's schema and dense fields as the wire config, pipeline defaults otherwise.
fn config(h: &support::Hybrid) -> IndexConfig {
    IndexConfig {
        fields: h
            .schema
            .fields
            .iter()
            .map(|f| FieldDef {
                name: f.name.to_string(),
                kind: wire_kind(&f.kind),
                indexed: f.indexed,
                stored: f.stored,
                boost: f.boost,
            })
            .collect(),
        dense_fields: h.dense_fields.iter().map(ToString::to_string).collect(),
        candidate_depth: 100,
        rrf_k: 60,
        rerank_depth: 20,
    }
}

fn document(d: &support::FixtureDoc) -> Document {
    Document {
        external_id: d.external_id.clone(),
        fields: d
            .fields
            .iter()
            .map(|(k, v)| (k.to_string(), wire_value(v)))
            .collect::<HashMap<_, _>>(),
        chunk: d.chunk.as_ref().map(|c| ChunkInfo {
            parent: c.parent.clone(),
            ordinal: c.ordinal,
            byte_start: c.byte_range.map(|r| r.0),
            byte_end: c.byte_range.map(|r| r.1),
        }),
    }
}

fn create(dir: &std::path::Path, h: &support::Hybrid, reranker: bool) -> std::sync::Arc<IndexHandle> {
    IndexHandle::create(
        s(dir),
        config(h),
        s(&support::embedder_dir()),
        reranker.then(|| s(&support::reranker_dir())),
        LoadPath::Buffered,
    )
    .unwrap()
}

fn open(dir: &std::path::Path, reranker: bool) -> std::sync::Arc<IndexHandle> {
    IndexHandle::open(
        s(dir),
        s(&support::embedder_dir()),
        reranker.then(|| s(&support::reranker_dir())),
        LoadPath::Buffered,
    )
    .unwrap()
}

/// Build the fixture through the wire, committed; returns the handle with the re-ranker.
fn build(dir: &std::path::Path, h: &support::Hybrid) -> std::sync::Arc<IndexHandle> {
    let handle = create(dir, h, true);
    handle
        .add(h.documents.iter().map(document).collect())
        .unwrap();
    handle.commit().unwrap();
    handle
}

fn goldens() -> serde_json::Value {
    let path = support::repo_root().join("swift/Xtriever/Tests/Fixtures/expected.json");
    serde_json::from_str(&std::fs::read_to_string(&path).expect("read expected.json")).unwrap()
}

/// Every golden query, both handles: ids, order, fused f64 bits, re-rank f32 bits.
fn assert_goldens(with: &IndexHandle, without: &IndexHandle) {
    let expected = goldens();
    let mut pairs = 0;
    for q in expected["queries"].as_array().unwrap() {
        let text = q["text"].as_str().unwrap().to_owned();
        let opts = SearchOptions {
            k: q["k"].as_u64().unwrap() as u32,
            depth: None,
            rerank_depth: Some(q["rerank_depth"].as_u64().unwrap() as u32),
            max_time_ms: None,
            max_items: None,
            strict: false,
            explain: true,
        };
        for (handle, key) in [(with, "with_reranker"), (without, "without_reranker")] {
            let r = handle.search(text.clone(), opts.clone()).unwrap();
            let golden = q[key]["hits"].as_array().unwrap();
            assert_eq!(r.hits.len(), golden.len(), "{} {key}", q["id"]);
            for (h, g) in r.hits.iter().zip(golden) {
                assert_eq!(g["external_id"], h.external_id, "{} {key}", q["id"]);
                assert_eq!(
                    g["score_bits"],
                    format!("{:016x}", h.score.to_bits()),
                    "{} {key}",
                    q["id"]
                );
                assert_eq!(
                    g["rerank_score_bits"],
                    serde_json::json!(h.rerank_score.map(|x| format!("{:08x}", x.to_bits()))),
                    "{} {key}",
                    q["id"]
                );
                let e = h.explain.as_ref().unwrap();
                assert_eq!(
                    g["bm25_score_bits"],
                    serde_json::json!(e.bm25_score.map(|x| format!("{:08x}", x.to_bits())))
                );
                assert_eq!(
                    g["dense_score_bits"],
                    serde_json::json!(e.dense_score.map(|x| format!("{:08x}", x.to_bits())))
                );
                assert_eq!(g["rerank_rank"], serde_json::json!(e.rerank_rank));
            }
            pairs += 1;
        }
    }
    assert_eq!(pairs, 16);
}

#[test]
#[ignore = "needs both models"]
fn fixture_built_through_the_wire_equals_the_goldens() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::fixture_docs();
    let with = build(tmp.path(), &h);
    let without = open(tmp.path(), false);
    assert_eq!(with.info().documents, 40);
    assert!(with.contains("d001".into()));
    assert!(!with.contains("nope".into()));
    assert_goldens(&with, &without);
}

#[test]
#[ignore = "needs both models"]
fn staged_changes_are_invisible_until_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::fixture_docs();
    let handle = build(tmp.path(), &h);
    let opts = SearchOptions {
        k: 5,
        depth: None,
        rerank_depth: Some(0),
        max_time_ms: None,
        max_items: None,
        strict: false,
        explain: false,
    };

    // A replace: the old text is searched until commit.
    let d001 = h.documents.iter().find(|d| d.external_id == "d001").unwrap();
    let mut replaced = document(d001);
    replaced.fields.insert(
        "text".into(),
        FieldValue::Text("zebraquark zebraquark zebraquark".into()),
    );
    handle.add(vec![replaced]).unwrap();
    assert!(handle.contains("d001".into()));
    let before = handle.search("zebraquark".into(), opts.clone()).unwrap();
    assert!(
        before
            .hits
            .iter()
            .all(|x| x.external_id != "d001" || !x.text.contains("zebraquark"))
    );
    handle.commit().unwrap();
    let after = handle.search("zebraquark".into(), opts.clone()).unwrap();
    assert_eq!(after.hits[0].external_id, "d001");
    assert!(after.hits[0].text.contains("zebraquark"));

    // A delete (with an unknown id ignored): staged, then committed, then reopened.
    handle
        .delete(vec!["d001".into(), "unknown-id".into()])
        .unwrap();
    assert!(handle.contains("d001".into()), "staged, not committed");
    handle.commit().unwrap();
    assert!(!handle.contains("d001".into()));
    assert_eq!(handle.info().documents, 39);
    drop(handle);
    let reopened = open(tmp.path(), false);
    assert!(!reopened.contains("d001".into()));
    assert_eq!(reopened.info().documents, 39);
}

#[test]
#[ignore = "needs both models"]
fn refusals_are_the_engines() {
    let h = support::fixture_docs();

    // A non-empty directory.
    let busy = tempfile::tempdir().unwrap();
    std::fs::write(busy.path().join("something"), b"x").unwrap();
    let err = IndexHandle::create(
        s(busy.path()),
        config(&h),
        s(&support::embedder_dir()),
        None,
        LoadPath::Buffered,
    )
    .err()
    .unwrap();
    assert!(matches!(err, XtrieverError::Corrupt { .. }), "{err:?}");

    // A dense field the schema does not have, and a keyword dense field.
    for dense in [vec!["not-a-field".to_string()], vec!["source".to_string()]] {
        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = config(&h);
        cfg.dense_fields = dense;
        let err = IndexHandle::create(
            s(&tmp.path().join("idx")),
            cfg,
            s(&support::embedder_dir()),
            None,
            LoadPath::Buffered,
        )
        .err()
        .unwrap();
        assert!(matches!(err, XtrieverError::Schema { .. }), "{err:?}");
    }

    // Vectors of the wrong width, and a document count that does not match the vectors.
    let tmp = tempfile::tempdir().unwrap();
    let handle = create(&tmp.path().join("idx"), &h, false);
    let doc = document(&h.documents[0]);
    let err = handle
        .add_embedded(vec![doc.clone()], vec![vec![0.1, 0.2, 0.3]])
        .err()
        .unwrap();
    assert!(
        matches!(err, XtrieverError::DimensionMismatch { .. }),
        "{err:?}"
    );
    let err = handle
        .add_embedded(
            vec![doc.clone(), document(&h.documents[1])],
            vec![vec![0.0; 384]],
        )
        .err()
        .unwrap();
    assert!(matches!(err, XtrieverError::Schema { .. }), "{err:?}");

    // An empty external id.
    let mut empty = doc;
    empty.external_id = String::new();
    let err = handle.add(vec![empty]).err().unwrap();
    assert!(matches!(err, XtrieverError::Schema { .. }), "{err:?}");

    // A directory the process cannot lock: every write is refused as read-only (unix).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let ro = tempfile::tempdir().unwrap();
        drop(build(ro.path(), &h));
        let mut entries = vec![ro.path().to_path_buf()];
        let mut stack = vec![ro.path().to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p.clone());
                }
                entries.push(p);
            }
        }
        for p in &entries {
            let mode = if p.is_dir() { 0o555 } else { 0o444 };
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let handle = open(ro.path(), false);
        let err = handle.add(vec![document(&h.documents[0])]).err().unwrap();
        assert!(matches!(err, XtrieverError::Io { .. }), "{err:?}");
        assert!(format!("{err}").contains("read-only"), "{err}");
        for p in &entries {
            let mode = if p.is_dir() { 0o755 } else { 0o644 };
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode));
        }
    }
}

#[test]
#[ignore = "needs both models"]
fn merge_leaves_identical_hits() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::fixture_docs();
    let with = build(tmp.path(), &h);
    with.merge().unwrap();
    let without = open(tmp.path(), false);
    assert_goldens(&with, &without);
}
