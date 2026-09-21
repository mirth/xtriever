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
                    rerank_mode: None,
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
    // The wrong model as the embedder: its directory holds the re-ranker's weights file, not
    // either of the embedder's (Feature 026), so it is refused naming what was expected — or,
    // were the names to coincide, by its pins.
    match open(&marker, &support::reranker_dir(), None) {
        Err(XtrieverError::Model { model, message }) => {
            assert!(!model.is_empty());
            assert!(
                message.contains("neither")
                    || message.contains("bytes")
                    || message.contains("sha256"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    // A tampered weights file: size mismatch naming the file and both sizes.
    let good = tmp.path().join("good");
    drop(support::build_fixture_index(&good));
    // Whichever pinned artefact the copy holds (Feature 026: the eight-bit GGUF by default, the
    // float file with XTRIEVER_MODEL_DIR): the engine names the file and both sizes.
    let copy = support::model_copy(&e);
    let weights = std::fs::read_dir(copy.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| name == "model.safetensors" || name.ends_with(".gguf"))
        .expect("a weights file in the model copy");
    let pinned = std::fs::metadata(copy.path().join(&weights)).unwrap().len();
    std::fs::OpenOptions::new()
        .append(true)
        .open(copy.path().join(&weights))
        .unwrap()
        .write_all(b"\0")
        .unwrap();
    match open(&good, copy.path(), None) {
        Err(XtrieverError::Model { message, .. }) => {
            assert!(message.contains(&weights), "{message}");
            assert!(
                message.contains(&pinned.to_string())
                    && message.contains(&(pinned + 1).to_string()),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
}

/// Feature 008 D11: the surface opens an index in a directory nobody can write — the shape an
/// iOS app bundle has — creates nothing, and searches it like a writable one.
#[cfg(unix)]
#[test]
#[ignore = "needs both models"]
fn a_read_only_directory_opens_in_place() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    drop(support::build_fixture_index(tmp.path()));
    let q = support::fixture_docs().queries[0].text.clone();
    let options = SearchOptions {
        k: 10,
        depth: None,
        rerank_depth: Some(5),
        rerank_mode: None,
        max_time_ms: None,
        max_items: None,
        strict: false,
        explain: true,
    };
    let want: Vec<(String, u64)> = {
        let ffi = open(
            tmp.path(),
            &support::embedder_dir(),
            Some(&support::reranker_dir()),
        )
        .unwrap();
        ffi.search(q.clone(), options.clone())
            .unwrap()
            .hits
            .iter()
            .map(|h| (h.external_id.clone(), h.score.to_bits()))
            .collect()
    };
    // dirs 0o555, files 0o444, restored at the end so the tempdir can be removed.
    let mut entries = vec![(tmp.path().to_path_buf(), true)];
    let mut stack = vec![tmp.path().to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p.clone());
            }
            entries.push((p.clone(), p.is_dir()));
        }
    }
    for (p, is_dir) in &entries {
        std::fs::set_permissions(
            p,
            std::fs::Permissions::from_mode(if *is_dir { 0o555 } else { 0o444 }),
        )
        .unwrap();
    }
    let before = snapshot(tmp.path());
    let got: Vec<(String, u64)> = {
        let ffi = open(
            tmp.path(),
            &support::embedder_dir(),
            Some(&support::reranker_dir()),
        )
        .expect("a read-only directory must open");
        ffi.search(q, options)
            .unwrap()
            .hits
            .iter()
            .map(|h| (h.external_id.clone(), h.score.to_bits()))
            .collect()
    };
    let after = snapshot(tmp.path());
    for (p, is_dir) in &entries {
        let _ = std::fs::set_permissions(
            p,
            std::fs::Permissions::from_mode(if *is_dir { 0o755 } else { 0o644 }),
        );
    }
    assert_eq!(
        got, want,
        "read-only hits must equal the writable ones bit for bit"
    );
    assert_eq!(after, before, "nothing may be created or touched");
}
