//! Feature 024, US3 (spec FR-006; contract guarantee 3): a crash at any byte boundary of a
//! commit or a compaction reopens to the previous committed state. The test replays every
//! prefix of the bytes each protocol writes — the row file at every length, the manifest either
//! old or new (its rename is atomic) — and checks the reopened index against the state before.
//! Offline; dim 4 so a row is 24 bytes and every boundary is cheap to try.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::vec_for;

use std::path::Path;

use xtriever_core::{DocId, Metric, VectorIndex};
use xtriever_dense::{DenseStats, FlatIndex};

const DIM: usize = 4;
const ROW: u64 = support::row_bytes(DIM);

type Files = Vec<(String, Vec<u8>)>;

fn snapshot(dir: &Path) -> Files {
    let mut files: Files = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

fn restore(dir: &Path, files: &Files) {
    for e in std::fs::read_dir(dir).unwrap() {
        std::fs::remove_file(e.unwrap().path()).unwrap();
    }
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
}

fn get<'a>(files: &'a Files, name: &str) -> &'a [u8] {
    &files.iter().find(|(n, _)| n == name).unwrap().1
}

#[derive(Debug, PartialEq)]
struct Observed {
    stats: DenseStats,
    hits: Vec<Vec<(u32, u32)>>,
    vectors: Vec<Option<Vec<f32>>>,
}

fn observe(index: &FlatIndex) -> Observed {
    let queries: Vec<Vec<f32>> = (0..5)
        .map(|i| vec![1.0, (i as f32).sin(), (i as f32 * 0.5).cos(), 0.25])
        .collect();
    Observed {
        stats: index.stats(),
        hits: queries
            .iter()
            .map(|q| {
                index
                    .search(q, None, 100)
                    .map(|h| support::hit_bits(&h))
                    .unwrap()
            })
            .collect(),
        vectors: (0..60).map(|i| index.vector(DocId(i))).collect(),
    }
}

#[test]
fn every_truncation_of_a_commit_reopens_to_the_previous_state() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut index = FlatIndex::create(dir, DIM, Metric::Cosine, "fp").unwrap();
    for i in 0..50 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    let s0 = snapshot(dir);
    let o0 = observe(&index);

    for i in 50..55 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.add(DocId(3), &vec_for(103)).unwrap();
    index.add(DocId(9), &vec_for(109)).unwrap();
    index.delete(&[DocId(11)]).unwrap();
    index.commit().unwrap();
    drop(index);
    let s1 = snapshot(dir);
    let o1 = observe(&FlatIndex::open(dir).unwrap());
    assert_eq!(o1.stats.rows, 57);
    assert_ne!(o0, o1);

    let rows1 = get(&s1, "vectors.0.bin");
    let mut old_manifest = s0.clone();
    for len in (50 * ROW)..=(57 * ROW) {
        // The old manifest with the row file cut at `len`: the crash happened before the rename.
        old_manifest.retain(|(n, _)| n != "vectors.0.bin");
        old_manifest.push(("vectors.0.bin".to_owned(), rows1[..len as usize].to_vec()));
        restore(dir, &old_manifest);
        let reopened = FlatIndex::open(dir).unwrap();
        assert_eq!(
            observe(&reopened),
            o0,
            "old manifest, row file at {len} bytes"
        );
        drop(reopened);
        assert_eq!(
            std::fs::metadata(dir.join("vectors.0.bin")).unwrap().len(),
            50 * ROW,
            "a writable open truncates the crashed tail (len {len})"
        );
    }
    // The new manifest with the whole row file: the commit completed.
    restore(dir, &s1);
    assert_eq!(observe(&FlatIndex::open(dir).unwrap()), o1);
}

#[test]
fn every_truncation_of_a_compaction_reopens_to_the_previous_state() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut index = FlatIndex::create(dir, DIM, Metric::Cosine, "fp").unwrap();
    for i in 0..50 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    for i in (0..50).step_by(5) {
        index.add(DocId(i), &vec_for(i + 100)).unwrap();
    }
    let dead: Vec<DocId> = (1..50).step_by(7).map(DocId).collect();
    index.delete(&dead).unwrap();
    index.commit().unwrap();
    let s1 = snapshot(dir);
    let o1 = observe(&index);
    assert!(o1.stats.dead > 0 && o1.stats.generation == 0);

    index.compact().unwrap();
    drop(index);
    let s2 = snapshot(dir);
    let o2 = observe(&FlatIndex::open(dir).unwrap());
    assert_eq!(o2.stats.generation, 1);
    assert_eq!(o2.stats.dead, 0);
    assert_eq!(o2.hits, o1.hits, "compaction keeps every bit");

    let rows2 = get(&s2, "vectors.1.bin");
    for len in 0..=rows2.len() {
        // The old manifest, the old row file intact, the new row file cut at `len`.
        let mut files = s1.clone();
        files.push(("vectors.1.bin".to_owned(), rows2[..len].to_vec()));
        restore(dir, &files);
        let reopened = FlatIndex::open(dir).unwrap();
        assert_eq!(
            observe(&reopened),
            o1,
            "old manifest, new row file at {len} bytes"
        );
        drop(reopened);
        assert!(
            !dir.join("vectors.1.bin").exists(),
            "a writable open sweeps the stale generation (len {len})"
        );
    }
    // The new manifest with both row files: the compaction completed; the old file is swept.
    let mut files = s2.clone();
    files.push((
        "vectors.0.bin".to_owned(),
        get(&s1, "vectors.0.bin").to_vec(),
    ));
    restore(dir, &files);
    let reopened = FlatIndex::open(dir).unwrap();
    assert_eq!(observe(&reopened), o2);
    drop(reopened);
    assert!(!dir.join("vectors.0.bin").exists());
}
