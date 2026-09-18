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
    rewrite_manifest_header(tmp.path(), |h| {
        assert!(h.contains("\"format_version\":2"), "{h}");
        h.replace("\"format_version\":2", "\"format_version\":3")
    });
    let err = FlatIndex::open(tmp.path()).unwrap_err();
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
