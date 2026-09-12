//! One model-backed round trip: the real embedder through the pipeline (US1 scenario 5 with the
//! real fingerprint). Needs `reference/models/all-MiniLM-L6-v2`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_core::{AnalyzerId, Error, FieldDef, FieldKind, FieldName, Schema, Value};
use xtriever_dense::{LoadPath, MiniLmEmbedder};
use xtriever_pipeline::{HybridConfig, HybridIndex, SearchOptions, SourceDocument};

fn schema() -> Schema {
    Schema {
        fields: vec![
            FieldDef {
                name: "title".into(),
                kind: FieldKind::Text(AnalyzerId("standard_en".into())),
                indexed: true,
                stored: false,
                boost: 2.0,
            },
            FieldDef {
                name: "text".into(),
                kind: FieldKind::Text(AnalyzerId("standard_en".into())),
                indexed: true,
                stored: false,
                boost: 1.0,
            },
        ],
    }
}

fn doc(id: &str, title: &str, text: &str) -> SourceDocument {
    let mut fields = BTreeMap::new();
    fields.insert(FieldName::from("title"), Value::Text(title.into()));
    fields.insert(FieldName::from("text"), Value::Text(text.into()));
    SourceDocument {
        external_id: id.into(),
        fields,
        chunk: None,
    }
}

#[test]
#[ignore = "needs the model (scripts/fetch-model.sh)"]
fn real_embedder_round_trip_and_fingerprint_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let embedder = MiniLmEmbedder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    let real_fp = xtriever_core::Embedder::fingerprint(&embedder).to_owned();
    let mut index = HybridIndex::create(
        tmp.path(),
        HybridConfig::new(schema(), vec!["title".into(), "text".into()]),
        Box::new(embedder),
    )
    .unwrap();
    index
        .add(&[
            doc("capital", "France", "Paris is the capital of France."),
            doc(
                "cells",
                "Biology",
                "Mitochondria produce ATP in eukaryotic cells.",
            ),
            doc(
                "tax",
                "Finance",
                "Capital gains are taxed when shares are sold.",
            ),
            doc(
                "rust",
                "Programming",
                "Rust guarantees memory safety without garbage collection.",
            ),
            doc(
                "weather",
                "Climate",
                "Monsoon rains arrive in June across the subcontinent.",
            ),
        ])
        .unwrap();
    index.commit().unwrap();
    let r = index
        .search(
            "what is the capital city of France?",
            None,
            3,
            &SearchOptions {
                explain: true,
                ..SearchOptions::default()
            },
        )
        .unwrap();
    assert_eq!(r.hits[0].external_id, "capital");
    assert!(r.stages.degraded.is_none());
    drop(index);

    let again = HybridIndex::open(
        tmp.path(),
        Box::new(MiniLmEmbedder::load(&support::model_dir(), LoadPath::Buffered).unwrap()),
    )
    .unwrap();
    let r2 = again
        .search(
            "what is the capital city of France?",
            None,
            3,
            &SearchOptions {
                explain: true,
                ..SearchOptions::default()
            },
        )
        .unwrap();
    assert_eq!(r.hits, r2.hits);

    let h = support::hybrid();
    match HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap_err()
    {
        Error::FingerprintMismatch { index, current } => {
            assert_eq!(index, real_fp);
            assert_eq!(current, "table-fp");
        }
        other => panic!("{other:?}"),
    }
}
