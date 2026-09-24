//! `HybridIndex`: the composition root (research D1–D4) — create/open, ingest, commit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use xtriever_core::{
    DocId, Document, Embedder, Error, FieldKind, FieldName, LexicalIndex, Reranker, Result,
    TextKind, Value, VectorIndex,
};
use xtriever_dense::FlatIndex;
use xtriever_dense::model::SPARSE_MODEL_NAME;
use xtriever_dense::sparse::{
    Expansion, QUERY_TABLE, QUERY_TOKENIZER, SparseEncoder, SparseQuery, field_text, validate_scale,
};
use xtriever_lexical::TantivyIndex;

use crate::types::OpenOptions;

/// The refusal every layer shares (`xtriever_core::Error::read_only`), for the pipeline-level
/// operations that never reach a stage (an unstaged `commit`, a `merge`).
fn read_only() -> Error {
    Error::read_only()
}

use crate::descriptor::Descriptor;
use crate::error::{corrupt, schema_err};
use crate::ids::IdMap;
use crate::passages::PassageStore;
use crate::types::lexical_schema;
use crate::{
    FORMAT_VERSION, HybridConfig, SPARSE_FIELD, SPARSE_FORMAT_VERSION, SourceDocument,
    SparseOption, SparseRecord,
};

pub(crate) const LEXICAL_DIR: &str = "lexical";
pub(crate) const DENSE_DIR: &str = "dense";
/// A sparse index's query side (Feature 027 research D6).
pub(crate) const SPARSE_DIR: &str = "sparse";
/// Present from just before the first stage commit until the descriptor is written: an
/// interrupted commit leaves it behind, and `open` refuses the directory (research D3, review
/// round 1 #1 — a same-cardinality partial commit is invisible to the count check alone).
pub(crate) const COMMIT_MARKER: &str = "commit.pending";

/// The hybrid index: a lexical stage, a dense stage, an embedder and the id map under one
/// directory (contract `hybrid-pipeline.md`).
pub struct HybridIndex {
    pub(crate) dir: PathBuf,
    pub(crate) config: HybridConfig,
    pub(crate) descriptor: Descriptor,
    /// The id map as of the last full commit — what `contains` and `search` see. Shared with
    /// `pending_ids` until a change is staged (Feature 010 FR-001, research D5).
    pub(crate) committed_ids: Arc<IdMap>,
    /// The id map with staged changes — what the next commit writes; a private copy
    /// (`Arc::make_mut`) from the first staged change until the commit rejoins the pair.
    pub(crate) pending_ids: Arc<IdMap>,
    pub(crate) dirty: bool,
    pub(crate) lexical: TantivyIndex,
    pub(crate) dense: FlatIndex,
    /// Every document's passage text, read per hit (format version 2, ADR-0008).
    pub(crate) passages: PassageStore,
    pub(crate) embedder: Box<dyn Embedder>,
    /// Attached per handle, never persisted: any re-ranker can serve any index.
    pub(crate) reranker: Option<Box<dyn Reranker>>,
    /// A sparse index's query side, built from its own `sparse/` files (Feature 027).
    pub(crate) sparse_query: Option<SparseQuery>,
    /// The document encoder, attached for building; never needed to search.
    pub(crate) sparse_encoder: Option<SparseEncoder>,
    /// Documents this handle's `add` expanded from a truncated window.
    pub(crate) sparse_truncated: u64,
}

impl std::fmt::Debug for HybridIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridIndex")
            .field("dir", &self.dir)
            .field("live_docs", &self.descriptor.live_docs)
            .field("generation", &self.descriptor.generation)
            .field("rerank_depth", &self.config.rerank_depth)
            .field("rerank_mode", &self.config.rerank_mode)
            .field("fingerprint", &self.embedder.fingerprint())
            .field("reranker", &self.reranker.as_ref().map(|r| r.model_id()))
            .field("sparse", &self.descriptor.sparse)
            .field("sparse_encoder", &self.sparse_encoder.is_some())
            .finish_non_exhaustive()
    }
}

impl HybridConfig {
    /// The configuration's own rules, as `create` and `create_sparse` apply them — public so a
    /// caller can refuse a bad configuration before loading any model (the FFI does).
    ///
    /// # Errors
    ///
    /// `Error::Schema` naming the first rule broken.
    pub fn validate(&self) -> Result<()> {
        if self.dense_fields.is_empty() {
            return Err(schema_err("dense_fields must name at least one text field"));
        }
        for name in &self.dense_fields {
            match self.schema.field(name) {
                Some(def) if matches!(def.kind, FieldKind::Text(_)) => {}
                Some(def) => {
                    return Err(schema_err(format!(
                        "dense field `{name}` is {:?}, expected a Text field",
                        def.kind
                    )));
                }
                None => {
                    return Err(schema_err(format!(
                        "dense field `{name}` is not in the schema"
                    )));
                }
            }
        }
        if self.candidate_depth == 0 {
            return Err(schema_err("candidate_depth must be at least 1"));
        }
        if self.rrf_k == 0 {
            return Err(schema_err("rrf_k must be at least 1"));
        }
        self.rerank_mode.validate()?;
        // The dense stage owns the rule; one definition (Feature 024 review).
        xtriever_dense::validate_compaction_threshold(self.dense_compact_dead_share)?;
        if let Some(option) = &self.sparse {
            validate_sparse(option)?;
            if self.schema.field(&FieldName::from(SPARSE_FIELD)).is_some() {
                return Err(schema_err(format!(
                    "field `{SPARSE_FIELD}` is reserved for the sparse expansion of a sparse index"
                )));
            }
        }
        Ok(())
    }
}

