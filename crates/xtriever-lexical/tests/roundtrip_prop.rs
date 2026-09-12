//! Invariant: index round-trips (Principle II). Random documents → add → commit → drop → open must
//! reproduce filters and statistics exactly, and stored values must survive (FR-008a, FR-011, FR-012).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use proptest::prelude::*;
use support::{TestIndex, fixture_schema};
use xtriever_core::{DocId, Document, Error, FieldName, Filter, LexicalIndex, Value};
use xtriever_lexical::TantivyIndex;

#[derive(Debug, Clone)]
struct RDoc {
    id: u32,
    title: String,
    source: &'static str,
    views: u64,
    published: bool,
    note: Option<String>,
}

fn rdoc() -> impl Strategy<Value = RDoc> {
    (
        0u32..50,
        "[a-z ]{1,30}",
        prop::sample::select(vec!["web", "book"]),
        0u64..1000,
        any::<bool>(),
        proptest::option::of("[a-z ]{0,20}"),
    )
        .prop_map(|(id, title, source, views, published, note)| RDoc {
            id,
            title,
            source,
            views,
            published,
            note,
        })
}

fn to_doc(d: &RDoc) -> Document {
    let mut fields: BTreeMap<FieldName, Value> = BTreeMap::new();
    fields.insert("title".into(), Value::Text(d.title.clone()));
    fields.insert("body".into(), Value::Text(format!("body of {}", d.id)));
    fields.insert("source".into(), Value::Keyword(d.source.into()));
    fields.insert("views".into(), Value::U64(d.views));
    fields.insert("rank".into(), Value::I64(0));
    fields.insert("quality".into(), Value::F64(0.5));
    fields.insert("published".into(), Value::Bool(d.published));
    fields.insert(
        "ts".into(),
        Value::DateMillis(1_700_000_000_000 + i64::from(d.id)),
    );
    if let Some(n) = &d.note {
        fields.insert("note".into(), Value::Text(n.clone()));
    }
    Document {
        id: DocId(d.id),
        fields,
        chunk: None,
    }
}

/// Stored-field readback through the backend: the crate exposes no read path (FR-008a), so the
/// test reads the `note` field directly from the on-disk index to prove the write happened.
fn stored_note(dir: &std::path::Path, id: u32) -> Option<String> {
    use tantivy::collector::TopDocs;
    use tantivy::query::TermQuery;
    use tantivy::schema::{IndexRecordOption, Value as _};
    let index = tantivy::Index::open_in_dir(dir).expect("backend open");
    let reader = index.reader().expect("reader");
    let searcher = reader.searcher();
    let id_field = index.schema().get_field("__xt_id").expect("__xt_id");
    let note_field = index.schema().get_field("note").expect("note");
    let q = TermQuery::new(
        tantivy::Term::from_field_u64(id_field, u64::from(id)),
        IndexRecordOption::Basic,
    );
    let top = searcher
        .search(&q, &TopDocs::with_limit(1).order_by_score())
        .expect("search");
    let (_, addr) = top.first()?;
    let doc: tantivy::TantivyDocument = searcher.doc(*addr).expect("doc");
    doc.get_first(note_field)
        .and_then(|v| v.as_str().map(str::to_owned))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, .. ProptestConfig::default() })]

    #[test]
    fn add_commit_reopen_reproduces_the_model(docs in prop::collection::vec(rdoc(), 1..20)) {
        // last write wins for duplicate ids, both in the model and in the index
        let mut model: BTreeMap<u32, RDoc> = BTreeMap::new();
        for d in &docs { model.insert(d.id, d.clone()); }

        let mut t = TestIndex::create_fixture();
        t.index.add(&docs.iter().map(to_doc).collect::<Vec<_>>()).unwrap();
        t.index.commit().unwrap();
        let t = t.reopen();

        prop_assert_eq!(t.index.stats().unwrap().num_docs, model.len() as u64);
        prop_assert_eq!(t.index.schema(), &fixture_schema());
        for (id, d) in &model {
            let one = t.index.resolve_filter(&Filter::Ids(vec![DocId(*id)])).unwrap();
            prop_assert!(one.contains(DocId(*id)));
            let by_views: Vec<u32> = t.index.resolve_filter(&Filter::Eq("views".into(), Value::U64(d.views))).unwrap().iter().map(|x| x.0).collect();
            let expected: Vec<u32> = model.values().filter(|m| m.views == d.views).map(|m| m.id).collect();
            prop_assert_eq!(by_views, expected);
            let stored = stored_note(&t.path(), *id);
            prop_assert_eq!(stored, d.note.clone());
        }
        let web: Vec<u32> = t.index.resolve_filter(&Filter::Eq("source".into(), Value::Keyword("web".into()))).unwrap().iter().map(|x| x.0).collect();
        let expected: Vec<u32> = model.values().filter(|m| m.source == "web").map(|m| m.id).collect();
        prop_assert_eq!(web, expected);
    }
}

