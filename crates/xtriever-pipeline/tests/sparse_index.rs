//! Feature 027 (User Story 1): a sparse index — created with the option, searched with the
//! expansion, carrying its own query side, refusing what it must (contract `sparse-option.md`).
//!
//! The refusals of the configuration need no model and run in CI. Everything that builds a
//! sparse index needs the pinned encoder (`scripts/fetch-model.sh --manifest
//! reference/models/manifest-sparse-doc-v3.json`) and is ignored by default:
//!
//!     cargo nextest run -p xtriever-pipeline --run-ignored all -E 'binary(sparse_index)'
//!
//! The dense side is the fixture's table embedder (Feature 005), so only the encoder is real.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use xtriever_core::{AnalyzerId, Error, FieldDef, FieldKind, FieldName, Value};
use xtriever_dense::LoadPath;
use xtriever_dense::model::{PINNED_SPARSE, SPARSE_IDENTITY, SPARSE_MODEL_NAME};
use xtriever_dense::sparse::{Expansion, MAX_SCALE, SparseEncoder};
use xtriever_pipeline::{
    FORMAT_VERSION, HybridConfig, HybridIndex, SPARSE_FIELD, SPARSE_FORMAT_VERSION, SourceDocument,
    SparseOption,
};

use support::{FixtureDoc, Hybrid, TableEmbedder, fused_bits, hybrid};

/// The git-ignored encoder directory, overridable with `XTRIEVER_SPARSE_MODEL_DIR`.
fn encoder_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_SPARSE_MODEL_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference/models/opensearch-neural-sparse-encoding-doc-v3-distill")
        },
        PathBuf::from,
    )
}

fn encoder() -> SparseEncoder {
    SparseEncoder::load(&encoder_dir(), LoadPath::Buffered).expect("load the sparse encoder")
}

fn sparse_config(h: &Hybrid, option: SparseOption) -> HybridConfig {
    HybridConfig {
        sparse: Some(option),
        ..support::fixture_config(h)
    }
}

fn schema_error(result: xtriever_core::Result<HybridIndex>) -> String {
    match result {
        Err(Error::Schema(m)) => m,
        Err(other) => panic!("expected Schema, got {other:?}"),
        Ok(_) => panic!("expected Schema, got an index"),
    }
}

fn docs(h: &Hybrid) -> Vec<SourceDocument> {
    h.documents.iter().map(FixtureDoc::source).collect()
}

/// Every fixture query's fused hits, as `(external id, score bits)`.
fn all_bits(index: &HybridIndex, h: &Hybrid) -> Vec<Vec<(String, u64)>> {
    h.queries
        .iter()
        .map(|q| fused_bits(index, &q.text))
        .collect()
}

// ── the configuration, without a model ──────────────────────────────────────────────────────

/// A scale outside `1..=MAX_SCALE` or a boost that is not finite and positive is refused by
/// value, by either constructor, before anything is written.
#[test]
fn an_invalid_option_is_refused_by_value() {
    let h = hybrid();
    let bad = [
        (
            SparseOption {
                scale: 0,
                boost: 1.0,
            },
            "0",
        ),
        (
            SparseOption {
                scale: MAX_SCALE + 1,
                boost: 1.0,
            },
            &(MAX_SCALE + 1).to_string()[..],
        ),
        (
            SparseOption {
                scale: 10,
                boost: 0.0,
            },
            "boost",
        ),
        (
            SparseOption {
                scale: 10,
                boost: -1.0,
            },
            "boost",
        ),
        (
            SparseOption {
                scale: 10,
                boost: f32::NAN,
            },
            "boost",
        ),
        (
            SparseOption {
                scale: 10,
                boost: f32::INFINITY,
            },
            "boost",
        ),
    ];
    for (option, needle) in bad {
        let tmp = tempfile::tempdir().unwrap();
        let m = schema_error(HybridIndex::create(
            tmp.path(),
            sparse_config(&h, option),
            support::fixture_embedder(&h),
        ));
        assert!(m.contains(needle), "{option:?}: {m}");
        assert_eq!(
            std::fs::read_dir(tmp.path()).unwrap().count(),
            0,
            "{option:?}: nothing written"
        );
    }
}

/// A sparse index reserves `_sparse`; a user field of that name is refused.
#[test]
fn a_user_field_named_sparse_is_refused() {
    let h = hybrid();
    let mut config = sparse_config(&h, SparseOption::default());
    config.schema.fields.push(FieldDef {
        name: FieldName::from(SPARSE_FIELD),
        kind: FieldKind::Text(AnalyzerId("standard".into())),
        indexed: true,
        stored: false,
        boost: 1.0,
    });
    let tmp = tempfile::tempdir().unwrap();
    let m = schema_error(HybridIndex::create(
        tmp.path(),
        config,
        support::fixture_embedder(&h),
    ));
    assert!(m.contains(SPARSE_FIELD) && m.contains("reserved"), "{m}");
}