/// The passage a document is embedded — and, on a sparse index, expanded — from: the
/// `dense_fields` that hold text, non-empty, in that order, joined by one space. Public so a
/// caller that encodes offline (`add_embedded`, `add_encoded`) encodes exactly this text.
#[must_use]
pub fn dense_passage(dense_fields: &[FieldName], fields: &BTreeMap<FieldName, Value>) -> String {
    let mut out = String::new();
    for name in dense_fields {
        if let Some(Value::Text(t)) = fields.get(name)
            && !t.is_empty()
        {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(t);
        }
    }
    out
}

/// Copy the encoder's query side into `side` and open it from there, as `open` will: the
/// record the descriptor keeps and the query side this handle searches with.
fn copy_query_side(
    encoder: &SparseEncoder,
    option: SparseOption,
    side: &Path,
) -> Result<(SparseRecord, SparseQuery)> {
    let (tokenizer_sha256, table_sha256) = encoder.write_query_side(side)?;
    let query = SparseQuery::open(
        &side.join(QUERY_TOKENIZER),
        &side.join(QUERY_TABLE),
        &tokenizer_sha256,
        &table_sha256,
    )?;
    let record = SparseRecord {
        scale: option.scale,
        boost: option.boost,
        field: SPARSE_FIELD.to_owned(),
        encoder: encoder.identity().to_owned(),
        tokenizer_sha256,
        table_sha256,
    };
    Ok((record, query))
}

/// The sparse option's rule, checked by `create` and by `open` (which reports a failure as
/// corruption): the scale is the dense crate's (`validate_scale`, which `field_text` applies),
/// the boost is the lexical field's, so its rule lives here. `Error::Schema`.
fn validate_sparse(option: &SparseOption) -> Result<()> {
    validate_scale(option.scale)?;
    if !(option.boost.is_finite() && option.boost > 0.0) {
        return Err(schema_err(format!(
            "sparse boost {} must be finite and above zero",
            option.boost
        )));
    }
    Ok(())
}

impl HybridIndex {
    /// Create at `dir` (created if absent; must be empty). Writes an empty generation.
    ///
    /// # Errors
    ///
    /// `Error::Schema` for an invalid configuration, `Error::Corrupt` for a non-empty directory,
    /// `Error::Io`, and the stages' own creation errors.
    pub fn create(dir: &Path, config: HybridConfig, embedder: Box<dyn Embedder>) -> Result<Self> {
        config.validate()?;
        if config.sparse.is_some() {
            return Err(schema_err(
                "a sparse index is created by HybridIndex::create_sparse, which takes the encoder",
            ));
        }
        Self::create_with(dir, config, embedder, None)
    }

    /// Create a sparse index (Feature 027): as [`create`](Self::create), plus the reserved
    /// `_sparse` field, the encoder's query side copied into `<dir>/sparse/`, and descriptor
    /// format version 3 ([`SPARSE_FORMAT_VERSION`](crate::SPARSE_FORMAT_VERSION), ADR-0016).
    /// `config.sparse` must be set; the encoder is attached to the returned handle, so `add`
    /// can expand documents at once.
    ///
    /// # Errors
    ///
    /// As [`create`](Self::create); `Error::Schema` for a missing or invalid option or a user
    /// field named `_sparse`; `Error::Model` if the encoder's query-side files fail their pins.
    pub fn create_sparse(
        dir: &Path,
        config: HybridConfig,
        embedder: Box<dyn Embedder>,
        encoder: SparseEncoder,
    ) -> Result<Self> {
        config.validate()?;
        if config.sparse.is_none() {
            return Err(schema_err(
                "create_sparse needs config.sparse; an index without the option is created by create",
            ));
        }
        Self::create_with(dir, config, embedder, Some(encoder))
    }

