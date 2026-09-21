//! Feature 024, PR B (spec US2, US4; FR-005, FR-013): `merge` compacts the dense file, and
//! `HybridConfig::dense_compact_dead_share` makes a commit that crosses it a rewrite — decided
//! by the dense stage, recorded in the descriptor, validated at create. Offline (the table
//! embedder), over the 005 hybrid fixture.
//!
//! What is bit-identical across a merge, and what is not: the dense stage's scores are, for
//! every live row (compared here through a read-only handle on the stage with the fixture
//! embedder's query vector); the lexical stage's BM25 statistics move across a merge that
//! physically drops deleted or replaced documents (Feature 002 FR-025; pinned by the lexical
//! crate's `merge_after_deletes_moves_bm25_bits_only_when_it_drops_documents`), so fused bits
//! are compared only where the lexical merge drops nothing — a single-segment index, or one
//! without deletes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::Path;

use support::{Hybrid, build_from_fixture_with, dense_stats, fixture_embedder, fused_bits};
use xtriever_core::{Error, TextKind, VectorIndex};
use xtriever_dense::{DenseStats, FlatIndex};
use xtriever_pipeline::{HybridIndex, OpenOptions};

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

/// The dense stage's complete answer per fixture query — every live row's `(DocId, score
/// bits)`, sorted — through one read-only handle and one embedder per pass.
fn dense_answers(dir: &Path, h: &Hybrid) -> Vec<Vec<(u32, u32)>> {
    let embedder = fixture_embedder(h);
    let dense = FlatIndex::open_read_only(&dir.join("dense")).unwrap();
    h.queries
        .iter()
        .map(|q| {
            let v = embedder
                .embed(&[q.text.as_str()], TextKind::Query)
                .unwrap()
                .remove(0);
            let mut hits: Vec<(u32, u32)> = dense
                .search(&v, None, 10_000)
                .unwrap()
                .iter()
                .map(|hit| (hit.id.0, hit.score.to_bits()))
                .collect();
            hits.sort_unstable();
            hits
        })
        .collect()
}

/// Every fixture query's fused hits through the live handle — the writer's own adopted state.
fn fused_answers(index: &HybridIndex, h: &Hybrid) -> Vec<Vec<(String, u64)>> {
    h.queries
        .iter()
        .map(|q| fused_bits(index, &q.text))
        .collect()
}

fn external_ids(h: &Hybrid) -> Vec<&str> {
    h.documents.iter().map(|d| d.external_id.as_str()).collect()
}

fn counts(dir: &Path) -> (u64, u64, u64) {
    let s = dense_stats(dir);
    (s.generation, s.rows, s.live)
}

#[test]
fn merge_compacts_the_dense_file_and_keeps_every_dense_bit() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index = build_from_fixture_with(tmp.path(), &h, support::fixture_config(&h));
    let ids = external_ids(&h);
    // Replace two, delete three: five dead rows in the dense file, and a second lexical
    // segment (the replacements) for the merge to drop.
    index
        .add(&[h.documents[0].source(), h.documents[5].source()])
        .unwrap();
    index.delete(&ids[1..4]).unwrap();
    index.commit().unwrap();
    assert_eq!(
        dense_stats(tmp.path()),
        DenseStats {
            rows: 42,
            live: 37,
            dead: 5,
            generation: 0,
            ordered: false
        },
        "40 + 2 replacements appended, 5 dead"
    );
    let dense_before = dense_answers(tmp.path(), &h);
    assert!(
        dense_before.iter().all(|b| b.len() == 37),
        "every live row scored"
    );
    index.merge().unwrap();
    assert_eq!(
        dense_stats(tmp.path()),
        DenseStats {
            rows: 37,
            live: 37,
            dead: 0,
            generation: 1,
            ordered: true
        }
    );
    assert_eq!(
        vector_files(tmp.path()),
        vec!["vectors.1.bin".to_owned()],
        "exactly one generation"
    );
    assert_eq!(
        std::fs::metadata(tmp.path().join("dense/vectors.1.bin"))
            .unwrap()
            .len(),
        // Format 3: `id u32 · norm f32 · scale f32 · codes dim×i8` (Feature 026, ADR-0015).
        37 * (12 + h.dim as u64)
    );
    // The dense stage: every live row's score, bit for bit, on disk …
    assert_eq!(
        dense_answers(tmp.path(), &h),
        dense_before,
        "dense scores across the compaction"
    );
    // … and the live handle's own adopted state equals what a fresh open reads: the writer
    // handle serves the compacted generation it switched to, not a stale layout.
    let live = fused_answers(&index, &h);
    drop(index);
    let reopened = HybridIndex::open(tmp.path(), fixture_embedder(&h)).unwrap();
    assert_eq!(reopened.len(), 37);
    assert_eq!(
        fused_answers(&reopened, &h),
        live,
        "the live handle after the merge equals a fresh open"
    );
    assert_eq!(dense_answers(tmp.path(), &h), dense_before);
}

