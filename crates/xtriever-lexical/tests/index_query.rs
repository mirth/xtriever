//! User Story 1 — documents go in and come back ranked (spec scenarios 1–10 plus FR-018 edges).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{TestIndex, fixture_index, fixture_schema, ids};
use xtriever_core::{
    AnalyzerId, ChunkInfo, DocId, Document, Error, FieldDef, FieldKind, FieldName, LexicalIndex,
    LexicalQuery, Schema, Value,
};

fn field(name: &str, kind: FieldKind, indexed: bool, stored: bool, boost: f32) -> FieldDef {
    FieldDef {
        name: name.into(),
        kind,
        indexed,
        stored,
        boost,
    }
}

fn text_kind(id: &str) -> FieldKind {
    FieldKind::Text(AnalyzerId(id.to_owned()))
}

fn doc(id: u32, fields: &[(&str, Value)]) -> Document {
    Document {
        id: DocId(id),
        fields: fields
            .iter()
            .map(|(n, v)| (FieldName::from(*n), v.clone()))
            .collect(),
        chunk: None,
    }
}

// Scenario 1
#[test]
fn schema_round_trips_through_create() {
    let t = TestIndex::create_fixture();
    assert_eq!(t.index.schema(), &fixture_schema());
}

// Scenario 2
#[test]
fn match_returns_exactly_k_descending() {
    let t = fixture_index();
    let q = LexicalQuery::Match(Some("body".into()), "lattice signal vector".into());
    let hits = t.index.search(&q, None, 10).expect("search");
    assert_eq!(hits.len(), 10);
    assert!(
        hits.windows(2).all(|w| w[0].score >= w[1].score),
        "not descending: {hits:?}"
    );
}

// Scenario 3
#[test]
fn fewer_than_k_matches_returns_all_of_them() {
    let t = fixture_index();
    let golden = support::query("term_all_matches"); // k = 1000 > matches
    let hits = t
        .index
        .search(&golden.query, None, golden.k)
        .expect("search");
    assert!(!hits.is_empty() && hits.len() < golden.k);
}

// Scenario 4
#[test]
fn no_match_is_an_empty_ok() {
    let t = fixture_index();
    let q = LexicalQuery::Match(Some("body".into()), "zzzzunknownword".into());
    assert!(t.index.search(&q, None, 10).expect("search").is_empty());
}

// Scenario 5 (FR-007)
#[test]
fn unknown_field_in_document_is_an_error_naming_it() {
    let mut t = TestIndex::create_fixture();
    let d = doc(
        1,
        &[("title", Value::Text("x".into())), ("nope", Value::U64(1))],
    );
    let err = t.index.add(&[d]).expect_err("must fail");
    assert!(
        matches!(&err, Error::UnknownField(f) if f.0 == "nope"),
        "{err}"
    );
}

#[test]
fn value_kind_mismatch_in_document_is_a_schema_error_naming_the_field() {
    let mut t = TestIndex::create_fixture();
    let d = doc(1, &[("views", Value::Text("not a number".into()))]);
    let err = t.index.add(&[d]).expect_err("must fail");
    assert!(
        matches!(&err, Error::Schema(m) if m.contains("views")),
        "{err}"
    );
}

// Scenario 6 — every golden, exact
#[test]
fn every_query_golden_matches_exactly() {
    let t = fixture_index();
    for entry in support::queries().queries {
        let expected = entry
            .expected
            .as_ref()
            .unwrap_or_else(|| panic!("{}: golden not minted", entry.name));
        let hits = t
            .index
            .search(&entry.query, entry.filter.as_ref(), entry.k)
            .expect("search");
        support::assert_hits_exact(&entry.name, &hits, expected);
    }
}

