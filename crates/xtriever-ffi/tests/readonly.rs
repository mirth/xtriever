//! US1 scenario 4 / FR-002 — the surface never writes; US3 scenarios 1–4 on the Rust side —
//! open failures arrive as the mirrored kinds naming what failed. Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use xtriever_ffi::{IndexHandle, LoadPath, SearchOptions, XtrieverError};

fn snapshot(dir: &Path) -> BTreeMap<String, (u64, std::time::SystemTime)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let meta = entry.metadata().unwrap();
                out.insert(
                    path.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    (meta.len(), meta.modified().unwrap()),
                );
            }
        }
    }
    out
}

fn open(
    dir: &Path,
    embedder: &Path,
    reranker: Option<&Path>,
) -> Result<std::sync::Arc<IndexHandle>, XtrieverError> {
    IndexHandle::open(
        dir.to_string_lossy().into_owned(),
        embedder.to_string_lossy().into_owned(),
        reranker.map(|p| p.to_string_lossy().into_owned()),
        LoadPath::Buffered,
    )
}

#[test]
#[ignore = "needs both models"]
fn open_and_search_leave_the_directory_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let before = snapshot(tmp.path());
    {
        let ffi = open(
            tmp.path(),
            &support::embedder_dir(),
            Some(&support::reranker_dir()),
        )
        .unwrap();
        for q in support::fixture_docs().queries.iter().take(3) {
            ffi.search(
                q.text.clone(),
                SearchOptions {
                    k: 10,
                    depth: None,
                    rerank_depth: Some(5),
                    max_time_ms: None,
                    max_items: None,
                    strict: false,
                    explain: true,
                },
            )
            .unwrap();
        }
    }
    assert_eq!(snapshot(tmp.path()), before, "the index directory changed");
}

#[test]
#[ignore = "needs the embedder"]
fn open_failures_name_what_failed() {
    let tmp = tempfile::tempdir().unwrap();
    let e = support::embedder_dir();
    // Not an index.
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    match open(&empty, &e, None) {
        Err(XtrieverError::Corrupt { message }) => assert!(message.contains("empty"), "{message}"),
        other => panic!("{other:?}"),
    }
    // A version-1 directory.
    let v1 = tmp.path().join("v1");
    drop(support::build_fixture_index(&v1));
    let desc = v1.join("xtriever-pipeline.json");
    let text = std::fs::read_to_string(&desc).unwrap();
    std::fs::write(
        &desc,
        text.replace("\"format_version\": 2", "\"format_version\": 1"),
    )
    .unwrap();
    match open(&v1, &e, None) {
        Err(XtrieverError::Corrupt { message }) => {
            assert!(
                message.contains('1') && message.contains('2') && message.contains("rebuild"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    // An interrupted commit.
    let marker = tmp.path().join("marker");
    drop(support::build_fixture_index(&marker));
    std::fs::write(marker.join("commit.pending"), "3").unwrap();
    match open(&marker, &e, None) {
        Err(XtrieverError::Corrupt { message }) => {
            assert!(message.contains("interrupted commit"), "{message}")
        }
        other => panic!("{other:?}"),
    }
    // The wrong model as the embedder (its pins fail).
    match open(&marker, &support::reranker_dir(), None) {
        Err(XtrieverError::Model { model, message }) => {
            assert!(!model.is_empty());
            assert!(
                message.contains("bytes") || message.contains("sha256"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    // A tampered weights file: size mismatch naming the file and both sizes.
    let good = tmp.path().join("good");
    drop(support::build_fixture_index(&good));
    let copy = support::model_copy(&e);
    std::fs::OpenOptions::new()
        .append(true)
        .open(copy.path().join("model.safetensors"))
        .unwrap()
        .write_all(b"\0")
        .unwrap();
    match open(&good, copy.path(), None) {
        Err(XtrieverError::Model { message, .. }) => {
            assert!(message.contains("model.safetensors"), "{message}");
            assert!(
                message.contains("90868376") && message.contains("90868377"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
}
