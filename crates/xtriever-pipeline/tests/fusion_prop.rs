//! Fusion invariants (Principle II).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use proptest::prelude::*;
use xtriever_core::{DocId, Hit};
use xtriever_pipeline::rrf;

fn hits(ids: &[u32]) -> Vec<Hit> {
    ids.iter()
        .map(|&id| Hit {
            id: DocId(id),
            score: 1.0,
        })
        .collect()
}

fn distinct(ids: Vec<u32>) -> Vec<u32> {
    let mut seen = BTreeSet::new();
    ids.into_iter().filter(|i| seen.insert(*i)).collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 500, ..ProptestConfig::default() })]

    #[test]
    fn fusion_invariants(
        lex in prop::collection::vec(0u32..40, 0..20),
        den in prop::collection::vec(0u32..40, 0..20),
        k in 0usize..30,
        rrf_k in 1u32..100,
    ) {
        let (lex, den) = (distinct(lex), distinct(den));
        let fused = rrf(&hits(&lex), &hits(&den), rrf_k, k);
        let union: BTreeSet<u32> = lex.iter().chain(&den).copied().collect();
        prop_assert_eq!(fused.len(), k.min(union.len()));
        prop_assert!(fused.iter().all(|(id, _)| union.contains(&id.0)));
        prop_assert!(fused.windows(2).all(|w| w[0].1 > w[1].1 || (w[0].1 == w[1].1 && w[0].0 < w[1].0)));
        // A document in both lists scores at least what it would in either list alone.
        for (id, score) in &fused {
            let lex_only = rrf(&hits(&lex), &[], rrf_k, usize::MAX).into_iter().find(|(i, _)| i == id).map(|(_, s)| s).unwrap_or(0.0);
            let den_only = rrf(&[], &hits(&den), rrf_k, usize::MAX).into_iter().find(|(i, _)| i == id).map(|(_, s)| s).unwrap_or(0.0);
            prop_assert!(*score >= lex_only.max(den_only) - 1e-12);
        }
        // Swapping the two lists leaves every score unchanged (the rule is symmetric).
        let swapped = rrf(&hits(&den), &hits(&lex), rrf_k, k);
        prop_assert_eq!(fused.len(), swapped.len());
        for ((a, sa), (b, sb)) in fused.iter().zip(&swapped) {
            prop_assert_eq!(a, b);
            prop_assert!((sa - sb).abs() <= 1e-12);
        }
    }
}
