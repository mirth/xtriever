//! US2 scenarios 4–5 — replace, delete, commit visibility, reopen without commit (spec FR-014).
//! Drives `mutations.json` step by step. Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::Step;
use xtriever_core::{DocId, VectorIndex};
use xtriever_dense::FlatIndex;

#[test]
fn scripted_mutations_match_the_oracle_after_every_commit() {
    let m = support::mutations();
    let tmp = tempfile::tempdir().unwrap();
    let metric = support::parse_metric(&m.metric);
    let mut index = FlatIndex::create(tmp.path(), m.dim, metric, &m.fingerprint).unwrap();
    for (n, step) in m.steps.iter().enumerate() {
        match step {
            Step::Add { id, vector } => index.add(DocId(*id), vector).unwrap(),
            Step::Delete { ids } => {
                let ids: Vec<DocId> = ids.iter().map(|&i| DocId(i)).collect();
                index.delete(&ids).unwrap();
            }
            Step::Commit => index.commit().unwrap(),
            Step::Reopen => {
                drop(index);
                index = FlatIndex::open(tmp.path()).unwrap();
            }
            Step::Expect {
                len,
                query,
                k,
                results,
            } => {
                assert_eq!(index.len(), *len, "step {n}: len");
                assert_eq!(index.is_empty(), *len == 0);
                let hits = index.search(query, None, *k).unwrap();
                support::assert_hits(&hits, results, m.score_abs_tol, &format!("step {n}"));
            }
        }
    }
}

#[test]
fn replace_is_invisible_until_commit_and_then_appears_once() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, xtriever_core::Metric::Dot, "fp").unwrap();
    index.add(DocId(1), &[1.0, 0.0]).unwrap();
    index.add(DocId(2), &[0.0, 1.0]).unwrap();
    index.commit().unwrap();
    let before = index.search(&[1.0, 0.0], None, 10).unwrap();
    assert_eq!(before[0].id, DocId(1));

    index.add(DocId(1), &[0.0, 1.0]).unwrap(); // replace, pending
    assert_eq!(
        index.search(&[1.0, 0.0], None, 10).unwrap(),
        before,
        "pending is invisible"
    );
    assert_eq!(index.len(), 2);
    index.commit().unwrap();
    let after = index.search(&[0.0, 1.0], None, 10).unwrap();
    assert_eq!(index.len(), 2, "replaced id appears once");
    assert_eq!(after.iter().filter(|h| h.id == DocId(1)).count(), 1);
    assert_eq!(after[0].score, 1.0);
}

#[test]
fn delete_unknown_ids_is_a_no_op_and_deleted_ids_never_return() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, xtriever_core::Metric::Dot, "fp").unwrap();
    for i in 0..5u32 {
        index.add(DocId(i), &[i as f32, 1.0]).unwrap();
    }
    index.commit().unwrap();
    index.delete(&[DocId(3), DocId(999)]).unwrap();
    assert_eq!(index.len(), 5, "delete pending is invisible");
    index.commit().unwrap();
    assert_eq!(index.len(), 4);
    let hits = index.search(&[1.0, 0.0], None, 10).unwrap();
    assert!(hits.iter().all(|h| h.id != DocId(3)));
    assert_eq!(hits.len(), 4);
    // Deleting everything leaves an empty, searchable index.
    index
        .delete(&(0..5).map(DocId).collect::<Vec<_>>())
        .unwrap();
    index.commit().unwrap();
    assert!(index.is_empty());
    assert!(index.search(&[1.0, 0.0], None, 10).unwrap().is_empty());
}

#[test]
fn add_within_a_batch_replaces_and_commit_without_pending_is_a_no_op() {
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), 2, xtriever_core::Metric::Dot, "fp").unwrap();
    index.add(DocId(7), &[1.0, 0.0]).unwrap();
    index.add(DocId(7), &[0.0, 1.0]).unwrap(); // second add wins
    index.commit().unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(index.search(&[0.0, 1.0], None, 1).unwrap()[0].score, 1.0);
    let before = std::fs::metadata(tmp.path().join("manifest.bin"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    index.commit().unwrap(); // nothing pending
    let after = std::fs::metadata(tmp.path().join("manifest.bin"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(before, after, "an empty commit must not rewrite the file");
}
