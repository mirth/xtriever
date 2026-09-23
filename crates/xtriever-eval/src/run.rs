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
    /// Cache layout version: the dense on-disk format the cache directory holds, plus, since
    /// Feature 026 (3), the embedder's own floats beside it in `vectors.f32.bin` — the index
    /// stores eight-bit rows, and the eval measures the embedder and the quantisation against
    /// what the embedder produced. A key that says 1 or 2 names a directory this build cannot
    /// open or one without the floats; either is a miss.
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
    /// The embedder's width: what the float sidecar's rows are, so a reader needs no index
    /// open and no inference from the file's length (Feature 026, review round 7).
    pub dim: usize,
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
        write_key(self, dir, Self::FILE)
    }

    /// Whether `dir/cache.json` exists, parses, and equals `self` in every field.
    #[must_use]
    pub fn matches(&self, dir: &Path) -> bool {
        self.mismatch(dir).is_none()
    }

    /// Why the cache at `dir` does not answer to this key — the missing key file, its parse
    /// error, or the first differing field with both values — or `None` when it matches. The
    /// text callers print before re-embedding, so nobody chases a phantom change.
    pub fn mismatch(&self, dir: &Path) -> Option<String> {
        key_mismatch(self, dir, Self::FILE)
    }
}

/// Write a cache key into `dir/file` (created if absent) as pretty JSON.
fn write_key<K: Serialize>(key: &K, dir: &Path, file: &str) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(file), serde_json::to_string_pretty(key)? + "\n")?;
    Ok(())
}

/// Why the key file at `dir/file` does not answer to `key` — the missing file, its parse error,
/// or the first field (by name) whose values differ, both named — or `None` when it matches.
/// Every field serde gives is compared, so a field added to a key later can never be left out
/// of the comparison; one the file has and the key does not is a difference too. The text
/// callers print before rebuilding a cache, so nobody chases a phantom change.
fn key_mismatch<K: Serialize>(key: &K, dir: &Path, file: &str) -> Option<String> {
    let path = dir.join(file);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return Some(format!("no key file at {} ({e})", path.display())),
    };
    let stored: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return Some(format!("key file {} is unreadable ({e})", path.display())),
    };
    let current = match serde_json::to_value(key) {
        Ok(v) => v,
        Err(e) => return Some(format!("cannot compare with {} ({e})", path.display())),
    };
    let (Some(stored), Some(current)) = (stored.as_object(), current.as_object()) else {
        return Some(format!("key file {} is not a JSON object", path.display()));
    };
    let show = |v: Option<&serde_json::Value>| match v {
        None => "nothing".to_owned(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    };
    current
        .iter()
        .find(|(name, now)| stored.get(*name) != Some(*now))
        .map(|(name, now)| {
            format!(
                "{name} differs: cache has {}, this run needs {}",
                show(stored.get(name)),
                show(Some(now))
            )
        })
        .or_else(|| {
            stored
                .keys()
                .find(|name| !current.contains_key(*name))
                .map(|name| format!("{name} differs: cache has it, this run has no such field"))
        })
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
    /// Sparse lexical expansion (Feature 027); `None` for every recipe before it, and omitted
    /// from a serialised report then, so earlier reports read and compare unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse: Option<SparseSettings>,
}

/// The harness's mirror of the pipeline's `SparseOption` (this crate names only core types):
/// how a sparse index writes and scores its expansions (Feature 027, data-model `SparseOption`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SparseSettings {
    /// A weight becomes `round(weight × scale)` occurrences of its term.
    pub scale: u32,
    /// The `_sparse` field's boost.
    pub boost: f32,
}

impl SparseSettings {
    /// The largest scale: `xtriever_dense::sparse::MAX_SCALE`, mirrored (a test keeps them
    /// equal), so a recipe the pipeline would refuse is refused before any encoding.
    pub const MAX_SCALE: u32 = 1_000;

    /// The spike's settings (research D4): scale 10, boost 1.0.
    pub const DEFAULT: Self = Self {
        scale: 10,
        boost: 1.0,
    };

