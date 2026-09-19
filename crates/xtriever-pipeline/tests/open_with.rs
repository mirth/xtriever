//! Feature 008 D11 / D10 / D12: `HybridIndex::open_with(OpenOptions)` — read-only open of a
//! directory nobody can write, `merge` to one segment with bit-identical results, and
//! tolerance of the `corpus.json` sidecar.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::Path;

use xtriever_pipeline::{HybridIndex, OpenOptions};

fn embedder() -> Box<dyn xtriever_core::Embedder> {
    support::fixture_embedder(&support::hybrid())
}

fn hits(index: &HybridIndex, text: &str) -> Vec<(String, u64)> {
    support::fused_bits(index, text)
}

/// dirs 0o555, files 0o444; restored on drop. POSIX permissions: the read-only test is unix-only.
#[cfg(unix)]
struct ReadOnlyTree(Vec<(std::path::PathBuf, bool)>);

#[cfg(unix)]
impl ReadOnlyTree {
    fn new(root: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let mut entries = vec![(root.to_path_buf(), true)];
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

#[cfg(unix)]
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

#[cfg(unix)]
#[test]
fn a_read_only_directory_opens_and_searches_like_a_writable_one() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0].text;
    let want = hits(&index, q);
    drop(index);

    let guard = ReadOnlyTree::new(tmp.path());
    let modes = [
        false,
        #[cfg(feature = "mmap")]
        true,
    ];
    for mapped in modes {
        let mut ro = HybridIndex::open_with(
            tmp.path(),
            embedder(),
            OpenOptions {
                mapped,
                read_only: true,
            },
        )
        .unwrap_or_else(|e| panic!("read-only open (mapped {mapped}): {e}"));
        assert_eq!(hits(&ro, q), want, "mapped {mapped}");
        let doc = h.documents[0].source();
        for (what, err) in [
            ("add", ro.add(std::slice::from_ref(&doc)).err()),
            ("commit", ro.commit().err()),
            ("merge", ro.merge().err()),
        ] {
            match err {
                Some(xtriever_core::Error::Io(e)) => {
                    assert!(e.to_string().contains("read-only index"), "{what}: {e}")
                }
                other => panic!("{what}: expected Error::Io(read-only index), got {other:?}"),
            }
        }
    }
    drop(guard);
}

#[test]
fn open_with_equals_the_named_constructors() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[1].text;
    let want = hits(&index, q);
    drop(index);
    let a = HybridIndex::open(tmp.path(), embedder()).unwrap();
    let b = HybridIndex::open_with(
        tmp.path(),
        embedder(),
        OpenOptions {
            mapped: false,
            read_only: false,
        },
    )
    .unwrap();
    for (name, i) in [("open", &a), ("open_with", &b)] {
        assert_eq!(hits(i, q), want, "{name}");
    }
    #[cfg(feature = "mmap")]
    {
        let c = HybridIndex::open_mapped(tmp.path(), embedder()).unwrap();
        let d = HybridIndex::open_with(
            tmp.path(),
            embedder(),
            OpenOptions {
                mapped: true,
                read_only: false,
            },
        )
        .unwrap();
        for (name, i) in [("open_mapped", &c), ("open_with mapped", &d)] {
            assert_eq!(hits(i, q), want, "{name}");
        }
    }
    #[cfg(not(feature = "mmap"))]
    {
        let err = HybridIndex::open_with(
            tmp.path(),
            embedder(),
            OpenOptions {
                mapped: true,
                read_only: false,
            },
        )
        .err();
        assert!(
            matches!(err, Some(xtriever_core::Error::Backend(_))),
            "mapped without the feature must be a plain error: {err:?}"
        );
    }
    assert_eq!(
        OpenOptions::default(),
        OpenOptions {
            mapped: false,
            read_only: false
        }
    );
}

#[test]
fn a_corpus_json_sidecar_is_ignored_and_its_absence_too() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let q = &h.queries[0].text;
    let want = hits(&index, q);
    drop(index);
    std::fs::write(
        tmp.path().join("corpus.json"),
        br#"{"schema_version":1,"corpus_identity":"x"}"#,
    )
    .unwrap();
    let with = HybridIndex::open(tmp.path(), embedder()).unwrap();
    assert_eq!(hits(&with, q), want);
    drop(with);
    std::fs::remove_file(tmp.path().join("corpus.json")).unwrap();
    let without = HybridIndex::open(tmp.path(), embedder()).unwrap();
    assert_eq!(hits(&without, q), want);
}

fn segment_stores(dir: &Path) -> usize {
    std::fs::read_dir(dir.join("lexical"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "store")
        })
        .count()
}

#[test]
fn merge_leaves_one_segment_and_identical_results() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index =
        HybridIndex::create(tmp.path(), support::fixture_config(&h), embedder()).unwrap();
    // Three commits so the lexical index has more than one segment to merge.
    for docs in h.documents.chunks(h.documents.len().div_ceil(3)) {
        let batch: Vec<_> = docs.iter().map(|d| d.source()).collect();
        index.add(&batch).unwrap();
        index.commit().unwrap();
    }
    let before: Vec<Vec<(String, u64)>> = h.queries.iter().map(|q| hits(&index, &q.text)).collect();
    let segments_before = segment_stores(tmp.path());
    index.merge().unwrap();
    assert_eq!(
        segment_stores(tmp.path()),
        1,
        "one segment after merge (had {segments_before})"
    );
    let after: Vec<Vec<(String, u64)>> = h.queries.iter().map(|q| hits(&index, &q.text)).collect();
    assert_eq!(after, before, "merge must not change any hit or score bit");
    drop(index);
    let reopened = HybridIndex::open(tmp.path(), embedder()).unwrap();
    let again: Vec<Vec<(String, u64)>> =
        h.queries.iter().map(|q| hits(&reopened, &q.text)).collect();
    assert_eq!(again, before);
}
