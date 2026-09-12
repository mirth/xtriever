//! Property tests for the index invariants (Principle II): results ⊆ allowed, total ordering,
//! `len` round-trips through add/delete/commit, and (under `mmap`) owned ≡ mapped. Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use proptest::prelude::*;
use xtriever_core::{DocId, Metric, VectorIndex};
use xtriever_dense::FlatIndex;

fn finite_vec(dim: usize) -> impl Strategy<Value = Vec<f32>> {
    // Avoid zero vectors so Cosine is defined; magnitudes are ordinary.
    prop::collection::vec(-5.0f32..5.0, dim)
        .prop_filter("non-zero", |v| v.iter().any(|x| x.abs() > 1e-3))
}

fn metric() -> impl Strategy<Value = Metric> {
    prop_oneof![
        Just(Metric::Cosine),
        Just(Metric::Dot),
        Just(Metric::Euclidean)
    ]
}

fn ordered(hits: &[xtriever_core::Hit]) -> bool {
    hits.windows(2)
        .all(|w| w[0].score > w[1].score || (w[0].score == w[1].score && w[0].id < w[1].id))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    #[test]
    fn search_invariants(
        dim in 4usize..16,
        metric in metric(),
        rows in prop::collection::btree_map(0u32..64, prop::collection::vec(-5.0f32..5.0, 16), 0..64),
        query in prop::collection::vec(-5.0f32..5.0, 16),
        allowed in prop::collection::btree_set(0u32..80, 0..40),
        k in 0usize..70,
    ) {
        // Trim to `dim` and drop zero rows/queries (undefined under Cosine).
        let rows: BTreeMap<u32, Vec<f32>> = rows.into_iter()
            .map(|(id, v)| (id, v[..dim].to_vec()))
            .filter(|(_, v)| v.iter().any(|x| x.abs() > 1e-3))
            .collect();
        let query = query[..dim].to_vec();
        prop_assume!(query.iter().any(|x| x.abs() > 1e-3));

        let tmp = tempfile::tempdir().unwrap();
        let mut index = FlatIndex::create(tmp.path(), dim, metric, "prop").unwrap();
        for (id, v) in &rows {
            index.add(DocId(*id), v).unwrap();
        }
        index.commit().unwrap();
        prop_assert_eq!(index.len(), rows.len() as u64);

        let allowed_ids: Vec<u32> = allowed.iter().copied().collect();
        let set = support::doc_set(&allowed_ids);
        let hits = index.search(&query, Some(&set), k).unwrap();
        let live_allowed = rows.keys().filter(|id| allowed.contains(id)).count();
        prop_assert_eq!(hits.len(), k.min(live_allowed));
        prop_assert!(hits.iter().all(|h| allowed.contains(&h.id.0) && rows.contains_key(&h.id.0)));
        prop_assert!(ordered(&hits));

        let all = index.search(&query, None, k).unwrap();
        prop_assert_eq!(all.len(), k.min(rows.len()));
        prop_assert!(ordered(&all));
        let everything: Vec<u32> = rows.keys().copied().collect();
        let via_full_set = index.search(&query, Some(&support::doc_set(&everything)), k).unwrap();
        prop_assert_eq!(&via_full_set, &all);

        // The filtered list is the unrestricted list filtered, scores untouched.
        let unrestricted = index.search(&query, None, rows.len()).unwrap();
        let expected: Vec<_> = unrestricted.into_iter().filter(|h| allowed.contains(&h.id.0)).take(k).collect();
        prop_assert_eq!(hits, expected);
    }

    #[test]
    fn len_round_trips_through_mutations(
        adds in prop::collection::vec((0u32..32, finite_vec(4)), 0..40),
        deletes in prop::collection::vec(0u32..40, 0..20),
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let mut index = FlatIndex::create(tmp.path(), 4, Metric::Dot, "prop").unwrap();
        let mut model: BTreeMap<u32, Vec<f32>> = BTreeMap::new();
        for (id, v) in &adds {
            index.add(DocId(*id), v).unwrap();
            model.insert(*id, v.clone());
        }
        prop_assert_eq!(index.len(), 0);
        index.commit().unwrap();
        prop_assert_eq!(index.len(), model.len() as u64);
        let ids: Vec<DocId> = deletes.iter().map(|&i| DocId(i)).collect();
        index.delete(&ids).unwrap();
        for d in &deletes { model.remove(d); }
        index.commit().unwrap();
        prop_assert_eq!(index.len(), model.len() as u64);
        drop(index);
        let reopened = FlatIndex::open(tmp.path()).unwrap();
        prop_assert_eq!(reopened.len(), model.len() as u64);
        let all = reopened.search(&[1.0, 1.0, 1.0, 1.0], None, 100).unwrap();
        let got: Vec<u32> = { let mut v: Vec<u32> = all.iter().map(|h| h.id.0).collect(); v.sort_unstable(); v };
        prop_assert_eq!(got, model.keys().copied().collect::<Vec<_>>());
    }
}

#[cfg(feature = "mmap")]
proptest! {
    #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

    #[test]
    fn owned_and_mapped_agree_bit_for_bit(
        metric in metric(),
        rows in prop::collection::btree_map(0u32..48, finite_vec(6), 1..48),
        query in finite_vec(6),
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let mut index = FlatIndex::create(tmp.path(), 6, metric, "prop").unwrap();
        for (id, v) in &rows { index.add(DocId(*id), v).unwrap(); }
        index.commit().unwrap();
        let mapped = FlatIndex::open_mapped(tmp.path()).unwrap();
        let a = index.search(&query, None, rows.len()).unwrap();
        let b = mapped.search(&query, None, rows.len()).unwrap();
        prop_assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            prop_assert_eq!(x.id, y.id);
            prop_assert_eq!(x.score.to_bits(), y.score.to_bits());
        }
    }
}
