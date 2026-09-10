//! Plain data types shared by every stage. No I/O; only small, total helpers.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Internal, dense, engine-assigned identifier of one indexed unit (a chunk).
///
/// External string ids are mapped to/from `DocId` by the pipeline's document store; backends
/// only ever see `DocId`. `u32` keeps bitmaps and postings small (Constitution §V).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DocId(pub u32);

impl fmt::Display for DocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Name of a schema field.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FieldName(pub String);

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for FieldName {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Identifier of an analyzer configuration, e.g. `"standard_en"` or `"hf:bert-base-uncased"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnalyzerId(pub String);

/// A field value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Analyzed full text (BM25-searchable).
    Text(String),
    /// Exact-match string (ids, tags, sources).
    Keyword(String),
    /// Unsigned integer.
    U64(u64),
    /// Signed integer.
    I64(i64),
    /// Floating point.
    F64(f64),
    /// Boolean.
    Bool(bool),
    /// Timestamp, milliseconds since the Unix epoch (UTC).
    DateMillis(i64),
}

/// Type of a field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    /// Full text, analyzed with the given analyzer.
    Text(AnalyzerId),
    /// Exact-match string.
    Keyword,
    /// Unsigned integer.
    U64,
    /// Signed integer.
    I64,
    /// Floating point.
    F64,
    /// Boolean.
    Bool,
    /// Timestamp in Unix milliseconds.
    DateMillis,
}

/// Declaration of one field in a [`Schema`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    /// Field name (unique within the schema).
    pub name: FieldName,
    /// Field type.
    pub kind: FieldKind,
    /// Searchable (text) or filterable (other kinds).
    pub indexed: bool,
    /// Original value is stored and can be returned with hits.
    pub stored: bool,
    /// Query-time weight multiplier for text fields (1.0 = neutral).
    pub boost: f32,
}

/// Ordered set of field declarations.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    /// Field declarations.
    pub fields: Vec<FieldDef>,
}

impl Schema {
    /// Look up a field by name.
    pub fn field(&self, name: &FieldName) -> Option<&FieldDef> {
        self.fields.iter().find(|f| &f.name == name)
    }
}

/// Provenance of a chunk; lets the pipeline group hits by parent document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkInfo {
    /// External id of the parent (source) document.
    pub parent: String,
    /// Position of this chunk within the parent (0-based).
    pub ordinal: u32,
    /// Byte range of the chunk within the parent text, if known.
    pub byte_range: Option<(u64, u64)>,
}

/// One indexable unit: a chunk with its fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Internal id, assigned by the pipeline, unique across the index.
    pub id: DocId,
    /// Field values; validated against the [`Schema`] by the index.
    pub fields: BTreeMap<FieldName, Value>,
    /// Chunk provenance, if this document is a chunk of a larger one.
    pub chunk: Option<ChunkInfo>,
}

/// Structured metadata filter.
///
/// Evaluated by the component that owns metadata (the lexical index in v0). Other stages
/// receive the result as a [`DocSet`] via [`crate::LexicalIndex::resolve_filter`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Filter {
    /// Field equals value.
    Eq(FieldName, Value),
    /// Field equals any of the values.
    In(FieldName, Vec<Value>),
    /// Field within inclusive bounds; `None` = unbounded on that side.
    Range(FieldName, Option<Value>, Option<Value>),
    /// Field is present.
    Exists(FieldName),
    /// All sub-filters match.
    And(Vec<Filter>),
    /// Any sub-filter matches.
    Or(Vec<Filter>),
    /// Sub-filter does not match.
    Not(Box<Filter>),
    /// Explicit allow-list of ids.
    Ids(Vec<DocId>),
}

/// A resolved set of document ids (compressed bitmap).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocSet(RoaringBitmap);

impl DocSet {
    /// Empty set.
    pub fn new() -> Self {
        Self::default()
    }
    /// Whether `id` is in the set.
    pub fn contains(&self, id: DocId) -> bool {
        self.0.contains(id.0)
    }
    /// Insert `id`; returns `true` if it was not already present.
    pub fn insert(&mut self, id: DocId) -> bool {
        self.0.insert(id.0)
    }
    /// Number of ids.
    pub fn len(&self) -> u64 {
        self.0.len()
    }
    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Ids in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = DocId> + '_ {
        self.0.iter().map(DocId)
    }
    /// Set intersection.
    pub fn intersection(&self, other: &Self) -> Self {
        Self(&self.0 & &other.0)
    }
    /// Set union.
    pub fn union(&self, other: &Self) -> Self {
        Self(&self.0 | &other.0)
    }
    /// Set difference (`self − other`).
    pub fn difference(&self, other: &Self) -> Self {
        Self(&self.0 - &other.0)
    }
    /// Underlying bitmap, for backends that can consume it directly.
    pub fn as_bitmap(&self) -> &RoaringBitmap {
        &self.0
    }
}

impl FromIterator<DocId> for DocSet {
    fn from_iter<I: IntoIterator<Item = DocId>>(iter: I) -> Self {
        Self(iter.into_iter().map(|d| d.0).collect())
    }
}

impl From<RoaringBitmap> for DocSet {
    fn from(b: RoaringBitmap) -> Self {
        Self(b)
    }
}

/// Lexical query tree. Text in `Match`/`Phrase` is analyzed by the field's analyzer;
/// `Term`/`Fuzzy` match indexed terms verbatim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LexicalQuery {
    /// Analyzed free text, terms OR-ed, BM25-scored. `None` field = all text fields.
    Match(Option<FieldName>, String),
    /// Analyzed phrase with positional slop.
    Phrase(FieldName, String, u32),
    /// Exact term.
    Term(FieldName, String),
    /// Term within a Levenshtein distance.
    Fuzzy(FieldName, String, u8),
    /// Boolean combination.
    Bool {
        /// Every sub-query must match.
        must: Vec<LexicalQuery>,
        /// At least one should match; each contributes to the score.
        should: Vec<LexicalQuery>,
        /// None may match.
        must_not: Vec<LexicalQuery>,
    },
    /// Multiply the sub-query's score.
    Boost(Box<LexicalQuery>, f32),
}

/// A scored document reference.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    /// Document id.
    pub id: DocId,
    /// Stage-specific score; higher is better.
    pub score: f32,
}

/// Corpus statistics for one term.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermStats {
    /// Number of documents containing the term.
    pub doc_freq: u64,
    /// Total occurrences across the corpus.
    pub total_term_freq: u64,
}

/// Corpus-level statistics.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexStats {
    /// Live (non-deleted) documents.
    pub num_docs: u64,
    /// Average token count per text field (BM25 length normalisation).
    pub avg_field_len: BTreeMap<FieldName, f32>,
}

/// Dense embedding.
pub type Vector = Vec<f32>;

/// Similarity metric of an embedding space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Metric {
    /// Cosine similarity (vectors normalised).
    Cosine,
    /// Inner product.
    Dot,
    /// Negative Euclidean distance.
    Euclidean,
}

/// Role of a text being embedded (asymmetric models prepend different instructions).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextKind {
    /// A user query.
    Query,
    /// An indexed passage/chunk.
    Passage,
}

/// A candidate passage handed to a [`crate::Reranker`].
#[derive(Clone, Copy, Debug)]
pub struct Passage<'a> {
    /// Document id.
    pub id: DocId,
    /// Text to score against the query.
    pub text: &'a str,
}

/// Resource budget for an optional stage. Expressed as a duration, not an instant, because
/// `std::time::Instant` is unavailable on `wasm32-unknown-unknown` (Constitution §III).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Budget {
    /// Wall-clock limit for the whole call.
    pub max_time: Option<Duration>,
    /// Maximum number of items to process.
    pub max_items: Option<usize>,
}

/// Token produced by an [`crate::Analyzer`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Normalised term text.
    pub text: String,
    /// Position in the token stream (phrase queries).
    pub position: u32,
    /// Byte offsets `[start, end)` in the source text (highlighting).
    pub offset: (usize, usize),
}

/// Name of a ranking feature, e.g. `"bm25.score"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FeatureName(pub Cow<'static, str>);

impl FeatureName {
    /// Feature name from a static string.
    pub const fn from_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }
}

impl From<String> for FeatureName {
    fn from(s: String) -> Self {
        Self(Cow::Owned(s))
    }
}

impl fmt::Display for FeatureName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Well-known feature names populated by the pipeline. The LTR spec may add more.
pub mod features {
    /// BM25 score from the lexical stage (`NaN` if not retrieved lexically).
    pub const BM25_SCORE: &str = "bm25.score";
    /// 1-based rank in the lexical result list.
    pub const BM25_RANK: &str = "bm25.rank";
    /// Dense similarity score.
    pub const DENSE_SCORE: &str = "dense.score";
    /// 1-based rank in the dense result list.
    pub const DENSE_RANK: &str = "dense.rank";
    /// Fused (RRF) score.
    pub const FUSED_SCORE: &str = "fused.score";
    /// Cross-encoder score.
    pub const RERANK_SCORE: &str = "rerank.score";
    /// Token count of the document text.
    pub const DOC_LEN: &str = "doc.len";
    /// Token count of the query.
    pub const QUERY_LEN: &str = "query.len";
    /// Fraction of query terms present in the document.
    pub const TERM_OVERLAP: &str = "overlap.ratio";
}

/// Row-major feature matrix, one row per candidate. `NaN` encodes a missing value, which
/// gradient-boosted trees handle natively.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureMatrix {
    names: Vec<FeatureName>,
    rows: usize,
    data: Vec<f32>,
}

