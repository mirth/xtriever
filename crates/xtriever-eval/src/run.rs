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

use crate::dataset::{Corpus, Dataset};
use crate::error::{Error, Result};

/// Which BEIR document field a schema field is built from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Source {
    /// The BEIR `title`.
    Title,
    /// The BEIR `text`.
    Text,
    /// `title + " " + text`; an empty title contributes nothing and no separator, an empty text
    /// likewise — the shape BEIR's reference BM25 indexes as `contents` and the dense passage
    /// already uses (Feature 013, research D1–D2).
    TitleAndText,
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

    /// `lexical-baseline-v2` (Feature 013, spec FR-001): one field `contents` =
    /// `title + " " + text` under `standard_en`, boost 1.0; otherwise v1. In the 012 spike's
    /// BM25 the v1 layout (`title` × 2.0 beside `text`) reproduced the engine (0.6207 /
    /// 0.3115 / 0.2473 nDCG@10 on SciFact / NFCorpus / FiQA vs the engine's 0.6270 / 0.3115 /
    /// 0.2502) and the joined field scored 0.6867 / 0.3228 / 0.2473 — a boosted short title
    /// field lets one title term outweigh several body matches (research D1).
    pub fn lexical_baseline_v2() -> Self {
        Self {
            name: "lexical-baseline-v2".into(),
            fields: vec![FieldSpec {
                name: "contents".into(),
                from: Source::TitleAndText,
                analyzer: "standard_en".into(),
                boost: 1.0,
            }],
            ..Self::lexical_baseline_v1()
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
        // The BEIR string id lives only in the IdMap — never in a document (FR-014).
        docs.push(Document {
            id: DocId(id),
            fields: document_fields(corpus, i, cfg),
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
    /// Placed between title and text — only when both are non-empty (since Feature 013 the
    /// passage and the lexical `contents` field come from one join, [`join_title_text`]).
    pub separator: String,
    /// An empty title contributes nothing — no separator either. Kept for the recorded
    /// configuration shape; the join never emits a separator beside an empty part, so the
    /// flag no longer changes the passage.
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
        if cfg.passage.title_then_text {
            passages.push(join_title_text(title, &cfg.passage.separator, text));
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

// ── Feature 005: the hybrid configuration and the stage-agnostic runner ────────────────────

/// The hybrid recipe: the 003 lexical fields, the 004 passage recipe, RRF (data-model 005).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HybridConfig {
    /// Cited by reports.
    pub name: String,
    /// Lexical fields and boosts.
    pub lexical: EvalConfig,
    /// Dense passage construction.
    pub dense: DenseConfig,
    /// Candidates per stage.
    pub candidate_depth: usize,
    /// Reciprocal rank fusion constant.
    pub rrf_k: u32,
    /// Retrieval depth of the fused list; ≥ 100.
    pub k: usize,
}

impl HybridConfig {
    /// `k ≥ 100`, `candidate_depth ≥ k`, and both sub-configurations valid.
    ///
    /// # Errors
    ///
    /// `Error::Run`.
    pub fn validate(&self) -> Result<()> {
        if self.k < 100 {
            return Err(Error::Run(format!(
                "k = {} but Recall@100 needs k ≥ 100",
                self.k
            )));
        }
        if self.candidate_depth < self.k {
            return Err(Error::Run(format!(
                "candidate_depth = {} is below k = {}; a stage could not fill the fused list",
                self.candidate_depth, self.k
            )));
        }
        self.lexical.validate()?;
        self.dense.validate()
    }

    /// `hybrid-baseline-v1`: `lexical-baseline-v1` + `dense-baseline-v1`, depth 100, `rrf_k` 60,
    /// `k` 100.
    pub fn hybrid_baseline_v1() -> Self {
        Self {
            name: "hybrid-baseline-v1".into(),
            lexical: EvalConfig::lexical_baseline_v1(),
            dense: DenseConfig::dense_baseline_v1(),
            candidate_depth: 100,
            rrf_k: 60,
            k: 100,
        }
    }

    /// `hybrid-baseline-v2` (Feature 013, spec FR-002): `hybrid-baseline-v1` over
    /// `lexical-baseline-v2`; the dense recipe, depth, `rrf_k` and `k` are v1's.
    pub fn hybrid_baseline_v2() -> Self {
        Self {
            name: "hybrid-baseline-v2".into(),
            lexical: EvalConfig::lexical_baseline_v2(),
            ..Self::hybrid_baseline_v1()
        }
    }
}

/// The lexical fields of document `i` under `cfg` (shared by `build` and `build_external`).
fn document_fields(corpus: &Corpus, i: usize, cfg: &EvalConfig) -> BTreeMap<FieldName, Value> {
    let mut fields = BTreeMap::new();
    for f in &cfg.fields {
        let value = match f.from {
            Source::Title => corpus.titles[i].clone(),
            Source::Text => corpus.texts[i].clone(),
            Source::TitleAndText => join_title_text(&corpus.titles[i], " ", &corpus.texts[i]),
        };
        if cfg.omit_empty_fields && value.is_empty() {
            continue;
        }
        fields.insert(FieldName::from(f.name.as_str()), Value::Text(value));
    }
    fields
}

/// `title + separator + text`, an empty side contributing neither itself nor the separator.
/// The one join behind the dense passage ([`build_passages`]) and the lexical
/// [`Source::TitleAndText`] field, so the two are equal for every document (Feature 013).
pub fn join_title_text(title: &str, separator: &str, text: &str) -> String {
    match (title.is_empty(), text.is_empty()) {
        (true, _) => text.to_owned(),
        (_, true) => title.to_owned(),
        (false, false) => {
            let mut joined = String::with_capacity(title.len() + separator.len() + text.len());
            joined.push_str(title);
            joined.push_str(separator);
            joined.push_str(text);
            joined
        }
    }
}

/// `(external id, fields)` per corpus document in corpus order, fields built as `build` does.
///
/// # Errors
///
/// `Error::Run` when the configuration is invalid.
pub fn build_external(
    dataset: &Dataset,
    cfg: &EvalConfig,
) -> Result<Vec<(String, BTreeMap<FieldName, Value>)>> {
    cfg.validate()?;
    let corpus = &dataset.corpus;
    Ok(corpus
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), document_fields(corpus, i, cfg)))
        .collect())
}

/// Run every judged query (ascending id) through `retrieve(query_id, text)`, which returns
/// external ids in rank order; the library names no retriever type. Judged queries without a
/// text (dangling, reported by `Dataset::load`) are skipped without a call.
///
/// # Errors
///
/// `Error::Run`, or whatever `retrieve` returns (wrapped as `Error::Core`).
pub fn execute_external(
    dataset: &Dataset,
    config_name: &str,
    k: usize,
    retrieve: &mut dyn FnMut(&str, &str) -> xtriever_core::Result<Vec<String>>,
) -> Result<Run> {
    if k < 100 {
        return Err(Error::Run(format!("k = {k} but Recall@100 needs k ≥ 100")));
    }
    let texts: BTreeMap<&str, &str> = dataset
        .queries
        .queries
        .iter()
        .map(|(id, t)| (id.as_str(), t.as_str()))
        .collect();
    let mut results = BTreeMap::new();
    for query_id in dataset.qrels.grades.keys() {
        let Some(text) = texts.get(query_id.as_str()) else {
            continue;
        };
        let mut ids = retrieve(query_id, text)?;
        ids.truncate(k);
        results.insert(query_id.clone(), ids);
    }
    Ok(Run {
        config: config_name.to_owned(),
        dataset: dataset.name.clone(),
        results,
        unjudged_queries: 0,
    })
}

// ── Feature 006: the re-ranked configuration ───────────────────────────────────────────────

/// How the re-ranked head is ordered (Feature 015) — the harness's own mirror of the
/// pipeline's mode (this crate names only core types), serialised in the same shape so a
/// report says which rule produced it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RerankMode {
    /// The cross-encoder's order replaces the fused order within the head (Feature 006).
    Replace,
    /// `(1 − alpha)·minmax(fused) + alpha·minmax(cross-encoder)` within the head, ties by
    /// fused rank (Feature 014's rule, the engine's default since 015).
    Interpolate {
        /// Weight of the cross-encoder term, within `[0, 1]`.
        alpha: f64,
    },
}