    fn create_with(
        dir: &Path,
        config: HybridConfig,
        embedder: Box<dyn Embedder>,
        encoder: Option<SparseEncoder>,
    ) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        if std::fs::read_dir(dir)?.next().is_some() {
            return Err(corrupt(format!("{} is not empty", dir.display())));
        }
        // The sparse query side first, while the directory is otherwise empty: it is the one
        // step that reads files outside the index (the encoder's), so if it fails the directory
        // is emptied again and a retry can create into it.
        let (sparse, sparse_query) = match (&encoder, config.sparse) {
            (Some(encoder), Some(option)) => {
                let side = dir.join(SPARSE_DIR);
                match copy_query_side(encoder, option, &side) {
                    Ok((record, query)) => (Some(record), Some(query)),
                    Err(e) => {
                        // Best effort: the copy's error is the one worth reporting.
                        let _ = std::fs::remove_dir_all(&side);
                        return Err(e);
                    }
                }
            }
            _ => (None, None),
        };
        let boost = config.sparse.map(|o| o.boost);
        let lexical = TantivyIndex::create(
            &dir.join(LEXICAL_DIR),
            lexical_schema(&config.schema, boost),
        )?;
        let mut dense = FlatIndex::create(
            &dir.join(DENSE_DIR),
            embedder.dim(),
            embedder.metric(),
            embedder.fingerprint(),
        )?;
        dense.set_compaction_threshold(config.dense_compact_dead_share)?;
        let passages = PassageStore::create(dir)?;
        let ids = Arc::new(IdMap::default());
        ids.write(dir)?;
        let descriptor = Descriptor {
            format_version: if sparse.is_some() {
                SPARSE_FORMAT_VERSION
            } else {
                FORMAT_VERSION
            },
            schema: config.schema.clone(),
            embedder_fingerprint: embedder.fingerprint().to_owned(),
            dense_fields: config.dense_fields.clone(),
            candidate_depth: config.candidate_depth,
            rrf_k: config.rrf_k,
            rerank_depth: config.rerank_depth,
            rerank_mode: config.rerank_mode,
            dense_compact_dead_share: config.dense_compact_dead_share,
            sparse,
            live_docs: 0,
            generation: 0,
        };
        descriptor.write(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            config,
            descriptor,
            committed_ids: Arc::clone(&ids),
            pending_ids: ids,
            dirty: false,
            lexical,
            dense,
            passages,
            embedder,
            reranker: None,
            sparse_query,
            sparse_encoder: encoder,
            sparse_truncated: 0,
        })
    }

    /// Open, reading the dense generation into memory.
    ///
    /// # Errors
    ///
    /// `Error::Corrupt` (format version, schema disagreement, partial commit — all naming both
    /// sides), `Error::FingerprintMismatch`, `Error::Io`.
    pub fn open(dir: &Path, embedder: Box<dyn Embedder>) -> Result<Self> {
        Self::open_with(dir, embedder, OpenOptions::default())
    }

    /// Open with the dense stage memory-mapped (feature `mmap`; the dense stage's precondition on
    /// external writers applies — see `xtriever_dense::LoadPath::Mmap`).
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    #[cfg(feature = "mmap")]
    pub fn open_mapped(dir: &Path, embedder: Box<dyn Embedder>) -> Result<Self> {
        Self::open_with(
            dir,
            embedder,
            OpenOptions {
                mapped: true,
                read_only: false,
            },
        )
    }

    /// Open with explicit [`OpenOptions`] (Feature 008 D11). `read_only` opens the lexical
    /// backend without its lock file so a directory nobody can write — an app bundle — opens;
    /// `add`, `delete`, `commit` and `merge` then return `Error::Io` "read-only index".
    /// `mapped` without the `mmap` feature is `Error::Backend`, not a panic.
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open), plus the two above.
    pub fn open_with(
        dir: &Path,
        embedder: Box<dyn Embedder>,
        options: OpenOptions,
    ) -> Result<Self> {
        let OpenOptions { mapped, read_only } = options;
        let descriptor = Descriptor::read(dir)?;
        // An interrupted commit leaves its marker; refuse before looking at anything else. The
        // count check below cannot see a same-cardinality partial commit (a replace that
        // crashed after the lexical stage committed), the marker can.
        let marker = dir.join(COMMIT_MARKER);
        if marker.exists() {
            let generation = std::fs::read_to_string(&marker).unwrap_or_default();
            return Err(corrupt(format!(
                "interrupted commit: {} exists (generation {}); the stages may hold mixed generations",
                marker.display(),
                generation.trim()
            )));
        }
        let lexical = if read_only {
            TantivyIndex::open_read_only(&dir.join(LEXICAL_DIR))?
        } else {
            TantivyIndex::open(&dir.join(LEXICAL_DIR))?
        };
        // A sparse record that could not have been created is corruption (Feature 027); the
        // lexical schema it implies is then checked with the rest of the identity.
        if let Some(record) = &descriptor.sparse {
            if record.field != SPARSE_FIELD {
                return Err(corrupt(format!(
                    "descriptor: sparse field is `{}`, expected `{SPARSE_FIELD}`",
                    record.field
                )));
            }
            validate_sparse(&SparseOption {
                scale: record.scale,
                boost: record.boost,
            })
            .map_err(|e| match e {
                Error::Schema(m) => corrupt(format!("descriptor: {m}")),
                other => other,
            })?;
        }
        descriptor.check_identity(lexical.schema(), embedder.fingerprint())?;
        // The query side is the index's own, verified against the recorded hashes.
        let sparse_query = match &descriptor.sparse {
            Some(record) => {
                let side = dir.join(SPARSE_DIR);
                Some(SparseQuery::open(
                    &side.join(QUERY_TOKENIZER),
                    &side.join(QUERY_TABLE),
                    &record.tokenizer_sha256,
                    &record.table_sha256,
                )?)
            }
            None => None,
        };
        let ids = Arc::new(IdMap::read(dir)?);
        // The store's slot count must equal the id map's length (the fifth count, ADR-0008).
        let passages = PassageStore::open(dir, ids.len())?;
        let dense_dir = dir.join(DENSE_DIR);
        let mut dense = if mapped {
            #[cfg(feature = "mmap")]
            {
                if read_only {
                    FlatIndex::open_mapped_read_only_for(&dense_dir, embedder.as_ref())?
                } else {
                    FlatIndex::open_mapped_for(&dense_dir, embedder.as_ref())?
                }
            }
            #[cfg(not(feature = "mmap"))]
            {
                return Err(Error::Backend(
                    "mapped open needs the `mmap` feature of xtriever-pipeline".into(),
                ));
            }
        } else if read_only {
            FlatIndex::open_read_only_for(&dense_dir, embedder.as_ref())?
        } else {
            FlatIndex::open_for(&dense_dir, embedder.as_ref())?
        };
        // A persisted share that could not have been created is corruption, not a schema
        // error, like `rerank_mode` below — checked whatever the open's mode (the dense stage's
        // one rule); installed on the dense stage only when this handle may write.
        xtriever_dense::validate_compaction_threshold(descriptor.dense_compact_dead_share)
            .map_err(|e| corrupt(format!("descriptor: {e}")))?;
        if !read_only {
            dense.set_compaction_threshold(descriptor.dense_compact_dead_share)?;
        }
        // The four-count check (research D3): every count-changing partial state is a
        // disagreement here — a second, cheap line of defence behind the marker.
        let lexical_live = lexical.stats()?.num_docs;
        let dense_live = dense.len();
        let map_live = ids.live();
        if !(descriptor.live_docs == map_live
            && map_live == lexical_live
            && lexical_live == dense_live)
        {
            return Err(corrupt(format!(
                "partial commit: descriptor {}, id map {}, lexical {}, dense {} live documents",
                descriptor.live_docs, map_live, lexical_live, dense_live
            )));
        }
        let config = HybridConfig {
            schema: descriptor.schema.clone(),
            dense_fields: descriptor.dense_fields.clone(),
            candidate_depth: descriptor.candidate_depth,
            rrf_k: descriptor.rrf_k,
            rerank_depth: descriptor.rerank_depth,
            rerank_mode: descriptor.rerank_mode,
            dense_compact_dead_share: descriptor.dense_compact_dead_share,
            sparse: descriptor.sparse.as_ref().map(|r| SparseOption {
                scale: r.scale,
                boost: r.boost,
            }),
        };
        // A persisted mode that could not have been created is corruption, not a schema error
        // (review round 1 #4): every search would apply an α outside [0, 1].
        config
            .rerank_mode
            .validate()
            .map_err(|e| corrupt(format!("descriptor: {e}")))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            config,
            descriptor,
            committed_ids: Arc::clone(&ids),
            pending_ids: ids,
            dirty: false,
            lexical,
            dense,
            passages,
            embedder,
            reranker: None,
            sparse_query,
            sparse_encoder: None,
            sparse_truncated: 0,
        })
    }

    /// Attach (or detach) the sparse document encoder; nothing is written. Needed only to add
    /// to a sparse index — searching never uses it. The encoder must be the one the index
    /// records, as the embedder must match the index's fingerprint: an index never holds
    /// expansions from two encoders.
    ///
    /// # Errors
    ///
    /// `Error::FingerprintMismatch` naming both identities for another encoder;
    /// `Error::Schema` for an encoder offered to an index without the option.
    pub fn set_sparse_encoder(&mut self, encoder: Option<SparseEncoder>) -> Result<()> {
        if let Some(encoder) = &encoder {
            match &self.descriptor.sparse {
                None => {
                    return Err(schema_err(
                        "this index has no sparse option; a sparse encoder has nothing to expand",
                    ));
                }
                Some(record) if record.encoder != encoder.identity() => {
                    return Err(Error::FingerprintMismatch {
                        index: record.encoder.clone(),
                        current: encoder.identity().to_owned(),
                    });
                }
                Some(_) => {}
            }
        }
        self.sparse_encoder = encoder;
        Ok(())
    }

    /// The sparse record, if this index has the option (Feature 027).
    #[must_use]
    pub fn sparse(&self) -> Option<&SparseRecord> {
        self.descriptor.sparse.as_ref()
    }

    /// The descriptor's format version: [`SPARSE_FORMAT_VERSION`] for a sparse index,
    /// [`FORMAT_VERSION`] otherwise.
    #[must_use]
    pub fn format_version(&self) -> u32 {
        self.descriptor.format_version
    }

    /// Documents this handle expanded from a window truncated to the encoder's 512 tokens.
    #[must_use]
    pub fn sparse_truncated(&self) -> u64 {
        self.sparse_truncated
    }
    /// The stored passage text of a committed internal id — the text the dense stage embedded
    /// (Feature 008: the build's verify pass walks every passage).
    ///
    /// # Errors
    ///
    /// `Error::Corrupt` for an id the store has no slot for; `Error::Io`/`Corrupt` from the store.
    pub fn passage_text(&self, id: DocId) -> Result<String> {
        self.passages.read(id)
    }

    /// The external id of a committed internal id, if live.
    #[must_use]
    pub fn external_id(&self, id: DocId) -> Option<&str> {
        self.committed_ids.external(id)
    }

    /// The configuration the index was created with.
    #[must_use]
    pub fn config(&self) -> &HybridConfig {
        &self.config
    }

    /// The embedder this handle uses.
    #[must_use]
    pub fn embedder(&self) -> &dyn Embedder {
        self.embedder.as_ref()
    }

    /// Attach (or detach) a re-ranker to this handle; nothing is written to the directory.
    pub fn set_reranker(&mut self, reranker: Option<Box<dyn Reranker>>) {
        self.reranker = reranker;
    }

    /// The re-ranker attached to this handle, if any.
    #[must_use]
    pub fn reranker(&self) -> Option<&dyn Reranker> {
        self.reranker.as_deref()
    }

    /// Committed live documents.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.descriptor.live_docs
    }

    /// Whether no document is committed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether `external_id` is a committed live document.
    #[must_use]
    pub fn contains(&self, external_id: &str) -> bool {
        self.committed_ids.internal(external_id).is_some()
    }

    /// The dense passage of a document under this index's configuration ([`dense_passage`]).
    fn passage(&self, fields: &BTreeMap<FieldName, Value>) -> String {
        dense_passage(&self.config.dense_fields, fields)
    }

    /// A document every add path accepts: a non-empty id, and — on a sparse index — no value
    /// for the reserved field, which only the expansion writes.
    fn check_document(&self, doc: &SourceDocument) -> Result<()> {
        if doc.external_id.is_empty() {
            return Err(schema_err("external id must not be empty"));
        }
        if self.descriptor.sparse.is_some()
            && doc.fields.contains_key(&FieldName::from(SPARSE_FIELD))
        {
            return Err(schema_err(format!(
                "document `{}` supplies `{SPARSE_FIELD}`, which only the sparse expansion writes",
                doc.external_id
            )));
        }
        Ok(())
    }

    /// A vector the dense stage can store: the embedder's width.
    fn check_vector(&self, vector: &[f32]) -> Result<()> {
        if vector.len() == self.embedder.dim() {
            Ok(())
        } else {
            Err(Error::DimensionMismatch {
                expected: self.embedder.dim(),
                actual: vector.len(),
            })
        }
    }

    /// The expansion of a passage for this index: `None` without the option, the attached
    /// encoder's otherwise — none needed for a passage with no text (see `stage_one`) — and a
    /// sparse index with no encoder attached cannot be added to.
    fn expand(&self, passage: &str) -> Result<Option<Expansion>> {
        match (&self.descriptor.sparse, &self.sparse_encoder) {
            (None, _) => Ok(None),
            (Some(_), Some(_)) if passage.trim().is_empty() => Ok(Some(Expansion::default())),
            (Some(_), Some(encoder)) => Ok(Some(encoder.encode(passage)?)),
            // Said the same way on every surface: the bindings cannot attach the encoder.
            (Some(_), None) => Err(Error::Model {
                model: SPARSE_MODEL_NAME.to_owned(),
                message: "this index is sparse: documents are added to it only on the build \
                          host, through a handle holding the sparse document encoder — the \
                          handle that created the index (or, in Rust, one given the encoder \
                          with set_sparse_encoder)"
                    .to_owned(),
            }),
        }
    }

    /// Stage one document: its passage, its vector and — on a sparse index — its expansion,
    /// written as the `_sparse` text at this index's scale (`field_text` refuses an expansion
    /// the encoder could not have produced). A passage with no text has an empty `_sparse`
    /// field whatever expansion comes with it: the encoder, given only `[CLS]` and `[SEP]`,
    /// still weights some two dozen vocabulary entries, and a document with no text must not
    /// be found through them. Every add path ends here; a truncated expansion is counted only
    /// once its document is staged.
    fn stage_one(
        &mut self,
        doc: &SourceDocument,
        passage: String,
        vector: &[f32],
        expansion: Option<&Expansion>,
    ) -> Result<()> {
        self.check_vector(vector)?;
        let sparse_text = match (&self.descriptor.sparse, expansion) {
            (Some(record), Some(expansion)) => {
                // Validated either way: an expansion the encoder could not produce is refused.
                let text = field_text(expansion, record.scale)?;
                Some(if passage.trim().is_empty() {
                    String::new()
                } else {
                    text
                })
            }
            (None, None) => None,
            // The callers pair them by construction; say so rather than index half a document.
            (Some(_), None) | (None, Some(_)) => {
                return Err(schema_err(
                    "an expansion is required on a sparse index and refused on any other",
                ));
            }
        };
        let id =
            Arc::make_mut(&mut self.pending_ids).assign(&doc.external_id, doc.chunk.clone())?;
        let fields = match sparse_text {
            Some(text) => {
                let mut fields = doc.fields.clone();
                fields.insert(FieldName::from(SPARSE_FIELD), Value::Text(text));
                fields
            }
            None => doc.fields.clone(),
        };
        self.lexical.add(&[Document {
            id,
            fields,
            chunk: doc.chunk.clone(),
        }])?;
        self.dense.add(id, vector)?;
        self.passages.stage(id.0, Some(passage));
        self.dirty = true;
        if expansion.is_some_and(|e| e.truncated) {
            self.sparse_truncated += 1;
        }
        Ok(())
    }

    /// A document whose vector (and expansion) the caller supplies: checked, then staged.
    fn add_prepared(
        &mut self,
        doc: &SourceDocument,
        vector: &[f32],
        expansion: Option<&Expansion>,
    ) -> Result<()> {
        self.check_document(doc)?;
        let passage = self.passage(&doc.fields);
        self.stage_one(doc, passage, vector, expansion)
    }

    /// Add or replace documents under their external ids; visible after `commit`. On a sparse
    /// index each passage is also expanded by the attached encoder (Feature 027).
    ///
    /// # Errors
    ///
    /// `Error::Schema` (empty id, fields the schema rejects, a document supplying `_sparse`),
    /// `Error::Model` (embedder; the sparse encoder, or its absence on a sparse index),
    /// `Error::DimensionMismatch`, stage errors.
    pub fn add(&mut self, docs: &[SourceDocument]) -> Result<()> {
        for doc in docs {
            self.check_document(doc)?;
            let passage = self.passage(&doc.fields);
            let expansion = self.expand(&passage)?;
            let mut vectors = self
                .embedder
                .embed(&[passage.as_str()], TextKind::Passage)?;
            let vector = vectors.pop().ok_or_else(|| Error::Model {
                model: "embedder".into(),
                message: "returned no vector for the passage".into(),
            })?;
            self.stage_one(doc, passage, &vector, expansion.as_ref())?;
        }
        Ok(())
    }

    /// As [`add`](Self::add) with caller-supplied vectors (embedded offline, or from a cache).
    /// On a sparse index each passage is expanded by the attached encoder, exactly as `add`
    /// expands it (Feature 027, PR C review) — so a cached build and a plain one agree.
    ///
    /// # Errors
    ///
    /// As [`add`](Self::add), including `Error::Model` on a sparse index with no encoder
    /// attached; `Error::DimensionMismatch` if a vector is not `embedder.dim()` wide.
    pub fn add_embedded(&mut self, docs: &[(SourceDocument, Vec<f32>)]) -> Result<()> {
        for (doc, vector) in docs {
            // The caller's vector first: a wrong width must not cost an encoder pass.
            self.check_vector(vector)?;
            self.check_document(doc)?;
            let passage = self.passage(&doc.fields);
            let expansion = self.expand(&passage)?;
            self.stage_one(doc, passage, vector, expansion.as_ref())?;
        }
        Ok(())
    }

    /// As [`add_embedded`](Self::add_embedded), with each document's sparse expansion supplied
    /// too — a sparse index built from caches (Feature 027; the evaluation harness). The
    /// expansion is written exactly as `add` writes the encoder's, so the same expansions and
    /// vectors give the same index.
    ///
    /// # Errors
    ///
    /// As [`add_embedded`](Self::add_embedded) on an index without the option, which refuses
    /// the call; `Error::Schema` for an expansion `field_text` refuses (an id out of order,
    /// repeated, special or outside the vocabulary; a weight the encoder cannot produce).
    pub fn add_encoded(&mut self, docs: &[(SourceDocument, Vec<f32>, Expansion)]) -> Result<()> {
        if self.descriptor.sparse.is_none() {
            return Err(schema_err(
                "add_encoded writes sparse expansions, and this index has no sparse option",
            ));
        }
        for (doc, vector, expansion) in docs {
            self.add_prepared(doc, vector, Some(expansion))?;
        }
        Ok(())
    }

    /// Delete documents by external id; unknown ids are ignored; visible after `commit`.
    ///
    /// # Errors
    ///
    /// Stage errors.
    pub fn delete(&mut self, external_ids: &[&str]) -> Result<()> {
        for ext in external_ids {
            // An unknown id is a no-op and must not split the shared map (review round 1 #2).
            if self.pending_ids.internal(ext).is_none() {
                continue;
            }
            if let Some(id) = Arc::make_mut(&mut self.pending_ids).remove(ext) {
                self.lexical.delete(&[id])?;
                self.dense.delete(&[id])?;
                self.passages.stage(id.0, None);
                self.dirty = true;
            }
        }
        Ok(())
    }

    /// Commit, then compact the dense file (the live rows only, in id order — Feature 024,
    /// ADR-0013) and merge the lexical stage's segments into one (Feature 008 D10). The passage
    /// store has no segments. Staged changes are committed with the dense stage set to rewrite
    /// on any dead row, so the whole merge is one dense protocol — never a durable append
    /// followed by a separate compaction.
    ///
    /// Scores across a merge: the dense stage's are bit-identical by construction (the tests
    /// compare every live row's score). The lexical stage's BM25 statistics are
    /// deletion-inclusive until a merge physically drops deleted or replaced documents
    /// (Feature 002 FR-025), so across a merge that does so BM25 — and hence RRF ranks and
    /// fused bits — can move; a merge that drops nothing (no deletes, no replacements, or a
    /// single segment where the lexical merge is a no-op) leaves every fused bit unchanged.
    /// Recorded in ADR-0013 and pinned by the lexical crate's own test.
    ///
    /// A shipped artefact is one segment: fewer files to map, and `DocAddress` order equal to
    /// `DocId` order.
    ///
    /// # Errors
    ///
    /// `Error::Io` "read-only index" on a read-only open; otherwise as [`commit`](Self::commit)
    /// (a commit that fails before completing stops the merge — nothing is compacted or
    /// merged over an incomplete commit; only a completed commit's unconfirmed dense sync is
    /// carried to the end), the dense compaction (`Error::Corrupt` if another writer changed the dense stage; the
    /// durability-unconfirmed `Error::Io` when the compaction switched but its directory sync
    /// failed — the lexical merge still runs and the error is returned at the end) and the
    /// lexical merge.
    pub fn merge(&mut self) -> Result<()> {
        if self.lexical.is_read_only() {
            return Err(read_only());
        }
        let mut unconfirmed = None;
        if self.dirty {
            // One dense protocol for the merge: a commit with any dead row is a rewrite.
            let configured = self.config.dense_compact_dead_share;
            self.dense.set_compaction_threshold(Some(0.0))?;
            let committed = self.commit();
            self.dense.set_compaction_threshold(configured)?;
            match committed {
                Ok(()) => {}
                // `commit` clears `dirty` only once its protocol is complete; the one error it
                // returns from that state is the dense stage's unconfirmed directory sync, and
                // only that may defer. A commit that failed earlier — a rejected sync retry, a
                // failed passage or id-map write — leaves `dirty` set and stops the merge here,
                // whatever the dense stage's `is_sync_pending()` says.
                Err(e) if !self.dirty => unconfirmed = Some(e),
                Err(e) => return Err(e),
            }
        }
        if let Some(e) = self.dense_step(|dense| dense.compact())? {
            unconfirmed.get_or_insert(e);
        }
        self.lexical.merge()?;
        unconfirmed.map_or(Ok(()), Err)
    }

    /// Run one dense-stage step of a protocol and tell a *switched* outcome from a failure: the
    /// stage reports its manifest switched with the directory sync unconfirmed by an error
    /// with `is_sync_pending()` **and** a changed state; an error that changed nothing (a failed
    /// retry of an earlier sync, a stale-writer refusal, an I/O failure before the switch) is
    /// a failure like any other, so the protocol never advances past a stage that did not
    /// commit. Returns the unconfirmed-durability error to be surfaced at the end.
    fn dense_step(
        &mut self,
        step: impl FnOnce(&mut FlatIndex) -> Result<()>,
    ) -> Result<Option<Error>> {
        let before = self.dense.stats();
        match step(&mut self.dense) {
            Ok(()) => Ok(None),
            Err(e) if self.dense.is_sync_pending() && self.dense.stats() != before => Ok(Some(e)),
            Err(e) => Err(e),
        }
    }

    /// Commit both stages, then the passage store, the id map and the descriptor (research
    /// D3; 006 D7). No-op when nothing is staged.
    ///
    /// # Errors
    ///
    /// `Error::Io`, stage errors. A failure between the steps leaves a state `open` refuses.
    pub fn commit(&mut self) -> Result<()> {
        if self.lexical.is_read_only() {
            return Err(read_only());
        }
        if !self.dirty {
            // Nothing staged — but a dense manifest switch whose directory sync failed earlier
            // (`is_sync_pending`) is retried by the stage's own empty commit, so a caller's
            // retry after that error confirms it.
            return self.dense.commit();
        }
        let generation = self.descriptor.generation + 1;
        // 1. The marker: from here until the descriptor is written, the directory is in flight.
        //    Durable before any stage commits (atomic write, directory synced), so a power loss
        //    cannot keep a stage's new generation while losing the marker that says so.
        let marker = self.dir.join(COMMIT_MARKER);
        xtriever_core::fs::write_atomically(
            &marker,
            &self.dir.join(format!("{COMMIT_MARKER}.tmp")),
            generation.to_string().as_bytes(),
        )?;
        xtriever_core::fs::sync_dir(&self.dir)?;
        // 2–3. The stages, each durable on its own. The dense stage has one non-failure error:
        //    its manifest switched but the directory sync failed (`is_sync_pending`). Its state
        //    is the new one, so the protocol continues to a consistent directory and the error
        //    is returned at the end; the next `commit` on this handle retries the sync.
        self.lexical.commit()?;
        let unconfirmed = self.dense_step(|dense| dense.commit())?;
        // 4. The passage store (format version 2, ADR-0008): one slot per assigned id.
        self.passages.commit(self.pending_ids.len())?;
        // 5–6. The id map, then the descriptor.
        self.pending_ids.write(&self.dir)?;
        let descriptor = Descriptor {
            live_docs: self.pending_ids.live(),
            generation,
            ..self.descriptor.clone()
        };
        descriptor.write(&self.dir)?;
        // 7. Only now is the generation complete — and its completion durable, so a power loss
        //    after a finished commit cannot leave the marker behind to refuse a consistent
        //    directory.
        std::fs::remove_file(&marker)?;
        xtriever_core::fs::sync_dir(&self.dir)?;
        self.descriptor = descriptor;
        self.committed_ids = Arc::clone(&self.pending_ids);
        self.dirty = false;
        unconfirmed.map_or(Ok(()), Err)
    }

    pub(crate) fn external_of(&self, id: DocId) -> Result<&str> {
        self.committed_ids.external(id).ok_or_else(|| {
            corrupt(format!(
                "stage returned internal id {id} unknown to the id map"
            ))
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use xtriever_core::{
        AnalyzerId, Embedder, FieldDef, FieldKind, FieldName, Metric, Result, Schema, TextKind,
        Value, Vector,
    };

    use super::*;

    /// Two dimensions, one vector, a fixed identity: enough to create, add and reopen.
    struct Stub;

    impl Embedder for Stub {
        fn dim(&self) -> usize {
            2
        }
        fn metric(&self) -> Metric {
            Metric::Cosine
        }
        fn fingerprint(&self) -> &str {
            "stub"
        }
        fn max_input_tokens(&self) -> Option<usize> {
            None
        }
        fn embed(&self, texts: &[&str], _: TextKind) -> Result<Vec<Vector>> {
            Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
        }
    }

    fn config() -> HybridConfig {
        let schema = Schema {
            fields: vec![FieldDef {
                name: FieldName::from("text"),
                kind: FieldKind::Text(AnalyzerId("standard".into())),
                indexed: true,
                stored: false,
                boost: 1.0,
            }],
        };
        HybridConfig::new(schema, vec![FieldName::from("text")])
    }

    fn doc(id: &str) -> (SourceDocument, Vec<f32>) {
        let mut fields = BTreeMap::new();
        fields.insert(
            FieldName::from("text"),
            Value::Text(format!("text of {id}")),
        );
        (
            SourceDocument {
                external_id: id.to_owned(),
                fields,
                chunk: None,
            },
            vec![1.0, 0.0],
        )
    }

    fn shared(index: &HybridIndex) -> bool {
        Arc::ptr_eq(&index.committed_ids, &index.pending_ids)
    }

    /// Feature 010 FR-001 (research D5): one id map after open or commit; a private pending
    /// copy only while a change is staged; searches see the committed one throughout.
    #[test]
    fn committed_and_pending_share_until_a_change_is_staged() {
        let tmp = tempfile::tempdir().unwrap();
        let mut index = HybridIndex::create(tmp.path(), config(), Box::new(Stub)).unwrap();
        assert!(shared(&index), "fresh index");
        index.add_embedded(&[doc("a"), doc("b")]).unwrap();
        assert!(!shared(&index), "staged additions split the pair");
        index.commit().unwrap();
        assert!(shared(&index), "commit joins the pair");
        drop(index);

        let mut index = HybridIndex::open(tmp.path(), Box::new(Stub)).unwrap();
        assert!(shared(&index), "after open");
        assert_eq!(index.committed_ids.len(), 2);

        index.add_embedded(&[doc("c")]).unwrap();
        assert!(!shared(&index));
        assert!(
            !index.contains("c"),
            "a search-side view sees the committed map only"
        );
        assert_eq!(index.committed_ids.len(), 2);
        assert_eq!(index.pending_ids.len(), 3);

        index.commit().unwrap();
        assert!(shared(&index));
        assert!(index.contains("c"));
        assert_eq!(index.committed_ids.len(), 3);

        index.delete(&["missing"]).unwrap();
        assert!(
            shared(&index),
            "an unknown-id delete is a no-op and keeps the pair shared"
        );
        index.commit().unwrap();
        assert!(shared(&index));
        index.delete(&["a"]).unwrap();
        assert!(!shared(&index));
        assert!(index.contains("a"), "the removal is staged, not committed");
        index.commit().unwrap();
        assert!(shared(&index));
        assert!(!index.contains("a"));
        assert_eq!(index.len(), 2);
        assert_eq!(index.committed_ids.len(), 3, "the slot stays reserved");
    }
}
