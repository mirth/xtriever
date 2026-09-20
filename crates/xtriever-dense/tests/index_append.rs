//! Feature 024, US1 (spec FR-001, FR-002, FR-004; contract guarantee 1): a commit appends the
//! new rows and never touches a committed byte; deletes and replacements are tombstones plus,
//! for a replacement, an appended row; `vector` and `len` follow the live rows. Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::vec_for;

use std::path::Path;

use xtriever_core::{DocId, Metric, VectorIndex};
use xtriever_dense::{DenseStats, FlatIndex};

const DIM: usize = 4;
const ROW: u64 = support::row_bytes(DIM);

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).unwrap().len()
}

#[test]
fn commit_appends_only_the_new_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), DIM, Metric::Dot, "fp").unwrap();
    for i in 0..100 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    let rows = support::row_file(tmp.path(), 0);
    assert_eq!(file_len(&rows), 100 * ROW);
    assert!(tmp.path().join("manifest.bin").is_file());
    assert!(!tmp.path().join("index.bin").exists());
    let before = std::fs::read(&rows).unwrap();

    for i in 100..110 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    assert_eq!(file_len(&rows), 110 * ROW);
    let after = std::fs::read(&rows).unwrap();
    assert_eq!(
        &after[..before.len()],
        &before[..],
        "committed bytes never change"
    );
    assert_eq!(
        index.stats(),
        DenseStats {
            rows: 110,
            live: 110,
            dead: 0,
            generation: 0,
            ordered: true
        }
    );
    assert_eq!(index.len(), 110);
}

#[test]
fn replace_and_delete_never_touch_committed_bytes() {
    // Cosine: a query equal to a row scores it 1.0, so "the old row 7 is gone" is testable.
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), DIM, Metric::Cosine, "fp").unwrap();
    for i in 0..110 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    let rows = support::row_file(tmp.path(), 0);
    let before = std::fs::read(&rows).unwrap();
    let old_seven = index.vector(DocId(7)).unwrap().unwrap();
    let old_seven_hit = index.search(&old_seven, None, 1).unwrap()[0];
    assert_eq!(old_seven_hit.id, DocId(7));

    let new_seven = vec![-3.0, -3.0, -3.0, -3.0];
    index.add(DocId(7), &new_seven).unwrap();
    index.delete(&[DocId(3)]).unwrap();
    index.commit().unwrap();

    assert_eq!(
        file_len(&rows),
        111 * ROW,
        "one appended row for the replacement"
    );
    let after = std::fs::read(&rows).unwrap();
    assert_eq!(&after[..before.len()], &before[..]);
    assert_eq!(
        index.stats(),
        DenseStats {
            rows: 111,
            live: 109,
            dead: 2,
            generation: 0,
            ordered: false
        }
    );
    assert_eq!(index.len(), 109);
    assert_eq!(index.vector(DocId(7)).unwrap(), Some(new_seven.clone()));
    assert_eq!(index.vector(DocId(3)).unwrap(), None);
    // The old row for 7 is dead: querying with its vector must not reproduce its old score.
    let hit = index.search(&old_seven, None, 1).unwrap()[0];
    assert!(
        hit.id != DocId(7) || hit.score.to_bits() != old_seven_hit.score.to_bits(),
        "the superseded row surfaced"
    );
    let all = index.search(&vec_for(3), None, 200).unwrap();
    assert_eq!(all.len(), 109);
    assert!(all.iter().all(|h| h.id != DocId(3)));
    assert_eq!(all.iter().filter(|h| h.id == DocId(7)).count(), 1);

    drop(index);
    let reopened = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(reopened.len(), 109);
    assert_eq!(reopened.vector(DocId(7)).unwrap(), Some(new_seven));
    assert_eq!(reopened.vector(DocId(3)).unwrap(), None);
}

#[test]
fn a_delete_only_commit_appends_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), DIM, Metric::Dot, "fp").unwrap();
    for i in 0..20 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    let rows = support::row_file(tmp.path(), 0);
    let manifest_before = std::fs::read(tmp.path().join("manifest.bin")).unwrap();
    index.delete(&[DocId(4), DocId(5), DocId(999)]).unwrap();
    index.commit().unwrap();
    assert_eq!(file_len(&rows), 20 * ROW);
    assert_ne!(
        std::fs::read(tmp.path().join("manifest.bin")).unwrap(),
        manifest_before,
        "the manifest carries the tombstones"
    );
    assert_eq!(index.stats().dead, 2);
    assert_eq!(index.len(), 18);
}

#[test]
fn bytes_written_are_proportional_to_the_change() {
    // SC-001 at test scale: 1,000 rows of dim 32, then 10 more. The write volume is the
    // handle's own count of the bytes it handed to the files (`bytes_written`) — exactly the
    // ten rows plus one manifest, so a same-length rewrite of the row file could not hide —
    // and the directory's net growth is reported beside it.
    const D: usize = 32;
    let row = support::row_bytes(D);
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), D, Metric::Cosine, "fp").unwrap();
    for i in 0..1000u32 {
        let v: Vec<f32> = (0..D)
            .map(|j| ((i * 31 + j as u32) % 17) as f32 + 1.0)
            .collect();
        index.add(DocId(i), &v).unwrap();
    }
    index.commit().unwrap();
    let written_before = index.bytes_written();
    let growth_before = support::dir_bytes(tmp.path());
    let manifest_before = file_len(&tmp.path().join("manifest.bin"));
    for i in 1000..1010u32 {
        let v: Vec<f32> = (0..D)
            .map(|j| ((i * 31 + j as u32) % 17) as f32 + 1.0)
            .collect();
        index.add(DocId(i), &v).unwrap();
    }
    index.commit().unwrap();
    let manifest_after = file_len(&tmp.path().join("manifest.bin"));
    assert_eq!(
        index.bytes_written() - written_before,
        10 * row + manifest_after,
        "the commit wrote the ten rows and one manifest, nothing else"
    );
    assert_eq!(
        support::dir_bytes(tmp.path()) - growth_before,
        10 * row + manifest_after - manifest_before,
        "net growth: the rows plus the manifest's own change"
    );
    assert!(manifest_after < 1024, "manifest is {manifest_after} bytes");
    // A delete-only commit writes one manifest and nothing else.
    let written_before = index.bytes_written();
    index.delete(&[DocId(3)]).unwrap();
    index.commit().unwrap();
    assert_eq!(
        index.bytes_written() - written_before,
        file_len(&tmp.path().join("manifest.bin"))
    );
}
