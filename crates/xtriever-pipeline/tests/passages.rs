//! FR-010 — every hybrid index stores its passage text; format version 2 (ADR-0008, research
//! D7). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_core::{Error, FieldName, Value};
use xtriever_pipeline::{FORMAT_VERSION, HybridIndex, SourceDocument};

fn open(dir: &std::path::Path) -> Result<HybridIndex, Error> {
    HybridIndex::open(
        dir,
        Box::new(support::TableEmbedder::from_fixture(&support::hybrid())),
    )
}

#[test]
fn every_hit_carries_the_dense_passage_text_after_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let index = open(tmp.path()).unwrap();
    let mut seen = 0;
    for q in &h.queries {
        let r = index
            .search(&q.text, q.filter.as_ref(), 100, &support::rerank_options(0))
            .unwrap();
        for hit in &r.hits {
            let doc = h
                .documents
                .iter()
                .find(|d| d.external_id == hit.external_id)
                .unwrap();
            assert_eq!(hit.text, doc.passage, "{}", hit.external_id);
            seen += 1;
        }
    }
    assert!(seen > 20);
}

#[test]
fn replace_and_delete_update_the_store_and_the_counts_agree() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let embedder_table =
        support::TableEmbedder::from_fixture(&h).with("replaced passage text", vec![1.0; 8]);
    drop(index);
    let mut index = HybridIndex::open(tmp.path(), Box::new(embedder_table)).unwrap();
    let target = h.documents[0].clone();
    let mut fields = BTreeMap::new();
    for name in &h.dense_fields {
        fields.insert(name.clone(), Value::Text(String::new()));
    }
    fields.insert(
        h.dense_fields[0].clone(),
        Value::Text("replaced passage text".into()),
    );
    index
        .add(&[SourceDocument {
            external_id: target.external_id.clone(),
            fields,
            chunk: None,
        }])
        .unwrap();
    index.delete(&[&h.documents[1].external_id]).unwrap();
    index.commit().unwrap();
    drop(index);
    let index = open(tmp.path()).unwrap();
    assert_eq!(index.len(), h.documents.len() as u64 - 1);
    let r = index
        .search(
            "replaced passage text",
            None,
            5,
            &support::rerank_options(0),
        )
        .unwrap();
    let hit = r
        .hits
        .iter()
        .find(|x| x.external_id == target.external_id)
        .expect("replaced document is found");
    assert_eq!(hit.text, "replaced passage text");
    assert!(!index.contains(&h.documents[1].external_id));
}

#[test]
fn a_document_with_empty_text_fields_stores_an_empty_passage() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let embedder = support::TableEmbedder::from_fixture(&h).with("", vec![0.5; 8]);
    let mut index =
        HybridIndex::create(tmp.path(), support::fixture_config(&h), Box::new(embedder)).unwrap();
    let mut fields = BTreeMap::new();
    for name in &h.dense_fields {
        fields.insert(name.clone(), Value::Text(String::new()));
    }
    fields.insert(FieldName::from("source"), Value::Keyword("a".into()));
    index
        .add(&[SourceDocument {
            external_id: "blank".into(),
            fields,
            chunk: None,
        }])
        .unwrap();
    index.commit().unwrap();
    index.set_reranker(Some(Box::new(support::TableReranker::from_fn(&h, |_| 0.0))));
    // The query text must be embeddable by the table: use the empty text.
    let r = index
        .search("", None, 5, &support::rerank_options(5))
        .unwrap();
    let hit = r
        .hits
        .iter()
        .find(|x| x.external_id == "blank")
        .expect("the blank document is retrieved by the dense stage");
    assert_eq!(hit.text, "");
    assert_eq!(hit.rerank_score, Some(0.0));
}

#[test]
fn a_version_one_directory_is_refused_naming_both_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let path = tmp.path().join("xtriever-pipeline.json");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace(
            &format!("\"format_version\": {FORMAT_VERSION}"),
            "\"format_version\": 1",
        ),
    )
    .unwrap();
    std::fs::remove_file(tmp.path().join("passages.bin")).unwrap();
    match open(tmp.path()) {
        Err(Error::Corrupt(m)) => {
            assert!(m.contains("version 1"), "{m}");
            assert!(m.contains(&FORMAT_VERSION.to_string()), "{m}");
            assert!(m.contains("rebuild"), "{m}");
        }
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

#[test]
fn a_torn_store_is_refused_naming_the_file_or_the_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let path = tmp.path().join("passages.bin");
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
    match open(tmp.path()) {
        Err(Error::Corrupt(m)) => assert!(m.contains("passages.bin"), "{m}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    // A header count off by one.
    std::fs::write(&path, &bytes).unwrap();
    let header_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let header = std::str::from_utf8(&bytes[16..16 + header_len]).unwrap();
    let mut wrong = bytes.clone();
    let count_key = "\"count\":";
    let at = header.find(count_key).unwrap() + count_key.len();
    let digits_start = 16 + at;
    // Bump the first digit of the count (keeping the byte length).
    wrong[digits_start] = if wrong[digits_start] == b'9' {
        b'8'
    } else {
        wrong[digits_start] + 1
    };
    std::fs::write(&path, &wrong).unwrap();
    match open(tmp.path()) {
        Err(Error::Corrupt(m)) => assert!(m.contains("count") || m.contains("passages.bin"), "{m}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    // The marker is still refused first.
    std::fs::write(&path, &bytes).unwrap();
    std::fs::write(tmp.path().join("commit.pending"), "9").unwrap();
    match open(tmp.path()) {
        Err(Error::Corrupt(m)) => assert!(m.contains("interrupted commit"), "{m}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
}
