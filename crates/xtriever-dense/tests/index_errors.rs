//! US2 scenarios 7–8 and the rejection rules (spec FR-015, FR-017, SC-005; research D9). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{DocId, Embedder, Error, Metric, Result, TextKind, Vector, VectorIndex};
use xtriever_dense::{FORMAT_VERSION, FlatIndex};

/// A stub embedder with a chosen identity, for the open-time agreement checks.
struct Stub {
    fp: &'static str,
    dim: usize,
    metric: Metric,
}

impl Embedder for Stub {
    fn dim(&self) -> usize {
        self.dim
    }
    fn metric(&self) -> Metric {
        self.metric
    }
    fn fingerprint(&self) -> &str {
        self.fp
    }
    fn max_input_tokens(&self) -> Option<usize> {
        None
    }
    fn embed(&self, texts: &[&str], _: TextKind) -> Result<Vec<Vector>> {
        Ok(texts.iter().map(|_| vec![0.0; self.dim]).collect())
    }
}

fn small(dir: &std::path::Path) -> FlatIndex {
    let mut index = FlatIndex::create(dir, 3, Metric::Cosine, "fp-a").unwrap();
    index.add(DocId(1), &[1.0, 0.0, 0.0]).unwrap();
    index.add(DocId(2), &[0.0, 1.0, 0.0]).unwrap();
    index.commit().unwrap();
    index
}

#[test]
fn fingerprint_mismatch_names_both_fingerprints() {
    let tmp = tempfile::tempdir().unwrap();
    drop(small(tmp.path()));
    let err = FlatIndex::open_for(
        tmp.path(),
        &Stub {
            fp: "fp-b",
            dim: 3,
            metric: Metric::Cosine,
        },
    )
    .unwrap_err();
    match err {
        Error::FingerprintMismatch { index, current } => {
            assert_eq!(index, "fp-a");
            assert_eq!(current, "fp-b");
        }
        other => panic!("expected FingerprintMismatch, got {other:?}"),
    }
    // The matching embedder opens fine.
    let ok = FlatIndex::open_for(
        tmp.path(),
        &Stub {
            fp: "fp-a",
            dim: 3,
            metric: Metric::Cosine,
        },
    )
    .unwrap();
    assert_eq!(ok.len(), 2);
}

#[test]
fn dim_or_metric_disagreement_with_the_embedder_is_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    drop(small(tmp.path()));
    let err = FlatIndex::open_for(
        tmp.path(),
        &Stub {
            fp: "fp-a",
            dim: 4,
            metric: Metric::Cosine,
        },
    )
    .unwrap_err();
    assert!(matches!(err, Error::Corrupt(_)), "{err:?}");
    let err = FlatIndex::open_for(
        tmp.path(),
        &Stub {
            fp: "fp-a",
            dim: 3,
            metric: Metric::Dot,
        },
    )
    .unwrap_err();
    assert!(matches!(err, Error::Corrupt(_)), "{err:?}");
}

/// Rewrite `manifest.bin`'s JSON header in place (magic · hdr_len · JSON · tombstones).
fn rewrite_manifest_header(dir: &std::path::Path, edit: impl Fn(&str) -> String) {
    let path = dir.join("manifest.bin");
    let bytes = std::fs::read(&path).unwrap();
    let hdr_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let header = std::str::from_utf8(&bytes[16..16 + hdr_len]).unwrap();
    let edited = edit(header);
    let mut out = Vec::new();
    out.extend_from_slice(&bytes[..8]);
    out.extend_from_slice(&(edited.len() as u64).to_le_bytes());
    out.extend_from_slice(edited.as_bytes());
    out.extend_from_slice(&bytes[16 + hdr_len..]);
    std::fs::write(&path, out).unwrap();
}

