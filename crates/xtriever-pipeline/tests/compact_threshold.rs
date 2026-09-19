//! Feature 024, PR B (spec US2, US4; FR-005, FR-013): `merge` compacts the dense file, and
//! `HybridConfig::dense_compact_dead_share` makes a commit that crosses it a rewrite — decided
//! by the dense stage, recorded in the descriptor, validated at create. Offline (the table
//! embedder), over the 005 hybrid fixture.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::Path;

use xtriever_core::{Embedder, Error, TextKind, VectorIndex};
use xtriever_dense::FlatIndex;
use xtriever_pipeline::{HybridConfig, HybridIndex, SearchOptions};

fn embedder() -> Box<dyn xtriever_core::Embedder> {
    Box::new(support::TableEmbedder::from_fixture(&support::hybrid()))
}

fn hits(index: &HybridIndex, text: &str) -> Vec<(String, u64)> {
    index
        .search(text, None, 10, &SearchOptions::default())
        .unwrap()
        .hits
        .iter()
        .map(|h| (h.external_id.clone(), h.score.to_bits()))
        .collect()
}

/// The dense manifest's JSON header (magic · hdr_len · JSON · tombstones).
fn dense_manifest(dir: &Path) -> serde_json::Value {
    let bytes = std::fs::read(dir.join("dense/manifest.bin")).unwrap();
    let hdr_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[16..16 + hdr_len]).unwrap()
}

/// The manifest's tombstone payload — the bytes after the header.
fn dense_tombstones(dir: &Path) -> Vec<u8> {
    let bytes = std::fs::read(dir.join("dense/manifest.bin")).unwrap();
    let hdr_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    bytes[16 + hdr_len..].to_vec()
}

/// `RoaringBitmap::new().serialize_into(..)`: the no-run-container cookie and zero containers.
const EMPTY_TOMBSTONES: [u8; 8] = [0x3a, 0x30, 0, 0, 0, 0, 0, 0];

/// The row files present under `dense/`, sorted.
fn vector_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("dense"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("vectors."))
        .collect();
    names.sort();
    names
}

/// The dense stage's complete answer for a query — every live row's `(DocId, score bits)` —
/// read through a read-only handle on the stage's directory (it alters nothing), with the
/// fixture embedder's query vector: the whole map, not a fused top-k's intersection.
fn dense_stage_bits(dir: &Path, text: &str) -> Vec<(u32, u32)> {
    let h = support::hybrid();
    let embedder = support::TableEmbedder::from_fixture(&h);
    let q = embedder.embed(&[text], TextKind::Query).unwrap().remove(0);
    let dense = FlatIndex::open_read_only(&dir.join("dense")).unwrap();
    let mut hits: Vec<(u32, u32)> = dense
        .search(&q, None, 10_000)
        .unwrap()
        .iter()
        .map(|h| (h.id.0, h.score.to_bits()))
        .collect();
    hits.sort_unstable();
    hits
}

fn build(dir: &Path, config: HybridConfig) -> HybridIndex {
    let h = support::hybrid();
    let mut index = HybridIndex::create(dir, config, embedder()).unwrap();
    let batch: Vec<_> = h.documents.iter().map(|d| d.source()).collect();
    index.add(&batch).unwrap();
    index.commit().unwrap();
    index
}

/// Every hit's dense score bits, by external id, for one query (`explain` on).
fn dense_bits(index: &HybridIndex, text: &str) -> Vec<(String, Option<u32>)> {
    let options = SearchOptions {
        explain: true,
        ..SearchOptions::default()
    };
    let mut hits: Vec<(String, Option<u32>)> = index
        .search(text, None, 10, &options)
        .unwrap()
        .hits
        .iter()
        .map(|h| {
            (
                h.external_id.clone(),
                h.explain
                    .as_ref()
                    .and_then(|e| e.dense_score)
                    .map(f32::to_bits),
            )
        })
        .collect();
    hits.sort();
    hits
}

