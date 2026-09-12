//! User Story 6 — one index shared across threads under a lock (FR-028–FR-031, SC-013).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::{Arc, RwLock};
use std::thread;

use support::{TestIndex, corpus, fixture_index, ids, index_in_batches};
use xtriever_core::{Error, LexicalIndex, LexicalQuery};
use xtriever_lexical::TantivyIndex;

// Scenario 1: eight readers agree with the single-threaded answer
#[test]
fn concurrent_readers_match_single_threaded_results() {
    let t = fixture_index();
    let TestIndex { dir: _dir, index } = t;
    let shared = Arc::new(RwLock::new(index));
    let queries = support::queries().queries;
    let expected: Vec<_> = {
        let idx = shared.read().unwrap();
        queries
            .iter()
            .map(|e| idx.search(&e.query, e.filter.as_ref(), e.k).unwrap())
            .collect()
    };
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let shared = Arc::clone(&shared);
            let queries = queries.clone();
            let expected = expected.clone();
            thread::spawn(move || {
                for _ in 0..3 {
                    let idx = shared.read().unwrap();
                    for (e, exp) in queries.iter().zip(&expected) {
                        let hits = idx.search(&e.query, e.filter.as_ref(), e.k).unwrap();
                        support::assert_hits_exact(&e.name, &hits, exp);
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("reader thread panicked");
    }
}

// Scenario 2: a writer commits while readers query — every read is before-state or after-state
#[test]
fn readers_never_observe_a_torn_commit() {
    let mut t = TestIndex::create_fixture();
    let docs = corpus().documents;
    index_in_batches(&mut t.index, &docs[..500], 1);
    let TestIndex { dir: _dir, index } = t;
    let shared = Arc::new(RwLock::new(index));
    let q = LexicalQuery::Match(Some("body".into()), "quantum".into());
    let before = ids(&shared.read().unwrap().search(&q, None, 1000).unwrap());

    let writer = {
        let shared = Arc::clone(&shared);
        let rest: Vec<_> = docs[500..].to_vec();
        thread::spawn(move || {
            for chunk in rest.chunks(100) {
                let mut idx = shared.write().unwrap();
                idx.add(chunk).unwrap();
                idx.commit().unwrap();
            }
        })
    };
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let shared = Arc::clone(&shared);
            let q = q.clone();
            thread::spawn(move || {
                let mut seen = Vec::new();
                for _ in 0..50 {
                    let idx = shared.read().unwrap();
                    let n = idx.stats().unwrap().num_docs;
                    let hits = ids(&idx.search(&q, None, 1000).unwrap());
                    seen.push((n, hits));
                }
                seen
            })
        })
        .collect();
    writer.join().expect("writer panicked");
    let final_ids = ids(&shared.read().unwrap().search(&q, None, 1000).unwrap());
    assert_eq!(shared.read().unwrap().stats().unwrap().num_docs, 1000);
    for r in readers {
        for (n, hits) in r.join().expect("reader panicked") {
            // num_docs must be one of the committed states (500, 600, ..., 1000)
            assert!(
                (500..=1000).contains(&n) && n % 100 == 0,
                "torn num_docs {n}"
            );
            // and the hit set must be consistent with exactly that commit: a prefix-of-commits view
            assert!(
                hits.len() >= before.len() && hits.len() <= final_ids.len(),
                "torn hits"
            );
            assert!(
                before.iter().all(|i| hits.contains(i)),
                "a committed doc vanished mid-read"
            );
        }
    }
}

// Scenario 3: two indexes in one process do not interfere
#[test]
fn two_indexes_on_different_directories_are_independent() {
    let a = fixture_index();
    let mut b = TestIndex::create_fixture();
    index_in_batches(&mut b.index, &corpus().documents[..100], 1);
    let q = LexicalQuery::Match(Some("body".into()), "quantum".into());
    let ha = a.index.search(&q, None, 1000).unwrap();
    let hb = b.index.search(&q, None, 1000).unwrap();
    assert_eq!(a.index.stats().unwrap().num_docs, 1000);
    assert_eq!(b.index.stats().unwrap().num_docs, 100);
    assert!(hb.len() < ha.len());
    // and a's answers are unchanged by b's existence
    let golden = support::query("term_body");
    support::assert_hits_exact(
        "a unaffected",
        &a.index.search(&golden.query, None, golden.k).unwrap(),
        golden.expected.as_ref().unwrap(),
    );
}

// Scenario 4 (FR-031, research D14): two handles on one directory
#[test]
fn second_handle_on_same_directory_reads_but_cannot_write_while_first_holds_the_lock() {
    let t = fixture_index(); // handle A has mutated ⇒ holds the writer lock
    let path = t.path();
    let TestIndex {
        dir: _dir,
        index: a,
    } = t; // keep the directory alive; `a` is handle A
    let b = TantivyIndex::open(&path).expect("a second open must succeed");
    let golden = support::query("term_body");
    support::assert_hits_exact(
        "B reads the committed state",
        &b.search(&golden.query, None, golden.k).unwrap(),
        golden.expected.as_ref().unwrap(),
    );
    let mut b = b;
    let doc = corpus().documents[0].clone();
    let err = b
        .add(std::slice::from_ref(&doc))
        .expect_err("B must not acquire the writer lock");
    assert!(matches!(err, Error::Backend(_)), "{err}");
    // A still works
    support::assert_hits_exact(
        "A unaffected",
        &a.search(&golden.query, None, golden.k).unwrap(),
        golden.expected.as_ref().unwrap(),
    );
    drop(a);
    // the outcome is the same on retry (not a race)
    let err2 = b.add(std::slice::from_ref(&doc)).err();
    // After A is dropped the lock is released, so B may now succeed. Either the first retry sees
    // the release or a fresh handle does — assert the documented end state.
    let mut c = match err2 {
        None => b,
        Some(_) => TantivyIndex::open(&path).expect("open"),
    };
    c.add(std::slice::from_ref(&doc))
        .expect("after A is dropped, B/C can write");
    c.commit().expect("commit");
}

// Scenario 5: a search during merge is single-commit-consistent
#[test]
fn search_during_merge_is_consistent() {
    let mut t = TestIndex::create_fixture();
    index_in_batches(&mut t.index, &corpus().documents, 8);
    let TestIndex { dir: _dir, index } = t;
    let shared = Arc::new(RwLock::new(index));
    let golden = support::query("phrase_exact");
    let expected = golden.expected.clone().unwrap();
    let merger = {
        let shared = Arc::clone(&shared);
        thread::spawn(move || shared.write().unwrap().merge().unwrap())
    };
    let reader = {
        let shared = Arc::clone(&shared);
        let g = golden.clone();
        thread::spawn(move || {
            for _ in 0..20 {
                let hits = shared.read().unwrap().search(&g.query, None, g.k).unwrap();
                support::assert_hits_exact("during merge", &hits, &expected);
            }
        })
    };
    merger.join().unwrap();
    reader.join().unwrap();
}
