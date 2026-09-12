//! [`TantivyIndex`]: the one public type, implementing [`LexicalIndex`] over tantivy.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use tantivy::indexer::IndexWriterOptions;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use xtriever_core::{
    DocId, DocSet, Document, FieldKind, FieldName, Filter, Hit, IndexStats, LexicalIndex,
    LexicalQuery, Result, Schema, TermStats, Value,
};

use crate::error::{corrupt, map, schema_err};
use crate::schema::{Descriptor, FieldMap};
use crate::{filter, query, search, stats};

/// Per-thread arena for the writer: the backend's own minimum, `MEMORY_BUDGET_NUM_BYTES_MIN =
/// MARGIN_IN_BYTES * 15` (tantivy-0.26.2 `src/indexer/index_writer.rs:29-32`). It is `pub` in a
/// private module and not nameable from here, so the value is repeated with that citation.
const WRITER_MEMORY_BUDGET: usize = 15_000_000;

/// The lexical stage: a BM25 index over tantivy implementing [`LexicalIndex`].
///
/// # Sharing
///
/// One handle, exclusive writes (spec FR-028/FR-029): mutation takes `&mut self` and the type adds
/// no interior locking. Share it through a read-write lock:
///
/// ```ignore
/// let index = std::sync::RwLock::new(TantivyIndex::open(&dir)?);
/// index.read().unwrap().search(&query, None, 10)?;      // many readers
/// { let mut w = index.write().unwrap(); w.add(&docs)?; w.commit()?; } // one writer; blocks readers
/// ```
///
/// A write lock held across `add` + `commit` blocks every query for that duration — accepted as
/// the price of the one-type interface. Uncommitted mutations are discarded on drop: the backend
/// writer's `Drop` abandons them and releases the directory lock (FR-010, FR-011).
///
/// # Two handles on one directory
///
/// Defined, not recommended (research D14):
///
/// | situation | outcome |
/// |---|---|
/// | second `open` while another handle exists | succeeds (reads only) |
/// | second handle mutates while the first holds the writer | `Error::Backend` (lock busy); the first is unaffected |
/// | second handle reads after the first commits | sees its own last reload, not the other's commit |
/// | first handle dropped, then the second mutates | succeeds |
pub struct TantivyIndex {
    schema: Schema,
    fields: FieldMap,
    index: Index,
    reader: IndexReader,
    writer: Option<IndexWriter>,
}

// FR-030: the index must be safely shareable under a caller-supplied lock. The backend's `Index`,
// `IndexReader` and `IndexWriter` are all `Send + Sync`, and this crate adds no `Rc`, `RefCell`,
// thread-local or mutable static; this assertion turns that into a compile error if it changes.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TantivyIndex>();
};

impl fmt::Debug for TantivyIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TantivyIndex")
            .field("fields", &self.schema.fields.len())
            .field("writer", &self.writer.is_some())
            .finish_non_exhaustive()
    }
}