#[test]
fn merge_compacts_the_dense_file_and_keeps_every_bit() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index = build(tmp.path(), support::fixture_config(&h));
    let ids: Vec<&str> = h.documents.iter().map(|d| d.external_id.as_str()).collect();
    // Replace two, delete three: five dead rows in the dense file.
    index
        .add(&[h.documents[0].source(), h.documents[5].source()])
        .unwrap();
    index.delete(&ids[1..4]).unwrap();
    index.commit().unwrap();
    let m = dense_manifest(tmp.path());
    assert_eq!(
        m["rows"].as_u64().unwrap(),
        42,
        "40 + 2 replacements appended"
    );
    assert_eq!(m["live"].as_u64().unwrap(), 37);
    assert_eq!(m["generation"].as_u64().unwrap(), 0);
    // The dense stage's scores cannot move across a compaction (ADR-0013): every hit's dense
    // score, by id. The *fused* bits are not compared here because this sequence *replaces*
    // documents: a lexical-only probe (the 002 fixture, `TantivyIndex` alone, no dense stage)
    // showed a merge after replacements moves BM25 bits (the merge garbage-collects the
    // replaced documents and the backend's statistics change) while a merge after plain
    // deletes does not — the lexical stage's behaviour on `main`, which this branch does not
    // touch. Spec FR-005 is scoped accordingly; the no-replacement case stays bit-identical
    // (`merge_without_deletes_keeps_every_fused_bit_and_compacts_nothing` and `open_with.rs`).
    // The complete dense answer per query — every live row — through a read-only handle on
    // the stage, plus the fused hits' explained dense scores.
    let before: Vec<Vec<(u32, u32)>> = h
        .queries
        .iter()
        .map(|q| dense_stage_bits(tmp.path(), &q.text))
        .collect();
    assert!(
        before.iter().all(|b| b.len() == 37),
        "every live row scored"
    );
    let explained_before: Vec<Vec<(String, Option<u32>)>> = h
        .queries
        .iter()
        .map(|q| dense_bits(&index, &q.text))
        .collect();
    index.merge().unwrap();
    let m = dense_manifest(tmp.path());
    assert_eq!(m["rows"].as_u64().unwrap(), 37, "live rows only");
    assert_eq!(m["live"].as_u64().unwrap(), 37);
    assert_eq!(m["generation"].as_u64().unwrap(), 1);
    assert!(m["ordered"].as_bool().unwrap());
    assert_eq!(
        vector_files(tmp.path()),
        vec!["vectors.1.bin".to_owned()],
        "exactly one generation"
    );
    assert_eq!(
        dense_tombstones(tmp.path()),
        EMPTY_TOMBSTONES,
        "an empty tombstone set"
    );
    assert_eq!(
        std::fs::metadata(tmp.path().join("dense/vectors.1.bin"))
            .unwrap()
            .len(),
        37 * (8 + 4 * h.dim as u64)
    );
    let after: Vec<Vec<(u32, u32)>> = h
        .queries
        .iter()
        .map(|q| dense_stage_bits(tmp.path(), &q.text))
        .collect();
    assert_eq!(
        after, before,
        "every live row's dense score, bit for bit, across the compaction"
    );
    let explained_after: Vec<Vec<(String, Option<u32>)>> = h
        .queries
        .iter()
        .map(|q| dense_bits(&index, &q.text))
        .collect();
    for (b, a) in explained_before.iter().zip(&explained_after) {
        for (id, bits) in b {
            if let Some((_, x)) = a.iter().find(|(i, _)| i == id) {
                assert_eq!(x, bits, "explained dense score of {id}");
            }
        }
    }
    drop(index);
    let reopened = HybridIndex::open(tmp.path(), embedder()).unwrap();
    assert_eq!(reopened.len(), 37);
    let again: Vec<Vec<(u32, u32)>> = h
        .queries
        .iter()
        .map(|q| dense_stage_bits(tmp.path(), &q.text))
        .collect();
    assert_eq!(again, before);
}

#[test]
fn merge_after_plain_deletes_keeps_every_fused_bit_and_compacts_the_dense_file() {
    // Deletes without replacements: the lexical statistics do not move across the merge, so
    // every fused bit is identical, while the dense file drops its dead rows.
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index = build(tmp.path(), support::fixture_config(&h));
    let ids: Vec<&str> = h.documents.iter().map(|d| d.external_id.as_str()).collect();
    index.delete(&ids[1..4]).unwrap();
    index.commit().unwrap();
    let before: Vec<Vec<(String, u64)>> = h.queries.iter().map(|q| hits(&index, &q.text)).collect();
    index.merge().unwrap();
    let m = dense_manifest(tmp.path());
    assert_eq!(
        (
            m["generation"].as_u64().unwrap(),
            m["rows"].as_u64().unwrap(),
            m["live"].as_u64().unwrap()
        ),
        (1, 37, 37)
    );
    let after: Vec<Vec<(String, u64)>> = h.queries.iter().map(|q| hits(&index, &q.text)).collect();
    assert_eq!(
        after, before,
        "merge after plain deletes must not change any hit or score bit"
    );
}

#[test]
fn merge_without_deletes_keeps_every_fused_bit_and_compacts_nothing() {
    // No dead rows: the dense file is already compact (ordered, no tombstones), so `merge`
    // leaves its generation alone; the fused results are bit-identical, as before Feature 024.
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index =
        HybridIndex::create(tmp.path(), support::fixture_config(&h), embedder()).unwrap();
    for docs in h.documents.chunks(h.documents.len().div_ceil(3)) {
        let batch: Vec<_> = docs.iter().map(|d| d.source()).collect();
        index.add(&batch).unwrap();
        index.commit().unwrap();
    }
    let before: Vec<Vec<(String, u64)>> = h.queries.iter().map(|q| hits(&index, &q.text)).collect();
    index.merge().unwrap();
    let m = dense_manifest(tmp.path());
    assert_eq!(
        (
            m["generation"].as_u64().unwrap(),
            m["rows"].as_u64().unwrap()
        ),
        (0, 40)
    );
    let after: Vec<Vec<(String, u64)>> = h.queries.iter().map(|q| hits(&index, &q.text)).collect();
    assert_eq!(after, before, "merge must not change any hit or score bit");
}

