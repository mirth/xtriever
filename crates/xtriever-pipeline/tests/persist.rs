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
            "\"format_version\": 2",
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
                msg.contains('2') && msg.contains(&FORMAT_VERSION.to_string()),
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

#[test]
fn a_partial_commit_is_refused_at_open_naming_four_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let (h, mut index) = support::build_from_fixture(tmp.path());
    let n = h.documents.len();
    // Three new documents the embedder knows nothing about would fail; reuse three fixture
    // passages under fresh ids instead.
    let extra: Vec<_> = h.documents[..3]
        .iter()
        .map(|d| {
            let mut s = d.source();
            s.external_id = format!("extra-{}", d.external_id);
            s
        })
        .collect();
    index.add(&extra).unwrap();
    index.commit_lexical_only_for_test().unwrap(); // crash after the lexical commit
    drop(index);
    match HybridIndex::open(
        tmp.path(),
        Box::new(support::TableEmbedder::from_fixture(&h)),
    )
    .unwrap_err()
    {
        Error::Corrupt(msg) => {
            assert!(
                msg.contains(&n.to_string()),
                "descriptor count missing: {msg}"
            );
            assert!(
                msg.contains(&(n + 3).to_string()),
                "lexical count missing: {msg}"
            );
            assert!(msg.to_lowercase().contains("partial"), "{msg}");
        }
        other => panic!("{other:?}"),
    }
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
