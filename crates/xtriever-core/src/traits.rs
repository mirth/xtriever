//! The stage contracts. Everything in the pipeline is programmed against these (Constitution §V).

use crate::error::Result;
use crate::types::{
    AnalyzerId, Budget, DocId, DocSet, Document, FeatureMatrix, FeatureName, FieldName, Filter,
    Hit, IndexStats, LexicalQuery, Metric, Passage, Schema, TermStats, TextKind, Token, Vector,
};

/// Text → tokens. Used by the lexical index (adapted to its native tokenizer API) and by the
/// pipeline for query-side features and highlighting. Must be deterministic and total.
pub trait Analyzer: Send + Sync {
    /// Stable identity of this configuration (recorded in the schema).
    fn id(&self) -> &AnalyzerId;
    /// Tokenise `text`.
    fn analyze(&self, text: &str) -> Vec<Token>;
}

/// Stage 1a — lexical retrieval (BM25). Also owns metadata and filter evaluation in v0.
///
/// Mutation takes `&mut self`; a pipeline needing concurrent readers wraps the index in a
/// lock. A writer/reader split is deferred to the lexical stage's own spec.
pub trait LexicalIndex: Send + Sync {
    /// The schema this index was created with.
    fn schema(&self) -> &Schema;
    /// Add or replace documents (same `DocId` ⇒ replaced after `commit`).
    fn add(&mut self, docs: &[Document]) -> Result<()>;
    /// Delete documents; unknown ids are ignored.
    fn delete(&mut self, ids: &[DocId]) -> Result<()>;
    /// Make prior mutations durable and visible to `search`.
    fn commit(&mut self) -> Result<()>;
    /// Top-`k` hits, highest score first, ties broken by ascending `DocId`.
    ///
    /// The tie-break is the implementation's responsibility, not the backend's: tantivy orders ties
    /// by ascending `DocAddress` (segment-ordinal-major), which agrees with this contract only
    /// while the index has one segment. Implementations re-sort into `(score DESC, DocId ASC)`
    /// before returning. See `docs/adr/0005-tie-breaking-contract.md`.
    fn search(&self, query: &LexicalQuery, filter: Option<&Filter>, k: usize) -> Result<Vec<Hit>>;
    /// Resolve a filter into an allow-list for other stages.
    fn resolve_filter(&self, filter: &Filter) -> Result<DocSet>;
    /// Corpus statistics for an indexed term; `None` if unseen.
    fn term_stats(&self, field: &FieldName, term: &str) -> Result<Option<TermStats>>;
    /// Corpus-level statistics.
    fn stats(&self) -> Result<IndexStats>;
}

/// Text → dense vectors.
pub trait Embedder: Send + Sync {
    /// Output dimensionality.
    fn dim(&self) -> usize;
    /// Similarity metric the model was trained for.
    fn metric(&self) -> Metric;
    /// Stable identity of the model **and** its preprocessing (revision SHA, pooling,
    /// normalisation, prefixes, max length). Stored with every vector index; a mismatch at
    /// open time is a hard error (Constitution §VI).
    fn fingerprint(&self) -> &str;
    /// Maximum input length in model tokens, if bounded.
    fn max_input_tokens(&self) -> Option<usize>;
    /// Embed a batch. Output length equals input length; every vector has `dim()` entries.
    fn embed(&self, texts: &[&str], kind: TextKind) -> Result<Vec<Vector>>;
}

/// Stage 1b — nearest-neighbour search over embeddings (flat or ANN).
pub trait VectorIndex: Send + Sync {
    /// Vector dimensionality accepted by this index.
    fn dim(&self) -> usize;
    /// Similarity metric used by `search`.
    fn metric(&self) -> Metric;
    /// Fingerprint of the embedder whose vectors this index holds.
    fn fingerprint(&self) -> &str;
    /// Add or replace a vector.
    fn add(&mut self, id: DocId, vector: &[f32]) -> Result<()>;
    /// Delete vectors; unknown ids are ignored.
    fn delete(&mut self, ids: &[DocId]) -> Result<()>;
    /// Make prior mutations durable and visible to `search`.
    fn commit(&mut self) -> Result<()>;
    /// Top-`k` by similarity restricted to `allowed` (if given), highest score first, ties
    /// broken by ascending `DocId`.
    ///
    /// As with [`LexicalIndex::search`], the implementation owns the tie-break — re-sort into
    /// `(score DESC, DocId ASC)` if the backend orders ties differently
    /// (`docs/adr/0005-tie-breaking-contract.md`).
    fn search(&self, query: &[f32], allowed: Option<&DocSet>, k: usize) -> Result<Vec<Hit>>;
    /// Number of live vectors.
    fn len(&self) -> u64;
    /// Whether the index holds no vectors.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Stage 2 — query-aware re-scoring of a small candidate set (cross-encoder, late interaction).
pub trait Reranker: Send + Sync {
    /// Stable model identity (name + revision).
    fn model_id(&self) -> &str;
    /// One score per passage, in input order. `None` means "not scored within `budget`"; the
    /// pipeline keeps the incoming score for those entries (graceful degradation, §VI).
    fn rerank(
        &self,
        query: &str,
        passages: &[Passage<'_>],
        budget: &Budget,
    ) -> Result<Vec<Option<f32>>>;
}

/// Stage 3 — learned ranking function (e.g. LightGBM LambdaMART) over per-candidate features.
pub trait Ranker: Send + Sync {
    /// Stable model identity (file hash or training run id).
    fn model_id(&self) -> &str;
    /// Features the model expects, in column order; the pipeline builds the matrix to match.
    fn feature_names(&self) -> &[FeatureName];
    /// One score per row of `features`; higher is better.
    fn score(&self, features: &FeatureMatrix) -> Result<Vec<f32>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check: stage traits must stay object-safe so the pipeline can hold
    // `Box<dyn Trait>` and swap backends via configuration.
    #[allow(dead_code)]
    fn assert_object_safe(
        _: &dyn Analyzer,
        _: &dyn LexicalIndex,
        _: &dyn Embedder,
        _: &dyn VectorIndex,
        _: &dyn Reranker,
        _: &dyn Ranker,
    ) {
    }
}
