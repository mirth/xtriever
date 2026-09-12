//! User Story 4 — filters restrict results and can be shared with other stages (FR-019–FR-022).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{Filters, fixture_index, ids};
use xtriever_core::{DocId, DocSet, Error, Filter, LexicalIndex, LexicalQuery, Value};

fn filters() -> Filters {
    support::load_json("filters.json")
}

fn as_vec(set: &DocSet) -> Vec<u32> {
    set.iter().map(|d| d.0).collect()
}

// Scenario 1: every filter golden, exact
#[test]
fn every_filter_golden_resolves_exactly() {
    let t = fixture_index();
    for entry in filters().filters {
        let set = t
            .index
            .resolve_filter(&entry.filter)
            .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
        assert_eq!(
            as_vec(&set),
            entry.expected_ids,
            "{}: set differs",
            entry.name
        );
    }
}

// Scenario 2 (FR-022): filtered search = unfiltered ∩ set, scores unchanged
#[test]
fn filtered_search_restricts_without_changing_scores() {
    let t = fixture_index();
    let q = LexicalQuery::Match(Some("body".into()), "quantum".into());
    let unfiltered = t.index.search(&q, None, 1000).expect("search");
    for entry in filters().filters {
        let set = t.index.resolve_filter(&entry.filter).expect("resolve");
        let filtered = t
            .index
            .search(&q, Some(&entry.filter), 1000)
            .expect("search");
        let expected: Vec<_> = unfiltered
            .iter()
            .filter(|h| set.contains(h.id))
            .copied()
            .collect();
        support::assert_hits_exact(&format!("filtered by {}", entry.name), &filtered, &expected);
    }
}

// Scenario 3 (FR-021): unknown field is an error, never an empty set
#[test]
fn unknown_field_in_filter_is_unknown_field() {
    let t = fixture_index();
    for f in [
        Filter::Eq("nope".into(), Value::Keyword("x".into())),
        Filter::Exists("nope".into()),
        Filter::Range("nope".into(), None, Some(Value::U64(1))),
        Filter::And(vec![
            Filter::Exists("body".into()),
            Filter::Not(Box::new(Filter::Exists("nope".into()))),
        ]),
    ] {
        let err = t.index.resolve_filter(&f).expect_err("must fail");
        assert!(
            matches!(&err, Error::UnknownField(n) if n.0 == "nope"),
            "{f:?}: {err}"
        );
    }
}

// Scenario 4: open bounds are unbounded, closed bounds inclusive — covered by the goldens
// (range_*_open_*, range_ts_one_ms); this test pins the millisecond precision explicitly.
#[test]
fn date_millis_range_is_millisecond_precise() {
    let t = fixture_index();
    let corpus = support::corpus();
    let ts = |i: usize| match corpus.documents[i].fields.get(&"ts".into()) {
        Some(Value::DateMillis(v)) => *v,
        other => panic!("{other:?}"),
    };
    let (t10, t11) = (ts(10), ts(11));
    assert_eq!(t11, t10 + 1, "fixture plants a one-millisecond gap");
    let only_10 = Filter::Range(
        "ts".into(),
        Some(Value::DateMillis(t10)),
        Some(Value::DateMillis(t10)),
    );
    assert_eq!(as_vec(&t.index.resolve_filter(&only_10).unwrap()), vec![10]);
    let both = Filter::Range(
        "ts".into(),
        Some(Value::DateMillis(t10)),
        Some(Value::DateMillis(t11)),
    );
    assert_eq!(
        as_vec(&t.index.resolve_filter(&both).unwrap()),
        vec![10, 11]
    );
    let eq_11 = Filter::Eq("ts".into(), Value::DateMillis(t11));
    assert_eq!(as_vec(&t.index.resolve_filter(&eq_11).unwrap()), vec![11]);
}

// Scenario 5 (FR-021): value type mismatch is an error, not a coercion
#[test]
fn value_type_mismatch_in_filter_is_invalid_query() {
    let t = fixture_index();
    for f in [
        Filter::Eq("views".into(), Value::Keyword("10".into())),
        Filter::Eq("source".into(), Value::U64(1)),
        Filter::In("published".into(), vec![Value::Text("true".into())]),
        Filter::Range("rank".into(), Some(Value::F64(1.0)), None),
        Filter::Eq("ts".into(), Value::I64(1)),
    ] {
        let err = t.index.resolve_filter(&f).expect_err("must fail");
        assert!(matches!(&err, Error::InvalidQuery(_)), "{f:?}: {err}");
    }
}

#[test]
fn eq_in_range_on_text_field_are_invalid_query_but_exists_works() {
    let t = fixture_index();
    for f in [
        Filter::Eq("body".into(), Value::Text("quantum".into())),
        Filter::In("body".into(), vec![Value::Text("quantum".into())]),
        Filter::Range("body".into(), Some(Value::Text("a".into())), None),
    ] {
        let err = t.index.resolve_filter(&f).expect_err("must fail");
        assert!(
            matches!(&err, Error::InvalidQuery(m) if m.contains("body")),
            "{f:?}: {err}"
        );
    }
    let n = t
        .index
        .resolve_filter(&Filter::Exists("summary".into()))
        .expect("exists on text")
        .len();
    assert_eq!(n, 500);
}

#[test]
fn range_on_bool_is_invalid_query() {
    let t = fixture_index();
    // every Range shape on a bool, including the fully-open one that otherwise rewrites to Exists
    for f in [
        Filter::Range(
            "published".into(),
            Some(Value::Bool(false)),
            Some(Value::Bool(false)),
        ),
        Filter::Range("published".into(), None, Some(Value::Bool(true))),
        Filter::Range("published".into(), None, None),
    ] {
        let err = t.index.resolve_filter(&f).expect_err("must fail");
        assert!(
            matches!(&err, Error::InvalidQuery(m) if m.contains("published")),
            "{f:?}: {err}"
        );
    }
}

#[test]
fn filter_on_unindexed_field_is_invalid_query() {
    let t = fixture_index();
    let err = t
        .index
        .resolve_filter(&Filter::Exists("note".into()))
        .expect_err("must fail");
    assert!(
        matches!(&err, Error::InvalidQuery(m) if m.contains("note")),
        "{err}"
    );
}

// Scenario 6 (FR-020): deleted docs are absent from filter results
#[test]
fn deleted_documents_are_absent_from_filter_results() {
    let mut t = fixture_index();
    let all = Filter::Exists("views".into());
    assert_eq!(t.index.resolve_filter(&all).unwrap().len(), 1000);
    t.index
        .delete(&[DocId(0), DocId(500), DocId(999)])
        .expect("delete");
    t.index.commit().expect("commit");
    let set = t.index.resolve_filter(&all).expect("resolve");
    assert_eq!(set.len(), 997);
    assert!(!set.contains(DocId(0)) && !set.contains(DocId(500)) && !set.contains(DocId(999)));
    let idset = t
        .index
        .resolve_filter(&Filter::Ids(vec![DocId(0), DocId(1)]))
        .unwrap();
    assert_eq!(as_vec(&idset), vec![1]);
    let notset = t
        .index
        .resolve_filter(&Filter::Not(Box::new(Filter::Exists("tags".into()))))
        .unwrap();
    assert!(
        !notset.contains(DocId(0)),
        "Not must be relative to live documents only"
    );
    let q = LexicalQuery::Match(Some("body".into()), "quantum".into());
    let hits = t.index.search(&q, Some(&all), 1000).expect("search");
    assert!(ids(&hits).iter().all(|i| ![0, 500, 999].contains(i)));
}
