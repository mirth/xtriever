//! User Story 3 (offline) — configuration → schema/documents, and execution against a stub
//! `LexicalIndex` (FR-012–FR-014).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{
    AnalyzerId, DocId, DocSet, Document, FieldKind, FieldName, Filter, Hit, IndexStats,
    LexicalIndex, LexicalQuery, Result as CoreResult, Schema, TermStats, Value,
};
use xtriever_eval::dataset::{Dataset, Manifest};
use xtriever_eval::run::{EvalConfig, IdMap, Source, build, execute};

fn mini() -> (tempfile::TempDir, Manifest, Dataset) {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, _) = support::synthetic_dataset(dir.path());
    let ds = Dataset::load(&manifest, "mini", dir.path()).unwrap();
    (dir, manifest, ds)
}

#[test]
fn baseline_config_is_as_specified() {
    let cfg = EvalConfig::lexical_baseline_v1();
    assert_eq!(cfg.name, "lexical-baseline-v1");
    assert_eq!(cfg.k, 100);
    assert!(cfg.omit_empty_fields);
    let title = cfg.fields.iter().find(|f| f.name == "title").unwrap();
    let text = cfg.fields.iter().find(|f| f.name == "text").unwrap();
    assert_eq!(
        (title.from, title.analyzer.as_str(), title.boost),
        (Source::Title, "standard_en", 2.0)
    );
    assert_eq!(
        (text.from, text.analyzer.as_str(), text.boost),
        (Source::Text, "standard_en", 1.0)
    );
}

// Scenario 2: the schema is exactly the configuration; ids map both ways; empty titles omitted
#[test]
fn build_produces_schema_documents_and_id_map() {
    let (_dir, _m, ds) = mini();
    let (schema, docs, ids) = build(&ds, &EvalConfig::lexical_baseline_v1()).unwrap();
    let expected = Schema {
        fields: vec![
            xtriever_core::FieldDef {
                name: "title".into(),
                kind: FieldKind::Text(AnalyzerId("standard_en".into())),
                indexed: true,
                stored: false,
                boost: 2.0,
            },
            xtriever_core::FieldDef {
                name: "text".into(),
                kind: FieldKind::Text(AnalyzerId("standard_en".into())),
                indexed: true,
                stored: false,
                boost: 1.0,
            },
        ],
    };
    assert_eq!(schema, expected);
    assert_eq!(docs.len(), 4);
    assert_eq!(
        ids,
        IdMap(vec!["d1".into(), "d2".into(), "d3".into(), "q2".into()])
    );
    for (i, d) in docs.iter().enumerate() {
        assert_eq!(d.id, DocId(i as u32), "DocId is the corpus position");
        assert!(d.chunk.is_none());
        // FR-014: only the configured text fields exist — no id field, no metadata
        assert!(
            d.fields.keys().all(|k| k.0 == "title" || k.0 == "text"),
            "{:?}",
            d.fields.keys()
        );
    }
    assert!(
        !docs[1].fields.contains_key(&FieldName::from("title")),
        "empty title omitted"
    );
    assert_eq!(
        docs[0].fields[&FieldName::from("title")],
        Value::Text("Quantum lattice".into())
    );
    assert_eq!(ids.external(DocId(3)), Some("q2"));
    assert_eq!(ids.external(DocId(4)), None);
}

#[test]
fn k_below_100_is_rejected_by_build_and_by_execute() {
    let (_dir, _m, ds) = mini();
    let mut cfg = EvalConfig::lexical_baseline_v1();
    cfg.k = 10;
    assert!(build(&ds, &cfg).is_err());
    // `execute` is public and takes the config independently: it must not silently retrieve ten
    // documents and let them be reported as Recall@100 (FR-012)
    let (schema, _docs, ids) = build(&ds, &EvalConfig::lexical_baseline_v1()).unwrap();
    let stub = Stub {
        schema,
        order: vec![0, 1, 2, 3],
        seen: std::sync::Mutex::new(Vec::new()),
    };
    let err = execute(&stub, &ids, &ds, &cfg).expect_err("execute must validate k");
    assert!(err.to_string().contains("k = 10"), "{err}");
    assert!(
        stub.seen.lock().unwrap().is_empty(),
        "no query may run under an invalid config"
    );
}

