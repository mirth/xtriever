//! Evaluation configurations and running a `LexicalIndex` over a dataset (spec FR-012–FR-016;
//! research D5).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use xtriever_core::{
    AnalyzerId, DocId, Document, FieldDef, FieldKind, FieldName, LexicalIndex, LexicalQuery,
    Schema, Value,
};

use crate::dataset::Dataset;
use crate::error::{Error, Result};

/// Which BEIR document field a schema field is built from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Source {
    /// The BEIR `title`.
    Title,
    /// The BEIR `text`.
    Text,
}

/// One indexed text field of an evaluation configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldSpec {
    /// Schema field name.
    pub name: String,
    /// Where its value comes from.
    pub from: Source,
    /// `xtriever-lexical` analyzer id.
    pub analyzer: String,
    /// Field boost.
    pub boost: f32,
}

/// How a query text becomes a `LexicalQuery`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueryKind {
    /// `LexicalQuery::Match(None, text)` — every indexed text field, OR-ed, boosts summed.
    MatchAll,
}

/// A named, reproducible recipe (data-model `EvalConfig`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvalConfig {
    /// Cited by reports.
    pub name: String,
    /// Indexed fields.
    pub fields: Vec<FieldSpec>,
    /// Skip a field whose source value is empty (BEIR indexes "the title (if available)").
    pub omit_empty_fields: bool,
    /// Query construction.
    pub query: QueryKind,
    /// Retrieval depth; must be ≥ 100 so Recall@100 is well-defined.
    pub k: usize,
}

impl EvalConfig {
    /// The baseline configuration (spec FR-016): `title` and `text` under `standard_en`, boosts
    /// 2.0 / 1.0, `Match(None, …)`, `k = 100`.
    pub fn lexical_baseline_v1() -> Self {
        Self {
            name: "lexical-baseline-v1".into(),
            fields: vec![
                FieldSpec {
                    name: "title".into(),
                    from: Source::Title,
                    analyzer: "standard_en".into(),
                    boost: 2.0,
                },
                FieldSpec {
                    name: "text".into(),
                    from: Source::Text,
                    analyzer: "standard_en".into(),
                    boost: 1.0,
                },
            ],
            omit_empty_fields: true,
            query: QueryKind::MatchAll,
            k: 100,
        }
    }
}

/// Internal `DocId` → BEIR string id. The backend never sees the string (Principle V).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdMap(pub Vec<String>);

impl IdMap {
    /// The external id of an internal one, if it exists.
    pub fn external(&self, id: DocId) -> Option<&str> {
        self.0.get(id.0 as usize).map(String::as_str)
    }
}

/// Build the schema and documents for `cfg`; `DocId(i)` is the corpus position.
pub fn build(dataset: &Dataset, cfg: &EvalConfig) -> Result<(Schema, Vec<Document>, IdMap)> {
    if cfg.k < 100 {
        return Err(Error::Run(format!(
            "k = {} but Recall@100 needs k ≥ 100",
            cfg.k
        )));
    }
    if cfg.fields.is_empty() {
        return Err(Error::Run("configuration indexes no fields".into()));
    }
    let schema = Schema {
        fields: cfg
            .fields
            .iter()
            .map(|f| FieldDef {
                name: FieldName::from(f.name.as_str()),
                kind: FieldKind::Text(AnalyzerId(f.analyzer.clone())),
                indexed: true,
                stored: false,
                boost: f.boost,
            })
            .collect(),
    };
    let corpus = &dataset.corpus;
    let mut docs = Vec::with_capacity(corpus.ids.len());
    for i in 0..corpus.ids.len() {
        let id =
            u32::try_from(i).map_err(|_| Error::Run("corpus exceeds u32 document ids".into()))?;
        let mut fields = BTreeMap::new();
        for f in &cfg.fields {
            let value = match f.from {
                Source::Title => &corpus.titles[i],
                Source::Text => &corpus.texts[i],
            };
            if cfg.omit_empty_fields && value.is_empty() {
                continue;
            }
            fields.insert(FieldName::from(f.name.as_str()), Value::Text(value.clone()));
        }
        // The BEIR string id lives only in the IdMap — never in a document (FR-014).
        docs.push(Document {
            id: DocId(id),
            fields,
            chunk: None,
        });
    }
    Ok((schema, docs, IdMap(corpus.ids.clone())))
}

/// Ordered results for every judged query (data-model `Run`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Run {
    /// Configuration name.
    pub config: String,
    /// Dataset name.
    pub dataset: String,
    /// Query id → external doc ids in the retriever's order, ≤ `k`. Raw: the identical-id
    /// rule is applied at scoring time, exactly where BEIR's wrapper applies it.
    pub results: BTreeMap<String, Vec<String>>,
    /// Queries run that are not judged — 0 by construction.
    pub unjudged_queries: u32,
}

impl Run {
    /// Write `{"query_id": …, "doc_ids": [...]}` per line, for `gen_003_fixtures.py --verify-run`.
    pub fn export_jsonl(&self, path: &Path) -> Result<()> {
        let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
        for (query_id, doc_ids) in &self.results {
            let line = serde_json::json!({ "query_id": query_id, "doc_ids": doc_ids });
            writeln!(out, "{line}")?;
        }
        out.flush()?;
        Ok(())
    }
}

/// Run every judged query through `index` (ascending query id), mapping hits back through `ids`.
pub fn execute(
    index: &dyn LexicalIndex,
    ids: &IdMap,
    dataset: &Dataset,
    cfg: &EvalConfig,
) -> Result<Run> {
    let texts: BTreeMap<&str, &str> = dataset
        .queries
        .queries
        .iter()
        .map(|(id, t)| (id.as_str(), t.as_str()))
        .collect();
    let mut results = BTreeMap::new();
    // Judged queries only, in ascending id order (BTreeMap), so a run is reproducible and every
    // query the metric layer will score is present (`not_retrieved` is 0 by construction).
    for query_id in dataset.qrels.grades.keys() {
        let Some(text) = texts.get(query_id.as_str()) else {
            continue; // dangling judged query — reported by Dataset::load, not run
        };
        let query = match cfg.query {
            QueryKind::MatchAll => LexicalQuery::Match(None, (*text).to_owned()),
        };
        let hits = index.search(&query, None, cfg.k)?;
        let mut external = Vec::with_capacity(hits.len());
        for hit in hits {
            let ext = ids.external(hit.id).ok_or_else(|| {
                Error::Run(format!("retriever returned unknown DocId {}", hit.id))
            })?;
            external.push(ext.to_owned());
        }
        results.insert(query_id.clone(), external);
    }
    Ok(Run {
        config: cfg.name.clone(),
        dataset: dataset.name.clone(),
        results,
        unjudged_queries: 0,
    })
}