#[test]
fn a_future_format_version_is_rejected_naming_both_versions() {
    let tmp = tempfile::tempdir().unwrap();
    drop(small(tmp.path()));
    // A genuine future format carries its own versioned magic (XTDENSE1, XTDENSE2, …) as well
    // as its header version: both spellings must name both versions.
    let path = tmp.path().join("manifest.bin");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[..8].copy_from_slice(b"XTDENSE3");
    std::fs::write(&path, &bytes).unwrap();
    match FlatIndex::open(tmp.path()).unwrap_err() {
        Error::Corrupt(msg) => {
            assert!(msg.contains("version 3"), "{msg}");
            assert!(msg.contains(&FORMAT_VERSION.to_string()), "{msg}");
        }
        other => panic!("expected Corrupt, got {other:?}"),
    }
    drop(small(&tmp.path().join("again")));
    rewrite_manifest_header(&tmp.path().join("again"), |h| {
        assert!(h.contains("\"format_version\":2"), "{h}");
        h.replace("\"format_version\":2", "\"format_version\":3")
    });
    let err = FlatIndex::open(&tmp.path().join("again")).unwrap_err();
    match err {
        Error::Corrupt(msg) => {
            assert!(msg.contains('3'), "{msg}");
            assert!(msg.contains(&FORMAT_VERSION.to_string()), "{msg}");
        }
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

#[test]
fn a_version_1_directory_is_refused_naming_both_versions() {
    // Feature 024 (spec FR-007): version 1 (`index.bin`, columnar) is not read. A hand-built
    // version-1 file with an empty body.
    let tmp = tempfile::tempdir().unwrap();
    let header = r#"{"format_version":1,"dim":3,"metric":"cosine","fingerprint":"fp-a","count":0}"#;
    let mut v1 = Vec::new();
    v1.extend_from_slice(b"XTDENSE1");
    v1.extend_from_slice(&(header.len() as u64).to_le_bytes());
    v1.extend_from_slice(header.as_bytes());
    std::fs::write(tmp.path().join("index.bin"), v1).unwrap();
    match FlatIndex::open(tmp.path()).unwrap_err() {
        Error::Corrupt(msg) => {
            assert!(msg.contains("version 1"), "{msg}");
            assert!(msg.contains(&FORMAT_VERSION.to_string()), "{msg}");
        }
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

#[test]
fn two_live_rows_for_one_id_are_corrupt() {
    // A row file with a duplicate id and no tombstone for either row (review round 1 #2): the
    // one-live-row invariant is checked while the id table is rebuilt at open.
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 3, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0, 0.0]).unwrap();
    index.commit().unwrap();
    drop(index);
    let rows = tmp.path().join("vectors.0.bin");
    let one = std::fs::read(&rows).unwrap();
    let mut twice = one.clone();
    twice.extend_from_slice(&one);
    std::fs::write(&rows, twice).unwrap();
    // An ordered file needs no id table (ids are unique by construction); this one is not
    // ordered (1, 1), so the manifest must say so — and the table build then finds the pair.
    rewrite_manifest_header(tmp.path(), |h| {
        h.replace(
            "\"rows\":1,\"live\":1,\"ordered\":true",
            "\"rows\":2,\"live\":2,\"ordered\":false",
        )
    });
    for open in [FlatIndex::open, FlatIndex::open_read_only] {
        match open(tmp.path()).unwrap_err() {
            Error::Corrupt(msg) => assert!(msg.contains("two live rows for id 1"), "{msg}"),
            other => panic!("expected Corrupt, got {other:?}"),
        }
    }
    // A manifest that *lies* about the order: a read-only open trusts it (as it trusts every
    // manifest field — it reads nothing beyond the manifest), a writable handle verifies the
    // ids before its first write and refuses to build on the lie.
    rewrite_manifest_header(tmp.path(), |h| {
        h.replace("\"ordered\":false", "\"ordered\":true")
    });
    assert_eq!(FlatIndex::open_read_only(tmp.path()).unwrap().len(), 2);
    let mut index = FlatIndex::open(tmp.path()).unwrap();
    index.add(DocId(9), &[0.0, 0.0, 1.0]).unwrap();
    match index.commit().unwrap_err() {
        Error::Corrupt(msg) => assert!(msg.contains("not ordered"), "{msg}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    assert!(matches!(index.compact().unwrap_err(), Error::Corrupt(_)));
}

#[test]
fn a_generation_that_cannot_advance_is_corrupt_not_an_overflow() {
    // The generation is read from disk (review round 2 #1): at u64::MAX, appends still work
    // (they do not advance it) and `compact` returns `Corrupt` instead of overflowing.
    let tmp = tempfile::tempdir().unwrap();
    drop(small(tmp.path()));
    rewrite_manifest_header(tmp.path(), |h| {
        h.replace("\"generation\":0", &format!("\"generation\":{}", u64::MAX))
    });
    std::fs::rename(
        tmp.path().join("vectors.0.bin"),
        tmp.path().join(format!("vectors.{}.bin", u64::MAX)),
    )
    .unwrap();
    let mut index = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(index.stats().generation, u64::MAX);
    index.add(DocId(3), &[0.0, 0.0, 1.0]).unwrap();
    index.delete(&[DocId(1)]).unwrap();
    index.commit().unwrap();
    assert_eq!(index.len(), 2);
    match index.compact().unwrap_err() {
        Error::Corrupt(msg) => assert!(msg.contains("cannot advance"), "{msg}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    // The handle is still coherent with the disk after the refusal.
    assert_eq!(index.len(), 2);
    assert_eq!(FlatIndex::open(tmp.path()).unwrap().len(), 2);
}

#[test]
fn an_unrepresentable_dim_is_refused_before_anything_is_written() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("idx");
    assert!(matches!(
        FlatIndex::create(&dir, usize::MAX, Metric::Dot, "fp").unwrap_err(),
        Error::Corrupt(_)
    ));
    assert!(
        !dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none(),
        "a refused create leaves nothing behind"
    );
    // A corrected retry succeeds in the same directory.
    assert_eq!(
        FlatIndex::create(&dir, 3, Metric::Dot, "fp").unwrap().len(),
        0
    );
}

#[test]
fn an_empty_index_still_needs_its_row_file() {
    // A manifest naming zero rows is still a manifest naming a generation: the row file must
    // exist, for a read-only open as for a writable one.
    let tmp = tempfile::tempdir().unwrap();
    drop(FlatIndex::create(tmp.path(), 3, Metric::Dot, "fp").unwrap());
    std::fs::remove_file(tmp.path().join("vectors.0.bin")).unwrap();
    assert!(matches!(
        FlatIndex::open_read_only(tmp.path()).unwrap_err(),
        Error::Io(_)
    ));
    assert!(matches!(
        FlatIndex::open(tmp.path()).unwrap_err(),
        Error::Io(_)
    ));
}

#[test]
fn bad_magic_and_truncation_are_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    drop(small(tmp.path()));
    let manifest = tmp.path().join("manifest.bin");
    let bytes = std::fs::read(&manifest).unwrap();
    // The header cut short.
    std::fs::write(&manifest, &bytes[..20]).unwrap();
    assert!(matches!(
        FlatIndex::open(tmp.path()).unwrap_err(),
        Error::Corrupt(_)
    ));
    // The tombstone bytes cut short.
    std::fs::write(&manifest, &bytes[..bytes.len() - 2]).unwrap();
    assert!(matches!(
        FlatIndex::open(tmp.path()).unwrap_err(),
        Error::Corrupt(_)
    ));
    let mut bad = bytes.clone();
    bad[0] = b'Y';
    std::fs::write(&manifest, bad).unwrap();
    assert!(matches!(
        FlatIndex::open(tmp.path()).unwrap_err(),
        Error::Corrupt(_)
    ));
    std::fs::write(&manifest, &bytes).unwrap();
    // A row file shorter than the manifest's rows: Corrupt, not a silent shrink.
    let rows = tmp.path().join("vectors.0.bin");
    let row_bytes = std::fs::read(&rows).unwrap();
    std::fs::write(&rows, &row_bytes[..row_bytes.len() - 1]).unwrap();
    assert!(matches!(
        FlatIndex::open(tmp.path()).unwrap_err(),
        Error::Corrupt(_)
    ));
    assert!(matches!(
        FlatIndex::open_read_only(tmp.path()).unwrap_err(),
        Error::Corrupt(_)
    ));
    #[cfg(feature = "mmap")]
    assert!(matches!(
        FlatIndex::open_mapped_read_only(tmp.path()).unwrap_err(),
        Error::Corrupt(_)
    ));
    std::fs::remove_file(&rows).unwrap();
    assert!(matches!(
        FlatIndex::open(tmp.path()).unwrap_err(),
        Error::Io(_)
    ));
    std::fs::remove_file(&manifest).unwrap();
    assert!(matches!(
        FlatIndex::open(tmp.path()).unwrap_err(),
        Error::Io(_)
    ));
}

#[test]
fn dimension_mismatch_names_both_dimensions_on_add_and_search() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = small(tmp.path());
    match index.add(DocId(9), &[1.0, 0.0]).unwrap_err() {
        Error::DimensionMismatch { expected, actual } => assert_eq!((expected, actual), (3, 2)),
        other => panic!("{other:?}"),
    }
    match index.search(&[1.0, 0.0, 0.0, 0.0], None, 1).unwrap_err() {
        Error::DimensionMismatch { expected, actual } => assert_eq!((expected, actual), (3, 4)),
        other => panic!("{other:?}"),
    }
    assert_eq!(index.len(), 2, "rejected add stored nothing");
    index.commit().unwrap();
    assert_eq!(index.len(), 2);
}

#[test]
fn non_finite_and_zero_norm_vectors_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = small(tmp.path());
    for bad in [
        [f32::NAN, 0.0, 0.0],
        [f32::INFINITY, 0.0, 0.0],
        [0.0, f32::NEG_INFINITY, 0.0],
    ] {
        assert!(
            matches!(index.add(DocId(9), &bad).unwrap_err(), Error::Schema(_)),
            "add {bad:?}"
        );
        assert!(
            matches!(
                index.search(&bad, None, 1).unwrap_err(),
                Error::InvalidQuery(_)
            ),
            "search {bad:?}"
        );
    }
    // Zero norm is undefined under cosine.
    assert!(matches!(
        index.add(DocId(9), &[0.0, 0.0, 0.0]).unwrap_err(),
        Error::Schema(_)
    ));
    assert!(matches!(
        index.search(&[0.0, 0.0, 0.0], None, 1).unwrap_err(),
        Error::InvalidQuery(_)
    ));
    index.commit().unwrap();
    assert_eq!(index.len(), 2, "nothing rejected was stored");

    // Under Dot a zero vector is fine.
    let tmp2 = tempfile::tempdir().unwrap();
    let mut dot = FlatIndex::create(tmp2.path(), 3, Metric::Dot, "fp").unwrap();
    dot.add(DocId(1), &[0.0, 0.0, 0.0]).unwrap();
    dot.commit().unwrap();
    assert_eq!(dot.search(&[0.0, 0.0, 0.0], None, 1).unwrap()[0].score, 0.0);
}

#[test]
fn non_unit_query_under_cosine_is_normalised_by_the_formula() {
    let tmp = tempfile::tempdir().unwrap();
    let index = small(tmp.path());
    let unit = index.search(&[1.0, 0.0, 0.0], None, 2).unwrap();
    let scaled = index.search(&[7.5, 0.0, 0.0], None, 2).unwrap();
    assert_eq!(unit, scaled);
    assert_eq!(unit[0].id, DocId(1));
    assert!((unit[0].score - 1.0).abs() <= 1e-6);
}

#[test]
fn k_and_allowed_edge_cases() {
    let tmp = tempfile::tempdir().unwrap();
    let index = small(tmp.path());
    let q = [1.0, 0.0, 0.0];
    assert!(index.search(&q, None, 0).unwrap().is_empty(), "k == 0");
    assert_eq!(
        index.search(&q, None, 100).unwrap().len(),
        2,
        "k > len returns all live"
    );
    assert!(
        index
            .search(&q, Some(&support::doc_set(&[])), 5)
            .unwrap()
            .is_empty(),
        "empty allowed"
    );
    let hits = index
        .search(&q, Some(&support::doc_set(&[2, 77, 78])), 5)
        .unwrap();
    assert_eq!(hits.len(), 1, "unseen ids are ignored");
    assert_eq!(hits[0].id, DocId(2));
    // Exactly tied scores at the last rank when k == len: both are orthogonal to this query.
    let hits = index.search(&[0.0, 0.0, 1.0], None, 2).unwrap();
    assert_eq!(hits[0].id, DocId(1));
    assert_eq!(hits[1].id, DocId(2));
    assert_eq!(hits[0].score.to_bits(), hits[1].score.to_bits());
}

// Review round 1 (Copilot) regressions.

#[test]
fn a_norm_that_does_not_fit_f32_is_rejected_on_add() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = small(tmp.path());
    // Finite components, norm ≈ 2.4e38 > f32::MAX: storing it as +inf would score its own
    // cosine as 0 instead of 1.
    let huge = [f32::MAX, f32::MAX, 0.0];
    assert!(matches!(
        index.add(DocId(9), &huge).unwrap_err(),
        Error::Schema(_)
    ));
    index.commit().unwrap();
    assert_eq!(index.len(), 2);
    // A large-but-representable norm is fine and scores 1.0 against itself.
    let big = [1.0e19, 1.0e19, 0.0];
    index.add(DocId(9), &big).unwrap();
    index.commit().unwrap();
    let hits = index.search(&big, None, 1).unwrap();
    assert_eq!(hits[0].id, DocId(9));
    assert!((hits[0].score - 1.0).abs() <= 1e-6, "{}", hits[0].score);
}

#[test]
fn a_failed_commit_keeps_the_staged_changes_for_a_retry() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = small(tmp.path());
    index.add(DocId(3), &[0.0, 0.0, 1.0]).unwrap();
    index.delete(&[DocId(1)]).unwrap();
    // Make the commit fail: the directory is replaced by a file, so nothing can be appended or
    // renamed. Nothing staged may be lost.
    let dir = tmp.path().to_path_buf();
    let stash = tmp.path().with_extension("moved");
    std::fs::rename(&dir, &stash).unwrap();
    std::fs::write(&dir, b"not a directory").unwrap();
    assert!(matches!(index.commit().unwrap_err(), Error::Io(_)));
    std::fs::remove_file(&dir).unwrap();
    std::fs::rename(&stash, &dir).unwrap();
    // Retry succeeds with exactly the staged changes applied.
    index.commit().unwrap();
    assert_eq!(index.len(), 2);
    let hits = index.search(&[0.0, 0.0, 1.0], None, 5).unwrap();
    assert_eq!(hits[0].id, DocId(3));
    assert!(hits.iter().all(|h| h.id != DocId(1)));
}

