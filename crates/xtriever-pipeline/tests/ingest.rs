//! US1 scenarios 1–3, 6 — ingest under external ids (spec FR-002, FR-007; SC-002). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_core::{Error, FieldName, Value};
use xtriever_pipeline::{HybridIndex, SearchOptions, SourceDocument};

fn doc(id: &str, title: &str, text: &str) -> SourceDocument {
    let mut fields = BTreeMap::new();
    fields.insert(FieldName::from("title"), Value::Text(title.to_owned()));
    fields.insert(FieldName::from("text"), Value::Text(text.to_owned()));
    fields.insert(FieldName::from("source"), Value::Keyword("a".into()));
    SourceDocument {
        external_id: id.to_owned(),
        fields,
        chunk: None,
    }
}

/// A thousand documents whose passages the table embedder knows (one shared vector each).
fn thousand(h: &support::Hybrid) -> (support::TableEmbedder, Vec<SourceDocument>) {
    let mut embedder = support::TableEmbedder::from_fixture(h);
    let mut docs = Vec::new();
    for i in 0..1000 {
        let title = format!("Title {i}");
        let text = format!("body of document {i} with word{}", i % 7);
        embedder = embedder.with(
            &format!("{title} {text}"),
            vec![(i % 13) as f32 + 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        );
        docs.push(doc(&format!("doc-{i:04}"), &title, &text));
    }
    (embedder, docs)
}

#[test]
fn a_thousand_documents_round_trip_replace_and_delete() {
    let h = support::hybrid();
    let (embedder, docs) = thousand(&h);
    let tmp = tempfile::tempdir().unwrap();
    let mut index =
        HybridIndex::create(tmp.path(), support::fixture_config(&h), Box::new(embedder)).unwrap();
    assert!(index.is_empty());
    for batch in docs.chunks(250) {
        index.add(batch).unwrap();
    }
    assert_eq!(index.len(), 0, "pending is invisible");
    index.commit().unwrap();
    assert_eq!(index.len(), 1000);
    for d in &docs {
        assert!(index.contains(&d.external_id), "{}", d.external_id);
    }
    assert!(!index.contains("doc-9999"));

    // Replace 50 with a new text; they must appear once with the new text.
    let h2 = support::hybrid();
    let mut replaced = Vec::new();
    for i in 0..50 {
        replaced.push(doc(
            &format!("doc-{i:04}"),
            "Replaced",
            &format!("replaced document {i} xylophone"),
        ));
    }
    // The replacement passages need vectors too: rebuild the index handle with an embedder that knows them.
    drop(index);
    let mut embedder = support::TableEmbedder::from_fixture(&h2);
    for i in 0..50 {
        embedder = embedder.with(
            &format!("Replaced replaced document {i} xylophone"),
            vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        );
    }
    let mut index = HybridIndex::open(tmp.path(), Box::new(embedder)).unwrap();
    index.add(&replaced).unwrap();
    index.commit().unwrap();
    assert_eq!(index.len(), 1000, "replacement does not grow the index");
    let r = index
        .search("xylophone", None, 100, &SearchOptions::default())
        .unwrap();
    assert_eq!(r.hits.len(), 50);
    let mut seen: Vec<&str> = r.hits.iter().map(|h| h.external_id.as_str()).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), 50, "each replaced id appears once");

    // Delete 100 known and 5 unknown ids.
    let mut to_delete: Vec<String> = (100..200).map(|i| format!("doc-{i:04}")).collect();
    to_delete.extend((0..5).map(|i| format!("nope-{i}")));
    let refs: Vec<&str> = to_delete.iter().map(String::as_str).collect();
    index.delete(&refs).unwrap();
    assert_eq!(index.len(), 1000, "pending delete is invisible");
    index.commit().unwrap();
    assert_eq!(index.len(), 900);
    assert!(!index.contains("doc-0150"));
    assert!(index.contains("doc-0050"));
    let r = index
        .search("document", None, 1000, &SearchOptions::default())
        .unwrap();
    assert!(
        r.hits
            .iter()
            .all(|h| !(100..200).contains(&h.external_id[4..].parse::<usize>().unwrap()))
    );
}

#[test]
fn an_empty_external_id_is_rejected() {
    let h = support::hybrid();
    let tmp = tempfile::tempdir().unwrap();
    let mut index = HybridIndex::create(
        tmp.path(),
        support::fixture_config(&h),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    let d = h.documents[0].source();
    let empty = SourceDocument {
        external_id: String::new(),
        ..d
    };
    assert!(matches!(index.add(&[empty]).unwrap_err(), Error::Schema(_)));
    index.commit().unwrap();
    assert!(index.is_empty());
}

#[test]
fn a_repeated_id_within_a_batch_keeps_the_last_document() {
    let h = support::hybrid();
    let tmp = tempfile::tempdir().unwrap();
    let a = h.documents[0].source();
    let mut b = h.documents[1].source();
    b.external_id = a.external_id.clone();
    let mut index = HybridIndex::create(
        tmp.path(),
        support::fixture_config(&h),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    index.add(&[a.clone(), b.clone()]).unwrap();
    index.commit().unwrap();
    assert_eq!(index.len(), 1);
    // The second document's text is what is searchable.
    let word = match &b.fields[&FieldName::from("text")] {
        Value::Text(t) => t.split(' ').next().unwrap().to_owned(),
        _ => unreachable!(),
    };
    let r = index
        .search(&word, None, 10, &SearchOptions::default())
        .unwrap();
    assert_eq!(
        r.hits.first().map(|h| h.external_id.as_str()),
        Some(a.external_id.as_str())
    );
}

#[test]
fn chunk_provenance_is_preserved_on_hits() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let chunked: Vec<&support::FixtureDoc> =
        h.documents.iter().filter(|d| d.chunk.is_some()).collect();
    assert!(chunked.len() >= 6);
    // Search a planted term with a large k: every chunked doc that appears carries its provenance.
    let r = index
        .search(
            "zephyr quasar obsidian marlin sextant",
            None,
            100,
            &SearchOptions::default(),
        )
        .unwrap();
    let mut seen = 0;
    for hit in &r.hits {
        if let Some(fd) = chunked.iter().find(|d| d.external_id == hit.external_id) {
            assert_eq!(hit.chunk.as_ref(), fd.chunk.as_ref(), "{}", hit.external_id);
            seen += 1;
        } else {
            assert!(hit.chunk.is_none());
        }
    }
    assert!(
        seen >= 2,
        "expected chunked hits in a broad search, saw {seen}"
    );
    // Two chunks of one parent are both present, ungrouped.
    let parents: Vec<&str> = r
        .hits
        .iter()
        .filter_map(|h| h.chunk.as_ref().map(|c| c.parent.as_str()))
        .collect();
    assert!(
        parents
            .iter()
            .any(|p| parents.iter().filter(|q| q == &p).count() >= 2),
        "no parent with two chunk hits: {parents:?}"
    );
}