/// `create` cannot build a sparse index — it has no encoder — and says which call can.
#[test]
fn create_refuses_the_option_and_names_create_sparse() {
    let h = hybrid();
    let tmp = tempfile::tempdir().unwrap();
    let m = schema_error(HybridIndex::create(
        tmp.path(),
        sparse_config(&h, SparseOption::default()),
        support::fixture_embedder(&h),
    ));
    assert!(m.contains("create_sparse"), "{m}");
}

/// Without the option an index is what it was: format version 2, no record, no `sparse/`.
#[test]
fn an_index_without_the_option_is_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, index) = support::build_from_fixture(tmp.path());
    assert_eq!(index.format_version(), FORMAT_VERSION);
    assert!(index.sparse().is_none());
    assert!(index.config().sparse.is_none());
    assert!(!tmp.path().join("sparse").exists());
    let text = std::fs::read_to_string(tmp.path().join("xtriever-pipeline.json")).unwrap();
    assert!(!text.contains("sparse"), "{text}");
}

// ── a sparse index, with the encoder ────────────────────────────────────────────────────────

fn build_sparse(dir: &Path, h: &Hybrid) -> HybridIndex {
    let mut index = HybridIndex::create_sparse(
        dir,
        sparse_config(h, SparseOption::default()),
        support::fixture_embedder(h),
        encoder(),
    )
    .expect("create_sparse");
    index.add(&docs(h)).expect("add");
    index.commit().expect("commit");
    index
}

#[test]
#[ignore = "needs the sparse encoder"]
fn create_sparse_records_the_option_and_copies_the_query_side() {
    let h = hybrid();
    let tmp = tempfile::tempdir().unwrap();
    let index = HybridIndex::create_sparse(
        tmp.path(),
        sparse_config(&h, SparseOption::default()),
        support::fixture_embedder(&h),
        encoder(),
    )
    .unwrap();
    assert_eq!(index.format_version(), SPARSE_FORMAT_VERSION);
    assert_eq!(index.config().sparse, Some(SparseOption::default()));
    let record = index.sparse().expect("a sparse record");
    assert_eq!((record.scale, record.boost), (10, 1.0));
    assert_eq!(record.field, SPARSE_FIELD);
    assert_eq!(record.encoder, SPARSE_IDENTITY);
    assert_eq!(record.tokenizer_sha256, PINNED_SPARSE.files[1].sha256);
    assert_eq!(record.table_sha256, PINNED_SPARSE.files[3].sha256);

    // The query side, byte for byte: the device needs nothing but the index (research D6).
    for (copy, pinned) in [("tokenizer.json", 1), ("query-table.json", 3)] {
        assert_eq!(
            std::fs::read(tmp.path().join("sparse").join(copy)).unwrap(),
            std::fs::read(encoder_dir().join(PINNED_SPARSE.files[pinned].name)).unwrap(),
            "{copy}"
        );
    }
    let text = std::fs::read_to_string(tmp.path().join("xtriever-pipeline.json")).unwrap();
    assert!(text.starts_with("{\n  \"format_version\": 3"), "{text}");
    assert!(text.contains("\"sparse\": {"), "{text}");
}

/// The expansion changes what is found, and an opened index — with no encoder — searches
/// exactly as the handle that built it.
#[test]
#[ignore = "needs the sparse encoder"]
fn a_sparse_index_searches_with_its_expansion_and_reopens_without_the_encoder() {
    let h = hybrid();
    let plain_dir = tempfile::tempdir().unwrap();
    let (_, plain) = support::build_from_fixture(plain_dir.path());
    let sparse_dir = tempfile::tempdir().unwrap();
    let sparse = build_sparse(sparse_dir.path(), &h);

    let before = all_bits(&sparse, &h);
    assert_ne!(
        before,
        all_bits(&plain, &h),
        "the expansion changed no result for any fixture query"
    );
    drop(sparse);

    let reopened = HybridIndex::open(sparse_dir.path(), support::fixture_embedder(&h)).unwrap();
    assert_eq!(reopened.format_version(), SPARSE_FORMAT_VERSION);
    assert_eq!(reopened.config().sparse, Some(SparseOption::default()));
    assert_eq!(all_bits(&reopened, &h), before);
}

/// Adding needs the encoder; `add_embedded` has no expansion to write; a document may not
/// supply the reserved field itself.
#[test]
#[ignore = "needs the sparse encoder"]
fn a_sparse_index_refuses_additions_it_cannot_expand() {
    let h = hybrid();
    let tmp = tempfile::tempdir().unwrap();
    drop(build_sparse(tmp.path(), &h));
    let mut index = HybridIndex::open(tmp.path(), support::fixture_embedder(&h)).unwrap();
    let doc = h.documents[0].source();

    match index.add(std::slice::from_ref(&doc)) {
        Err(Error::Model { model, message }) => {
            assert_eq!(model, SPARSE_MODEL_NAME);
            assert!(message.contains("set_sparse_encoder"), "{message}");
        }
        other => panic!("expected Model, got {other:?}"),
    }
    match index.add_embedded(&[(doc.clone(), h.documents[0].vector.clone())]) {
        Err(Error::Schema(m)) => assert!(m.contains("add_encoded"), "{m}"),
        other => panic!("expected Schema, got {other:?}"),
    }

    index.set_sparse_encoder(Some(encoder()));
    let mut forged = doc;
    forged.fields.insert(
        FieldName::from(SPARSE_FIELD),
        Value::Text("s2003 s2003".into()),
    );
    match index.add(&[forged]) {
        Err(Error::Schema(m)) => assert!(m.contains(SPARSE_FIELD), "{m}"),
        other => panic!("expected Schema, got {other:?}"),
    }
}

