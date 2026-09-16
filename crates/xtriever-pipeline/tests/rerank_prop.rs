//! Invariants of the ordering rule (research D8): the scored prefix is sorted, the suffix is
//! the fused order minus the scored ids, nothing appears or disappears. Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use xtriever_core::DocId;
use xtriever_pipeline::{order_interpolated, order_reranked};

fn fused_list() -> impl Strategy<Value = Vec<(DocId, f64)>> {
    proptest::collection::btree_set(0u32..50, 0..20).prop_map(|ids| {
        let mut v: Vec<u32> = ids.into_iter().collect();
        // A deterministic shuffle so the fused order is not ascending by id.
        v.sort_by_key(|id| (id.wrapping_mul(2_654_435_761)) % 97);
        v.into_iter()
            .enumerate()
            .map(|(i, id)| (DocId(id), 1.0 - i as f64 * 0.001))
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn ordering_invariants(
        fused in fused_list(),
        raw_scores in proptest::collection::vec(proptest::option::of(-3i32..3), 0..20),
        k in 0usize..25,
    ) {
        let d = raw_scores.len().min(fused.len());
        let scores: Vec<Option<f32>> = raw_scores.iter().take(d).map(|s| s.map(|v| v as f32 * 0.5)).collect();
        let out = order_reranked(&fused, &scores, k);

        prop_assert_eq!(out.len(), k.min(fused.len()));
        let ids: Vec<u32> = out.iter().map(|(id, _, _)| id.0).collect();
        let mut dedup = ids.clone();
        dedup.sort_unstable();
        dedup.dedup();
        prop_assert_eq!(dedup.len(), ids.len(), "no duplicates");
        prop_assert!(ids.iter().all(|id| fused.iter().any(|(f, _)| f.0 == *id)), "subset of fused");

        // Scored prefix, sorted (score DESC, id ASC); then unscored in fused order.
        let scored_len = out.iter().take_while(|(_, _, s)| s.is_some()).count();
        prop_assert!(out[scored_len..].iter().all(|(_, _, s)| s.is_none()), "scored form a prefix");
        for w in out[..scored_len].windows(2) {
            let (a, b) = (w[0].2.unwrap(), w[1].2.unwrap());
            prop_assert!(a > b || (a == b && w[0].0 < w[1].0), "prefix sorted");
        }
        let scored_ids: Vec<u32> = out[..scored_len].iter().map(|(id, _, _)| id.0).collect();
        let expected_suffix: Vec<u32> = fused
            .iter()
            .map(|(id, _)| id.0)
            .filter(|id| !scored_ids.contains(id))
            .collect();
        let suffix: Vec<u32> = out[scored_len..].iter().map(|(id, _, _)| id.0).collect();
        prop_assert_eq!(&suffix[..], &expected_suffix[..suffix.len()], "suffix in fused order");

        // Every scored candidate that exists is in the prefix (when k allows): the prefix is
        // exactly the first min(k, #scored) of the sorted scored set.
        let total_scored = scores.iter().filter(|s| s.is_some()).count();
        prop_assert_eq!(scored_len, total_scored.min(k));

        if scores.iter().all(Option::is_none) {
            let plain: Vec<u32> = fused.iter().take(k).map(|(id, _)| id.0).collect();
            prop_assert_eq!(ids, plain, "all None ⇒ fused order");
        }
    }
}

// Feature 015: the interpolating rule's invariants.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn interpolation_invariants(
        fused in fused_list(),
        raw_scores in proptest::collection::vec(proptest::option::of(-3i32..3), 0..20),
        k in 0usize..25,
        alpha_pct in 0u32..=100,
    ) {
        let alpha = f64::from(alpha_pct) / 100.0;
        let d = raw_scores.len().min(fused.len());
        let scores: Vec<Option<f32>> = raw_scores.iter().take(d).map(|s| s.map(|v| v as f32 * 0.5)).collect();
        let out = order_interpolated(&fused, &scores, k, alpha);

        prop_assert_eq!(out.len(), k.min(fused.len()));
        let ids: Vec<u32> = out.iter().map(|(id, _, _, _)| id.0).collect();
        let mut dedup = ids.clone();
        dedup.sort_unstable();
        dedup.dedup();
        prop_assert_eq!(dedup.len(), ids.len(), "no duplicates");

        let head_len = out.iter().take_while(|(_, _, s, _)| s.is_some()).count();
        prop_assert!(out[head_len..].iter().all(|(_, _, s, c)| s.is_none() && c.is_none()), "head is a prefix");
        prop_assert!(out[..head_len].iter().all(|(_, _, _, c)| c.is_some_and(|c| (0.0..=1.0).contains(&c))), "combined in [0, 1]");
        for w in out[..head_len].windows(2) {
            prop_assert!(w[0].3.unwrap() >= w[1].3.unwrap(), "head sorted by combined score");
        }
        let head_ids: Vec<u32> = out[..head_len].iter().map(|(id, _, _, _)| id.0).collect();
        let expected_suffix: Vec<u32> = fused.iter().map(|(id, _)| id.0).filter(|id| !head_ids.contains(id)).collect();
        let suffix: Vec<u32> = out[head_len..].iter().map(|(id, _, _, _)| id.0).collect();
        prop_assert_eq!(&suffix[..], &expected_suffix[..suffix.len()], "suffix in fused order");

        // alpha 0: the head keeps its fused order (fused scores are strictly decreasing in fused_list).
        if alpha == 0.0 {
            let fused_order: Vec<u32> = fused.iter().zip(&scores).filter(|(_, s)| s.is_some()).map(|((id, _), _)| id.0).collect();
            prop_assert_eq!(&head_ids[..], &fused_order[..head_ids.len()], "alpha 0 keeps the fused order");
        }
        // alpha 1: the head is the replace order except that ties fall back to the fused order.
        if alpha == 1.0 {
            let replace = order_reranked(&fused, &scores, k);
            for (a, b) in out[..head_len].iter().zip(&replace) {
                prop_assert_eq!(a.2, b.2, "alpha 1 follows the cross-encoder scores");
            }
        }
    }
}

#[test]
fn a_constant_cross_encoder_column_keeps_the_fused_order() {
    let fused: Vec<(DocId, f64)> = (0..6)
        .map(|i| (DocId(10 - i), 0.05 - f64::from(i) * 0.005))
        .collect();
    let scores = vec![Some(2.0f32); 4];
    let out = order_interpolated(&fused, &scores, 6, 0.75);
    let ids: Vec<u32> = out.iter().map(|(id, _, _, _)| id.0).collect();
    assert_eq!(ids, vec![10, 9, 8, 7, 6, 5]);
    // The cross-encoder term is zero for every head member; only the fused term remains,
    // which is what keeps the fused order: (1 - 0.75) * [1, 2/3, 1/3, 0].
    let combined: Vec<f64> = out[..4].iter().map(|(_, _, _, c)| c.unwrap()).collect();
    for (c, f) in combined.iter().zip([1.0, 2.0 / 3.0, 1.0 / 3.0, 0.0]) {
        assert!((c - 0.25 * f).abs() < 1e-12, "{combined:?}");
    }
}
