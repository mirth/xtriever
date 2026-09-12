//! User Story 2 (offline) — hash-verified loading against a synthetic dataset (FR-006–FR-010).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_eval::Error;
use xtriever_eval::dataset::Dataset;

// Scenario 3 (FR-007): a tampered file fails naming the path and both hashes
#[test]
fn tampered_file_fails_naming_both_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, _) = support::synthetic_dataset(dir.path());
    let qrels = dir.path().join("mini/qrels/test.tsv");
    let good = manifest
        .verify_file(dir.path(), "mini", "qrels/test.tsv")
        .expect("pristine file verifies");
    assert_eq!(good, qrels);
    support::tamper(&qrels);
    let err = manifest
        .verify_file(dir.path(), "mini", "qrels/test.tsv")
        .expect_err("must fail");
    match &err {
        Error::HashMismatch {
            path,
            expected,
            actual,
        } => {
            assert_eq!(path, &qrels);
            assert_ne!(expected, actual);
            assert!(
                expected.len() > 64 && actual.len() > 64,
                "both sides carry bytes/sha256: {expected} vs {actual}"
            );
        }
        other => panic!("wrong variant: {other}"),
    }
    let err = Dataset::load(&manifest, "mini", dir.path()).expect_err("loading must refuse");
    assert!(matches!(err, Error::HashMismatch { .. }), "{err}");
}

// Scenario 4 (FR-009): counts match; CRLF + header parse correctly
#[test]
fn synthetic_dataset_loads_with_exact_counts_and_crlf_qrels() {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, counts) = support::synthetic_dataset(dir.path());
    let ds = Dataset::load(&manifest, "mini", dir.path()).expect("load");
    assert_eq!(ds.counts, counts);
    assert_eq!(ds.corpus.ids, vec!["d1", "d2", "d3", "q2"]);
    assert_eq!(ds.corpus.titles[1], "", "empty title kept as empty string");
    assert_eq!(ds.queries.queries.len(), 3);
    assert_eq!(ds.qrels.grades.len(), 2, "two judged queries");
    assert_eq!(ds.qrels.pairs(), 5);
    assert_eq!(
        ds.qrels.grades["q1"]["d1"], 2,
        "grade parsed as an integer despite the trailing \\r"
    );
    assert_eq!(
        ds.qrels.grades["q2"]["dX"], 0,
        "explicit zero grade preserved"
    );
    assert!(
        !ds.qrels.grades.contains_key("query-id"),
        "header line must be skipped"
    );
    assert_eq!(ds.hashes.len(), 3);
}

// Scenario 5 (FR-010): dangling judged ids are reported, not fatal
#[test]
fn dangling_judged_ids_are_reported_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, _) = support::synthetic_dataset(dir.path());
    let ds = Dataset::load(&manifest, "mini", dir.path()).expect("load");
    assert_eq!(
        ds.dangling.documents,
        vec!["dX".to_owned()],
        "dX is judged but not in the corpus"
    );
    assert!(ds.dangling.queries.is_empty());
}

#[test]
fn manifest_rejects_unknown_dataset_names() {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, _) = support::synthetic_dataset(dir.path());
    let err = manifest.dataset("msmarco").expect_err("unknown");
    assert!(matches!(err, Error::Manifest(_)), "{err}");
    let err = Dataset::load(&manifest, "msmarco", dir.path()).expect_err("unknown");
    assert!(matches!(err, Error::Manifest(_)), "{err}");
}

#[test]
fn count_mismatch_is_a_manifest_error_naming_the_count() {
    let dir = tempfile::tempdir().unwrap();
    let (mut manifest, _) = support::synthetic_dataset(dir.path());
    manifest
        .datasets
        .get_mut("mini")
        .unwrap()
        .counts
        .judged_queries = 7;
    let err = Dataset::load(&manifest, "mini", dir.path()).expect_err("must fail");
    assert!(
        matches!(&err, Error::Manifest(m) if m.contains("judged_queries")),
        "{err}"
    );
}

#[test]
fn malformed_qrels_row_is_a_parse_error_with_line_number() {
    let dir = tempfile::tempdir().unwrap();
    let (mut manifest, _) = support::synthetic_dataset(dir.path());
    let qrels = dir.path().join("mini/qrels/test.tsv");
    let body = "query-id\tcorpus-id\tscore\r\nq1\td1\tnot-a-number\r\n";
    std::fs::write(&qrels, body).unwrap();
    let entry = manifest
        .datasets
        .get_mut("mini")
        .unwrap()
        .files
        .get_mut("qrels/test.tsv")
        .unwrap();
    entry.bytes = body.len() as u64;
    entry.sha256 = support::sha256_hex(body.as_bytes());
    let err = Dataset::load(&manifest, "mini", dir.path()).expect_err("must fail");
    assert!(matches!(&err, Error::Parse { line: 2, .. }), "{err}");
}