#[test]
fn a_malformed_query_is_an_error_even_when_no_work_would_be_done() {
    // Validation precedes the `k == 0` / empty-allowed shortcut (contract): a caller error is
    // reported regardless of how much work the call would do.
    let tmp = tempfile::tempdir().unwrap();
    let index = small(tmp.path());
    assert!(matches!(
        index.search(&[1.0, 0.0], None, 0).unwrap_err(),
        Error::DimensionMismatch { .. }
    ));
    assert!(matches!(
        index
            .search(&[f32::NAN, 0.0, 0.0], Some(&support::doc_set(&[])), 5)
            .unwrap_err(),
        Error::InvalidQuery(_)
    ));
}

#[test]
fn a_stale_writable_handle_refuses_to_commit_over_another_writer() {
    // Two writable handles violate the precondition; the guard turns a silent lost update —
    // v1 lost it too, v2 would also have cut the other handle's rows in place — into Corrupt.
    let tmp = tempfile::tempdir().unwrap();
    let mut a = FlatIndex::create(tmp.path(), 2, Metric::Dot, "fp").unwrap();
    a.add(DocId(1), &[1.0, 0.0]).unwrap();
    a.commit().unwrap();
    let mut b = FlatIndex::open(tmp.path()).unwrap();
    b.add(DocId(2), &[0.0, 1.0]).unwrap();
    b.commit().unwrap();
    let rows_before = std::fs::read(tmp.path().join("vectors.0.bin")).unwrap();
    a.add(DocId(3), &[0.5, 0.5]).unwrap();
    match a.commit().unwrap_err() {
        Error::Corrupt(msg) => assert!(msg.contains("another writer"), "{msg}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    a.delete(&[DocId(1)]).unwrap();
    assert!(matches!(a.compact().unwrap_err(), Error::Corrupt(_)));
    assert_eq!(
        std::fs::read(tmp.path().join("vectors.0.bin")).unwrap(),
        rows_before,
        "B's rows are intact"
    );
    let fresh = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(fresh.len(), 2);
    assert_eq!(fresh.vector(DocId(2)), Some(vec![0.0, 1.0]));

    // The check applies to no-ops too: a stale handle never reports success over another
    // writer's state.
    let mut idle = FlatIndex::open(tmp.path()).unwrap();
    let mut other = FlatIndex::open(tmp.path()).unwrap();
    other.add(DocId(4), &[0.1, 0.9]).unwrap();
    other.commit().unwrap();
    assert!(
        matches!(idle.commit().unwrap_err(), Error::Corrupt(_)),
        "an empty commit"
    );
    assert!(
        matches!(idle.compact().unwrap_err(), Error::Corrupt(_)),
        "a no-op compact"
    );

    // A delete-only commit by another handle changes neither generation nor rows — only the
    // live count and the tombstone set — and must be caught the same way, or the stale
    // handle's next commit would resurrect the deleted document.
    let mut c = FlatIndex::open(tmp.path()).unwrap();
    let mut d = FlatIndex::open(tmp.path()).unwrap();
    d.delete(&[DocId(2)]).unwrap();
    d.commit().unwrap();
    c.add(DocId(5), &[1.0, 1.0]).unwrap();
    assert!(matches!(c.commit().unwrap_err(), Error::Corrupt(_)));
    assert_eq!(
        FlatIndex::open(tmp.path()).unwrap().vector(DocId(2)),
        None,
        "the delete stands"
    );
    // Two delete-only commits with the same live count but different tombstones, likewise.
    let mut e = FlatIndex::open(tmp.path()).unwrap();
    e.add(DocId(6), &[0.5, 0.5]).unwrap();
    e.add(DocId(7), &[0.25, 0.75]).unwrap();
    e.commit().unwrap();
    let mut f2 = FlatIndex::open(tmp.path()).unwrap();
    f2.delete(&[DocId(6)]).unwrap();
    f2.commit().unwrap();
    e.delete(&[DocId(7)]).unwrap();
    assert!(matches!(e.commit().unwrap_err(), Error::Corrupt(_)));
    let fresh = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(
        (fresh.vector(DocId(6)), fresh.vector(DocId(7)).is_some()),
        (None, true)
    );
}

#[cfg(unix)]
#[test]
fn a_failed_create_leaves_the_directory_empty_for_a_retry() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("idx");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    assert!(matches!(
        FlatIndex::create(&dir, 3, Metric::Dot, "fp").unwrap_err(),
        Error::Io(_)
    ));
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        std::fs::read_dir(&dir).unwrap().next().is_none(),
        "nothing left behind"
    );
    assert_eq!(
        FlatIndex::create(&dir, 3, Metric::Dot, "fp").unwrap().len(),
        0
    );
}