impl FeatureMatrix {
    /// Empty matrix with the given columns.
    pub fn new(names: Vec<FeatureName>) -> Self {
        Self {
            names,
            rows: 0,
            data: Vec::new(),
        }
    }
    /// Column names, in order.
    pub fn names(&self) -> &[FeatureName] {
        &self.names
    }
    /// Number of columns.
    pub fn width(&self) -> usize {
        self.names.len()
    }
    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.rows
    }
    /// Append one row; its length must equal [`Self::width`].
    pub fn push_row(&mut self, row: &[f32]) -> Result<()> {
        if row.len() != self.width() {
            return Err(Error::DimensionMismatch {
                expected: self.width(),
                actual: row.len(),
            });
        }
        self.data.extend_from_slice(row);
        self.rows += 1;
        Ok(())
    }
    /// Row `i`, or `None` if out of range.
    pub fn row(&self, i: usize) -> Option<&[f32]> {
        if i >= self.rows {
            return None;
        }
        let w = self.width();
        self.data.get(i * w..(i + 1) * w)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn docset_algebra() {
        let a: DocSet = [1, 2, 3].into_iter().map(DocId).collect();
        let b: DocSet = [2, 3, 4].into_iter().map(DocId).collect();
        assert_eq!(
            a.intersection(&b).iter().collect::<Vec<_>>(),
            vec![DocId(2), DocId(3)]
        );
        assert_eq!(a.union(&b).len(), 4);
        assert!(a.difference(&b).contains(DocId(1)));
    }

    #[test]
    fn feature_matrix_rejects_ragged_rows() {
        let mut m = FeatureMatrix::new(vec![
            FeatureName::from_static("a"),
            FeatureName::from_static("b"),
        ]);
        m.push_row(&[1.0, 2.0]).unwrap();
        assert!(matches!(
            m.push_row(&[1.0]),
            Err(Error::DimensionMismatch {
                expected: 2,
                actual: 1
            })
        ));
        assert_eq!(m.row(0), Some(&[1.0, 2.0][..]));
        assert_eq!(m.row(1), None);
    }
}
