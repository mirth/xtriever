//! Feature 026 (spec FR-001, FR-002, FR-004; contracts/dense-format-v3.md): what a version-3
//! directory looks like on disk and what the stage promises about scoring it.
//!
//! - a committed index has 396-byte rows at dimension 384, the manifest carries the versioned
//!   magic, `format_version` 3 and the scheme's name, and the row file is under a third of the
//!   float file version 2 wrote for the same rows;
//! - the same query gives identical score bits twice, after a reopen, and after a compaction
//!   (FR-002, determinism);
//! - cosine is the cosine of the stored rows: a row's cosine with itself is one and no score
//!   exceeds one beyond the final rounding (review finding 6).
//!
//! Candidate agreement with a float ranking (SC-001) is **not** asserted here: the crate holds
//! no float index, and on synthetic vectors the agreement sits at the bound (98.7–99.2 % at
//! depth 100) where real embeddings measure above it. It is measured on real corpora by
//! `reference/int8_vectors_study.py`, which reads the embedder's floats the evaluation harness
//! keeps beside each dataset's cache and the rows this engine stored, and by the three-dataset
//! gate in PR B; a test on random vectors would be a test of random vectors.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{Lcg, hit_bits, row_bytes, row_file};
use xtriever_core::{DocId, Metric, VectorIndex};
use xtriever_dense::{FORMAT_VERSION, FlatIndex};

const DIM: usize = 384;
const ROWS: u32 = 200;

fn filled(dir: &std::path::Path, metric: Metric) -> (FlatIndex, Vec<Vec<f32>>) {
    let mut rng = Lcg(0x0026_0384);
    let mut index = FlatIndex::create(dir, DIM, metric, "fp-026").unwrap();
    let vectors: Vec<Vec<f32>> = (0..ROWS).map(|_| rng.vector(DIM)).collect();
    for (i, v) in vectors.iter().enumerate() {
        index.add(DocId(i as u32), v).unwrap();
    }
    index.commit().unwrap();
    (index, vectors)
}

#[test]
fn a_committed_index_is_version_3_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let (index, _) = filled(tmp.path(), Metric::Cosine);
    let generation = index.stats().generation;
    let rows = std::fs::metadata(row_file(tmp.path(), generation))
        .unwrap()
        .len();
    assert_eq!(
        rows,
        u64::from(ROWS) * row_bytes(DIM),
        "396 bytes per row at dim 384"
    );
    assert_eq!(row_bytes(DIM), 396);
    // Under a third of what version 2 wrote for the same rows: `8 + 4 × dim` each (ADR-0013).
    let float_rows = u64::from(ROWS) * (8 + 4 * DIM as u64);
    assert!(
        rows * 3 < float_rows,
        "{rows} against {float_rows} float bytes"
    );

    let manifest = std::fs::read(tmp.path().join("manifest.bin")).unwrap();
    assert_eq!(&manifest[..8], b"XTDENSE3", "the versioned magic");
    let hdr_len = u64::from_le_bytes(manifest[8..16].try_into().unwrap()) as usize;
    let header: serde_json::Value = serde_json::from_slice(&manifest[16..16 + hdr_len]).unwrap();
    assert_eq!(header["format_version"], FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 3);
    assert_eq!(
        header["scheme"], "i8-symmetric-per-vector",
        "the header names the scheme"
    );
    assert_eq!(header["dim"], DIM);
}

#[test]
fn the_same_query_gives_the_same_bits_across_runs_reopens_and_compactions() {
    for metric in [Metric::Cosine, Metric::Dot, Metric::Euclidean] {
        let tmp = tempfile::tempdir().unwrap();
        let (mut index, vectors) = filled(tmp.path(), metric);
        let mut rng = Lcg(0x0026_0001);
        let queries: Vec<Vec<f32>> = (0..5).map(|_| rng.vector(DIM)).collect();
        let run = |index: &FlatIndex| -> Vec<Vec<(u32, u32)>> {
            queries
                .iter()
                .map(|q| hit_bits(&index.search(q, None, 20).unwrap()))
                .collect()
        };
        let first = run(&index);
        assert_eq!(run(&index), first, "{metric:?}: twice");
        drop(index);
        let mut reopened = FlatIndex::open(tmp.path()).unwrap();
        assert_eq!(run(&reopened), first, "{metric:?}: after a reopen");
        reopened.delete(&[DocId(ROWS + 7)]).unwrap(); // an unknown id: nothing changes
        reopened.commit().unwrap();
        reopened.compact().unwrap();
        assert_eq!(run(&reopened), first, "{metric:?}: after a compaction");
        index = reopened;
        // And the rows recover to what the scheme says they recover to.
        for (i, v) in vectors.iter().enumerate().take(10) {
            support::assert_recovered(index.vector(DocId(i as u32)), v, &format!("{metric:?} {i}"));
        }
    }
}

#[test]
fn cosine_is_the_cosine_of_the_stored_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let (index, vectors) = filled(tmp.path(), Metric::Cosine);
    // Every row against itself: one, to within the one rounding of the f64 division into f32.
    for (i, v) in vectors.iter().enumerate().take(25) {
        let hits = index.search(v, None, 1).unwrap();
        assert_eq!(hits[0].id, DocId(i as u32), "row {i} is its own best match");
        assert!(
            (hits[0].score - 1.0).abs() <= 2.0 * f32::EPSILON,
            "row {i}: self-similarity {}",
            hits[0].score
        );
    }
    // The case from the review, through the public surface: [1.0, 0.006] scored 1.000026
    // against the float norm.
    let small = tempfile::tempdir().unwrap();
    let mut two = FlatIndex::create(small.path(), 2, Metric::Cosine, "fp").unwrap();
    two.add(DocId(1), &[1.0, 0.006]).unwrap();
    two.commit().unwrap();
    let s = two.search(&[1.0, 0.006], None, 1).unwrap()[0].score;
    assert!((s - 1.0).abs() <= 2.0 * f32::EPSILON, "{s}");
    // And nothing exceeds one beyond that rounding, over every row for every query.
    let mut rng = Lcg(0x0026_0002);
    for _ in 0..20 {
        let q = rng.vector(DIM);
        for hit in index.search(&q, None, ROWS as usize).unwrap() {
            assert!(hit.score <= 1.0 + 2.0 * f32::EPSILON, "{hit:?}");
            assert!(hit.score >= -1.0 - 2.0 * f32::EPSILON, "{hit:?}");
        }
    }
}
