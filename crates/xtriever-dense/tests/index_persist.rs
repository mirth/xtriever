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
    assert_eq!(
        std::fs::metadata(dir.join("vectors.0.bin")).unwrap().len(),
        0
    );
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

#[cfg(feature = "mmap")]
#[test]
fn a_mapped_handle_maps_after_its_first_append_and_after_compacting_to_empty() {
    // An empty index is a heap buffer (a zero-length mapping does not exist); the first
    // commit that appends must map, and a compaction to zero rows returns to the buffer
    // (review round 3 #4, spec FR-008).
    let tmp = tempfile::tempdir().unwrap();
    drop(FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap());
    let mut mapped = FlatIndex::open_mapped(tmp.path()).unwrap();
    assert!(!mapped.is_mapped(), "an empty index is not a mapping");
    mapped.add(DocId(1), &[1.0, 0.0]).unwrap();
    mapped.commit().unwrap();
    assert!(
        mapped.is_mapped(),
        "the first append maps the committed rows"
    );
    mapped.add(DocId(2), &[0.0, 1.0]).unwrap();
    mapped.commit().unwrap();
    assert!(mapped.is_mapped());
    assert_eq!(mapped.len(), 2);
    assert_eq!(mapped.search(&[0.0, 1.0], None, 1).unwrap()[0].id, DocId(2));
    mapped.delete(&[DocId(1), DocId(2)]).unwrap();
    mapped.compact().unwrap();
    assert!(!mapped.is_mapped(), "compacted to no rows: a buffer again");
    assert_eq!(mapped.len(), 0);
    mapped.add(DocId(3), &[1.0, 1.0]).unwrap();
    mapped.commit().unwrap();
    assert!(mapped.is_mapped());
    assert_eq!(mapped.vector(DocId(3)).unwrap(), Some(vec![1.0, 1.0]));
}

