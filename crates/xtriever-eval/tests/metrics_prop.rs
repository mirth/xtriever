//! Metric invariants, property-tested (FR-004).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use proptest::prelude::*;
use xtriever_eval::dataset::Qrels;
use xtriever_eval::metrics::{ndcg_at, recall_at, score_queries};

fn grades_strategy() -> impl Strategy<Value = BTreeMap<String, u32>> {
    prop::collection::btree_map("d[0-9]{1,2}", 0u32..3, 1..8)
}

fn ranked_strategy() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("d[0-9]{1,2}", 0..30)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 500, .. ProptestConfig::default() })]

    // permuting ids strictly below the cutoff cannot change nDCG@k
    #[test]
    fn ndcg_ignores_order_below_the_cutoff(grades in grades_strategy(), ranked in ranked_strategy(), k in 1usize..12) {
        let refs: Vec<&str> = ranked.iter().map(String::as_str).collect();
        let base = ndcg_at(&refs, &grades, k);
        // distinct-id positions ≥ k are below the cutoff; reverse that tail
        let mut seen = std::collections::BTreeSet::new();
        let mut head = Vec::new();
        let mut tail = Vec::new();
        for id in &refs {
            if seen.insert(*id) && head.len() < k { head.push(*id); } else { tail.push(*id); }
        }
        tail.reverse();
        let permuted: Vec<&str> = head.iter().chain(tail.iter()).copied().collect();
        prop_assert_eq!(base.to_bits(), ndcg_at(&permuted, &grades, k).to_bits());
    }

    // recall@k is monotone non-decreasing in k
    #[test]
    fn recall_is_monotone_in_k(grades in grades_strategy(), ranked in ranked_strategy()) {
        let refs: Vec<&str> = ranked.iter().map(String::as_str).collect();
        let mut last = 0.0;
        for k in 1..=40 {
            let r = recall_at(&refs, &grades, k);
            prop_assert!(r >= last, "k={k}: {r} < {last}");
            prop_assert!((0.0..=1.0).contains(&r));
            last = r;
        }
    }

    // the mean does not depend on the order queries are supplied in, and lies in [0, 1]
    #[test]
    fn mean_is_order_independent(
        qrels in prop::collection::btree_map("q[0-9]{1,2}", grades_strategy(), 1..10),
        runs in prop::collection::vec(ranked_strategy(), 10),
    ) {
        let q = Qrels { grades: qrels.clone() };
        let ids: Vec<&String> = qrels.keys().collect();
        let run: BTreeMap<String, Vec<String>> = ids.iter().zip(runs.iter()).map(|(k, v)| ((*k).clone(), v.clone())).collect();
        let a = score_queries(&run, &q);
        // BTreeMap iteration is already sorted, so build the same map through a different insertion order
        let mut rev: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in run.iter().rev() { rev.insert(k.clone(), v.clone()); }
        let b = score_queries(&rev, &q);
        prop_assert_eq!(a.mean_ndcg_10.to_bits(), b.mean_ndcg_10.to_bits());
        prop_assert!((0.0..=1.0).contains(&a.mean_ndcg_10) && (0.0..=1.0).contains(&a.mean_recall_100));
        prop_assert_eq!(a.scored_queries as usize, run.len());
    }
}
