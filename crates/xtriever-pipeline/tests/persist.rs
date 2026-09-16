//! US1 scenarios 4–5 — persistence, identity checks, partial commits (spec FR-003, FR-005,
//! FR-006; SC-003). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::Error;
use xtriever_pipeline::{FORMAT_VERSION, HybridIndex, SearchOptions};

#[test]
fn reopened_index_gives_identical_hits_and_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let opts = SearchOptions {
        explain: true,
        ..SearchOptions::default()
    };
    let before: Vec<_> = h
        .queries
        .iter()
        .map(|q| {
            index
                .search(&q.text, q.filter.as_ref(), 100, &opts)
                .unwrap()
        })
        .collect();
    drop(index);
    let reopened = HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    assert_eq!(reopened.len(), h.documents.len() as u64);
    for d in &h.documents {
        assert!(reopened.contains(&d.external_id));
    }
    for (q, want) in h.queries.iter().zip(&before) {
        let got = reopened
            .search(&q.text, q.filter.as_ref(), 100, &opts)
            .unwrap();
        assert_eq!(got.hits, want.hits, "{}: reopen changed hits", q.id);
        assert_eq!(got.stages, want.stages);
    }
    assert_eq!(reopened.config().rrf_k, 60);
    assert_eq!(reopened.config().candidate_depth, 100);
}

#[test]
fn a_different_embedder_fingerprint_is_refused_naming_both() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let other = support::TableEmbedder::from_fixture(&h).with_fingerprint("other-fp");
    match HybridIndex::open(tmp.path(), Box::new(other)).unwrap_err() {
        Error::FingerprintMismatch { index, current } => {
            assert_eq!(index, "table-fp");
            assert_eq!(current, "other-fp");
        }
        other => panic!("expected FingerprintMismatch, got {other:?}"),
    }
}

#[test]
fn a_future_descriptor_version_is_corrupt_naming_both_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let path = tmp.path().join("xtriever-pipeline.json");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(&format!("\"format_version\": {FORMAT_VERSION}")),
        "{text}"
    );
    std::fs::write(
        &path,
        text.replace(
            &format!("\"format_version\": {FORMAT_VERSION}"),
            // A version this build does not read (2 became the current version in 006).
            "\"format_version\": 7",
        ),
    )
    .unwrap();
    match HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap_err()
    {
        Error::Corrupt(msg) => {
            assert!(
                msg.contains('7') && msg.contains(&FORMAT_VERSION.to_string()),
                "{msg}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_descriptor_schema_that_disagrees_with_the_lexical_index_is_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let path = tmp.path().join("xtriever-pipeline.json");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"boost\": 2.0"), "{text}");
    std::fs::write(&path, text.replacen("\"boost\": 2.0", "\"boost\": 3.0", 1)).unwrap();
    assert!(matches!(
        HybridIndex::open(
            tmp.path(),
            Box::new(support::TableEmbedder::from_fixture(&h))
        )
        .unwrap_err(),
        Error::Corrupt(_)
    ));
}

/// Reproduce the on-disk state a crash leaves after the lexical stage has committed but before
/// the dense stage, id map and descriptor have: the commit marker is present and the lexical
/// sub-index holds the new generation. Done through the lexical stage's own handle and the
/// filesystem — the pipeline exposes no way to commit one stage (review round 1 #3).
fn crash_after_lexical_commit(dir: &std::path::Path, docs: &[xtriever_core::Document]) {
    use xtriever_core::LexicalIndex;
    std::fs::write(dir.join("commit.pending"), "2").unwrap();
    let mut lexical = xtriever_lexical::TantivyIndex::open(&dir.join("lexical")).unwrap();
    lexical.add(docs).unwrap();
    lexical.commit().unwrap();
}

#[test]
fn a_partial_commit_that_changes_counts_is_refused_at_open() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let n = h.documents.len();
    drop(index);
    // Three new documents reach the lexical stage only.
    let extra: Vec<xtriever_core::Document> = (0..3)
        .map(|i| xtriever_core::Document {
            id: xtriever_core::DocId((n + i) as u32),
            fields: h.documents[i].fields.clone(),
            chunk: None,
        })
        .collect();
    crash_after_lexical_commit(tmp.path(), &extra);
    match HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap_err()
    {
        Error::Corrupt(msg) => assert!(
            msg.contains("interrupted commit") && msg.contains("commit.pending"),
            "{msg}"
        ),
        other => panic!("{other:?}"),
    }
    // Without the marker the count check still catches this state (second line of defence).
    std::fs::remove_file(tmp.path().join("commit.pending")).unwrap();
    match HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap_err()
    {
        Error::Corrupt(msg) => {
            assert!(msg.to_lowercase().contains("partial"), "{msg}");
            assert!(
                msg.contains(&n.to_string()) && msg.contains(&(n + 3).to_string()),
                "{msg}"
            );
        }
        other => panic!("{other:?}"),
    }
}

