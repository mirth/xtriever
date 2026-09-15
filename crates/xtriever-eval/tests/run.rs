//! User Story 3 (offline) — configuration → schema/documents, and execution against a stub
//! `LexicalIndex` (FR-012–FR-014).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{
    AnalyzerId, DocId, DocSet, Document, FieldKind, FieldName, Filter, Hit, IndexStats,
    LexicalIndex, LexicalQuery, Result as CoreResult, Schema, TermStats, Value,
};
use xtriever_eval::dataset::{Dataset, Manifest};
use xtriever_eval::run::{
    EvalConfig, FieldSpec, HybridConfig, IdMap, RerankConfig, Source, build, execute,
};

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

// Feature 013 (spec FR-001, FR-002, FR-004): the v2 configurations differ from v1 only in the
// lexical field layout — one joined `contents` field instead of `title` × 2.0 + `text`.
#[test]
fn v2_config_is_v1_with_one_joined_field() {
    let v1 = EvalConfig::lexical_baseline_v1();
    let v2 = EvalConfig::lexical_baseline_v2();
    assert_eq!(v2.name, "lexical-baseline-v2");
    assert_eq!(
        v2.fields,
        vec![FieldSpec {
            name: "contents".into(),
            from: Source::TitleAndText,
            analyzer: "standard_en".into(),
            boost: 1.0,
        }],
        "exactly one field, the joined one, boost 1.0"
    );
    assert_eq!(v2.k, v1.k);
    assert_eq!(v2.omit_empty_fields, v1.omit_empty_fields);
    assert_eq!(v2.query, v1.query);
    v2.validate().unwrap();
}

#[test]
fn v2_hybrid_and_rerank_wrap_the_v2_lexical() {
    let h1 = HybridConfig::hybrid_baseline_v1();
    let h2 = HybridConfig::hybrid_baseline_v2();
    assert_eq!(h2.name, "hybrid-baseline-v2");
    assert_eq!(h2.lexical, EvalConfig::lexical_baseline_v2());
    assert_eq!(h2.dense, h1.dense);
    assert_eq!(
        (h2.candidate_depth, h2.rrf_k, h2.k),
        (h1.candidate_depth, h1.rrf_k, h1.k)
    );
    h2.validate().unwrap();

    let r1 = RerankConfig::hybrid_rerank_v1();
    let r2 = RerankConfig::hybrid_rerank_v2();
    assert_eq!(r2.name, "hybrid-rerank-v2");
    assert_eq!(r2.hybrid, h2);
    assert_eq!(r2.rerank_depth, r1.rerank_depth);
    r2.validate().unwrap();
}

#[test]
fn v1_is_unchanged() {
    let v1 = EvalConfig::lexical_baseline_v1();
    let names: Vec<(&str, Source, f32)> = v1
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.from, f.boost))
        .collect();
    assert_eq!(
        names,
        vec![("title", Source::Title, 2.0), ("text", Source::Text, 1.0)]
    );
    assert_eq!(HybridConfig::hybrid_baseline_v1().lexical, v1);
    assert_eq!(
        RerankConfig::hybrid_rerank_v1().hybrid,
        HybridConfig::hybrid_baseline_v1()
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
