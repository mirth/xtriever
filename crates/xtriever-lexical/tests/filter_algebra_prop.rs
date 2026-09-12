//! User Story 4 — the filter algebra, property-tested (FR-023, SC-006; Principle II names it).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::OnceLock;

use proptest::prelude::*;
use support::{TestIndex, fixture_index};
use xtriever_core::{DocId, DocSet, Filter, LexicalIndex, LexicalQuery, Value};

fn index() -> &'static TestIndex {
    static IDX: OnceLock<TestIndex> = OnceLock::new();
    IDX.get_or_init(fixture_index)
}

fn resolve(f: &Filter) -> DocSet {
    index()
        .index
        .resolve_filter(f)
        .unwrap_or_else(|e| panic!("{f:?}: {e}"))
}

fn leaf() -> impl Strategy<Value = Filter> {
    prop_oneof![
        prop::sample::select(vec!["web", "book", "paper", "forum", "zzz"])
            .prop_map(|s| Filter::Eq("source".into(), Value::Keyword(s.into()))),
        any::<bool>().prop_map(|b| Filter::Eq("published".into(), Value::Bool(b))),
        (
            proptest::option::of(0u64..100_000),
            proptest::option::of(0u64..100_000)
        )
            .prop_map(|(lo, hi)| Filter::Range(
                "views".into(),
                lo.map(Value::U64),
                hi.map(Value::U64)
            )),
        (
            proptest::option::of(-60i64..60),
            proptest::option::of(-60i64..60)
        )
            .prop_map(|(lo, hi)| Filter::Range(
                "rank".into(),
                lo.map(Value::I64),
                hi.map(Value::I64)
            )),
        prop::sample::select(vec!["tags", "summary", "views"])
            .prop_map(|f| Filter::Exists(f.into())),
        prop::collection::vec(
            prop::sample::select(vec!["alpha", "beta", "gamma", "delta", "epsilon"]),
            0..3
        )
        .prop_map(|v| Filter::In(
            "tags".into(),
            v.into_iter().map(|s| Value::Keyword(s.into())).collect()
        )),
        prop::collection::vec(0u32..1100, 0..8)
            .prop_map(|v| Filter::Ids(v.into_iter().map(DocId).collect())),
    ]
}

fn filter() -> impl Strategy<Value = Filter> {
    leaf().prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Filter::And),
            prop::collection::vec(inner.clone(), 0..4).prop_map(Filter::Or),
            inner.prop_map(|f| Filter::Not(Box::new(f))),
        ]
    })
}

fn alive() -> DocSet {
    resolve(&Filter::Exists("views".into()))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1000, .. ProptestConfig::default() })]

    #[test]
    fn and_is_commutative_and_associative(a in filter(), b in filter(), c in filter()) {
        prop_assert_eq!(resolve(&Filter::And(vec![a.clone(), b.clone()])), resolve(&Filter::And(vec![b.clone(), a.clone()])));
        let l = Filter::And(vec![Filter::And(vec![a.clone(), b.clone()]), c.clone()]);
        let r = Filter::And(vec![a.clone(), Filter::And(vec![b.clone(), c.clone()])]);
        prop_assert_eq!(resolve(&l), resolve(&r));
        prop_assert_eq!(resolve(&Filter::And(vec![a.clone(), b.clone()])), resolve(&a).intersection(&resolve(&b)));
    }

    #[test]
    fn or_is_commutative_and_associative(a in filter(), b in filter(), c in filter()) {
        prop_assert_eq!(resolve(&Filter::Or(vec![a.clone(), b.clone()])), resolve(&Filter::Or(vec![b.clone(), a.clone()])));
        let l = Filter::Or(vec![Filter::Or(vec![a.clone(), b.clone()]), c.clone()]);
        let r = Filter::Or(vec![a.clone(), Filter::Or(vec![b.clone(), c.clone()])]);
        prop_assert_eq!(resolve(&l), resolve(&r));
        prop_assert_eq!(resolve(&Filter::Or(vec![a.clone(), b.clone()])), resolve(&a).union(&resolve(&b)));
    }

    #[test]
    fn not_is_an_involution_and_complements_the_live_set(a in filter()) {
        let not_not = Filter::Not(Box::new(Filter::Not(Box::new(a.clone()))));
        prop_assert_eq!(resolve(&not_not), resolve(&a));
        prop_assert_eq!(resolve(&Filter::Not(Box::new(a.clone()))), alive().difference(&resolve(&a)));
    }

    #[test]
    fn de_morgan(a in filter(), b in filter()) {
        let not_and = Filter::Not(Box::new(Filter::And(vec![a.clone(), b.clone()])));
        let or_nots = Filter::Or(vec![Filter::Not(Box::new(a.clone())), Filter::Not(Box::new(b.clone()))]);
        prop_assert_eq!(resolve(&not_and), resolve(&or_nots));
        let not_or = Filter::Not(Box::new(Filter::Or(vec![a.clone(), b.clone()])));
        let and_nots = Filter::And(vec![Filter::Not(Box::new(a.clone())), Filter::Not(Box::new(b.clone()))]);
        prop_assert_eq!(resolve(&not_or), resolve(&and_nots));
    }

    #[test]
    fn filtered_search_agrees_with_resolve(f in filter()) {
        let q = LexicalQuery::Match(Some("body".into()), "quantum lattice photon".into());
        let set = resolve(&f);
        let hits = index().index.search(&q, Some(&f), 1000).unwrap();
        let unfiltered = index().index.search(&q, None, 1000).unwrap();
        prop_assert!(hits.iter().all(|h| set.contains(h.id)));
        let expected: Vec<_> = unfiltered.into_iter().filter(|h| set.contains(h.id)).collect();
        prop_assert_eq!(hits, expected);
    }
}
