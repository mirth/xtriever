//! User Story 2 — ties, segments and repeated runs behave identically (ADR-0005, FR-013–FR-015).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{TestIndex, corpus, ids, index_in_batches, queries};
use xtriever_core::LexicalIndex;

fn build(batches: usize) -> TestIndex {
    let mut t = TestIndex::create_fixture();
    index_in_batches(&mut t.index, &corpus().documents, batches);
    t
}

// Scenario 1: one batch vs four batches — identical for every golden
#[test]
fn single_and_multi_batch_layouts_rank_identically() {
    let one = build(1);
    let four = build(4);
    for entry in queries().queries {
        let a = one
            .index
            .search(&entry.query, entry.filter.as_ref(), entry.k)
            .expect("search");
        let b = four
            .index
            .search(&entry.query, entry.filter.as_ref(), entry.k)
            .expect("search");
        support::assert_hits_exact(&format!("{} (1 vs 4 batches)", entry.name), &b, &a);
    }
}

// Scenario 2: ties come back in ascending DocId in a multi-segment index
#[test]
fn ties_are_ordered_by_ascending_doc_id_across_segments() {
    let four = build(4);
    for name in ["fuzzy_d1", "fuzzy_d2", "term_tag_tie"] {
        let entry = support::query(name);
        let hits = four
            .index
            .search(&entry.query, None, entry.k)
            .expect("search");
        let mut last: Option<(f32, u32)> = None;
        for h in &hits {
            if let Some((ls, lid)) = last {
                if ls == h.score {
                    assert!(
                        lid < h.id.0,
                        "{name}: tie not in ascending DocId: {lid} before {}",
                        h.id.0
                    );
                } else {
                    assert!(ls > h.score, "{name}: not descending");
                }
            }
            last = Some((h.score, h.id.0));
        }
    }
}

// Scenario 3: repeat call identical
#[test]
fn repeated_search_is_identical() {
    let t = build(3);
    for entry in queries().queries {
        let a = t
            .index
            .search(&entry.query, entry.filter.as_ref(), entry.k)
            .expect("search");
        let b = t
            .index
            .search(&entry.query, entry.filter.as_ref(), entry.k)
            .expect("search");
        support::assert_hits_exact(&entry.name, &b, &a);
    }
}

// Scenario 4: identical before and after merge
#[test]
fn merge_does_not_change_results() {
    let mut t = build(4);
    let before: Vec<_> = queries()
        .queries
        .iter()
        .map(|e| {
            t.index
                .search(&e.query, e.filter.as_ref(), e.k)
                .expect("search")
        })
        .collect();
    t.index.merge().expect("merge");
    for (e, b) in queries().queries.iter().zip(before) {
        let a = t
            .index
            .search(&e.query, e.filter.as_ref(), e.k)
            .expect("search");
        support::assert_hits_exact(&format!("{} (after merge)", e.name), &a, &b);
    }
}

// Scenario 5 (FR-014 as revised): a tie spanning the k-boundary is broken by ascending DocId —
// the returned members are the k lowest ids of the tie group, in every segment layout.
#[test]
fn k_boundary_tie_membership_and_order() {
    let entry = support::query("term_tag_tie");
    let golden = entry.expected.as_ref().expect("minted");
    assert_eq!(
        golden.len(),
        entry.k,
        "the tie group must be larger than k for this test to mean anything"
    );
    assert!(
        golden.windows(2).all(|w| w[0].score == w[1].score),
        "planted tie is not a tie"
    );
    assert!(
        golden.windows(2).all(|w| w[0].id < w[1].id),
        "tied golden must be in ascending DocId"
    );
    for batches in [1usize, 4, 7] {
        let t = build(batches);
        let hits = t.index.search(&entry.query, None, entry.k).expect("search");
        assert_eq!(hits.len(), entry.k);
        // the whole tie group, then its k lowest ids — that is what must come back
        let group = t.index.search(&entry.query, None, 1000).expect("search");
        assert!(group.len() > entry.k && group.windows(2).all(|w| w[0].score == w[1].score));
        let lowest: Vec<u32> = ids(&group).into_iter().take(entry.k).collect();
        assert_eq!(
            ids(&hits),
            lowest,
            "k-boundary membership at {batches} batches is not DocId-minimal"
        );
        assert_eq!(
            ids(&hits),
            ids(golden),
            "differs from the golden at {batches} batches"
        );
    }
}