#[test]
fn merge_on_a_single_segment_compacts_the_dense_file_and_keeps_every_fused_bit() {
    // Deletes only, one lexical segment: the lexical merge has nothing to merge, so no
    // statistics move and every fused bit is identical, while the dense file drops its dead
    // rows. (With more than one segment the lexical statistics *would* move — the lexical
    // crate's own test pins that boundary.)
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index = build_from_fixture_with(tmp.path(), &h, support::fixture_config(&h));
    let ids = external_ids(&h);
    index.delete(&ids[1..4]).unwrap();
    index.commit().unwrap();
    let before = fused_answers(&index, &h);
    index.merge().unwrap();
    assert_eq!(counts(tmp.path()), (1, 37, 37));
    assert_eq!(
        fused_answers(&index, &h),
        before,
        "one segment: nothing dropped, every fused bit identical"
    );
}

#[test]
fn merge_without_deletes_keeps_every_fused_bit_and_compacts_nothing() {
    // No dead rows: the dense file is already compact (ordered, no tombstones), so `merge`
    // leaves its generation alone; the fused results are bit-identical, as before Feature 024.
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index = HybridIndex::create(
        tmp.path(),
        support::fixture_config(&h),
        fixture_embedder(&h),
    )
    .unwrap();
    for docs in h.documents.chunks(h.documents.len().div_ceil(3)) {
        let batch: Vec<_> = docs.iter().map(|d| d.source()).collect();
        index.add(&batch).unwrap();
        index.commit().unwrap();
    }
    let before = fused_answers(&index, &h);
    index.merge().unwrap();
    assert_eq!(counts(tmp.path()), (0, 40, 40));
    assert_eq!(
        fused_answers(&index, &h),
        before,
        "merge must not change any hit or score bit"
    );
}

#[test]
fn merge_with_staged_changes_is_one_dense_protocol() {
    // Staged deletes at merge time: the merge's commit rewrites the dense file directly (one
    // generation advance, no append that a compaction then supersedes), whatever the index's
    // configured share.
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index = build_from_fixture_with(tmp.path(), &h, support::fixture_config(&h));
    let ids = external_ids(&h);
    index.delete(&ids[..5]).unwrap();
    index.merge().unwrap();
    assert_eq!(
        counts(tmp.path()),
        (1, 35, 35),
        "one rewrite: generation 1, no appended dead rows"
    );
    assert_eq!(
        index.config().dense_compact_dead_share,
        None,
        "the configured share is restored"
    );
    assert_eq!(vector_files(tmp.path()), vec!["vectors.1.bin".to_owned()]);
}

