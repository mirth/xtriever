//! Property tests for the index invariants (Principle II): results ⊆ allowed, total ordering,
//! `len` round-trips through add/delete/commit, (under `mmap`) owned ≡ mapped, and — Feature
//! 024 — random add/replace/delete/commit sequences whose results survive `compact` and a
//! reopen bit for bit, with no dead row ever surfacing. Offline.
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

/// One step of a random mutation sequence (Feature 024).
#[derive(Debug, Clone)]
enum Op {
    Add(u32, Vec<f32>),
    Delete(Vec<u32>),
    Commit,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        5 => (0u32..40, finite_vec(6)).prop_map(|(id, v)| Op::Add(id, v)),
        2 => prop::collection::vec(0u32..44, 1..4).prop_map(Op::Delete),
        2 => Just(Op::Commit),
    ]
}

use support::hit_bits as bits;

/// The eight-bit scheme, restated here rather than imported: one scale per vector,
/// `scale = max|component| / 127`, codes rounded and clamped (Feature 026, ADR-0015). A
/// reference that called the crate's own code would prove nothing.
fn quantise(vector: &[f32]) -> (Vec<i8>, f32) {
    let peak = vector.iter().fold(0.0f32, |p, v| p.max(v.abs()));
    let scale = if peak > 0.0 { peak / 127.0 } else { 1.0 };
    let codes = vector
        .iter()
        .map(|v| (v / scale).round().clamp(-127.0, 127.0) as i8)
        .collect();
    (codes, scale)
}

/// What the stage recovers for a stored row.
fn recovered(vector: &[f32]) -> Vec<f32> {
    let (codes, scale) = quantise(vector);
    codes.iter().map(|c| f32::from(*c) * scale).collect()
}

/// An independent scorer over the reference model, in the contract's arithmetic.
///
/// Since Feature 026 the stage stores eight-bit codes, so the oracle scores what the stage
/// stores: the dot product of the quantised query and the quantised row accumulated in `i32` —
/// exactly, nothing rounds — then one multiply by the two scales. The row norm is still the
/// `f32` norm of the row as added, and Euclidean still works on recovered components, because a
/// distance is not a dot product.
fn reference(
    metric: Metric,
    model: &BTreeMap<u32, Vec<f32>>,
    allowed: Option<&[u32]>,
    q: &[f32],
    k: usize,
) -> Vec<(u32, u32)> {
    let q_norm: f64 = q
        .iter()
        .map(|x| f64::from(*x) * f64::from(*x))
        .sum::<f64>()
        .sqrt();
    let mut scored: Vec<(f32, u32)> = model
        .iter()
        .filter(|(id, _)| allowed.is_none_or(|a| a.contains(id)))
        .map(|(id, row)| {
            let (q_codes, q_scale) = quantise(q);
            let (row_codes, row_scale) = quantise(row);
            let accumulator: i32 = q_codes
                .iter()
                .zip(&row_codes)
                .map(|(a, b)| i32::from(*a) * i32::from(*b))
                .sum();
            let dot = f64::from(accumulator) * f64::from(q_scale) * f64::from(row_scale);
            let score = match metric {
                Metric::Dot => dot,
                Metric::Cosine => {
                    let row_norm = row
                        .iter()
                        .map(|x| f64::from(*x) * f64::from(*x))
                        .sum::<f64>()
                        .sqrt() as f32;
                    dot / (q_norm * f64::from(row_norm))
                }
                Metric::Euclidean => -q
                    .iter()
                    .zip(recovered(row))
                    .map(|(a, b)| {
                        let d = f64::from(*a) - f64::from(b);
                        d * d
                    })
                    .sum::<f64>()
                    .sqrt(),
            };
            (score as f32, *id)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(&b.1)));
    scored.truncate(k);
    scored
        .into_iter()
        .map(|(s, id)| (id, s.to_bits()))
        .collect()
}