    /// `1 ≤ scale ≤ MAX_SCALE`, boost finite and above zero.
    ///
    /// # Errors
    ///
    /// `Error::Run` naming the value.
    pub fn validate(&self) -> Result<()> {
        if !(1..=Self::MAX_SCALE).contains(&self.scale) {
            return Err(Error::Run(format!(
                "sparse scale {} is outside 1..={}",
                self.scale,
                Self::MAX_SCALE
            )));
        }
        if !(self.boost.is_finite() && self.boost > 0.0) {
            return Err(Error::Run(format!(
                "sparse boost {} must be finite and above zero",
                self.boost
            )));
        }
        Ok(())
    }
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
        if let Some(sparse) = &self.sparse {
            sparse.validate()?;
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
            sparse: None,
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

    /// `hybrid-sparse-rerank-v1` (Feature 027, contract `surfaces-and-eval.md`):
    /// `hybrid-rerank-v3` over `hybrid-sparse-v1` — the re-rank depth and interpolation
    /// unchanged, so the comparison with v3 measures the option alone.
    pub fn hybrid_sparse_rerank_v1() -> Self {
        Self {
            name: "hybrid-sparse-rerank-v1".into(),
            hybrid: HybridConfig::hybrid_sparse_v1(),
            ..Self::hybrid_rerank_v3()
        }
    }
}

impl HybridConfig {
    /// `hybrid-sparse-v1` (Feature 027, contract `surfaces-and-eval.md`):
    /// `hybrid-baseline-v2` with sparse expansion at the spike's settings (scale 10, boost 1.0).
    pub fn hybrid_sparse_v1() -> Self {
        Self {
            name: "hybrid-sparse-v1".into(),
            sparse: Some(SparseSettings::DEFAULT),
            ..Self::hybrid_baseline_v2()
        }
    }
}

// ── Feature 027: the sparse-weights cache ──────────────────────────────────────────────────

/// One document's cached expansion, as the encoder produced it (the harness's mirror of
/// `xtriever_dense::sparse::Expansion`; this crate names only core types).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CachedExpansion {
    /// `(token id, weight)`, ascending by id.
    pub entries: Vec<(u32, f32)>,
    /// The passage ran past the encoder's window.
    pub truncated: bool,
}

/// What a cached set of document expansions was encoded from (contract `surfaces-and-eval.md`).
/// Stored as `key.json` beside `weights.bin`; any field disagreement is a miss.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SparseCacheKey {
    /// Cache layout version (1).
    pub format_version: u32,
    /// Dataset name.
    pub dataset: String,
    /// The encoder's identity (`xtriever_dense::model::SPARSE_IDENTITY`).
    pub encoder: String,
    /// What the encoded passages were built from: the lexical configuration whose fields the
    /// passage joins (`EvalConfig::name`). Another recipe is other text, so another cache.
    pub recipe: String,
    /// The manifest's `corpus.jsonl` SHA-256 the loader verified.
    pub corpus_sha256: String,
    /// Corpus size.
    pub documents: u64,
}

impl SparseCacheKey {
    /// File name inside the cache directory.
    pub const FILE: &'static str = "key.json";

    /// The layout this build writes: `weights.bin` as described at [`write_sparse_weights`] —
    /// 2 since each record carries its truncation flag (review round 6); a key that says 1 is
    /// a miss.
    pub const FORMAT_VERSION: u32 = 2;

    /// Write `key.json` into `dir` (created if absent).
    ///
    /// # Errors
    ///
    /// `Error::Io`, `Error::Json`.
    pub fn write(&self, dir: &Path) -> Result<()> {
        write_key(self, dir, Self::FILE)
    }

    /// Why the cache at `dir` does not answer to this key — the missing key file, its parse
    /// error, or the first differing field with both values — or `None` when it matches.
    pub fn mismatch(&self, dir: &Path) -> Option<String> {
        key_mismatch(self, dir, Self::FILE)
    }
}

/// Every document's expansion, in corpus order, as `weights.bin`: per document a little-endian
/// `u32` entry count, a `u8` truncation flag (0 or 1), then that many `(u32 token id, f32
/// weight)` pairs. The raw weights, not
/// term frequencies, so a run at another scale re-uses them (research D10). Written the way
/// every durable file in the engine is (`xtriever_core::fs`): a temporary file, synced, renamed
/// into place, and the directory synced.
///
/// # Errors
///
/// `Error::Io`; `Error::Run` for an expansion longer than `u32::MAX` entries.
pub fn write_sparse_weights(path: &Path, expansions: &[CachedExpansion]) -> Result<()> {
    let mut bytes = Vec::new();
    for expansion in expansions {
        append_sparse_weights(&mut bytes, expansion)?;
    }
    xtriever_core::fs::write_atomically(path, &path.with_extension("bin.tmp"), &bytes)?;
    if let Some(dir) = path.parent() {
        xtriever_core::fs::sync_dir(dir)?;
    }
    Ok(())
}