#[test]
fn a_commit_that_fails_inside_merge_stops_the_merge() {
    // The merge's own commit fails after the stages committed (the id map's temporary path is
    // a directory, so its atomic write cannot complete): the merge returns that error at once
    // — no dense compaction, no lexical merge, the changes still staged — instead of carrying
    // on over an incomplete commit. Only a *completed* commit's unconfirmed dense sync defers.
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut index = build_from_fixture_with(tmp.path(), &h, support::fixture_config(&h));
    let ids = external_ids(&h);
    index.delete(&ids[..5]).unwrap();
    let blocker = tmp.path().join("ids.json.tmp");
    std::fs::create_dir(&blocker).unwrap();
    let err = index.merge().unwrap_err();
    assert!(matches!(err, Error::Io(_)), "{err}");
    std::fs::remove_dir(&blocker).unwrap();
    // The dense stage's commit ran (its rewrite is the merge's one protocol) but nothing
    // followed it: no second generation from a compaction, and the pipeline's own state is
    // the previous generation with the changes still staged.
    assert_eq!(
        counts(tmp.path()),
        (1, 35, 35),
        "the dense commit itself, nothing after it"
    );
    assert_eq!(
        index.len(),
        40,
        "the pipeline did not adopt the failed commit"
    );
    assert_eq!(
        index.config().dense_compact_dead_share,
        None,
        "the configured share is restored"
    );
    // Retrying completes the protocol from where it stopped.
    index.merge().unwrap();
    assert_eq!(index.len(), 35);
    assert_eq!(
        counts(tmp.path()),
        (1, 35, 35),
        "the retry has nothing dense left to rewrite"
    );
}

#[test]
fn a_dead_share_compacts_on_the_crossing_commit_and_not_before() {
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut config = support::fixture_config(&h);
    config.dense_compact_dead_share = Some(0.25);
    let mut index = build_from_fixture_with(tmp.path(), &h, config);
    let ids = external_ids(&h);
    index.delete(&ids[..8]).unwrap(); // 8 / 40 = 20 %
    index.commit().unwrap();
    assert_eq!(counts(tmp.path()), (0, 40, 32), "20 % is not over 25 %");
    index.delete(&ids[8..10]).unwrap(); // 10 / 40 = 25 %: not over
    index.commit().unwrap();
    assert_eq!(
        counts(tmp.path()),
        (0, 40, 30),
        "exactly 25 % is not over 25 %"
    );
    index.delete(&ids[10..11]).unwrap(); // 11 / 40 > 25 %
    index.commit().unwrap();
    assert_eq!(
        counts(tmp.path()),
        (1, 29, 29),
        "over the share: rewritten within the commit"
    );
    assert_eq!(index.len(), 29);
    drop(index);
    let reopened = HybridIndex::open(tmp.path(), fixture_embedder(&h)).unwrap();
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
    let mut index = build_from_fixture_with(tmp.path(), &h, config);
    let ids = external_ids(&h);
    index.delete(&ids[..30]).unwrap();
    index.commit().unwrap();
    assert_eq!(counts(tmp.path()), (0, 40, 10));
    index.merge().unwrap();
    assert_eq!(counts(tmp.path()), (1, 10, 10));
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
                HybridIndex::create(&dir, config, fixture_embedder(&h)).unwrap_err(),
                Error::Schema(_)
            ),
            "{bad}"
        );
    }
}

#[test]
fn a_descriptor_without_the_field_reads_as_none() {
    // Indexes written before PR B carry no `dense_compact_dead_share`; the format version is
    // unchanged and the `Option` field reads as `None`.
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut config = support::fixture_config(&h);
    config.dense_compact_dead_share = Some(0.5);
    drop(build_from_fixture_with(tmp.path(), &h, config));
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
    let reopened = HybridIndex::open(tmp.path(), fixture_embedder(&h)).unwrap();
    assert_eq!(reopened.config().dense_compact_dead_share, None);
    // A persisted value outside 0..1 is corruption at open, read-only or not.
    descriptor["dense_compact_dead_share"] = serde_json::json!(1.5);
    std::fs::write(&path, serde_json::to_vec_pretty(&descriptor).unwrap()).unwrap();
    assert!(matches!(
        HybridIndex::open(tmp.path(), fixture_embedder(&h)).unwrap_err(),
        Error::Corrupt(_)
    ));
    let ro = HybridIndex::open_with(
        tmp.path(),
        fixture_embedder(&h),
        OpenOptions {
            mapped: false,
            read_only: true,
        },
    );
    assert!(matches!(ro.unwrap_err(), Error::Corrupt(_)));
}
