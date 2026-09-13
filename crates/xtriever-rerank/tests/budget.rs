//! US2 scenarios 1, 3, 4 — the budget loop over a stub scorer (spec FR-007, SC-003 item part).
//! Offline: no model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use xtriever_core::{Budget, DocId, Error, Passage, Result};
use xtriever_rerank::rerank_with;

fn stub(_q: &str, p: &str) -> Result<f32> {
    Ok(p.len() as f32)
}

fn passages(n: usize) -> Vec<String> {
    (0..n).map(|i| "x".repeat(i + 1)).collect()
}

fn as_passages(texts: &[String]) -> Vec<Passage<'_>> {
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| Passage {
            id: DocId(i as u32),
            text: t,
        })
        .collect()
}

fn run(texts: &[String], budget: Budget) -> Vec<Option<f32>> {
    rerank_with(&stub, "q", &as_passages(texts), &budget).unwrap()
}

#[test]
fn item_limit_scores_exactly_the_first_n_in_input_order() {
    let texts = passages(10);
    for (limit, expect) in [(0, 0), (1, 1), (5, 5), (10, 10), (20, 10)] {
        let out = run(
            &texts,
            Budget {
                max_items: Some(limit),
                max_time: None,
            },
        );
        assert_eq!(out.len(), 10, "limit {limit}: output length");
        let scored = out.iter().filter(|s| s.is_some()).count();
        assert_eq!(scored, expect, "limit {limit}");
        for (i, s) in out.iter().enumerate() {
            match s {
                Some(v) if i < expect => assert_eq!(*v, (i + 1) as f32),
                None if i >= expect => {}
                other => panic!("limit {limit}: position {i} is {other:?}"),
            }
        }
    }
}

#[test]
fn zero_time_limit_scores_nothing() {
    let out = run(
        &passages(5),
        Budget {
            max_time: Some(Duration::ZERO),
            max_items: None,
        },
    );
    assert_eq!(out, vec![None; 5]);
}

#[test]
fn no_budget_scores_everything() {
    let out = run(&passages(7), Budget::default());
    assert_eq!(out.len(), 7);
    assert!(out.iter().all(Option::is_some));
}

#[test]
fn the_first_scorer_error_aborts_the_call() {
    let texts = passages(5);
    let failing = |_q: &str, p: &str| -> Result<f32> {
        if p.len() == 3 {
            Err(Error::Model {
                model: "stub".into(),
                message: "boom".into(),
            })
        } else {
            Ok(1.0)
        }
    };
    let err = rerank_with(&failing, "q", &as_passages(&texts), &Budget::default()).unwrap_err();
    assert!(matches!(err, Error::Model { .. }), "{err}");
}

#[test]
fn empty_input_yields_empty_output() {
    assert!(run(&[], Budget::default()).is_empty());
}