#[cfg(all(feature = "mmap", unix))]
#[test]
fn a_mapping_covers_only_the_committed_rows() {
    // A crashed tail beyond the committed rows is never mapped: the row file is made read-only
    // so the open cannot cut the tail, the file stays extended, and the mapping still exposes
    // exactly the committed rows — the invariant `bytes::map_readonly` relies on.
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.commit().unwrap();
    drop(index);
    let rows = tmp.path().join("vectors.0.bin");
    let committed = std::fs::metadata(&rows).unwrap().len();
    let mut with_tail = std::fs::read(&rows).unwrap();
    with_tail.extend_from_slice(&[0xAB; 40]);
    std::fs::write(&rows, &with_tail).unwrap();
    std::fs::set_permissions(&rows, std::fs::Permissions::from_mode(0o400)).unwrap();
    let mapped = FlatIndex::open_mapped(tmp.path()).unwrap();
    assert_eq!(
        std::fs::metadata(&rows).unwrap().len(),
        committed + 40,
        "the tail survived the open"
    );
    assert!(mapped.is_mapped());
    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped.vector(DocId(1)).unwrap(), Some(vec![1.0, 0.0]));
    assert_eq!(mapped.search(&[1.0, 0.0], None, 5).unwrap().len(), 1);
    drop(mapped);
    std::fs::set_permissions(&rows, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn a_read_only_open_touches_nothing_and_refuses_mutations() {
    // A logical read-only open holds no writer's role: a crashed tail, a manifest temporary and
    // a stale generation all survive it (it alters nothing it did not write), and every
    // mutation is refused — the state is still the committed one.
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.commit().unwrap();
    drop(index);
    let rows = tmp.path().join("vectors.0.bin");
    let committed = std::fs::metadata(&rows).unwrap().len();
    let mut with_tail = std::fs::read(&rows).unwrap();
    with_tail.extend_from_slice(&[0xCD; 24]);
    std::fs::write(&rows, &with_tail).unwrap();
    std::fs::write(tmp.path().join("manifest.bin.tmp"), b"in flight").unwrap();
    std::fs::write(
        tmp.path().join("vectors.9.bin"),
        b"a generation being prepared",
    )
    .unwrap();
    let mut ro = FlatIndex::open_read_only(tmp.path()).unwrap();
    assert!(ro.is_read_only());
    assert_eq!(std::fs::metadata(&rows).unwrap().len(), committed + 24);
    assert!(tmp.path().join("manifest.bin.tmp").exists());
    assert!(tmp.path().join("vectors.9.bin").exists());
    assert_eq!(ro.len(), 1);
    assert_eq!(ro.vector(DocId(1)).unwrap(), Some(vec![1.0, 0.0]));
    let refused = [
        ro.add(DocId(2), &[0.0, 1.0]).unwrap_err(),
        ro.delete(&[DocId(1)]).unwrap_err(),
        ro.set_compaction_threshold(Some(0.5)).unwrap_err(),
        ro.commit().unwrap_err(),
        ro.compact().unwrap_err(),
    ];
    for err in refused {
        match err {
            xtriever_core::Error::Io(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
                assert!(e.to_string().contains("read-only"), "{e}");
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }
    assert_eq!(ro.len(), 1);
    drop(ro);
    // A writable open then does the cleanup.
    let rw = FlatIndex::open(tmp.path()).unwrap();
    assert!(!rw.is_read_only());
    assert_eq!(std::fs::metadata(&rows).unwrap().len(), committed);
    assert!(!tmp.path().join("manifest.bin.tmp").exists());
    assert!(!tmp.path().join("vectors.9.bin").exists());
}

#[cfg(feature = "mmap")]
#[test]
fn a_mapped_read_only_open_exposes_the_committed_rows_only() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.commit().unwrap();
    drop(index);
    let rows = tmp.path().join("vectors.0.bin");
    let mut with_tail = std::fs::read(&rows).unwrap();
    with_tail.extend_from_slice(&[0xCD; 24]);
    std::fs::write(&rows, &with_tail).unwrap();
    let ro = FlatIndex::open_mapped_read_only(tmp.path()).unwrap();
    assert!(ro.is_read_only() && ro.is_mapped());
    assert_eq!(
        std::fs::metadata(&rows).unwrap().len(),
        with_tail.len() as u64
    );
    assert_eq!(ro.len(), 1);
    assert_eq!(ro.search(&[1.0, 0.0], None, 5).unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn an_unopenable_directory_fails_a_commit_before_the_switch() {
    // The directory handle for the post-rename sync is opened *before* the rename, so a
    // directory that cannot be opened (no read permission) fails the commit cleanly: nothing
    // on disk changes, the pending changes are kept, and nothing is left "switched but
    // unconfirmed" — the only after-switch failure left is an `fsync` error on an open
    // handle, which no test can provoke and the retry state (`is_sync_pending`) covers.
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("idx");
    let mut index = FlatIndex::create(&dir, 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    let manifest_before = std::fs::read(dir.join("manifest.bin")).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o300)).unwrap();
    let err = index.commit().unwrap_err();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(err, xtriever_core::Error::Io(_)), "{err}");
    assert!(!index.is_sync_pending());
    assert_eq!(index.len(), 0, "nothing switched");
    assert_eq!(
        std::fs::read(dir.join("manifest.bin")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::metadata(dir.join("vectors.0.bin")).unwrap().len(),
        0,
        "the append was rolled back"
    );
    index.commit().unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(FlatIndex::open(&dir).unwrap().len(), 1);
}

#[test]
fn an_append_that_succeeds_before_the_manifest_fails_is_rolled_back() {
    // A directory named `manifest.bin.tmp` makes the append succeed and the manifest's
    // `File::create` fail: the rows are cut back to the committed length, the buffer (or
    // mapping) too, the pending changes survive, and the retry — once the obstacle is gone —
    // commits exactly them. `bytes_written` counts the rolled-back rows (they were written)
    // but not a manifest.
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.commit().unwrap();
    let rows = tmp.path().join("vectors.0.bin");
    let committed = std::fs::read(&rows).unwrap();
    let written_before = index.bytes_written();
    std::fs::create_dir(tmp.path().join("manifest.bin.tmp")).unwrap();
    index.add(DocId(2), &[0.0, 1.0]).unwrap();
    index.delete(&[DocId(1)]).unwrap();
    assert!(matches!(
        index.commit().unwrap_err(),
        xtriever_core::Error::Io(_)
    ));
    assert_eq!(
        std::fs::read(&rows).unwrap(),
        committed,
        "the appended row was cut"
    );
    assert_eq!(
        index.bytes_written() - written_before,
        support::row_bytes(2),
        "one row of dim 2 was written, no manifest"
    );
    assert_eq!(index.len(), 1);
    assert_eq!(
        index.vector(DocId(1)).unwrap(),
        Some(vec![1.0, 0.0]),
        "the buffer was cut too"
    );
    assert_eq!(index.vector(DocId(2)).unwrap(), None);
    std::fs::remove_dir(tmp.path().join("manifest.bin.tmp")).unwrap();
    index.commit().unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(index.vector(DocId(2)).unwrap(), Some(vec![0.0, 1.0]));
    assert_eq!(index.vector(DocId(1)).unwrap(), None);
    assert_eq!(FlatIndex::open(tmp.path()).unwrap().len(), 1);
}

#[cfg(feature = "mmap")]
#[test]
fn a_mapped_append_that_fails_before_the_manifest_drops_its_mapping_first() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.commit().unwrap();
    drop(index);
    let mut mapped = FlatIndex::open_mapped(tmp.path()).unwrap();
    std::fs::create_dir(tmp.path().join("manifest.bin.tmp")).unwrap();
    mapped.add(DocId(2), &[0.0, 1.0]).unwrap();
    assert!(mapped.commit().is_err());
    assert_eq!(mapped.len(), 1);
    assert_eq!(
        mapped.search(&[0.0, 1.0], None, 5).unwrap().len(),
        1,
        "the old mapping still serves"
    );
    std::fs::remove_dir(tmp.path().join("manifest.bin.tmp")).unwrap();
    mapped.commit().unwrap();
    assert_eq!(mapped.len(), 2);
    assert!(mapped.is_mapped());
}

#[test]
fn a_sparse_id_costs_one_entry_not_a_table() {
    // `add` accepts any id (review round 4 #1): the live-row table is keyed by id, so
    // committing `u32::MAX` and reopening cost one entry each, not a four-billion-slot table.
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(u32::MAX), &[1.0, 0.0]).unwrap();
    index.add(DocId(0), &[0.0, 1.0]).unwrap();
    index.commit().unwrap();
    assert_eq!(index.vector(DocId(u32::MAX)).unwrap(), Some(vec![1.0, 0.0]));
    assert_eq!(
        index.search(&[1.0, 0.0], None, 1).unwrap()[0].id,
        DocId(u32::MAX)
    );
    drop(index);
    let mut reopened = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(reopened.len(), 2);
    reopened.delete(&[DocId(0)]).unwrap();
    reopened.compact().unwrap();
    assert_eq!(
        reopened.vector(DocId(u32::MAX)).unwrap(),
        Some(vec![1.0, 0.0])
    );
    assert_eq!(reopened.len(), 1);
}

#[cfg(unix)] // file permissions as the read-only setup
#[test]
fn a_buffered_open_reads_only_the_committed_bytes() {
    use std::os::unix::fs::PermissionsExt;

    // A crashed tail is never read into memory (review round 4 #4): make the tail far larger
    // than the committed rows and check the open neither fails nor reads it.
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.commit().unwrap();
    drop(index);
    let rows = tmp.path().join("vectors.0.bin");
    let committed = std::fs::read(&rows).unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(&rows).unwrap();
    file.set_len(committed.len() as u64 + 64 * 1024 * 1024)
        .unwrap(); // a sparse 64 MB tail
    drop(file);
    // The row file read-only: the tail survives the open and only the committed rows are read.
    std::fs::set_permissions(&rows, std::fs::Permissions::from_mode(0o400)).unwrap();
    let opened = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(
        std::fs::metadata(&rows).unwrap().len(),
        committed.len() as u64 + 64 * 1024 * 1024
    );
    assert_eq!(opened.len(), 1);
    assert_eq!(opened.vector(DocId(1)).unwrap(), Some(vec![1.0, 0.0]));
    drop(opened);
    std::fs::set_permissions(&rows, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn vector_returns_committed_rows_within_the_quantisation_step() {
    let set = set384();
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), set.dim, Metric::Cosine, "test-fp").unwrap();
    for row in &set.rows {
        index.add(DocId(row.id), &row.vector).unwrap();
    }
    assert_eq!(
        index.vector(DocId(set.rows[0].id)).unwrap(),
        None,
        "pending rows are not visible"
    );
    index.commit().unwrap();
    for row in &set.rows {
        let got = index.vector(DocId(row.id)).unwrap().unwrap();
        // Since Feature 026 a committed row is eight-bit codes and a scale, so `vector` returns
        // what those recover — never the bytes that were added (ADR-0015, spec FR-003). Every
        // component is within half a quantisation step — the row's scale, floored — which is
        // the promise the format makes; the step comes from the scheme's restatement in
        // `support`, not from an unfloored `peak / 127` (review round 4, finding 4).
        let (_, step) = support::quantise(&row.vector);
        assert_eq!(got.len(), row.vector.len(), "row {}", row.id);
        for (i, (recovered, original)) in got.iter().zip(&row.vector).enumerate() {
            assert!(
                (recovered - original).abs() <= step / 2.0 + f32::EPSILON,
                "row {} component {i}: {recovered} against {original}, step {step}",
                row.id
            );
        }
    }
    assert_eq!(index.vector(DocId(u32::MAX)).unwrap(), None);
    let reopened = FlatIndex::open(tmp.path()).unwrap();
    // A reopen recovers exactly what this handle recovers — the codes on disk are the truth.
    assert_eq!(
        reopened
            .vector(DocId(set.rows[7].id))
            .unwrap()
            .map(|v| support::bits(&v)),
        index
            .vector(DocId(set.rows[7].id))
            .unwrap()
            .map(|v| support::bits(&v))
    );
}
