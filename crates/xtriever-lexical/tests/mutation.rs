//! User Story 3 — documents can be replaced and removed (FR-008–FR-011).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{Mutations, TestIndex, fixture_index, ids};
use xtriever_core::{DocId, LexicalIndex, LexicalQuery};

fn mutations() -> Mutations {
    support::load_json("mutations.json")
}

// Scenario 1: replace-by-id
#[test]
fn add_with_same_id_replaces_after_commit() {
    let m = mutations();
    let mut t = fixture_index();
    let phrase = support::query(&m.replace.absent_from);
    assert!(ids(&t.index.search(&phrase.query, None, phrase.k).unwrap()).contains(&m.replace.id));
    t.index
        .add(std::slice::from_ref(&m.replace.new_document))
        .expect("add");
    t.index.commit().expect("commit");
    assert!(
        !ids(&t.index.search(&phrase.query, None, phrase.k).unwrap()).contains(&m.replace.id),
        "old content survived"
    );
    let hits = t
        .index
        .search(&m.replace.present_in.query, None, 10)
        .expect("search");
    assert_eq!(
        ids(&hits),
        m.replace.present_in.expected_ids,
        "new content must appear exactly once"
    );
    assert_eq!(
        t.index.stats().expect("stats").num_docs,
        1000,
        "replace must not change the live count"
    );
}

// Scenarios 2 & 4: delete removes from results and from num_docs
#[test]
fn delete_removes_documents_and_counts_live_only() {
    let m = mutations();
    let mut t = fixture_index();
    let phrase = support::query("phrase_exact");
    let before = t
        .index
        .search(&phrase.query, None, phrase.k)
        .expect("search");
    let deleted: Vec<DocId> = m.delete.ids.iter().map(|&i| DocId(i)).collect();
    t.index.delete(&deleted).expect("delete");
    t.index.commit().expect("commit");
    let after = t
        .index
        .search(&phrase.query, None, phrase.k)
        .expect("search");
    let expected: Vec<u32> = ids(&before)
        .into_iter()
        .filter(|i| !m.delete.ids.contains(i))
        .collect();
    assert_eq!(
        ids(&after)[..expected.len()],
        expected[..],
        "deleted docs must vanish, order preserved"
    );
    assert!(after.iter().all(|h| !m.delete.ids.contains(&h.id.0)));
    assert_eq!(
        t.index.stats().expect("stats").num_docs,
        m.delete.expected_num_docs
    );
}

// Scenario 3: unknown ids are ignored
#[test]
fn deleting_unknown_ids_is_ok_and_changes_nothing() {
    let mut t = fixture_index();
    let before = t.index.stats().expect("stats");
    t.index
        .delete(&[DocId(4_000_000), DocId(999_999)])
        .expect("delete unknown");
    t.index.commit().expect("commit");
    assert_eq!(t.index.stats().expect("stats"), before);
}

// Scenario 5 (FR-010): uncommitted mutations are invisible
#[test]
fn uncommitted_mutations_are_invisible() {
    let m = mutations();
    let mut t = fixture_index();
    let phrase = support::query("phrase_exact");
    let before = t
        .index
        .search(&phrase.query, None, phrase.k)
        .expect("search");
    let stats_before = t.index.stats().expect("stats");
    let set_before = t
        .index
        .resolve_filter(&xtriever_core::Filter::Exists("body".into()))
        .expect("resolve");
    let ts_before = t
        .index
        .term_stats(&"body".into(), "quantum")
        .expect("term_stats");

    t.index.delete(&[DocId(m.delete.ids[0])]).expect("delete");
    t.index
        .add(std::slice::from_ref(&m.replace.new_document))
        .expect("add");

    support::assert_hits_exact(
        "search before commit",
        &t.index.search(&phrase.query, None, phrase.k).unwrap(),
        &before,
    );
    assert_eq!(t.index.stats().unwrap(), stats_before);
    assert_eq!(
        t.index
            .resolve_filter(&xtriever_core::Filter::Exists("body".into()))
            .unwrap(),
        set_before
    );
    assert_eq!(
        t.index.term_stats(&"body".into(), "quantum").unwrap(),
        ts_before
    );
}

// FR-011: drop without commit, reopen → committed state only
#[test]
fn reopen_exposes_only_committed_state() {
    let m = mutations();
    let t = fixture_index();
    let phrase = support::query("phrase_exact");
    let committed = t
        .index
        .search(&phrase.query, None, phrase.k)
        .expect("search");
    let TestIndex { dir, mut index } = t;
    index.delete(&[DocId(m.delete.ids[0])]).expect("delete");
    index
        .add(std::slice::from_ref(&m.replace.new_document))
        .expect("add");
    drop(index); // no commit
    let reopened = xtriever_lexical::TantivyIndex::open(&dir.path().join("idx")).expect("open");
    support::assert_hits_exact(
        "after reopen",
        &reopened.search(&phrase.query, None, phrase.k).unwrap(),
        &committed,
    );
    assert_eq!(reopened.stats().unwrap().num_docs, 1000);
    let probe = LexicalQuery::Match(Some("title".into()), "replaced".into());
    assert!(reopened.search(&probe, None, 10).unwrap().is_empty());
}
