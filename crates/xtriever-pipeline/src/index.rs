//! `HybridIndex`: the composition root (research D1–D4) — create/open, ingest, commit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use xtriever_core::{
    DocId, Document, Embedder, Error, FieldKind, FieldName, LexicalIndex, Result, TextKind, Value,
    VectorIndex,
};
use xtriever_dense::FlatIndex;
use xtriever_lexical::TantivyIndex;

use crate::FORMAT_VERSION;
use crate::descriptor::Descriptor;
use crate::error::{corrupt, schema_err};
use crate::ids::IdMap;
use crate::{HybridConfig, SourceDocument};

pub(crate) const LEXICAL_DIR: &str = "lexical";
pub(crate) const DENSE_DIR: &str = "dense";

/// The hybrid index: a lexical stage, a dense stage, an embedder and the id map under one
/// directory (contract `hybrid-pipeline.md`).
pub struct HybridIndex {
    pub(crate) dir: PathBuf,
    pub(crate) config: HybridConfig,
    pub(crate) descriptor: Descriptor,
    /// The id map as of the last full commit — what `contains` and `search` see.
    pub(crate) committed_ids: IdMap,
    /// The id map with staged changes — what the next commit writes.
    pub(crate) pending_ids: IdMap,
    pub(crate) dirty: bool,
    pub(crate) lexical: TantivyIndex,
    pub(crate) dense: FlatIndex,
    pub(crate) embedder: Box<dyn Embedder>,
}

impl std::fmt::Debug for HybridIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridIndex")
            .field("dir", &self.dir)
            .field("live_docs", &self.descriptor.live_docs)
            .field("generation", &self.descriptor.generation)
            .field("fingerprint", &self.embedder.fingerprint())
            .finish_non_exhaustive()
    }
}