// FR-012: descriptor mismatches are hard errors at open
#[test]
fn open_rejects_missing_or_wrong_descriptor() {
    let t = TestIndex::create_fixture();
    let TestIndex { dir, index } = t;
    drop(index);
    let idx_dir = dir.path().join("idx");
    let descriptor = idx_dir.join("xtriever-lexical.json");
    let original = std::fs::read_to_string(&descriptor).expect("descriptor exists");

    std::fs::write(
        &descriptor,
        original
            .replace("\"format_version\":1", "\"format_version\":99")
            .replace("\"format_version\": 1", "\"format_version\": 99"),
    )
    .unwrap();
    let err = TantivyIndex::open(&idx_dir).expect_err("wrong version must fail");
    assert!(matches!(err, Error::Corrupt(_)), "{err}");

    std::fs::remove_file(&descriptor).unwrap();
    let err = TantivyIndex::open(&idx_dir).expect_err("missing descriptor must fail");
    assert!(matches!(err, Error::Corrupt(_)), "{err}");

    std::fs::write(&descriptor, original).unwrap();
    TantivyIndex::open(&idx_dir).expect("restored descriptor opens");

    let err =
        TantivyIndex::open(&dir.path().join("does-not-exist")).expect_err("missing dir must fail");
    assert!(matches!(err, Error::Corrupt(_) | Error::Io(_)), "{err}");
}

#[test]
fn create_refuses_a_non_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("junk"), b"x").unwrap();
    let err = TantivyIndex::create(dir.path(), fixture_schema()).expect_err("must fail");
    assert!(matches!(err, Error::Io(_)), "{err}");
}

// FR-012: a descriptor whose schema differs from the on-disk index — same format version — is a
// hard error at open, never a silent reinterpretation.
#[test]
fn open_rejects_descriptor_schema_mismatch() {
    let t = TestIndex::create_fixture();
    let TestIndex { dir, index } = t;
    drop(index);
    let idx_dir = dir.path().join("idx");
    let descriptor = idx_dir.join("xtriever-lexical.json");
    let original = std::fs::read_to_string(&descriptor).expect("descriptor exists");
    let mut json: serde_json::Value = serde_json::from_str(&original).unwrap();
    assert_eq!(json["format_version"], 1);

    // (a) a different analyzer on `title`: the backend tokenizer name changes
    json["schema"]["fields"][0]["kind"] = serde_json::json!({"Text": "standard_en"});
    std::fs::write(&descriptor, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    let err = TantivyIndex::open(&idx_dir).expect_err("analyzer change must fail");
    assert!(
        matches!(&err, Error::Corrupt(m) if m.contains("schema")),
        "{err}"
    );

    // (b) a different field kind on `views`
    let mut json: serde_json::Value = serde_json::from_str(&original).unwrap();
    json["schema"]["fields"][5]["kind"] = serde_json::json!("I64");
    std::fs::write(&descriptor, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    let err = TantivyIndex::open(&idx_dir).expect_err("kind change must fail");
    assert!(
        matches!(&err, Error::Corrupt(m) if m.contains("schema")),
        "{err}"
    );

    // (c) an extra field
    let mut json: serde_json::Value = serde_json::from_str(&original).unwrap();
    json["schema"]["fields"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(
            {"name": "extra", "kind": "Keyword", "indexed": true, "stored": false, "boost": 1.0}
        ));
    std::fs::write(&descriptor, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    let err = TantivyIndex::open(&idx_dir).expect_err("extra field must fail");
    assert!(
        matches!(&err, Error::Corrupt(m) if m.contains("schema")),
        "{err}"
    );

    std::fs::write(&descriptor, original).unwrap();
    TantivyIndex::open(&idx_dir).expect("restored descriptor opens");
}
