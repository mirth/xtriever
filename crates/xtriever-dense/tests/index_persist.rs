//! US2 scenario 6 and the persistence edge cases (spec FR-015, SC-004; research D8). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{DocId, Metric, VectorIndex};
use xtriever_dense::FlatIndex;

fn set384() -> support::SearchSet {
    support::search()
        .sets
        .into_iter()
        .find(|s| s.id == "dim384_cosine")
        .unwrap()
}

#[test]
fn reopened_index_returns_bit_identical_results() {
    let set = set384();
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), set.dim, Metric::Cosine, "test-fp").unwrap();
    for row in &set.rows {
        index.add(DocId(row.id), &row.vector).unwrap();
    }
    index.commit().unwrap();
    let before: Vec<Vec<(u32, u32)>> = set
        .queries
        .iter()
        .map(|q| {
            index
                .search(&q.vector, None, set.rows.len())
                .unwrap()
                .iter()
                .map(|h| (h.id.0, h.score.to_bits()))
                .collect()
        })
        .collect();
    drop(index);

    let reopened = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(reopened.len(), set.rows.len() as u64);
    assert_eq!(reopened.dim(), set.dim);
    assert_eq!(reopened.metric(), Metric::Cosine);
    assert_eq!(reopened.fingerprint(), "test-fp");
    for (q, want) in set.queries.iter().zip(&before) {
        let got: Vec<(u32, u32)> = reopened
            .search(&q.vector, None, set.rows.len())
            .unwrap()
            .iter()
            .map(|h| (h.id.0, h.score.to_bits()))
            .collect();
        assert_eq!(&got, want, "{}: reopen changed results", q.id);
    }
}

#[test]
fn create_writes_an_empty_generation_that_opens() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("idx");
    let index = FlatIndex::create(&dir, 4, Metric::Dot, "fp").unwrap();
    assert!(dir.join("manifest.bin").is_file());
    assert_eq!(std::fs::metadata(dir.join("vectors.0.bin")).unwrap().len(), 0);
    assert!(!dir.join("index.bin").exists());
    drop(index);
    let opened = FlatIndex::open(&dir).unwrap();
    assert!(opened.is_empty());
    assert_eq!(opened.dim(), 4);
    assert!(
        opened
            .search(&[1.0, 0.0, 0.0, 0.0], None, 5)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn create_refuses_a_non_empty_directory() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("something"), b"x").unwrap();
    let err = FlatIndex::create(tmp.path(), 4, Metric::Dot, "fp").unwrap_err();
    assert!(matches!(err, xtriever_core::Error::Corrupt(_)), "{err:?}");
}

#[test]
fn uncommitted_adds_are_gone_after_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.commit().unwrap();
    index.add(DocId(2), &[0.0, 1.0]).unwrap();
    drop(index);
    let reopened = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(reopened.len(), 1);
    assert!(
        reopened
            .search(&[0.0, 1.0], None, 5)
            .unwrap()
            .iter()
            .all(|h| h.id != DocId(2))
    );
}

/// Two handles on one directory: the second sees the generation current at its open, and a
/// handle never observes another's commit until reopened (Feature 002 semantics).
#[test]
fn a_stale_handle_serves_its_snapshot_until_reopened() {
    let tmp = tempfile::tempdir().unwrap();
    let mut a = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    a.add(DocId(1), &[1.0, 0.0]).unwrap();
    a.commit().unwrap();

    let stale = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(stale.len(), 1);

    let mut b = FlatIndex::open(tmp.path()).unwrap();
    b.add(DocId(2), &[0.0, 1.0]).unwrap();
    b.commit().unwrap();
    assert_eq!(b.len(), 2);

    assert_eq!(stale.len(), 1, "stale handle keeps its snapshot");
    assert_eq!(stale.search(&[0.0, 1.0], None, 5).unwrap().len(), 1);
    let fresh = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(fresh.len(), 2);
}

#[test]
fn a_leftover_tmp_file_is_ignored_and_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.commit().unwrap();
    std::fs::write(
        tmp.path().join("manifest.bin.tmp"),
        b"garbage from a crashed commit",
    )
    .unwrap();
    std::fs::write(tmp.path().join("vectors.7.bin"), b"a stale generation").unwrap();
    let opened = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(opened.len(), 1);
    drop(opened);
    index.add(DocId(2), &[0.0, 1.0]).unwrap();
    index.commit().unwrap();
    assert!(
        !tmp.path().join("manifest.bin.tmp").exists(),
        "commit leaves no tmp behind"
    );
    assert!(
        !tmp.path().join("vectors.7.bin").exists(),
        "a writable open sweeps stale generations"
    );
    assert_eq!(FlatIndex::open(tmp.path()).unwrap().len(), 2);
}

#[cfg(feature = "mmap")]
#[test]
fn a_mapped_handle_survives_a_commit_by_another_handle() {
    // The writer never modifies a mapped byte (ADR-0007 condition 2 as amended by ADR-0013): an
    // append extends the row file beyond the mapping, a compaction replaces it by rename — so a
    // mapping of the previous state stays valid and unchanged through both.
    let tmp = tempfile::tempdir().unwrap();
    let mut a = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    a.add(DocId(1), &[1.0, 0.0]).unwrap();
    a.commit().unwrap();
    let mapped = FlatIndex::open_mapped(tmp.path()).unwrap();
    let before = mapped.search(&[1.0, 0.0], None, 5).unwrap();
    a.add(DocId(2), &[0.5, 0.5]).unwrap();
    a.delete(&[DocId(1)]).unwrap();
    a.commit().unwrap();
    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped.search(&[1.0, 0.0], None, 5).unwrap(), before);
    a.compact().unwrap();
    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped.search(&[1.0, 0.0], None, 5).unwrap(), before);
    assert_eq!(FlatIndex::open_mapped(tmp.path()).unwrap().len(), 1);
    assert_eq!(
        FlatIndex::open_mapped(tmp.path())
            .unwrap()
            .search(&[1.0, 0.0], None, 5)
            .unwrap()[0]
            .id,
        DocId(2)
    );
}

#[test]
fn vector_returns_committed_rows_exactly_as_added() {
    let set = set384();
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), set.dim, Metric::Cosine, "test-fp").unwrap();
    for row in &set.rows {
        index.add(DocId(row.id), &row.vector).unwrap();
    }
    assert_eq!(
        index.vector(DocId(set.rows[0].id)),
        None,
        "pending rows are not visible"
    );
    index.commit().unwrap();
    for row in &set.rows {
        let got = index.vector(DocId(row.id)).unwrap();
        assert_eq!(
            support::bits(&got),
            support::bits(&row.vector),
            "row {}",
            row.id
        );
    }
    assert_eq!(index.vector(DocId(u32::MAX)), None);
    let reopened = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(
        reopened
            .vector(DocId(set.rows[7].id))
            .map(|v| support::bits(&v)),
        Some(support::bits(&set.rows[7].vector))
    );
}