impl TantivyIndex {
    /// Create a new index at `dir` for `schema`. `dir` must not exist or must be empty.
    ///
    /// The schema is validated before anything touches the filesystem, so an invalid schema
    /// produces no directory (FR-006, FR-005).
    pub fn create(dir: &Path, schema: Schema) -> Result<Self> {
        let fields = FieldMap::build(&schema)?;
        if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} exists and is not empty", dir.display()),
            )
            .into());
        }
        std::fs::create_dir_all(dir)?;
        let index = Index::create_in_dir(dir, fields.backend().clone()).map_err(map)?;
        Descriptor::new(schema.clone()).write(dir)?;
        let reader = Self::manual_reader(&index)?;
        Ok(Self {
            schema,
            fields,
            index,
            reader,
            writer: None,
        })
    }

    /// Open an existing index. The descriptor is read first; a missing descriptor, an unsupported
    /// format version, or a schema that does not match the on-disk index is `Error::Corrupt`.
    pub fn open(dir: &Path) -> Result<Self> {
        let descriptor = Descriptor::read(dir)?;
        let fields = FieldMap::build(&descriptor.schema)?;
        let index = Index::open_in_dir(dir).map_err(map)?;
        if index.schema() != *fields.backend() {
            return Err(corrupt(format!(
                "{}: descriptor schema does not match the index schema",
                dir.display()
            )));
        }
        let reader = Self::manual_reader(&index)?;
        Ok(Self {
            schema: descriptor.schema,
            fields,
            index,
            reader,
            writer: None,
        })
    }

    /// Merge all segments into one and make the result visible. Commits pending mutations first.
    ///
    /// Operational compaction, and the controlled "after a merge" step the determinism and
    /// statistics tests need (research D15). A no-op on zero or one segment.
    pub fn merge(&mut self) -> Result<()> {
        self.commit()?;
        // The backend's own merge policy runs in the background, so the segment list read from
        // `meta.json` can be stale by the time `merge` is called; the backend then reports the
        // ids as unknown. Re-read and retry a bounded number of times rather than fail.
        const ATTEMPTS: usize = 8;
        for attempt in 0..ATTEMPTS {
            let ids = self.index.searchable_segment_ids().map_err(map)?;
            if ids.len() < 2 {
                return Ok(());
            }
            let writer = self.writer()?;
            match writer.merge(&ids).wait() {
                Ok(_) => {
                    writer.commit().map_err(map)?;
                    return self.reader.reload().map_err(map);
                }
                Err(tantivy::TantivyError::InvalidArgument(_)) if attempt + 1 < ATTEMPTS => {
                    continue;
                }
                Err(e) => return Err(map(e)),
            }
        }
        Ok(())
    }

    /// `ReloadPolicy::Manual`: nothing changes between two calls unless this handle commits
    /// (research D3). Also the only policy that spawns no watcher thread.
    fn manual_reader(index: &Index) -> Result<IndexReader> {
        index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(map)
    }

    /// The writer, created on first use (research D2). One indexing worker, one merge worker,
    /// minimum arena (D1): thread count is a constant of the crate, not of the host (FR-004), and
    /// one worker is what makes `DocId` allocation deterministic (tantivy `index.rs:596-601`).
    /// Acquiring the directory lock fails with `Error::Backend(LockFailure)` if another writer
    /// holds it, in this or any process (D14).
    fn writer(&mut self) -> Result<&mut IndexWriter> {
        if self.writer.is_none() {
            let options = IndexWriterOptions::builder()
                .num_worker_threads(1)
                .num_merge_threads(1)
                .memory_budget_per_thread(WRITER_MEMORY_BUDGET)
                .build();
            self.writer = Some(self.index.writer_with_options(options).map_err(map)?);
        }
        match self.writer.as_mut() {
            Some(w) => Ok(w),
            None => Err(corrupt("writer vanished after creation")),
        }
    }

    fn id_term(&self, id: DocId) -> Term {
        Term::from_field_u64(self.fields.id_field(), u64::from(id.0))
    }

    /// Validate one document against the schema and build the backend document (FR-007, FR-008a,
    /// FR-008b). `chunk` is neither indexed nor stored. Text fields also get their exact token
    /// count written to the hidden length column (D7).
    fn build_document(&self, doc: &Document) -> Result<TantivyDocument> {
        let mut out = TantivyDocument::new();
        out.add_u64(self.fields.id_field(), u64::from(doc.id.0));
        for (name, value) in &doc.fields {
            let mf = self.fields.get(name)?;
            let mismatch = || {
                schema_err(format!(
                    "field `{name}`: value {} does not match declared kind {:?}",
                    value_kind(value),
                    mf.kind
                ))
            };
            match (&mf.kind, value) {
                (FieldKind::Text(_), Value::Text(s)) => {
                    out.add_text(mf.field, s);
                    if let Some(len_field) = mf.len_field {
                        let n = query::analyze(&self.index, mf.field, s)?.len();
                        out.add_u64(len_field, n as u64);
                    }
                }
                (FieldKind::Keyword, Value::Keyword(s)) => out.add_text(mf.field, s),
                (FieldKind::U64, Value::U64(v)) => out.add_u64(mf.field, *v),
                (FieldKind::I64, Value::I64(v)) | (FieldKind::DateMillis, Value::DateMillis(v)) => {
                    out.add_i64(mf.field, *v);
                }
                (FieldKind::F64, Value::F64(v)) => out.add_f64(mf.field, *v),
                (FieldKind::Bool, Value::Bool(v)) => out.add_bool(mf.field, *v),
                _ => return Err(mismatch()),
            }
        }
        Ok(out)
    }
}

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Text(_) => "Text",
        Value::Keyword(_) => "Keyword",
        Value::U64(_) => "U64",
        Value::I64(_) => "I64",
        Value::F64(_) => "F64",
        Value::Bool(_) => "Bool",
        Value::DateMillis(_) => "DateMillis",
    }
}