impl HybridConfig {
    fn validate(&self) -> Result<()> {
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
        Ok(())
    }
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
        std::fs::create_dir_all(dir)?;
        if std::fs::read_dir(dir)?.next().is_some() {
            return Err(corrupt(format!("{} is not empty", dir.display())));
        }
        let lexical = TantivyIndex::create(&dir.join(LEXICAL_DIR), config.schema.clone())?;
        let dense = FlatIndex::create(
            &dir.join(DENSE_DIR),
            embedder.dim(),
            embedder.metric(),
            embedder.fingerprint(),
        )?;
        let ids = IdMap::default();
        ids.write(dir)?;
        let descriptor = Descriptor {
            format_version: FORMAT_VERSION,
            schema: config.schema.clone(),
            embedder_fingerprint: embedder.fingerprint().to_owned(),
            dense_fields: config.dense_fields.clone(),
            candidate_depth: config.candidate_depth,
            rrf_k: config.rrf_k,
            live_docs: 0,
            generation: 0,
        };
        descriptor.write(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            config,
            descriptor,
            committed_ids: ids.clone(),
            pending_ids: ids,
            dirty: false,
            lexical,
            dense,
            embedder,
        })
    }

    /// Open, reading the dense generation into memory.
    ///
    /// # Errors
    ///
    /// `Error::Corrupt` (format version, schema disagreement, partial commit — all naming both
    /// sides), `Error::FingerprintMismatch`, `Error::Io`.
    pub fn open(dir: &Path, embedder: Box<dyn Embedder>) -> Result<Self> {
        Self::open_with(dir, embedder, false)
    }

    /// Open with the dense stage memory-mapped (feature `mmap`; the dense stage's precondition on
    /// external writers applies — see `xtriever_dense::LoadPath::Mmap`).
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    #[cfg(feature = "mmap")]
    pub fn open_mapped(dir: &Path, embedder: Box<dyn Embedder>) -> Result<Self> {
        Self::open_with(dir, embedder, true)
    }

    fn open_with(dir: &Path, embedder: Box<dyn Embedder>, mapped: bool) -> Result<Self> {
        let descriptor = Descriptor::read(dir)?;
        let lexical = TantivyIndex::open(&dir.join(LEXICAL_DIR))?;
        descriptor.check_identity(lexical.schema(), embedder.fingerprint())?;
        let ids = IdMap::read(dir)?;
        let dense_dir = dir.join(DENSE_DIR);
        let dense = if mapped {
            #[cfg(feature = "mmap")]
            {
                FlatIndex::open_mapped_for(&dense_dir, embedder.as_ref())?
            }
            #[cfg(not(feature = "mmap"))]
            {
                unreachable!("mapped open is only reachable with the `mmap` feature")
            }
        } else {
            FlatIndex::open_for(&dense_dir, embedder.as_ref())?
        };
        // The four-count check (research D3): every state a crash between the commit steps can
        // leave is a disagreement here.
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
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            config,
            descriptor,
            committed_ids: ids.clone(),
            pending_ids: ids,
            dirty: false,
            lexical,
            dense,
            embedder,
        })
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

    /// The dense passage: the configured text fields, non-empty, joined by one space.
    fn passage(&self, fields: &BTreeMap<FieldName, Value>) -> String {
        let mut out = String::new();
        for name in &self.config.dense_fields {
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

    fn stage_one(&mut self, doc: &SourceDocument, vector: &[f32]) -> Result<()> {
        if vector.len() != self.embedder.dim() {
            return Err(Error::DimensionMismatch {
                expected: self.embedder.dim(),
                actual: vector.len(),
            });
        }
        let id = self
            .pending_ids
            .assign(&doc.external_id, doc.chunk.clone())?;
        self.lexical.add(&[Document {
            id,
            fields: doc.fields.clone(),
            chunk: doc.chunk.clone(),
        }])?;
        self.dense.add(id, vector)?;
        self.dirty = true;
        Ok(())
    }

    /// Add or replace documents under their external ids; visible after `commit`.
    ///
    /// # Errors
    ///
    /// `Error::Schema` (empty id, fields the schema rejects), `Error::Model` (embedder),
    /// `Error::DimensionMismatch`, stage errors.
    pub fn add(&mut self, docs: &[SourceDocument]) -> Result<()> {
        for doc in docs {
            if doc.external_id.is_empty() {
                return Err(schema_err("external id must not be empty"));
            }
            let passage = self.passage(&doc.fields);
            let mut vectors = self
                .embedder
                .embed(&[passage.as_str()], TextKind::Passage)?;
            let vector = vectors.pop().ok_or_else(|| Error::Model {
                model: "embedder".into(),
                message: "returned no vector for the passage".into(),
            })?;
            self.stage_one(doc, &vector)?;
        }
        Ok(())
    }

    /// As [`add`](Self::add) with caller-supplied vectors (embedded offline, or from a cache).
    ///
    /// # Errors
    ///
    /// As [`add`](Self::add); `Error::DimensionMismatch` if a vector is not `embedder.dim()` wide.
    pub fn add_embedded(&mut self, docs: &[(SourceDocument, Vec<f32>)]) -> Result<()> {
        for (doc, vector) in docs {
            if doc.external_id.is_empty() {
                return Err(schema_err("external id must not be empty"));
            }
            self.stage_one(doc, vector)?;
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
            if let Some(id) = self.pending_ids.remove(ext) {
                self.lexical.delete(&[id])?;
                self.dense.delete(&[id])?;
                self.dirty = true;
            }
        }
        Ok(())
    }

    /// Commit both stages, then the id map, then the descriptor (research D3). No-op when
    /// nothing is staged.
    ///
    /// # Errors
    ///
    /// `Error::Io`, stage errors. A failure between the steps leaves a state `open` refuses.
    pub fn commit(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        self.lexical.commit()?;
        self.dense.commit()?;
        self.finish_commit()
    }

    fn finish_commit(&mut self) -> Result<()> {
        self.pending_ids.write(&self.dir)?;
        let descriptor = Descriptor {
            live_docs: self.pending_ids.live(),
            generation: self.descriptor.generation + 1,
            ..self.descriptor.clone()
        };
        descriptor.write(&self.dir)?;
        self.descriptor = descriptor;
        self.committed_ids = self.pending_ids.clone();
        self.dirty = false;
        Ok(())
    }

    /// Simulates a crash after the lexical commit (spec FR-005 test): the lexical stage is
    /// committed, the dense stage, id map and descriptor are not.
    #[doc(hidden)]
    pub fn commit_lexical_only_for_test(&mut self) -> Result<()> {
        self.lexical.commit()
    }

    pub(crate) fn external_of(&self, id: DocId) -> Result<&str> {
        self.committed_ids.external(id).ok_or_else(|| {
            corrupt(format!(
                "stage returned internal id {id} unknown to the id map"
            ))
        })
    }
}
