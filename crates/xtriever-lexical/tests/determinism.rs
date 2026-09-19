//! User Story 2 — ties, segments and repeated runs behave identically (ADR-0005, FR-013–FR-015).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{TestIndex, corpus, ids, index_in_batches, queries};
use xtriever_core::{DocId, LexicalIndex};

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

/// Child half of the cross-process check: build a fresh 4-batch index and print the k-boundary
/// hits as JSON. Runs only when the parent sets `XT_BOUNDARY_CHILD`.
#[test]
fn boundary_child() {
    if std::env::var_os("XT_BOUNDARY_CHILD").is_none() {
        return;
    }
    let entry = support::query("term_tag_tie");
    let t = build(4);
    let hits = t.index.search(&entry.query, None, entry.k).expect("search");
    let all = t.index.search(&entry.query, None, 1000).expect("search");
    println!(
        "XT_HITS {}",
        serde_json::to_string(&(hits, all.len())).unwrap()
    );
}

// FR-014 / FR-015 across processes. Segment ordinals for equal-size segments come from a HashMap
// whose seed differs per process, so an in-process loop cannot exercise the failure that motivated
// F-001. Each child builds the same corpus in its own process; every child must return the same
// k-boundary membership, and it must be the DocId-minimal one.
#[test]
fn k_boundary_membership_is_identical_across_processes() {
    let exe = std::env::current_exe().expect("test binary path");
    let mut outputs = Vec::new();
    for _ in 0..3 {
        let out = std::process::Command::new(&exe)
            .args(["--exact", "boundary_child", "--nocapture"])
            .env("XT_BOUNDARY_CHILD", "1")
            .output()
            .expect("spawn child");
        assert!(
            out.status.success(),
            "child failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let line = stdout
            .lines()
            .find(|l| l.starts_with("XT_HITS "))
            .expect("child printed hits");
        let (hits, group_len): (Vec<xtriever_core::Hit>, usize) =
            serde_json::from_str(&line["XT_HITS ".len()..]).unwrap();
        assert!(group_len > hits.len(), "the tie group must exceed k");
        outputs.push(ids(&hits));
    }
    let golden = ids(support::query("term_tag_tie").expected.as_ref().unwrap());
    for (i, o) in outputs.iter().enumerate() {
        assert_eq!(
            o, &golden,
            "process {i} returned different k-boundary membership"
        );
    }
}

// Feature 024 (ADR-0013): the backend's BM25 statistics are deletion-inclusive until a merge
// physically drops the deleted documents (`stats.rs`; Feature 002 FR-025), so a merge that
// does so moves BM25 score bits — and a merge on a single segment, which has nothing to
// merge, moves none. This pins the boundary the pipeline's merge doc and spec FR-005 rely on.
#[test]
fn merge_after_deletes_moves_bm25_bits_only_when_it_drops_documents() {
    let q = support::query("match_body").query;
    let bits = |t: &support::TestIndex| -> Vec<(u32, u32)> {
        t.index
            .search(&q, None, 10)
            .expect("search")
            .iter()
            .map(|h| (h.id.0, h.score.to_bits()))
            .collect()
    };
    let mut moved = Vec::new();
    for batches in [1usize, 3] {
        let mut t = support::TestIndex::create_fixture();
        support::index_in_batches(&mut t.index, &support::corpus().documents, batches);
        let victims: Vec<DocId> = support::ids(&t.index.search(&q, None, 10).expect("search"))
            [1..4]
            .iter()
            .map(|&i| DocId(i))
            .collect();
        t.index.delete(&victims).expect("delete");
        t.index.commit().expect("commit");
        let after_delete = bits(&t);
        t.index.merge().expect("merge");
        let after_merge = bits(&t);
        assert_eq!(
            support::ids_of(&after_delete),
            support::ids_of(&after_merge),
            "{batches} batches: the same documents rank in the top 10"
        );
        moved.push(after_delete != after_merge);
    }
    assert_eq!(
        moved,
        vec![false, true],
        "one segment: nothing to merge, bits unchanged; three segments: the merge drops the \
         deleted documents and the BM25 bits move"
    );
}
