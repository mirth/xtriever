//! User Story 5 — corpus statistics (FR-024–FR-027), plus the FR-025 divergence measurement.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::{Mutations, Stats, TestIndex, corpus, fixture_index, index_in_batches};
use xtriever_core::{DocId, FieldName, LexicalIndex, TermStats};

fn stats_golden() -> Stats {
    support::load_json("stats.json")
}

fn mutations() -> Mutations {
    support::load_json("mutations.json")
}

fn assert_avg_close(
    actual: &std::collections::BTreeMap<FieldName, f32>,
    expected: &std::collections::BTreeMap<String, f32>,
) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "fields differ: {actual:?} vs {expected:?}"
    );
    for (name, e) in expected {
        let a = actual
            .get(&FieldName::from(name.as_str()))
            .unwrap_or_else(|| panic!("missing {name}"));
        // Python computed the exact rational as f64; f32 rounding is the only allowed difference.
        assert!((a - e).abs() <= 1e-5 * e.abs(), "{name}: {a} vs golden {e}");
    }
}

// Scenarios 1 & 2 (FR-026)
#[test]
fn term_stats_match_golden_and_absent_is_none() {
    let t = fixture_index();
    for term in stats_golden().terms {
        let got = t
            .index
            .term_stats(&FieldName::from(term.field.as_str()), &term.term)
            .expect("term_stats");
        match (term.doc_freq, term.total_term_freq) {
            (None, None) => assert_eq!(
                got, None,
                "{}:{} must be None (unseen)",
                term.field, term.term
            ),
            (Some(df), Some(ttf)) => assert_eq!(
                got,
                Some(TermStats {
                    doc_freq: df,
                    total_term_freq: ttf
                }),
                "{}:{}",
                term.field,
                term.term
            ),
            other => panic!("malformed golden {other:?}"),
        }
    }
}

// Scenario 3 (FR-027)
#[test]
fn stats_match_golden() {
    let t = fixture_index();
    let s = t.index.stats().expect("stats");
    let g = stats_golden();
    assert_eq!(s.num_docs, g.num_docs);
    assert_avg_close(&s.avg_field_len, &g.avg_field_len);
}

// Scenario 4 (FR-024): after deletes, both methods are live-only
#[test]
fn statistics_are_live_only_after_deletes() {
    let m = mutations();
    let mut t = fixture_index();
    let deleted: Vec<DocId> = m.delete.ids.iter().map(|&i| DocId(i)).collect();
    t.index.delete(&deleted).expect("delete");
    t.index.commit().expect("commit");
    let s = t.index.stats().expect("stats");
    assert_eq!(s.num_docs, m.delete.expected_num_docs);
    assert_avg_close(&s.avg_field_len, &m.delete.expected_avg_field_len);
    for term in &m.delete.expected_term_stats {
        let got = t
            .index
            .term_stats(&FieldName::from(term.field.as_str()), &term.term)
            .expect("term_stats");
        assert_eq!(
            got,
            Some(TermStats {
                doc_freq: term.doc_freq.unwrap(),
                total_term_freq: term.total_term_freq.unwrap()
            }),
            "{}:{}",
            term.field,
            term.term
        );
    }
    // "lettuce" is planted in exactly one document (66); deleting it makes the term *seen zero
    // times* — `Some(0, 0)` until a merge drops it — which is distinct from `None` (unseen).
    let mut t2 = fixture_index();
    t2.index.delete(&[DocId(66)]).expect("delete");
    t2.index.commit().expect("commit");
    let got = t2
        .index
        .term_stats(&"body".into(), "lettuce")
        .expect("term_stats");
    assert_eq!(
        got,
        Some(TermStats {
            doc_freq: 0,
            total_term_freq: 0
        })
    );
    assert_eq!(
        t2.index
            .term_stats(&"body".into(), "zzzzunknownword")
            .unwrap(),
        None
    );
}

// Scenario 5 (SC-011): same live documents by a different history ⇒ identical statistics
#[test]
fn history_pair_has_identical_live_statistics() {
    let m = mutations();
    let baseline = fixture_index();
    let mut variant = TestIndex::create_fixture();
    let mut docs = corpus().documents;
    docs.extend(m.history_pair.extra_documents.iter().cloned());
    index_in_batches(&mut variant.index, &docs, 2);
    let del: Vec<DocId> = m
        .history_pair
        .delete_ids
        .iter()
        .map(|&i| DocId(i))
        .collect();
    variant.index.delete(&del).expect("delete");
    variant.index.commit().expect("commit");

    assert_eq!(
        variant.index.stats().unwrap(),
        baseline.index.stats().unwrap()
    );
    for term in stats_golden().terms {
        let f = FieldName::from(term.field.as_str());
        assert_eq!(
            variant.index.term_stats(&f, &term.term).unwrap(),
            baseline.index.term_stats(&f, &term.term).unwrap(),
            "{}:{}",
            term.field,
            term.term
        );
    }
}

// Scenario 6 (FR-025, SC-012): measure — never adjust — the divergence between live statistics and
// what the backend scored with. Run with `--ignored`; prints a DivergenceRecord for report.md.
#[test]
#[ignore = "measurement, not an assertion: prints the FR-025 DivergenceRecord"]
fn divergence_measurement() {
    let m = mutations();
    let baseline = fixture_index();
    let mut variant = TestIndex::create_fixture();
    let mut docs = corpus().documents;
    docs.extend(m.history_pair.extra_documents.iter().cloned());
    index_in_batches(&mut variant.index, &docs, 2);
    let del: Vec<DocId> = m
        .history_pair
        .delete_ids
        .iter()
        .map(|&i| DocId(i))
        .collect();
    variant.index.delete(&del).expect("delete");
    variant.index.commit().expect("commit");

    let entry = support::query(&m.history_pair.divergence_query);
    let base_hits = baseline.index.search(&entry.query, None, entry.k).unwrap();
    let report = |label: &str, hits: &[xtriever_core::Hit]| {
        let mut max_delta = 0f32;
        for (b, v) in base_hits.iter().zip(hits) {
            max_delta = max_delta.max((b.score - v.score).abs());
        }
        let same_ids = support::ids(&base_hits) == support::ids(hits);
        println!(
            "DivergenceRecord scenario=history-pair query={} phase={label} same_ids={same_ids} score_delta_max={max_delta:e} verdict={}",
            entry.name,
            if max_delta == 0.0 && same_ids {
                "agree"
            } else {
                "diverge"
            }
        );
    };
    let before = variant.index.search(&entry.query, None, entry.k).unwrap();
    let live = variant.index.term_stats(&"body".into(), "quantum").unwrap();
    println!(
        "DivergenceRecord live_stats body:quantum={live:?} num_docs={}",
        variant.index.stats().unwrap().num_docs
    );
    report("before-merge", &before);
    variant.index.merge().expect("merge");
    let after = variant.index.search(&entry.query, None, entry.k).unwrap();
    report("after-merge", &after);
}
