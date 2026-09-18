//! `HybridIndex`: the composition root (research D1–D4) — create/open, ingest, commit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use xtriever_core::{
    DocId, Document, Embedder, Error, FieldKind, FieldName, LexicalIndex, Reranker, Result,
    TextKind, Value, VectorIndex,
};
use xtriever_dense::FlatIndex;
use xtriever_lexical::TantivyIndex;

use crate::types::OpenOptions;

/// The lexical stage's own refusal, repeated here for the pipeline-level operations that never
/// reach it (an unstaged `commit`, a `merge`).
fn read_only() -> Error {
    Error::Io(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "read-only index",
    ))
}

use crate::FORMAT_VERSION;
use crate::descriptor::Descriptor;
use crate::error::{corrupt, schema_err};
use crate::ids::IdMap;
use crate::passages::PassageStore;
use crate::{HybridConfig, SourceDocument};

pub(crate) const LEXICAL_DIR: &str = "lexical";
pub(crate) const DENSE_DIR: &str = "dense";
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
        self.rerank_mode.validate()?;
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
        let passages = PassageStore::create(dir)?;
        let ids = Arc::new(IdMap::default());
        ids.write(dir)?;
        let descriptor = Descriptor {
            format_version: FORMAT_VERSION,
            schema: config.schema.clone(),
            embedder_fingerprint: embedder.fingerprint().to_owned(),
            dense_fields: config.dense_fields.clone(),
            candidate_depth: config.candidate_depth,
            rrf_k: config.rrf_k,
            rerank_depth: config.rerank_depth,
            rerank_mode: config.rerank_mode,
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
        descriptor.check_identity(lexical.schema(), embedder.fingerprint())?;
        let ids = Arc::new(IdMap::read(dir)?);
        // The store's slot count must equal the id map's length (the fifth count, ADR-0008).
        let passages = PassageStore::open(dir, ids.len())?;
        let dense_dir = dir.join(DENSE_DIR);
        let dense = if mapped {
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
        })
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

    fn stage_one(&mut self, doc: &SourceDocument, passage: String, vector: &[f32]) -> Result<()> {
        if vector.len() != self.embedder.dim() {
            return Err(Error::DimensionMismatch {
                expected: self.embedder.dim(),
                actual: vector.len(),
            });
        }
        let id =
            Arc::make_mut(&mut self.pending_ids).assign(&doc.external_id, doc.chunk.clone())?;
        self.lexical.add(&[Document {
            id,
            fields: doc.fields.clone(),
            chunk: doc.chunk.clone(),
        }])?;
        self.dense.add(id, vector)?;
        self.passages.stage(id.0, Some(passage));
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
            self.stage_one(doc, passage, &vector)?;
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
            let passage = self.passage(&doc.fields);
            self.stage_one(doc, passage, vector)?;
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

    /// Commit, then merge the lexical stage's segments into one (Feature 008 D10). The dense
    /// stage and the passage store have no segments. A shipped artefact is one segment: fewer
    /// files to map, and `DocAddress` order equal to `DocId` order. Scores do not depend on the
    /// segment layout (the backend computes IDF and average field length from searcher-wide
    /// totals); the pipeline tests assert every hit and score bit-identical across a merge.
    ///
    /// # Errors
    ///
    /// `Error::Io` "read-only index" on a read-only open; otherwise as [`commit`](Self::commit)
    /// and the lexical merge.
    pub fn merge(&mut self) -> Result<()> {
        if self.lexical.is_read_only() {
            return Err(read_only());
        }
        self.commit()?;
        self.lexical.merge()
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
            return Ok(());
        }
        let generation = self.descriptor.generation + 1;
        // 1. The marker: from here until the descriptor is written, the directory is in flight.
        let marker = self.dir.join(COMMIT_MARKER);
        std::fs::write(&marker, generation.to_string())?;
        // 2–3. The stages, each durable on its own.
        self.lexical.commit()?;
        self.dense.commit()?;
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
        // 7. Only now is the generation complete.
        std::fs::remove_file(&marker)?;
        self.descriptor = descriptor;
        self.committed_ids = Arc::clone(&self.pending_ids);
        self.dirty = false;
        Ok(())
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
