//! US5 scenarios 1–2 at the library level (offline) — the hybrid configuration, external-id
//! document building and the closure runner (spec 005 FR-022).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_core::{FieldName, Value};
use xtriever_eval::dataset::Dataset;
use xtriever_eval::run::{
    DenseConfig, EvalConfig, HybridConfig, build_external, build_passages, execute_external,
};

fn mini() -> (tempfile::TempDir, Dataset) {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, _) = support::synthetic_dataset(dir.path());
    let ds = Dataset::load(&manifest, "mini", dir.path()).unwrap();
    (dir, ds)
}

#[test]
fn hybrid_baseline_v1_is_the_two_recipes_combined_and_validates() {
    let cfg = HybridConfig::hybrid_baseline_v1();
    assert_eq!(cfg.name, "hybrid-baseline-v1");
    assert_eq!((cfg.k, cfg.candidate_depth, cfg.rrf_k), (100, 100, 60));
    assert_eq!(cfg.lexical, EvalConfig::lexical_baseline_v1());
    assert_eq!(cfg.dense, DenseConfig::dense_baseline_v1());
    cfg.validate().unwrap();
    let mut low = cfg.clone();
    low.k = 10;
    assert!(low.validate().is_err());
    let mut shallow = cfg.clone();
    shallow.candidate_depth = 50;
    assert!(
        shallow.validate().is_err(),
        "candidate depth below k cannot fill Recall@100"
    );
}

#[test]
fn build_external_keeps_corpus_order_and_omits_empty_fields() {
    let (_dir, ds) = mini();
    let docs = build_external(&ds, &EvalConfig::lexical_baseline_v1()).unwrap();
    assert_eq!(docs.len(), 4);
    assert_eq!(docs[0].0, "d1");
    assert_eq!(docs[1].0, "d2");
    assert!(docs[0].1.contains_key(&FieldName::from("title")));
    assert!(
        !docs[1].1.contains_key(&FieldName::from("title")),
        "empty title omitted"
    );
    assert_eq!(
        docs[1].1[&FieldName::from("text")],
        Value::Text("river valley harbor".into())
    );
    let mut keep_empty = EvalConfig::lexical_baseline_v1();
    keep_empty.omit_empty_fields = false;
    let docs = build_external(&ds, &keep_empty).unwrap();
    assert!(docs[1].1.contains_key(&FieldName::from("title")));
}

// Feature 013 (spec FR-001, FR-007, edge cases): `Source::TitleAndText` joins with one space,
// omits an empty side and its separator, and matches the dense passage of `dense-baseline-v1`.
#[test]
fn title_and_text_joins_with_one_space_and_omits_empty_sides() {
    let (_dir, mut ds) = mini();
    // Two documents the synthetic set lacks: a title with an empty text, and both empty.
    ds.corpus.ids.push("d_title_only".into());
    ds.corpus.titles.push("Only a title".into());
    ds.corpus.texts.push(String::new());
    ds.corpus.ids.push("d_empty".into());
    ds.corpus.titles.push(String::new());
    ds.corpus.texts.push(String::new());

    let contents = FieldName::from("contents");
    let docs = build_external(&ds, &EvalConfig::lexical_baseline_v2()).unwrap();
    assert_eq!(docs.len(), 6);
    let field = |i: usize| docs[i].1.get(&contents).cloned();
    assert_eq!(docs[0].1.len(), 1, "one field only");
    assert_eq!(
        field(0),
        Some(Value::Text("Quantum lattice quantum lattice photon".into())),
        "title + one space + text"
    );
    assert_eq!(
        field(1),
        Some(Value::Text("river valley harbor".into())),
        "empty title: the text alone, no leading separator"
    );
    assert_eq!(
        field(4),
        Some(Value::Text("Only a title".into())),
        "empty text: the title alone, no trailing separator"
    );
    assert_eq!(
        field(5),
        None,
        "both empty: omitted under omit_empty_fields"
    );

    let mut keep_empty = EvalConfig::lexical_baseline_v2();
    keep_empty.omit_empty_fields = false;
    let kept = build_external(&ds, &keep_empty).unwrap();
    assert_eq!(kept[5].1.get(&contents), Some(&Value::Text(String::new())));

    // The joined field is the dense passage of `dense-baseline-v1` for every document —
    // both present, empty title, empty text, both empty (data-model invariant; one join).
    let (passages, _) = build_passages(&ds, &DenseConfig::dense_baseline_v1()).unwrap();
    assert_eq!(passages.len(), kept.len());
    for (i, passage) in passages.iter().enumerate() {
        assert_eq!(
            kept[i].1.get(&contents),
            Some(&Value::Text(passage.clone())),
            "document {i}"
        );
    }
    assert_eq!(
        passages[4], "Only a title",
        "no trailing separator in the passage either"
    );
}

#[test]
fn execute_external_runs_every_judged_query_in_order_with_k() {
    let (_dir, ds) = mini();
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut retrieve = |query_id: &str, text: &str| -> xtriever_core::Result<Vec<String>> {
        seen.push((query_id.to_owned(), text.to_owned()));
        Ok(vec!["q2".into(), "d1".into(), "d2".into()])
    };
    let run = execute_external(&ds, "hybrid-baseline-v1", 100, &mut retrieve).unwrap();
    assert_eq!(run.config, "hybrid-baseline-v1");
    assert_eq!(run.dataset, "mini");
    assert_eq!(
        seen.iter()
            .map(|(id, t)| (id.as_str(), t.as_str()))
            .collect::<Vec<_>>(),
        vec![("q1", "quantum lattice"), ("q2", "river")],
        "judged queries with text, ascending id, each with its own id"
    );
    assert_eq!(run.results["q1"], vec!["q2", "d1", "d2"]);
    assert_eq!(run.results.len(), 2);
    assert_eq!(run.unjudged_queries, 0);
    let _ = BTreeMap::<String, String>::new();
}
