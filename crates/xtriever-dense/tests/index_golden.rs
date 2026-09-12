//! US2 scenarios 1–3 — the exact-search oracle (spec FR-010–FR-013, FR-018, SC-003). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{DocId, VectorIndex};
use xtriever_dense::FlatIndex;

fn build(set: &support::SearchSet, dir: &std::path::Path) -> FlatIndex {
    let mut index = FlatIndex::create(dir, set.dim, support::parse_metric(&set.metric), "test-fp")
        .expect("create");
    for row in &set.rows {
        index.add(DocId(row.id), &row.vector).expect("add");
    }
    index.commit().expect("commit");
    assert_eq!(index.len(), set.rows.len() as u64);
    index
}

fn run_cases(index: &dyn VectorIndex, set: &support::SearchSet, tol: f64, label: &str) {
    for q in &set.queries {
        for case in &q.cases {
            let allowed = case.allowed.as_deref().map(support::doc_set);
            let hits = index
                .search(&q.vector, allowed.as_ref(), case.k)
                .expect("search");
            support::assert_hits(
                &hits,
                &case.expected,
                tol,
                &format!(
                    "{label} {} {} k={} allowed={:?}",
                    set.id,
                    q.id,
                    case.k,
                    case.allowed.as_ref().map(Vec::len)
                ),
            );
        }
    }
}

#[test]
fn every_search_golden_matches_exactly() {
    let goldens = support::search();
    for set in &goldens.sets {
        let tmp = tempfile::tempdir().unwrap();
        let index = build(set, tmp.path());
        run_cases(&index, set, goldens.score_abs_tol, "open");
    }
}

/// The designed ties at the k-th rank are asserted by name so they cannot be silently dropped
/// from the goldens (FR-011: the tie-break holds at the k-th rank).
#[test]
fn designed_ties_at_the_k_th_rank_go_to_the_lower_id() {
    let goldens = support::search();
    let mut checked = 0;
    for set in &goldens.sets {
        let tmp = tempfile::tempdir().unwrap();
        let index = build(set, tmp.path());
        for tie in &set.designed_ties {
            let q = set.queries.iter().find(|q| q.id == tie.query).unwrap();
            let hits = index.search(&q.vector, None, tie.k).unwrap();
            assert_eq!(hits.len(), tie.k);
            assert_eq!(
                hits[tie.k - 1].id,
                DocId(tie.winner),
                "{} {}: rank {} winner",
                set.id,
                tie.query,
                tie.k
            );
            // With k + 1 both members appear, adjacent, equal score, lower id first.
            let more = index.search(&q.vector, None, tie.k + 1).unwrap();
            assert_eq!(more[tie.k - 1].id, DocId(tie.winner));
            assert_eq!(more[tie.k].id, DocId(*tie.ids.iter().max().unwrap()));
            assert_eq!(
                more[tie.k - 1].score.to_bits(),
                more[tie.k].score.to_bits(),
                "tied scores are bit-identical"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 8,
        "expected at least two designed ties per set, checked {checked}"
    );
}

/// FR-013: an allowed set filters the unrestricted results without changing scores.
#[test]
fn allowed_results_are_the_unrestricted_results_filtered() {
    let goldens = support::search();
    let set = goldens
        .sets
        .iter()
        .find(|s| s.id == "dim384_cosine")
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let index = build(set, tmp.path());
    let all_ids: Vec<u32> = set.rows.iter().map(|r| r.id).collect();
    let subset: Vec<u32> = all_ids.iter().copied().filter(|i| i % 3 == 0).collect();
    for q in &set.queries {
        let unrestricted = index.search(&q.vector, None, all_ids.len()).unwrap();
        let filtered = index
            .search(&q.vector, Some(&support::doc_set(&subset)), all_ids.len())
            .unwrap();
        let expected: Vec<_> = unrestricted
            .iter()
            .filter(|h| subset.contains(&h.id.0))
            .collect();
        assert_eq!(filtered.len(), expected.len());
        for (f, e) in filtered.iter().zip(expected) {
            assert_eq!(f.id, e.id);
            assert_eq!(
                f.score.to_bits(),
                e.score.to_bits(),
                "score changed under a filter"
            );
        }
        // The full allowed set is the identity.
        let full = index
            .search(&q.vector, Some(&support::doc_set(&all_ids)), 10)
            .unwrap();
        let none = index.search(&q.vector, None, 10).unwrap();
        assert_eq!(full, none);
    }
}

#[cfg(feature = "mmap")]
#[test]
fn mapped_open_is_bit_identical_to_buffered_open() {
    let goldens = support::search();
    for set in &goldens.sets {
        let tmp = tempfile::tempdir().unwrap();
        let owned = build(set, tmp.path());
        let mapped = FlatIndex::open_mapped(tmp.path()).expect("open_mapped");
        run_cases(&mapped, set, goldens.score_abs_tol, "open_mapped");
        for q in &set.queries {
            let a = owned.search(&q.vector, None, set.rows.len()).unwrap();
            let b = mapped.search(&q.vector, None, set.rows.len()).unwrap();
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(&b) {
                assert_eq!(x.id, y.id);
                assert_eq!(
                    x.score.to_bits(),
                    y.score.to_bits(),
                    "{} {}: mapped vs owned bits",
                    set.id,
                    q.id
                );
            }
        }
    }
}