#[test]
fn a_dead_share_compacts_on_the_crossing_commit_and_not_before() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut config = support::fixture_config(&h);
    config.dense_compact_dead_share = Some(0.25);
    let mut index = build(tmp.path(), config);
    let ids: Vec<&str> = h.documents.iter().map(|d| d.external_id.as_str()).collect();
    index.delete(&ids[..8]).unwrap(); // 8 / 40 = 20 %
    index.commit().unwrap();
    let m = dense_manifest(tmp.path());
    assert_eq!(
        (
            m["generation"].as_u64().unwrap(),
            m["rows"].as_u64().unwrap()
        ),
        (0, 40),
        "20 % is not over 25 %"
    );
    index.delete(&ids[8..10]).unwrap(); // 10 / 40 = 25 %: not over
    index.commit().unwrap();
    assert_eq!(
        dense_manifest(tmp.path())["generation"].as_u64().unwrap(),
        0
    );
    index.delete(&ids[10..11]).unwrap(); // 11 / 40 > 25 %
    index.commit().unwrap();
    let m = dense_manifest(tmp.path());
    assert_eq!(
        (
            m["generation"].as_u64().unwrap(),
            m["rows"].as_u64().unwrap(),
            m["live"].as_u64().unwrap()
        ),
        (1, 29, 29)
    );
    assert_eq!(index.len(), 29);
    drop(index);
    let reopened = HybridIndex::open(tmp.path(), embedder()).unwrap();
    assert_eq!(
        reopened.config().dense_compact_dead_share,
        Some(0.25),
        "recorded in the descriptor"
    );
    assert_eq!(reopened.len(), 29);
}

#[test]
fn without_a_dead_share_nothing_compacts_until_merge() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let config = support::fixture_config(&h);
    assert_eq!(config.dense_compact_dead_share, None, "the default");
    let mut index = build(tmp.path(), config);
    let ids: Vec<&str> = h.documents.iter().map(|d| d.external_id.as_str()).collect();
    index.delete(&ids[..30]).unwrap();
    index.commit().unwrap();
    let m = dense_manifest(tmp.path());
    assert_eq!(
        (
            m["generation"].as_u64().unwrap(),
            m["rows"].as_u64().unwrap(),
            m["live"].as_u64().unwrap()
        ),
        (0, 40, 10)
    );
    index.merge().unwrap();
    let m = dense_manifest(tmp.path());
    assert_eq!(
        (
            m["generation"].as_u64().unwrap(),
            m["rows"].as_u64().unwrap()
        ),
        (1, 10)
    );
}

#[test]
fn a_dead_share_outside_zero_to_one_is_a_schema_error_at_create() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    for bad in [1.5f32, -0.1, f32::NAN] {
        let mut config = support::fixture_config(&h);
        config.dense_compact_dead_share = Some(bad);
        let dir = tmp.path().join(format!("{bad}"));
        assert!(
            matches!(
                HybridIndex::create(&dir, config, embedder()).unwrap_err(),
                Error::Schema(_)
            ),
            "{bad}"
        );
    }
}

#[test]
fn a_descriptor_without_the_field_reads_as_none() {
    // Indexes written before PR B carry no `dense_compact_dead_share`; the format version is
    // unchanged and the field defaults (as `rerank_mode` did in Feature 015).
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut config = support::fixture_config(&h);
    config.dense_compact_dead_share = Some(0.5);
    drop(build(tmp.path(), config));
    let path = tmp.path().join("xtriever-pipeline.json");
    let mut descriptor: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        descriptor["dense_compact_dead_share"],
        serde_json::json!(0.5)
    );
    descriptor
        .as_object_mut()
        .unwrap()
        .remove("dense_compact_dead_share");
    std::fs::write(&path, serde_json::to_vec_pretty(&descriptor).unwrap()).unwrap();
    let reopened = HybridIndex::open(tmp.path(), embedder()).unwrap();
    assert_eq!(reopened.config().dense_compact_dead_share, None);
    // A persisted value outside 0..1 is corruption at open, read-only or not.
    descriptor["dense_compact_dead_share"] = serde_json::json!(1.5);
    std::fs::write(&path, serde_json::to_vec_pretty(&descriptor).unwrap()).unwrap();
    assert!(matches!(
        HybridIndex::open(tmp.path(), embedder()).unwrap_err(),
        Error::Corrupt(_)
    ));
    let ro = HybridIndex::open_with(
        tmp.path(),
        embedder(),
        xtriever_pipeline::OpenOptions {
            mapped: false,
            read_only: true,
        },
    );
    assert!(matches!(ro.unwrap_err(), Error::Corrupt(_)));
}