/// How far an interruptible encode has durably got (Feature 027): `documents` complete records
/// occupying the first `bytes` of `weights.partial`, synced before this record was written.
/// A resume truncates the partial file to `bytes` and reads exactly `documents` back, so what a
/// crash leaves after the last sync — torn bytes, or blocks that read back as zeros — is never
/// taken for documents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SparseProgress {
    /// Complete documents, from the corpus's first.
    pub documents: u64,
    /// The bytes they occupy.
    pub bytes: u64,
}

impl SparseProgress {
    /// File name inside the cache directory.
    pub const FILE: &'static str = "progress.json";

    /// Write `progress.json` into `dir` durably (atomic replace, directory synced). Call it only
    /// after the partial file's first `bytes` are synced.
    ///
    /// # Errors
    ///
    /// `Error::Io`, `Error::Json`.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let path = dir.join(Self::FILE);
        xtriever_core::fs::write_atomically(
            &path,
            &dir.join("progress.json.tmp"),
            serde_json::to_string_pretty(self)?.as_bytes(),
        )?;
        xtriever_core::fs::sync_dir(dir)?;
        Ok(())
    }

    /// The recorded progress, or `None` when the encode never reached a sync point.
    ///
    /// # Errors
    ///
    /// `Error::Io` other than a missing file; `Error::Json` for an unreadable record.
    pub fn read(dir: &Path) -> Result<Option<Self>> {
        match std::fs::read_to_string(dir.join(Self::FILE)) {
            Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// One document's record in the `weights.bin` layout ([`write_sparse_weights`]): its entry count,
/// its truncation flag, then its pairs — the one encoding of the format, shared by a writer that appends as it
/// encodes (an interruptible run) and by [`write_sparse_weights`].
///
/// # Errors
///
/// `Error::Io`; `Error::Run` for more than `u32::MAX` entries.
pub fn append_sparse_weights(out: &mut impl Write, expansion: &CachedExpansion) -> Result<()> {
    let entries = &expansion.entries;
    let count = u32::try_from(entries.len())
        .map_err(|_| Error::Run(format!("an expansion of {} entries", entries.len())))?;
    out.write_all(&count.to_le_bytes())?;
    out.write_all(&[u8::from(expansion.truncated)])?;
    for &(id, weight) in entries {
        out.write_all(&id.to_le_bytes())?;
        out.write_all(&weight.to_le_bytes())?;
    }
    Ok(())
}

/// Read `weights.bin` back, requiring exactly `documents` expansions and nothing after them.
///
/// # Errors
///
/// `Error::Io`; `Error::Run` for a file that is short, long, or holds another count.
pub fn read_sparse_weights(path: &Path, documents: usize) -> Result<Vec<CachedExpansion>> {
    let bytes = std::fs::read(path)?;
    let short = |i: usize| {
        Error::Run(format!(
            "{} ends inside document {i} of {documents}",
            path.display()
        ))
    };
    let word = |at: usize, i: usize| -> Result<[u8; 4]> {
        bytes
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| short(i))
    };
    let mut out = Vec::with_capacity(documents);
    let mut at = 0usize;
    for i in 0..documents {
        let count = u32::from_le_bytes(word(at, i)?) as usize;
        let truncated = match bytes.get(at + 4) {
            Some(0) => false,
            Some(1) => true,
            Some(flag) => {
                return Err(Error::Run(format!(
                    "{}: document {i} has truncation flag {flag}, not 0 or 1",
                    path.display()
                )));
            }
            None => return Err(short(i)),
        };
        at += 5;
        let mut entries = Vec::with_capacity(count.min(bytes.len() / 8));
        for _ in 0..count {
            let id = u32::from_le_bytes(word(at, i)?);
            let weight = f32::from_le_bytes(word(at + 4, i)?);
            entries.push((id, weight));
            at += 8;
        }
        out.push(CachedExpansion { entries, truncated });
    }
    if at != bytes.len() {
        return Err(Error::Run(format!(
            "{} holds {} bytes after its {documents} documents; the cache and the corpus disagree",
            path.display(),
            bytes.len() - at
        )));
    }
    Ok(out)
}
