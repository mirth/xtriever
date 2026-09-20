//! Feature 026 (spec FR-001, SC-006; contracts/dense-format-v3.md "Refusals"): every directory
//! this build must not read is refused by name, and a row this engine could not have written
//! is refused as corrupt.
//!
//! - a version-1 directory (`index.bin`) and a version-2 manifest — by its magic, and by a
//!   version-2 header behind the current magic — are refused naming both versions, with the
//!   instruction to rebuild;
//! - an unknown quantisation scheme, or none, is refused naming the scheme this build reads;
//! - a row count beyond the row file is refused as corrupt (as in version 2);
//! - a zero, denormal or NaN scale, and a NaN or zero norm under Cosine, are refused as corrupt
//!   **by the readers** (the scan and `vector`): an open reads nothing beyond the manifest
//!   (Feature 024), so the first reader that reaches the row is where it is caught, before a
//!   NaN could order anything; under Dot and Euclidean the norm is never read and so never
//!   refused — the index degrades instead.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::rewrite_manifest_header;
use xtriever_core::{DocId, Error, Metric, VectorIndex};
use xtriever_dense::{FORMAT_VERSION, FlatIndex};

fn small(dir: &std::path::Path, metric: Metric) -> u64 {
    let mut index = FlatIndex::create(dir, 3, metric, "fp-a").unwrap();
    index.add(DocId(1), &[1.0, 0.0, 0.0]).unwrap();
    index.add(DocId(2), &[0.0, 1.0, 0.5]).unwrap();
    index.commit().unwrap();
    index.stats().generation
}