/// A canned retriever: returns, for any query, the documents in a fixed order.
struct Stub {
    schema: Schema,
    order: Vec<u32>,
    seen: std::sync::Mutex<Vec<(Option<String>, String, usize)>>,
}

impl LexicalIndex for Stub {
    fn schema(&self) -> &Schema {
        &self.schema
    }
    fn add(&mut self, _docs: &[Document]) -> CoreResult<()> {
        Ok(())
    }
    fn delete(&mut self, _ids: &[DocId]) -> CoreResult<()> {
        Ok(())
    }
    fn commit(&mut self) -> CoreResult<()> {
        Ok(())
    }
    fn search(
        &self,
        query: &LexicalQuery,
        _filter: Option<&Filter>,
        k: usize,
    ) -> CoreResult<Vec<Hit>> {
        let LexicalQuery::Match(field, text) = query else {
            panic!("baseline uses Match")
        };
        self.seen
            .lock()
            .unwrap()
            .push((field.as_ref().map(|f| f.0.clone()), text.clone(), k));
        Ok(self
            .order
            .iter()
            .take(k)
            .enumerate()
            .map(|(i, id)| Hit {
                id: DocId(*id),
                score: 10.0 - i as f32,
            })
            .collect())
    }
    fn resolve_filter(&self, _filter: &Filter) -> CoreResult<DocSet> {
        Ok(DocSet::new())
    }
    fn term_stats(&self, _field: &FieldName, _term: &str) -> CoreResult<Option<TermStats>> {
        Ok(None)
    }
    fn stats(&self) -> CoreResult<IndexStats> {
        Ok(IndexStats::default())
    }
}

// Scenario 1: every judged query is run, in id order, and the order returned is the order recorded
#[test]
fn execute_runs_every_judged_query_preserving_order_and_dropping_identical_ids() {
    let (_dir, _m, ds) = mini();
    let cfg = EvalConfig::lexical_baseline_v1();
    let (schema, _docs, ids) = build(&ds, &cfg).unwrap();
    let stub = Stub {
        schema,
        order: vec![3, 0, 2, 1],
        seen: std::sync::Mutex::new(Vec::new()),
    };
    let run = execute(&stub, &ids, &ds, &cfg).unwrap();
    assert_eq!(run.config, "lexical-baseline-v1");
    assert_eq!(run.dataset, "mini");
    // only judged queries (q1, q2), in ascending id order; q3 is unjudged and not run
    let seen = stub.seen.lock().unwrap();
    assert_eq!(
        seen.iter()
            .map(|(f, t, k)| (f.clone(), t.as_str(), *k))
            .collect::<Vec<_>>(),
        vec![(None, "quantum lattice", 100), (None, "river", 100)]
    );
    assert_eq!(
        run.results.keys().cloned().collect::<Vec<_>>(),
        vec!["q1", "q2"]
    );
    assert_eq!(run.unjudged_queries, 0);
    // q1: order 3,0,2,1 → external q2,d1,d3,d2 (no self id)
    assert_eq!(run.results["q1"], vec!["q2", "d1", "d3", "d2"]);
    // q2: the retriever's raw output is recorded, self-id included — the identical-id rule is
    // applied at scoring time (BEIR's evaluate()), see report.rs
    assert_eq!(run.results["q2"], vec!["q2", "d1", "d3", "d2"]);
    let out = _dir.path().join("run.jsonl");
    run.export_jsonl(&out).unwrap();
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(&out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["query_id"], "q1");
    assert_eq!(
        lines[0]["doc_ids"],
        serde_json::json!(["q2", "d1", "d3", "d2"])
    );
}
