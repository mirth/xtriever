//! US2 scenario 2 — a time limit the scoring cannot finish within scores at least one and
//! fewer than all passages (spec FR-007, SC-003 time part). Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use xtriever_core::{Budget, DocId, Passage, Reranker};
use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

fn long_passages(n: usize) -> Vec<String> {
    let sentence = "The retrieval engine indexes every passage of the corpus and scores each one against the query. ";
    (0..n)
        .map(|i| format!("{i} {}", sentence.repeat(45)))
        .collect()
}

#[test]
#[ignore = "needs the model"]
fn an_unmeetable_time_limit_scores_a_proper_prefix() {
    let r = MiniLmCrossEncoder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    let texts = long_passages(40);
    let passages: Vec<Passage<'_>> = texts
        .iter()
        .enumerate()
        .map(|(i, t)| Passage {
            id: DocId(i as u32),
            text: t,
        })
        .collect();
    let out = r
        .rerank(
            "what does the engine do",
            &passages,
            &Budget {
                max_time: Some(Duration::from_millis(100)),
                max_items: None,
            },
        )
        .unwrap();
    assert_eq!(out.len(), 40);
    let scored = out.iter().take_while(|s| s.is_some()).count();
    assert!(
        out[scored..].iter().all(Option::is_none),
        "the Somes form a prefix"
    );
    assert!(scored >= 1, "the first pair fits the limit");
    assert!(scored < 40, "40 max-length pairs cannot fit 100 ms");
    eprintln!("scored {scored} of 40 within 100 ms");
}

#[test]
#[ignore = "needs the model"]
fn an_item_limit_holds_with_the_model() {
    let r = MiniLmCrossEncoder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    let texts = long_passages(5);
    let passages: Vec<Passage<'_>> = texts
        .iter()
        .enumerate()
        .map(|(i, t)| Passage {
            id: DocId(i as u32),
            text: t,
        })
        .collect();
    let out = r
        .rerank(
            "q",
            &passages,
            &Budget {
                max_items: Some(3),
                max_time: None,
            },
        )
        .unwrap();
    assert_eq!(out.iter().filter(|s| s.is_some()).count(), 3);
    assert!(out[3..].iter().all(Option::is_none));
}