fn corrupt_message(result: Result<FlatIndex, Error>) -> String {
    match result.map(|_| ()).unwrap_err() {
        Error::Corrupt(msg) => msg,
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

fn says_rebuild(msg: &str, version: &str) {
    assert!(msg.contains(version), "{msg}");
    assert!(msg.contains(&FORMAT_VERSION.to_string()), "{msg}");
    assert!(msg.contains("rebuild"), "{msg}");
    assert!(msg.contains("ADR-0015"), "{msg}");
}

#[test]
fn a_version_1_directory_is_refused_with_the_rebuild_instruction() {
    let tmp = tempfile::tempdir().unwrap();
    let header = r#"{"format_version":1,"dim":3,"metric":"cosine","fingerprint":"fp-a","count":0}"#;
    let mut v1 = b"XTDENSE1".to_vec();
    v1.extend_from_slice(&(header.len() as u64).to_le_bytes());
    v1.extend_from_slice(header.as_bytes());
    std::fs::write(tmp.path().join("index.bin"), v1).unwrap();
    says_rebuild(&corrupt_message(FlatIndex::open(tmp.path())), "version 1");
}

#[test]
fn a_version_2_manifest_is_refused_with_the_rebuild_instruction() {
    // By its magic: what every Feature 024/025 artefact carries.
    let tmp = tempfile::tempdir().unwrap();
    small(tmp.path(), Metric::Cosine);
    let path = tmp.path().join("manifest.bin");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[..8].copy_from_slice(b"XTDENSE2");
    std::fs::write(&path, &bytes).unwrap();
    says_rebuild(&corrupt_message(FlatIndex::open(tmp.path())), "version 2");

    // By its header, behind the current magic.
    let again = tempfile::tempdir().unwrap();
    small(again.path(), Metric::Cosine);
    rewrite_manifest_header(again.path(), |h| {
        assert!(h.contains("\"format_version\":3"), "{h}");
        h.replace("\"format_version\":3", "\"format_version\":2")
    });
    says_rebuild(&corrupt_message(FlatIndex::open(again.path())), "version 2");
}

#[test]
fn an_unknown_or_missing_scheme_is_refused_by_name() {
    for (edit, label) in [
        (r#""scheme":"i4-blockwise""#, "another scheme"),
        (r#""scheme":"""#, "no scheme"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        small(tmp.path(), Metric::Dot);
        rewrite_manifest_header(tmp.path(), |h| {
            assert!(h.contains(r#""scheme":"i8-symmetric-per-vector""#), "{h}");
            h.replace(r#""scheme":"i8-symmetric-per-vector""#, edit)
        });
        let msg = corrupt_message(FlatIndex::open(tmp.path()));
        assert!(msg.contains("scheme"), "{label}: {msg}");
        assert!(msg.contains("i8-symmetric-per-vector"), "{label}: {msg}");
        assert!(msg.contains("rebuild"), "{label}: {msg}");
    }
}

#[test]
fn a_row_count_beyond_the_row_file_is_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    let generation = small(tmp.path(), Metric::Dot);
    let rows = support::row_file(tmp.path(), generation);
    let bytes = std::fs::read(&rows).unwrap();
    assert_eq!(bytes.len() as u64, 2 * support::row_bytes(3));
    std::fs::write(&rows, &bytes[..bytes.len() - 1]).unwrap();
    corrupt_message(FlatIndex::open(tmp.path()));
}

/// Overwrite four bytes at `offset` of row `r` in the committed row file.
fn doctor_row(dir: &std::path::Path, generation: u64, r: usize, offset: usize, value: f32) {
    let path = support::row_file(dir, generation);
    let mut bytes = std::fs::read(&path).unwrap();
    let at = r * support::row_bytes(3) as usize + offset;
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
}

#[test]
fn a_scale_this_engine_never_writes_is_refused_by_the_scan() {
    // Offset 8 of a row is its scale (contract "A row").
    for (value, label) in [
        (0.0f32, "zero"),
        (f32::NAN, "NaN"),
        (-1.0, "negative"),
        (1e-40, "denormal"),
        (f32::INFINITY, "infinite"),
    ] {
        for metric in [Metric::Cosine, Metric::Dot, Metric::Euclidean] {
            let tmp = tempfile::tempdir().unwrap();
            let generation = small(tmp.path(), metric);
            doctor_row(tmp.path(), generation, 1, 8, value);
            // The open succeeds: it reads the manifest, not the rows.
            let index = FlatIndex::open(tmp.path()).unwrap();
            match index.search(&[1.0, 0.0, 0.0], None, 2).unwrap_err() {
                Error::Corrupt(msg) => {
                    assert!(msg.contains("scale"), "{label} under {metric:?}: {msg}");
                    assert!(msg.contains("row 1"), "{label} under {metric:?}: {msg}");
                    assert!(msg.contains("id 2"), "{label} under {metric:?}: {msg}");
                }
                other => panic!("{label} under {metric:?}: expected Corrupt, got {other:?}"),
            }
            // A search that never reaches the row is unaffected: the damage is local.
            let only_first = support::doc_set(&[1]);
            assert_eq!(
                index
                    .search(&[1.0, 0.0, 0.0], Some(&only_first), 2)
                    .unwrap()
                    .len(),
                1
            );
            // And `vector` refuses the same row rather than recovering NaN or zeros from it
            // (review round 3, finding 4), while the intact row still recovers.
            assert!(
                matches!(index.vector(DocId(2)), Err(Error::Corrupt(_))),
                "{label}"
            );
            assert!(index.vector(DocId(1)).unwrap().is_some());
        }
    }
}

#[test]
fn a_norm_cosine_cannot_divide_by_is_refused_by_the_scan() {
    // Offset 4 of a row is its norm. A NaN norm would score NaN and make the order arbitrary;
    // a zero norm under Cosine cannot have been written (`add` refuses the vector).
    for (value, label) in [
        (f32::NAN, "NaN"),
        (0.0f32, "zero"),
        (f32::INFINITY, "infinite"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let generation = small(tmp.path(), Metric::Cosine);
        doctor_row(tmp.path(), generation, 0, 4, value);
        let index = FlatIndex::open(tmp.path()).unwrap();
        match index.search(&[1.0, 0.0, 0.0], None, 2).unwrap_err() {
            Error::Corrupt(msg) => {
                assert!(msg.contains("norm"), "{label}: {msg}");
                assert!(msg.contains("row 0"), "{label}: {msg}");
            }
            other => panic!("{label}: expected Corrupt, got {other:?}"),
        }
    }
    // Under Dot and Euclidean the norm is never read, so a damaged one changes no score and
    // refuses nothing: the index degrades to exactly what it would have returned (Principle
    // VI; review round 6, finding 7). A zero norm is also what a zero vector stores under Dot.
    for metric in [Metric::Dot, Metric::Euclidean] {
        for value in [0.0f32, f32::NAN, f32::INFINITY, -1.0] {
            let tmp = tempfile::tempdir().unwrap();
            let generation = small(tmp.path(), metric);
            let intact = FlatIndex::open(tmp.path()).unwrap();
            let before = support::hit_bits(&intact.search(&[1.0, 0.0, 0.0], None, 2).unwrap());
            drop(intact);
            doctor_row(tmp.path(), generation, 0, 4, value);
            let index = FlatIndex::open(tmp.path()).unwrap();
            let after = support::hit_bits(&index.search(&[1.0, 0.0, 0.0], None, 2).unwrap());
            assert_eq!(after, before, "{metric:?} with norm {value}");
            assert!(index.vector(DocId(1)).unwrap().is_some());
        }
    }
}

#[test]
fn a_vector_that_is_zero_at_eight_bit_precision_is_refused_under_cosine_at_add() {
    // Its float norm is not zero, but every component is below half the scale floor, so the
    // stored row would be all-zero codes with a zero norm — which cosine cannot point with.
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, Metric::Cosine, "fp").unwrap();
    match index.add(DocId(1), &[1e-44, 0.0]).unwrap_err() {
        Error::Schema(msg) => assert!(msg.contains("eight-bit"), "{msg}"),
        other => panic!("expected Schema, got {other:?}"),
    }
    // The same vector is fine under Dot: it is a zero vector, which Dot allows.
    let dot = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(dot.path(), 2, Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1e-44, 0.0]).unwrap();
    index.commit().unwrap();
    assert_eq!(index.search(&[1.0, 1.0], None, 1).unwrap()[0].score, 0.0);
}
