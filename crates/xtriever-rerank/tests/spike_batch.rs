//! Feature 028 (the accelerated inference spike), research D7: with `spike-batch`, `rerank`
//! scores every pair of a call in one forward pass. Padding changes the arithmetic, so the
//! batched scores are compared with one-pair-at-a-time `score` on the same pairs — within the
//! device tolerance the harnesses already use (1e-3), never assumed equal. The largest
//! difference is printed for the report. Spike code.
#![cfg(feature = "spike-batch")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use xtriever_core::{Budget, DocId, Passage, Reranker};
use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

const TOLERANCE: f32 = 1e-3;

#[test]
#[ignore = "needs the model"]
fn batched_scores_match_one_pair_at_a_time() {
    let reranker = MiniLmCrossEncoder::load(&support::model_dir_q8(), LoadPath::Buffered)
        .expect("load the pinned eight-bit re-ranker");
    let mut worst = 0.0f32;
    for q in &support::goldens().queries {
        let passages: Vec<Passage<'_>> = q
            .passages
            .iter()
            .enumerate()
            .map(|(i, p)| Passage {
                id: DocId(i as u32),
                text: &p.text,
            })
            .collect();
        let batched = reranker
            .rerank(&q.query, &passages, &Budget::default())
            .unwrap_or_else(|e| panic!("{}: batched rerank: {e}", q.name));
        assert_eq!(
            batched.len(),
            passages.len(),
            "{}: one score per pair",
            q.name
        );
        for (i, (got, p)) in batched.iter().zip(&q.passages).enumerate() {
            let got = got.unwrap_or_else(|| panic!("{} passage {i}: no score", q.name));
            let single = reranker.score(&q.query, &p.text).unwrap();
            let diff = (got - single).abs();
            worst = worst.max(diff);
            assert!(
                diff <= TOLERANCE,
                "{} passage {i}: batched {got} vs single {single} (|Δ| = {diff})",
                q.name
            );
        }
    }
    eprintln!("batched vs single, worst |Δ| over the golden pairs: {worst:.3e}");
}

/// Review of the spike: the batch honours a time budget as the one-pair loop does — a pass that
/// ends past `max_time` is a spent budget, and the stage returns no score (all or none), so the
/// pipeline degrades to the fused order instead of reporting pairs it scored out of time.
#[test]
#[ignore = "needs the model"]
fn a_batch_that_overruns_its_budget_scores_nothing() {
    let reranker = MiniLmCrossEncoder::load(&support::model_dir_q8(), LoadPath::Buffered)
        .expect("load the pinned eight-bit re-ranker");
    let q = &support::goldens().queries[0];
    let passages: Vec<Passage<'_>> = q
        .passages
        .iter()
        .enumerate()
        .map(|(i, p)| Passage {
            id: DocId(i as u32),
            text: &p.text,
        })
        .collect();
    // One nanosecond: never zero, never enough for a forward pass.
    let budget = Budget {
        max_time: Some(Duration::from_nanos(1)),
        max_items: None,
    };
    let scores = reranker.rerank(&q.query, &passages, &budget).unwrap();
    assert_eq!(scores.len(), passages.len());
    assert!(scores.iter().all(Option::is_none), "{scores:?}");
}
