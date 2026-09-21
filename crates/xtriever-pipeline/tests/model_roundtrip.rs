//! One model-backed round trip: the real embedder through the pipeline (US1 scenario 5 with the
//! real fingerprint). Needs `reference/models/all-MiniLM-L6-v2-q8` (Feature 026: the eight-bit
//! artefact every tool defaults to; `XTRIEVER_MODEL_DIR` names another).
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

/// Feature 006: the real cross-encoder attached to the real embedder's index.
#[test]
#[ignore = "needs both models (scripts/fetch-model.sh, scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json)"]
fn real_reranker_over_the_real_embedder_index() {
    use xtriever_core::Reranker;
    let tmp = tempfile::tempdir().unwrap();
    let embedder = MiniLmEmbedder::load(&support::model_dir(), LoadPath::Buffered).unwrap();
    let mut index = HybridIndex::create(
        tmp.path(),
        HybridConfig::new(schema(), vec!["title".into(), "text".into()]),
        Box::new(embedder),
    )
    .unwrap();
    index
        .add(&[
            doc(
                "berlin",
                "Berlin",
                "Berlin has a population of 3.5 million people.",
            ),
            doc("paris", "Paris", "Paris is the capital of France."),
            doc("cats", "Cats", "Cats sleep for most of the day."),
            doc("rust", "Rust", "Rust is a systems programming language."),
            doc(
                "tea",
                "Tea",
                "Green tea is brewed at a lower temperature than black tea.",
            ),
        ])
        .unwrap();
    index.commit().unwrap();
    let rerank_dir = std::env::var_os("XTRIEVER_RERANK_MODEL_DIR").map_or_else(
        || {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference/models/ms-marco-MiniLM-L-6-v2-q8")
        },
        std::path::PathBuf::from,
    );
    let reranker =
        xtriever_rerank::MiniLmCrossEncoder::load(&rerank_dir, xtriever_rerank::LoadPath::Buffered)
            .unwrap();
    let model_id = reranker.model_id().to_owned();
    index.set_reranker(Some(Box::new(reranker)));
    assert_eq!(index.reranker().unwrap().model_id(), model_id);
    let r = index
        .search(
            "how many people live in berlin",
            None,
            5,
            &SearchOptions {
                rerank_depth: Some(5),
                explain: true,
                ..SearchOptions::default()
            },
        )
        .unwrap();
    assert_eq!(r.stages.rerank.as_ref().unwrap().scored, 5);
    assert_eq!(r.hits[0].external_id, "berlin");
    assert!(
        r.hits
            .iter()
            .all(|h| h.rerank_score.is_some_and(f32::is_finite))
    );
    assert_eq!(r.hits[0].explain.as_ref().unwrap().rerank_rank, Some(1));
}