/// `add_encoded` with the encoder's own expansions builds the same index `add` does; an
/// expansion the encoder could not have produced is refused; an index without the option
/// refuses `add_encoded` altogether.
#[test]
#[ignore = "needs the sparse encoder"]
fn add_encoded_builds_what_add_builds() {
    let h = hybrid();
    let by_add = tempfile::tempdir().unwrap();
    let expected = all_bits(&build_sparse(by_add.path(), &h), &h);

    let enc = encoder();
    let embedder = TableEmbedder::from_fixture(&h);
    let encoded: Vec<(SourceDocument, Vec<f32>, Expansion)> = h
        .documents
        .iter()
        .map(|d| {
            let e = enc.encode(&d.passage).unwrap();
            (d.source(), embedder_vector(&embedder, &d.passage), e)
        })
        .collect();
    let by_cache = tempfile::tempdir().unwrap();
    let mut index = HybridIndex::create_sparse(
        by_cache.path(),
        sparse_config(&h, SparseOption::default()),
        support::fixture_embedder(&h),
        enc,
    )
    .unwrap();
    index.add_encoded(&encoded).unwrap();
    index.commit().unwrap();
    assert_eq!(all_bits(&index, &h), expected);

    let (doc, vector, _) = encoded[0].clone();
    let duplicated = Expansion {
        entries: vec![(2003, 0.5), (2003, 0.5)],
        truncated: false,
    };
    match index.add_encoded(&[(doc.clone(), vector.clone(), duplicated)]) {
        Err(Error::Schema(m)) => assert!(m.contains("2003"), "{m}"),
        other => panic!("expected Schema, got {other:?}"),
    }

    let plain_dir = tempfile::tempdir().unwrap();
    let (_, mut plain) = support::build_from_fixture(plain_dir.path());
    match plain.add_encoded(&[(doc, vector, Expansion::default())]) {
        Err(Error::Schema(m)) => assert!(m.contains("sparse"), "{m}"),
        other => panic!("expected Schema, got {other:?}"),
    }
}

fn embedder_vector(embedder: &TableEmbedder, text: &str) -> Vec<f32> {
    use xtriever_core::{Embedder, TextKind};
    embedder
        .embed(&[text], TextKind::Passage)
        .unwrap()
        .pop()
        .unwrap()
}

/// A document past the encoder's window is expanded from its first 512 tokens and counted.
#[test]
#[ignore = "needs the sparse encoder"]
fn truncated_documents_are_counted() {
    let h = hybrid();
    let long_text = "saltmarsh lantern harbour ".repeat(300);
    let tmp = tempfile::tempdir().unwrap();
    let embedder = TableEmbedder::from_fixture(&h).with(&long_text, vec![0.5; 8]);
    let mut index = HybridIndex::create_sparse(
        tmp.path(),
        sparse_config(&h, SparseOption::default()),
        Box::new(embedder),
        encoder(),
    )
    .unwrap();
    index.add(&docs(&h)).unwrap();
    assert_eq!(
        index.sparse_truncated(),
        0,
        "the fixture's passages are short"
    );
    let mut fields = BTreeMap::new();
    fields.insert(FieldName::from("text"), Value::Text(long_text));
    index
        .add(&[SourceDocument {
            external_id: "long".into(),
            fields,
            chunk: None,
        }])
        .unwrap();
    assert_eq!(index.sparse_truncated(), 1);
}

/// The stored query side is verified at open: an altered tokenizer, or a descriptor that lost
/// its record, is corruption named as such.
#[test]
#[ignore = "needs the sparse encoder"]
fn an_altered_query_side_or_record_is_refused_at_open() {
    let h = hybrid();
    let tmp = tempfile::tempdir().unwrap();
    drop(build_sparse(tmp.path(), &h));

    let tokenizer = tmp.path().join("sparse/tokenizer.json");
    let original = std::fs::read(&tokenizer).unwrap();
    let mut altered = original.clone();
    altered.push(b'\n');
    std::fs::write(&tokenizer, &altered).unwrap();
    match HybridIndex::open(tmp.path(), support::fixture_embedder(&h)) {
        Err(Error::Corrupt(m)) => {
            assert!(m.contains("tokenizer.json"), "{m}");
            assert!(m.contains(PINNED_SPARSE.files[1].sha256), "{m}");
        }
        other => panic!("expected Corrupt, got {other:?}"),
    }
    std::fs::write(&tokenizer, &original).unwrap();

    let descriptor = tmp.path().join("xtriever-pipeline.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&descriptor).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("sparse");
    std::fs::write(&descriptor, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    match HybridIndex::open(tmp.path(), support::fixture_embedder(&h)) {
        Err(Error::Corrupt(m)) => assert!(m.contains('3') && m.contains("sparse"), "{m}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
}
