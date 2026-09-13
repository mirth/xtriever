//! Feature 008 D11: `TantivyIndex::open_read_only` opens an index in a directory nobody can
//! write — the case an iOS app bundle presents — takes no lock, creates no file, and refuses
//! mutation with the crate's own message.
#![cfg(unix)] // permissions are POSIX; the read-only open itself is exercised on every OS by the FFI suite
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use xtriever_core::{Error, LexicalIndex};
use xtriever_lexical::TantivyIndex;

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

/// Makes a tree read-only (dirs 0o555, files 0o444) and restores 0o755/0o644 on drop so the
/// tempdir can be removed.
struct ReadOnlyTree(Vec<(PathBuf, bool)>);

impl ReadOnlyTree {
    fn new(root: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let mut entries = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap() {
                let p = entry.unwrap().path();
                if p.is_dir() {
                    stack.push(p.clone());
                }
                entries.push((p.clone(), p.is_dir()));
            }
        }
        entries.push((root.to_path_buf(), true));
        // Files first, then directories (deepest last is not required for chmod).
        for (p, is_dir) in &entries {
            std::fs::set_permissions(
                p,
                std::fs::Permissions::from_mode(if *is_dir { 0o555 } else { 0o444 }),
            )
            .unwrap();
        }
        Self(entries)
    }
}

impl Drop for ReadOnlyTree {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        for (p, is_dir) in &self.0 {
            let _ = std::fs::set_permissions(
                p,
                std::fs::Permissions::from_mode(if *is_dir { 0o755 } else { 0o644 }),
            );
        }
    }
}

#[test]
fn a_read_only_directory_opens_searches_and_refuses_mutation() {
    let writable = support::fixture_index();
    let q = support::query("match_body");
    let want = writable.index.search(&q.query, None, q.k).unwrap();
    let path = writable.path();
    drop(writable.index);

    let before = snapshot(&path);
    let guard = ReadOnlyTree::new(&path);

    // The defect the wrapper fixes: a normal open still needs to write its lock file.
    let normal = TantivyIndex::open(&path);
    assert!(
        normal.is_err(),
        "a normal open of a read-only directory must fail (lock file)"
    );
    drop(normal);

    let mut ro = TantivyIndex::open_read_only(&path).expect("read-only open");
    let got = ro.search(&q.query, None, q.k).unwrap();
    support::assert_hits_exact("read-only search", &got, &want);

    let doc = support::corpus().documents[0].clone();
    for (what, err) in [
        ("add", ro.add(std::slice::from_ref(&doc)).err()),
        ("commit", ro.commit().err()),
        ("merge", ro.merge().err()),
    ] {
        match err {
            Some(Error::Io(e)) => {
                assert!(e.to_string().contains("read-only index"), "{what}: {e}");
            }
            other => panic!("{what}: expected Error::Io(\"read-only index\"), got {other:?}"),
        }
    }
    drop(ro);
    drop(guard);
    assert_eq!(
        snapshot(&path),
        before,
        "open + search must leave the directory untouched"
    );
    let _ = writable.dir;
}