impl LexicalIndex for TantivyIndex {
    fn schema(&self) -> &Schema {
        &self.schema
    }

    fn add(&mut self, docs: &[Document]) -> Result<()> {
        // Validate every document before touching the writer, so a bad batch changes nothing.
        let built: Vec<(Term, TantivyDocument)> = docs
            .iter()
            .map(|d| Ok((self.id_term(d.id), self.build_document(d)?)))
            .collect::<Result<_>>()?;
        let writer = self.writer()?;
        for (id_term, doc) in built {
            // delete-then-add: replaces an earlier document with the same id, whether it was
            // committed or added earlier in this batch (backend `delete_term` semantics, D7).
            writer.delete_term(id_term);
            writer.add_document(doc).map_err(map)?;
        }
        Ok(())
    }

    fn delete(&mut self, ids: &[DocId]) -> Result<()> {
        let terms: Vec<Term> = ids.iter().map(|id| self.id_term(*id)).collect();
        let writer = self.writer()?;
        for term in terms {
            // Unknown ids simply match no document (FR-009).
            writer.delete_term(term);
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if let Some(writer) = self.writer.as_mut() {
            writer.commit().map_err(map)?;
            self.reader.reload().map_err(map)?;
        }
        Ok(())
    }

    /// Top-`k` by `(score DESC, DocId ASC)`. The `DocId` tie-break holds **including at the
    /// k-boundary**: the backend's top-k collector is keyed on `(score, DocId)` rather than on its
    /// segment-ordinal-major `DocAddress`, whose order for equal-size segments is random per
    /// process (spec FR-014 as revised; ADR-0005 "Resolution"). A filtered search uses the same
    /// collector, so its scores are bit-identical to the unfiltered ones (FR-022).
    fn search(&self, query: &LexicalQuery, filter: Option<&Filter>, k: usize) -> Result<Vec<Hit>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let searcher = self.reader.searcher();
        let q = query::translate(query, &self.fields, &self.index)?;
        let bitmap = match filter {
            Some(f) => Some(Arc::new(
                filter::resolve(f, &self.fields, &searcher)?
                    .as_bitmap()
                    .clone(),
            )),
            None => None,
        };
        search::top_k(&searcher, q.as_ref(), k, bitmap)
    }

    fn resolve_filter(&self, filter: &Filter) -> Result<DocSet> {
        let searcher = self.reader.searcher();
        filter::resolve(filter, &self.fields, &searcher)
    }

    fn term_stats(&self, field: &FieldName, term: &str) -> Result<Option<TermStats>> {
        let searcher = self.reader.searcher();
        stats::term_stats(&searcher, &self.fields, field, term)
    }

    fn stats(&self) -> Result<IndexStats> {
        let searcher = self.reader.searcher();
        stats::stats(&searcher, &self.fields)
    }
}