/// The re-ranked recipe: a hybrid recipe plus a re-rank depth and mode (data-model 006, 015).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RerankConfig {
    /// Cited by reports.
    pub name: String,
    /// The fused recipe underneath.
    pub hybrid: HybridConfig,
    /// Fused candidates re-scored per query.
    pub rerank_depth: usize,
    /// How the re-scored head is ordered.
    pub mode: RerankMode,
}

impl RerankConfig {
    /// `1 ≤ rerank_depth ≤ hybrid.k`, and the hybrid configuration valid.
    ///
    /// # Errors
    ///
    /// `Error::Run`.
    pub fn validate(&self) -> Result<()> {
        if self.rerank_depth == 0 {
            return Err(Error::Run("rerank_depth must be at least 1".into()));
        }
        if self.rerank_depth > self.hybrid.k {
            return Err(Error::Run(format!(
                "rerank_depth = {} exceeds k = {}; candidates beyond k cannot be returned",
                self.rerank_depth, self.hybrid.k
            )));
        }
        self.hybrid.validate()
    }

    /// `hybrid-rerank-v1`: `hybrid-baseline-v1` re-ranked at depth 20.
    pub fn hybrid_rerank_v1() -> Self {
        Self {
            name: "hybrid-rerank-v1".into(),
            hybrid: HybridConfig::hybrid_baseline_v1(),
            rerank_depth: 20,
            mode: RerankMode::Replace,
        }
    }

    /// `hybrid-rerank-v2` (Feature 013, spec FR-002): `hybrid-baseline-v2` re-ranked at depth 20.
    pub fn hybrid_rerank_v2() -> Self {
        Self {
            name: "hybrid-rerank-v2".into(),
            hybrid: HybridConfig::hybrid_baseline_v2(),
            ..Self::hybrid_rerank_v1()
        }
    }

    /// `hybrid-rerank-v3` (Feature 015): `hybrid-baseline-v2` re-ranked at depth 20 under the
    /// interpolating rule, α 0.5 — Feature 014's `lin-0.5-d20` cells (0.7207 / 0.3622 / 0.3910
    /// nDCG@10 on SciFact / NFCorpus / FiQA, against v2's 0.6954 / 0.3609 / 0.3742), which this
    /// configuration must reproduce per query.
    pub fn hybrid_rerank_v3() -> Self {
        Self {
            name: "hybrid-rerank-v3".into(),
            mode: RerankMode::Interpolate { alpha: 0.5 },
            ..Self::hybrid_rerank_v2()
        }
    }
}