// Scenario 7 (FR-006)
#[test]
fn unknown_analyzer_id_fails_at_create_naming_field_and_id() {
    let schema = Schema {
        fields: vec![field(
            "body",
            text_kind("hf:bert-base-uncased"),
            true,
            false,
            1.0,
        )],
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let err = xtriever_lexical::TantivyIndex::create(&dir.path().join("idx"), schema)
        .expect_err("must fail");
    assert!(
        matches!(&err, Error::Schema(m) if m.contains("body") && m.contains("hf:bert-base-uncased")),
        "{err}"
    );
    assert!(
        !dir.path()
            .join("idx")
            .join("xtriever-lexical.json")
            .exists(),
        "no index may be produced"
    );
}

// Scenario 8 (FR-005): field boost doubles Term scores and the field's contribution in Match(None)
#[test]
fn field_boost_multiplies_scores() {
    let make = |boost: f32| Schema {
        fields: vec![
            field("title", text_kind("standard"), true, false, boost),
            field("body", text_kind("standard"), true, false, 1.0),
        ],
    };
    let docs = vec![
        doc(
            0,
            &[
                ("title", Value::Text("quantum kernel".into())),
                ("body", Value::Text("river valley".into())),
            ],
        ),
        doc(
            1,
            &[
                ("title", Value::Text("river valley".into())),
                ("body", Value::Text("quantum kernel".into())),
            ],
        ),
        doc(
            2,
            &[
                // No `quantum` here: both fields need identical statistics (df, total tokens)
                // for the no-boost comparison below to be meaningful.
                ("title", Value::Text("piano harp".into())),
                ("body", Value::Text("piano harp".into())),
            ],
        ),
    ];
    let mut plain = TestIndex::create(make(1.0));
    let mut boosted = TestIndex::create(make(2.0));
    for t in [&mut plain, &mut boosted] {
        t.index.add(&docs).expect("add");
        t.index.commit().expect("commit");
    }
    let term = LexicalQuery::Term("title".into(), "quantum".into());
    let p = plain.index.search(&term, None, 10).expect("search");
    let b = boosted.index.search(&term, None, 10).expect("search");
    assert_eq!(ids(&p), ids(&b));
    for (x, y) in p.iter().zip(&b) {
        assert_eq!(
            y.score,
            x.score * 2.0,
            "Term on the boosted field must be exactly doubled"
        );
    }
    // Match(None): doc 0 (term in boosted title) must now outrank doc 1 (term in body).
    let all = LexicalQuery::Match(None, "quantum".into());
    let p = plain.index.search(&all, None, 10).expect("search");
    let b = boosted.index.search(&all, None, 10).expect("search");
    let score = |hits: &[xtriever_core::Hit], id: u32| {
        hits.iter().find(|h| h.id.0 == id).map(|h| h.score).unwrap()
    };
    assert_eq!(
        score(&p, 0),
        score(&p, 1),
        "same text, same length: identical without boost"
    );
    assert_eq!(score(&b, 0), score(&p, 0) * 2.0);
    assert_eq!(score(&b, 1), score(&p, 1));
}

// Scenario 9 (FR-005)
#[test]
fn boost_on_non_text_field_fails_at_create() {
    let schema = Schema {
        fields: vec![field("source", FieldKind::Keyword, true, false, 2.0)],
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let err = xtriever_lexical::TantivyIndex::create(&dir.path().join("idx"), schema)
        .expect_err("must fail");
    assert!(
        matches!(&err, Error::Schema(m) if m.contains("source")),
        "{err}"
    );
}

#[test]
fn reserved_field_prefix_fails_at_create() {
    let schema = Schema {
        fields: vec![field("__xt_id", FieldKind::U64, true, false, 1.0)],
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let err = xtriever_lexical::TantivyIndex::create(&dir.path().join("idx"), schema)
        .expect_err("must fail");
    assert!(
        matches!(&err, Error::Schema(m) if m.contains("__xt_")),
        "{err}"
    );
}

// Scenario 10 (FR-008b)
#[test]
fn chunk_info_is_neither_indexed_nor_stored() {
    let corpus = support::corpus();
    let with_chunk: Vec<Document> = corpus
        .documents
        .iter()
        .map(|d| Document {
            chunk: Some(ChunkInfo {
                parent: format!("parentzzz{}", d.id),
                ordinal: d.id.0,
                byte_range: Some((0, 7)),
            }),
            ..d.clone()
        })
        .collect();
    let mut a = TestIndex::create_fixture();
    support::index_in_batches(&mut a.index, &corpus.documents, 1);
    let mut b = TestIndex::create_fixture();
    support::index_in_batches(&mut b.index, &with_chunk, 1);
    for entry in support::queries().queries {
        let ha = a
            .index
            .search(&entry.query, entry.filter.as_ref(), entry.k)
            .expect("search");
        let hb = b
            .index
            .search(&entry.query, entry.filter.as_ref(), entry.k)
            .expect("search");
        support::assert_hits_exact(&entry.name, &hb, &ha);
    }
    let probe = LexicalQuery::Match(None, "parentzzz42".into());
    assert!(
        b.index.search(&probe, None, 10).expect("search").is_empty(),
        "parent id leaked into the index"
    );
}

// FR-018 / D11 edges
#[test]
fn k_zero_is_an_empty_ok() {
    let t = fixture_index();
    let q = LexicalQuery::Match(Some("body".into()), "quantum".into());
    assert!(
        t.index
            .search(&q, None, 0)
            .expect("k=0 must not panic")
            .is_empty()
    );
}

#[test]
fn phrase_on_keyword_field_is_invalid_query() {
    let t = fixture_index();
    let q = LexicalQuery::Phrase("source".into(), "web paper".into(), 0);
    let err = t.index.search(&q, None, 10).expect_err("must fail");
    assert!(
        matches!(&err, Error::InvalidQuery(m) if m.contains("source")),
        "{err}"
    );
}

#[test]
fn query_on_unindexed_field_is_invalid_query() {
    let t = fixture_index();
    for q in [
        LexicalQuery::Match(Some("note".into()), "note".into()),
        LexicalQuery::Term("note".into(), "note".into()),
        LexicalQuery::Phrase("note".into(), "note for".into(), 0),
        LexicalQuery::Fuzzy("note".into(), "note".into(), 1),
    ] {
        let err = t.index.search(&q, None, 10).expect_err("must fail");
        assert!(
            matches!(&err, Error::InvalidQuery(m) if m.contains("note")),
            "{q:?}: {err}"
        );
    }
}

#[test]
fn match_on_non_text_field_is_invalid_query() {
    let t = fixture_index();
    let q = LexicalQuery::Match(Some("views".into()), "1".into());
    let err = t.index.search(&q, None, 10).expect_err("must fail");
    assert!(
        matches!(&err, Error::InvalidQuery(m) if m.contains("views")),
        "{err}"
    );
}

#[test]
fn unknown_field_in_query_is_unknown_field() {
    let t = fixture_index();
    let q = LexicalQuery::Term("nope".into(), "x".into());
    let err = t.index.search(&q, None, 10).expect_err("must fail");
    assert!(
        matches!(&err, Error::UnknownField(f) if f.0 == "nope"),
        "{err}"
    );
}

#[test]
fn fuzzy_distance_above_two_is_invalid_query() {
    let t = fixture_index();
    let q = LexicalQuery::Fuzzy("body".into(), "lattice".into(), 3);
    let err = t.index.search(&q, None, 10).expect_err("must fail");
    assert!(matches!(&err, Error::InvalidQuery(_)), "{err}");
}

#[test]
fn short_phrases_do_not_panic() {
    let t = fixture_index();
    // one token: behaves as a term query
    let one = LexicalQuery::Phrase("body".into(), "quantum".into(), 0);
    let term = LexicalQuery::Term("body".into(), "quantum".into());
    let a = t.index.search(&one, None, 10).expect("single-token phrase");
    let b = t.index.search(&term, None, 10).expect("term");
    support::assert_hits_exact("single-token phrase == term", &a, &b);
    // zero tokens: empty
    let zero = LexicalQuery::Phrase("body".into(), "!!! ...".into(), 0);
    assert!(
        t.index
            .search(&zero, None, 10)
            .expect("zero-token phrase")
            .is_empty()
    );
    // duplicate DocIds in one batch: last write wins (edge case)
    let mut t2 = TestIndex::create_fixture();
    let a = doc(
        7,
        &[
            ("title", Value::Text("first".into())),
            ("body", Value::Text("alpha".into())),
        ],
    );
    let b = doc(
        7,
        &[
            ("title", Value::Text("second".into())),
            ("body", Value::Text("beta".into())),
        ],
    );
    t2.index.add(&[a, b]).expect("add");
    t2.index.commit().expect("commit");
    assert!(
        t2.index
            .search(&LexicalQuery::Term("body".into(), "alpha".into()), None, 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        ids(&t2
            .index
            .search(&LexicalQuery::Term("body".into(), "beta".into()), None, 10)
            .unwrap()),
        vec![7]
    );
}