proptest! {
    // The default case count honours `PROPTEST_CASES` (SC-002's 1,000-sequence run).
    #![proptest_config(ProptestConfig::default())]

    #[test]
    fn compact_and_reopen_preserve_every_bit(
        metric in metric(),
        ops in prop::collection::vec(op(), 1..200),
        queries in prop::collection::vec((finite_vec(6), prop::collection::vec(0u32..44, 0..12), prop_oneof![Just(3usize), Just(64)]), 1..4),
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let mut index = FlatIndex::create(tmp.path(), 6, metric, "prop").unwrap();
        // Under `mmap`, the same sequence on a writable mapped handle in a second directory:
        // every append re-maps the committed prefix and every compaction maps a new
        // generation — the paths the SAFETY argument rests on, compared bit for bit with the
        // buffered handle after each commit (ADR-0007 condition 3).
        #[cfg(feature = "mmap")]
        let tmp2 = tempfile::tempdir().unwrap();
        #[cfg(feature = "mmap")]
        let mut mapped = {
            drop(FlatIndex::create(tmp2.path(), 6, metric, "prop").unwrap());
            FlatIndex::open_mapped(tmp2.path()).unwrap()
        };
        let mut model: BTreeMap<u32, Vec<f32>> = BTreeMap::new();
        for o in &ops {
            match o {
                Op::Add(id, v) => {
                    index.add(DocId(*id), v).unwrap();
                    #[cfg(feature = "mmap")]
                    mapped.add(DocId(*id), v).unwrap();
                    model.insert(*id, v.clone());
                }
                Op::Delete(ids) => {
                    let d: Vec<DocId> = ids.iter().map(|&i| DocId(i)).collect();
                    index.delete(&d).unwrap();
                    #[cfg(feature = "mmap")]
                    mapped.delete(&d).unwrap();
                    for i in ids { model.remove(i); }
                }
                Op::Commit => {
                    index.commit().unwrap();
                    prop_assert_eq!(index.len(), model.len() as u64);
                    #[cfg(feature = "mmap")]
                    {
                        mapped.commit().unwrap();
                        for (q, _, k) in &queries {
                            prop_assert_eq!(bits(&mapped.search(q, None, *k).unwrap()), bits(&index.search(q, None, *k).unwrap()), "mapped vs buffered after a commit");
                        }
                    }
                }
            }
        }
        index.commit().unwrap();
        #[cfg(feature = "mmap")]
        mapped.commit().unwrap();
        let committed = model;
        prop_assert_eq!(index.len(), committed.len() as u64);
        // Every query, unfiltered and filtered, against the reference scorer: ids and score bits.
        let run = |index: &FlatIndex| -> Vec<Vec<(u32, u32)>> {
            queries.iter().flat_map(|(q, allowed, k)| {
                let set = support::doc_set(allowed);
                [bits(&index.search(q, None, *k).unwrap()), bits(&index.search(q, Some(&set), *k).unwrap())]
            }).collect()
        };
        let expected: Vec<Vec<(u32, u32)>> = queries.iter().flat_map(|(q, allowed, k)| {
            [reference(metric, &committed, None, q, *k), reference(metric, &committed, Some(allowed), q, *k)]
        }).collect();
        let before = run(&index);
        prop_assert_eq!(&before, &expected, "against the reference scorer");
        for (id, v) in &committed {
            // What a committed row recovers, not what was added: the stage stores codes and a
            // scale (Feature 026). The oracle quantises the same way, so this is still exact.
            let got = index.vector(DocId(*id));
            let want = recovered(v);
            prop_assert_eq!(got.as_deref(), Some(want.as_slice()));
        }
        index.compact().unwrap();
        prop_assert_eq!(index.stats().dead, 0);
        prop_assert_eq!(index.stats().rows, committed.len() as u64);
        prop_assert_eq!(&run(&index), &expected, "after compaction");
        #[cfg(feature = "mmap")]
        {
            prop_assert_eq!(&run(&mapped), &expected, "mapped, before compaction");
            mapped.compact().unwrap();
            prop_assert_eq!(&run(&mapped), &expected, "mapped, after compaction");
            prop_assert_eq!(&run(&FlatIndex::open_mapped(tmp2.path()).unwrap()), &expected, "mapped, reopened");
        }
        drop(index);
        let reopened = FlatIndex::open(tmp.path()).unwrap();
        prop_assert_eq!(&run(&reopened), &expected, "after reopen");
        prop_assert_eq!(reopened.len(), committed.len() as u64);
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