/// A replacement that crashed after the lexical commit leaves every live count unchanged — only
/// the marker can tell (review round 1 #1).
#[test]
fn a_same_cardinality_partial_commit_is_refused_at_open() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    // Replace document 0 (same internal id, new text) in the lexical stage only.
    let mut fields = h.documents[0].fields.clone();
    fields.insert(
        "text".into(),
        xtriever_core::Value::Text("replaced text only in the lexical stage".into()),
    );
    let replaced = xtriever_core::Document {
        id: xtriever_core::DocId(0),
        fields,
        chunk: None,
    };
    crash_after_lexical_commit(tmp.path(), std::slice::from_ref(&replaced));
    match HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap_err()
    {
        Error::Corrupt(msg) => assert!(msg.contains("interrupted commit"), "{msg}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_completed_commit_leaves_no_marker_and_the_next_generation_opens() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    assert!(!tmp.path().join("commit.pending").exists());
    index
        .delete(&[h.documents[0].external_id.as_str()])
        .unwrap();
    index.commit().unwrap();
    assert!(!tmp.path().join("commit.pending").exists());
    drop(index);
    let reopened = HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    assert_eq!(reopened.len(), h.documents.len() as u64 - 1);
}

#[test]
fn a_stale_handle_keeps_its_snapshot_until_reopened() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let n = index.len();
    let stale = HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    drop(index);
    let mut writer = HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    writer
        .delete(&[h.documents[0].external_id.as_str()])
        .unwrap();
    writer.commit().unwrap();
    assert_eq!(writer.len(), n - 1);
    assert_eq!(stale.len(), n, "stale handle keeps its snapshot");
    assert!(stale.contains(&h.documents[0].external_id));
    let fresh = HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    assert_eq!(fresh.len(), n - 1);
}

#[test]
fn create_refuses_a_non_empty_directory_and_writes_an_empty_generation() {
    let h = support::hybrid();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("something"), b"x").unwrap();
    assert!(matches!(
        HybridIndex::create(
            tmp.path(),
            support::fixture_config(&h),
            Box::new(support::TableEmbedder::from_fixture(&h))
        )
        .unwrap_err(),
        Error::Corrupt(_)
    ));
    let dir = tmp.path().join("fresh");
    let index = HybridIndex::create(
        &dir,
        support::fixture_config(&h),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    drop(index);
    for f in ["xtriever-pipeline.json", "ids.json", "lexical", "dense"] {
        assert!(dir.join(f).exists(), "{f}");
    }
    let reopened =
        HybridIndex::open(&dir, Box::new(support::TableEmbedder::from_fixture(&h))).unwrap();
    assert!(reopened.is_empty());
    assert!(
        reopened
            .search("anything", None, 5, &SearchOptions::default())
            .unwrap()
            .hits
            .is_empty()
    );
}

#[cfg(feature = "mmap")]
#[test]
fn mapped_open_gives_identical_hits() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    let opts = SearchOptions {
        explain: true,
        ..SearchOptions::default()
    };
    let before: Vec<_> = h
        .queries
        .iter()
        .map(|q| {
            index
                .search(&q.text, q.filter.as_ref(), 100, &opts)
                .unwrap()
        })
        .collect();
    drop(index);
    let mapped = HybridIndex::open_mapped(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    for (q, want) in h.queries.iter().zip(&before) {
        assert_eq!(
            mapped
                .search(&q.text, q.filter.as_ref(), 100, &opts)
                .unwrap(),
            *want,
            "{}",
            q.id
        );
    }
}

// ── Feature 015: the re-rank mode in the descriptor (ADR-0012) ───────────────────────────────

#[test]
fn descriptor_round_trips_rerank_mode() {
    use xtriever_pipeline::RerankMode;
    let tmp = tempfile::tempdir().unwrap();
    let h = support::hybrid();
    let mut cfg = support::fixture_config(&h);
    cfg.rerank_mode = RerankMode::Interpolate { alpha: 0.25 };
    let index = HybridIndex::create(
        tmp.path(),
        cfg,
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    drop(index);
    let text = std::fs::read_to_string(tmp.path().join("xtriever-pipeline.json")).unwrap();
    let depth = text.find("\"rerank_depth\"").expect("rerank_depth key");
    let mode = text.find("\"rerank_mode\"").expect("rerank_mode key");
    assert!(
        mode > depth,
        "rerank_mode follows rerank_depth on disk:\n{text}"
    );
    assert!(text.contains("\"interpolate\""), "{text}");
    assert!(text.contains("\"alpha\": 0.25"), "{text}");
    let reopened = HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    assert_eq!(
        reopened.config().rerank_mode,
        RerankMode::Interpolate { alpha: 0.25 }
    );
    assert!(
        text.contains(&format!("\"format_version\": {FORMAT_VERSION}")),
        "the format version does not change: {text}"
    );
}

#[test]
fn descriptor_without_rerank_mode_reads_as_interpolate_half() {
    use xtriever_pipeline::RerankMode;
    let tmp = tempfile::tempdir().unwrap();
    let (h, index) = support::build_from_fixture(tmp.path());
    drop(index);
    let path = tmp.path().join("xtriever-pipeline.json");
    let text = std::fs::read_to_string(&path).unwrap();
    // Strip the key the way an index written before Feature 015 lacks it.
    let start = text.find("  \"rerank_mode\"").expect("rerank_mode key");
    let end = text[start..]
        .find("\n  \"live_docs\"")
        .expect("live_docs follows")
        + start
        + 1;
    let stripped = format!("{}{}", &text[..start], &text[end..]);
    assert!(!stripped.contains("rerank_mode"), "{stripped}");
    std::fs::write(&path, stripped).unwrap();
    let reopened = HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap();
    assert_eq!(
        reopened.config().rerank_mode,
        RerankMode::Interpolate { alpha: 0.5 },
        "a pre-015 index reads as the default (ADR-0012)"
    );
}
