//! Evaluation configurations and running a `LexicalIndex` (Feature 003, spec FR-012–FR-016;
//! research D5) or an `Embedder` + `VectorIndex` (Feature 004, spec FR-019–FR-020; research D10)
//! over a dataset. The library names only core traits — never a stage crate's type.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use xtriever_core::{
    AnalyzerId, DocId, Document, Embedder, FieldDef, FieldKind, FieldName, LexicalIndex,
    LexicalQuery, Schema, TextKind, Value, VectorIndex,
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
    /// The invariants every entry point relies on (FR-012): `k ≥ 100` so Recall@100 is
    /// well-defined, and at least one indexed field. Checked by both `build` and `execute`,
    /// since the fields are public and the two can be called independently.
    pub fn validate(&self) -> Result<()> {
        if self.k < 100 {
            return Err(Error::Run(format!(
                "k = {} but Recall@100 needs k ≥ 100",
                self.k
            )));
        }
        if self.fields.is_empty() {
            return Err(Error::Run("configuration indexes no fields".into()));
        }
        Ok(())
    }

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
    cfg.validate()?;
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
    cfg.validate()?;
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

// ── Feature 004: the dense configuration ──────────────────────────────────────────────────────

/// How a BEIR document becomes one passage text (data-model "Dense Evaluation Configuration").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PassageSpec {
    /// Prepend the title when it is non-empty (BEIR's own dense baselines feed `title + text`).
    pub title_then_text: bool,
    /// Placed between title and text.
    pub separator: String,
    /// An empty title contributes nothing — no separator either.
    pub omit_empty_title: bool,
}

/// A named, reproducible dense recipe (spec FR-019).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DenseConfig {
    /// Cited by reports.
    pub name: String,
    /// Passage construction.
    pub passage: PassageSpec,
    /// Retrieval depth; must be ≥ 100 so Recall@100 is well-defined.
    pub k: usize,
}

impl DenseConfig {
    /// The same `k ≥ 100` invariant as [`EvalConfig::validate`].
    ///
    /// # Errors
    ///
    /// `Error::Run` when `k < 100`.
    pub fn validate(&self) -> Result<()> {
        if self.k < 100 {
            return Err(Error::Run(format!(
                "k = {} but Recall@100 needs k ≥ 100",
                self.k
            )));
        }
        Ok(())
    }

    /// `dense-baseline-v1` (research D11): `title + " " + text` (title omitted when empty), the
    /// model's own truncation, `k = 100`.
    pub fn dense_baseline_v1() -> Self {
        Self {
            name: "dense-baseline-v1".into(),
            passage: PassageSpec {
                title_then_text: true,
                separator: " ".into(),
                omit_empty_title: true,
            },
            k: 100,
        }
    }
}

/// One passage per corpus document, in corpus order; `DocId(i)` is the corpus position.
///
/// # Errors
///
/// `Error::Run` when the configuration is invalid or the corpus exceeds `u32` ids.
pub fn build_passages(dataset: &Dataset, cfg: &DenseConfig) -> Result<(Vec<String>, IdMap)> {
    cfg.validate()?;
    let corpus = &dataset.corpus;
    u32::try_from(corpus.ids.len())
        .map_err(|_| Error::Run("corpus exceeds u32 document ids".into()))?;
    let mut passages = Vec::with_capacity(corpus.ids.len());
    for (title, text) in corpus.titles.iter().zip(&corpus.texts) {
        let use_title =
            cfg.passage.title_then_text && !(cfg.passage.omit_empty_title && title.is_empty());
        if use_title {
            let mut p =
                String::with_capacity(title.len() + cfg.passage.separator.len() + text.len());
            p.push_str(title);
            p.push_str(&cfg.passage.separator);
            p.push_str(text);
            passages.push(p);
        } else {
            passages.push(text.clone());
        }
    }
    Ok((passages, IdMap(corpus.ids.clone())))
}

/// Embed every judged query (ascending id, `TextKind::Query`) and search `index` with `k`,
/// mapping hits back through `ids` in the retriever's order.
///
/// # Errors
///
/// `Error::Run` on an invalid configuration or an unknown `DocId`; core errors from the stages.
pub fn execute_dense(
    embedder: &dyn Embedder,
    index: &dyn VectorIndex,
    ids: &IdMap,
    dataset: &Dataset,
    cfg: &DenseConfig,
) -> Result<Run> {
    cfg.validate()?;
    let texts: BTreeMap<&str, &str> = dataset
        .queries
        .queries
        .iter()
        .map(|(id, t)| (id.as_str(), t.as_str()))
        .collect();
    let mut results = BTreeMap::new();
    for query_id in dataset.qrels.grades.keys() {
        let Some(text) = texts.get(query_id.as_str()) else {
            continue; // dangling judged query — reported by Dataset::load, not run
        };
        let mut vectors = embedder.embed(&[text], TextKind::Query)?;
        let vector = vectors.pop().ok_or_else(|| {
            Error::Run(format!("embedder returned no vector for query {query_id}"))
        })?;
        let hits = index.search(&vector, None, cfg.k)?;
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

/// What a cached corpus embedding was built from (spec FR-020). Stored as `cache.json` beside
/// the vector index; any field disagreement is a miss.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingCacheKey {
    /// Cache layout version.
    pub format_version: u32,
    /// `DenseConfig::name`.
    pub config: String,
    /// Dataset name.
    pub dataset: String,
    /// `Embedder::fingerprint()`.
    pub embedder_fingerprint: String,
    /// The manifest's `corpus.jsonl` SHA-256 the loader verified.
    pub corpus_sha256: String,
    /// Corpus size.
    pub documents: u64,
}

impl EmbeddingCacheKey {
    /// File name inside the cache directory.
    pub const FILE: &'static str = "cache.json";

    /// Write `cache.json` into `dir` (created if absent).
    ///
    /// # Errors
    ///
    /// `Error::Io`, `Error::Json`.
    pub fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(
            dir.join(Self::FILE),
            serde_json::to_string_pretty(self)? + "\n",
        )?;
        Ok(())
    }

    /// Whether `dir/cache.json` exists, parses, and equals `self` in every field.
    #[must_use]
    pub fn matches(&self, dir: &Path) -> bool {
        std::fs::read_to_string(dir.join(Self::FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Self>(&text).ok())
            .is_some_and(|stored| stored == *self)
    }
}
