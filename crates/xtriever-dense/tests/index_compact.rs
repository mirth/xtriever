//! Feature 024, US2 and US4 (spec FR-005, FR-013; contract): `compact` rewrites the live rows
//! in ascending id order under a new generation and changes no result bit; the dead-row share
//! threshold makes `commit` compact on the crossing commit and never otherwise. Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::Path;

use xtriever_core::{DocId, Error, Metric, VectorIndex};
use xtriever_dense::{DenseStats, FlatIndex};

const DIM: usize = 4;
const ROW: usize = 8 + DIM * 4;

fn vec_for(i: u32) -> Vec<f32> {
    let f = i as f32;
    vec![f + 1.0, (f * 0.7).sin(), (f * 0.3).cos(), 1.0]
}

fn queries() -> Vec<Vec<f32>> {
    (0..20)
        .map(|i| vec![1.0, (i as f32).sin(), (i as f32 * 0.5).cos(), 0.25])
        .collect()
}

fn results(index: &FlatIndex) -> Vec<Vec<(u32, u32)>> {
    queries()
        .iter()
        .map(|q| {
            index
                .search(q, None, 50)
                .unwrap()
                .iter()
                .map(|h| (h.id.0, h.score.to_bits()))
                .collect()
        })
        .collect()
}

fn ids_in_file(path: &Path) -> Vec<u32> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes.len() % ROW, 0);
    bytes
        .chunks_exact(ROW)
        .map(|r| u32::from_le_bytes([r[0], r[1], r[2], r[3]]))
        .collect()
}

/// 60 rows, then 10 replaced and 8 deleted, committed: 70 rows in the file, 52 live.
fn churned(dir: &Path) -> FlatIndex {
    let mut index = FlatIndex::create(dir, DIM, Metric::Cosine, "fp").unwrap();
    for i in 0..60 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    for i in (0..60).step_by(6) {
        index.add(DocId(i), &vec_for(i + 100)).unwrap();
    }
    index.commit().unwrap();
    let dead: Vec<DocId> = (1..60).step_by(8).map(DocId).collect();
    index.delete(&dead).unwrap();
    index.commit().unwrap();
    assert_eq!(
        index.stats(),
        DenseStats {
            rows: 70,
            live: 52,
            dead: 18,
            generation: 0
        }
    );
    index
}

#[test]
fn compact_drops_dead_rows_and_keeps_every_bit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = churned(tmp.path());
    let before = results(&index);
    index.compact().unwrap();
    assert_eq!(
        index.stats(),
        DenseStats {
            rows: 52,
            live: 52,
            dead: 0,
            generation: 1
        }
    );
    assert!(tmp.path().join("vectors.1.bin").is_file());
    assert!(!tmp.path().join("vectors.0.bin").exists());
    let ids = ids_in_file(&tmp.path().join("vectors.1.bin"));
    assert_eq!(ids.len(), 52);
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "ids ascend: {ids:?}");
    assert_eq!(results(&index), before);
    assert_eq!(index.len(), 52);
    drop(index);
    let reopened = FlatIndex::open(tmp.path()).unwrap();
    assert_eq!(results(&reopened), before);
    assert_eq!(reopened.stats().generation, 1);
}

#[test]
fn compact_is_a_no_op_when_nothing_is_dead_and_ids_ascend() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), DIM, Metric::Dot, "fp").unwrap();
    for i in 0..30 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    for i in 30..40 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    let manifest = std::fs::read(tmp.path().join("manifest.bin")).unwrap();
    index.compact().unwrap();
    assert_eq!(index.stats().generation, 0);
    assert_eq!(
        std::fs::read(tmp.path().join("manifest.bin")).unwrap(),
        manifest
    );
    assert!(tmp.path().join("vectors.0.bin").is_file());
}

#[test]
fn compact_commits_pending_changes_first() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = churned(tmp.path());
    index.add(DocId(200), &vec_for(200)).unwrap();
    index.delete(&[DocId(0)]).unwrap();
    index.compact().unwrap();
    assert_eq!(index.len(), 52);
    assert_eq!(index.vector(DocId(200)), Some(vec_for(200)));
    assert_eq!(index.vector(DocId(0)), None);
    assert_eq!(index.stats().dead, 0);
}

#[test]
fn threshold_compacts_on_the_crossing_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), DIM, Metric::Dot, "fp").unwrap();
    index.set_compaction_threshold(Some(0.25)).unwrap();
    for i in 0..100 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    let dead: Vec<DocId> = (0..20).map(DocId).collect();
    index.delete(&dead).unwrap();
    index.commit().unwrap();
    assert_eq!(
        index.stats(),
        DenseStats {
            rows: 100,
            live: 80,
            dead: 20,
            generation: 0
        },
        "20 % is not over 25 %"
    );
    let more: Vec<DocId> = (20..30).map(DocId).collect();
    index.delete(&more).unwrap();
    index.commit().unwrap();
    assert_eq!(
        index.stats(),
        DenseStats {
            rows: 70,
            live: 70,
            dead: 0,
            generation: 1
        },
        "30 % crosses 25 %: compacted in that commit"
    );
    assert!(!tmp.path().join("vectors.0.bin").exists());
}

#[test]
fn without_a_threshold_nothing_compacts_until_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), DIM, Metric::Dot, "fp").unwrap();
    for i in 0..100 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    let dead: Vec<DocId> = (0..90).map(DocId).collect();
    index.delete(&dead).unwrap();
    index.commit().unwrap();
    assert_eq!(index.stats().dead, 90);
    assert_eq!(index.stats().generation, 0);
    index.compact().unwrap();
    assert_eq!(index.stats().dead, 0);
    assert_eq!(index.stats().rows, 10);
}

#[test]
fn threshold_edges() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), DIM, Metric::Dot, "fp").unwrap();
    assert!(matches!(
        index.set_compaction_threshold(Some(1.5)).unwrap_err(),
        Error::Schema(_)
    ));
    assert!(matches!(
        index.set_compaction_threshold(Some(-0.1)).unwrap_err(),
        Error::Schema(_)
    ));
    assert!(matches!(
        index.set_compaction_threshold(Some(f32::NAN)).unwrap_err(),
        Error::Schema(_)
    ));
    for i in 0..10 {
        index.add(DocId(i), &vec_for(i)).unwrap();
    }
    index.commit().unwrap();
    // 0.0: any dead row compacts.
    index.set_compaction_threshold(Some(0.0)).unwrap();
    index.delete(&[DocId(1)]).unwrap();
    index.commit().unwrap();
    assert_eq!(index.stats().generation, 1);
    assert_eq!(index.stats().dead, 0);
    // 1.0: never (dead / rows cannot exceed 1).
    index.set_compaction_threshold(Some(1.0)).unwrap();
    let rest: Vec<DocId> = (2..10).map(DocId).collect();
    index.delete(&rest).unwrap();
    index.commit().unwrap();
    assert_eq!(index.stats().generation, 1);
    assert_eq!(index.stats().dead, 8);
    // None: back to explicit only.
    index.set_compaction_threshold(None).unwrap();
    index.delete(&[DocId(0)]).unwrap();
    index.commit().unwrap();
    assert_eq!(index.stats().generation, 1);
    assert_eq!(index.len(), 0);
}
