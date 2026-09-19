//! `FlatIndex`: a flat, exact `VectorIndex` over an append-only row file and an atomically
//! replaced manifest (Feature 024, ADR-0013; research D3–D7). `commit` appends, `compact`
//! rewrites under a new generation; a committed byte is never modified in place.

mod format;
mod search;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use roaring::RoaringBitmap;
use xtriever_core::{DocId, DocSet, Embedder, Error, Hit, Metric, Result, VectorIndex};

use crate::FORMAT_VERSION;
use crate::LoadPath;
use crate::bytes::{self, Bytes};
use crate::error::{corrupt, dim_mismatch, schema_err};
use format::{Header, MANIFEST, MANIFEST_TMP, Rows, V1_FILE};

/// What the manifest says about the committed state — for tests, records and `about`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DenseStats {
    /// Committed rows in the row file, dead ones included.
    pub rows: u64,
    /// Rows that search visits (`len()`).
    pub live: u64,
    /// Tombstoned rows: deleted, or superseded by a replacement.
    pub dead: u64,
    /// The row file's generation (`vectors.<generation>.bin`); `compact` advances it.
    pub generation: u64,
}

/// A flat, exact vector index: `vectors.<g>.bin` (rows, append-only) + `manifest.bin`.
///
/// `add` and `delete` stage changes in memory; `commit` appends the new rows, marks superseded
/// and deleted rows dead, and replaces the manifest by `rename`; `compact` writes the live rows
/// under a new generation and switches the manifest the same way. A reader (or a mapping) of
/// the previous state is never disturbed by either, and a crash at any point leaves either the
/// previous manifest or the new one — the previous committed state or the fully committed
/// new one, never a partial state.
#[derive(Debug)]
pub struct FlatIndex {
    dir: PathBuf,
    header: Header,
    load_path: LoadPath,
    /// The row file at its committed length (possibly longer on disk after a crash: only
    /// `layout.count` rows are ever read).
    rows: Bytes,
    layout: Rows,
    dead: RoaringBitmap,
    /// The live row of every live id. A map, not a table indexed by id: memory follows the
    /// row count, whatever ids the caller chooses, and it iterates in ascending id order —
    /// the order a compaction writes.
    rows_by_id: BTreeMap<u32, u32>,
    /// Whether the file's ids are strictly ascending (so a compaction with no dead rows would
    /// write the same rows again — skipped).
    ascending: bool,
    /// `Some(vector)` = add or replace, `None` = delete. Invisible until `commit`.
    pending: BTreeMap<DocId, Option<Vec<f32>>>,
    /// `commit` compacts when `dead / rows` exceeds this (`None` = only on `compact`).
    compaction_threshold: Option<f32>,
    /// A read-only open: no truncation or sweep at open, every mutation refused.
    read_only: bool,
}

impl FlatIndex {
    /// Create at `dir` (created if absent; must be empty). Writes an empty generation.
    ///
    /// # Errors
    ///
    /// `Error::Corrupt` if `dir` is not empty or `dim == 0`; `Error::Io` otherwise.
    pub fn create(dir: &Path, dim: usize, metric: Metric, fingerprint: &str) -> Result<Self> {
        if dim == 0 {
            return Err(corrupt("dim must be at least 1"));
        }
        // The row layout must be representable before a byte is written: a refused `dim` must
        // not leave a populated, unusable directory behind.
        if Rows::checked(0, dim).is_none() {
            return Err(corrupt(format!(
                "dim {dim} does not fit this platform's row layout"
            )));
        }
        std::fs::create_dir_all(dir)?;
        if std::fs::read_dir(dir)?.next().is_some() {
            return Err(corrupt(format!("{} is not empty", dir.display())));
        }
        let header = Header {
            format_version: FORMAT_VERSION,
            dim,
            metric: metric.into(),
            fingerprint: fingerprint.to_owned(),
            generation: 0,
            rows: 0,
            live: 0,
            tombstones_len: 0,
        };
        std::fs::File::create(dir.join(format::row_file(0)))?.sync_all()?;
        sync_dir(dir)?;
        write_manifest(dir, &header, &RoaringBitmap::new()).map_err(ManifestFailure::into_error)?;
        Self::open_with(dir, LoadPath::Buffered, false)
    }

    /// Open, reading the committed rows into memory.
    ///
    /// # Errors
    ///
    /// `Error::Io` if a file cannot be read; `Error::Corrupt` if it is not a version-2 index
    /// (a version-1 `index.bin` is named as such).
    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_with(dir, LoadPath::Buffered, false)
    }

    /// Open read-only, reading the committed rows into memory: nothing in the directory is
    /// touched — no crashed tail is cut, no stale generation swept — and `commit` / `compact`
    /// are refused. For a directory that is read-only by construction (an app bundle) or one
    /// this handle must not alter. The precondition every open has stands: no writer changes
    /// the directory while the handle lives — a concurrent compaction could switch the
    /// manifest and remove the generation between the manifest and the row file being read.
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    pub fn open_read_only(dir: &Path) -> Result<Self> {
        Self::open_with(dir, LoadPath::Buffered, true)
    }

    /// [`open_read_only`](Self::open_read_only) plus the agreement checks of
    /// [`open_for`](Self::open_for).
    ///
    /// # Errors
    ///
    /// As [`open_for`](Self::open_for).
    pub fn open_read_only_for(dir: &Path, embedder: &dyn Embedder) -> Result<Self> {
        let index = Self::open_read_only(dir)?;
        index.check_agreement(embedder)?;
        Ok(index)
    }

    /// Open, mapping the committed rows read-only (feature `mmap`, ADR-0007 as amended by
    /// ADR-0013).
    ///
    /// **Precondition the caller owns**: no other process may modify or truncate the row file
    /// `dir/vectors.<g>.bin` while this handle lives. This crate itself never modifies a mapped
    /// byte — `commit` only appends beyond every mapping's end and `compact` replaces the file
    /// by `rename` — but a mapping cannot defend against external writers, which is why this
    /// constructor exists only behind the opt-in feature. See [`crate::LoadPath::Mmap`].
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    #[cfg(feature = "mmap")]
    pub fn open_mapped(dir: &Path) -> Result<Self> {
        Self::open_with(dir, LoadPath::Mmap, false)
    }

    /// [`open_mapped`](Self::open_mapped) as a read-only open (see
    /// [`open_read_only`](Self::open_read_only)).
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    #[cfg(feature = "mmap")]
    pub fn open_mapped_read_only(dir: &Path) -> Result<Self> {
        Self::open_with(dir, LoadPath::Mmap, true)
    }

    /// [`open_mapped_read_only`](Self::open_mapped_read_only) plus the agreement checks of
    /// [`open_for`](Self::open_for).
    ///
    /// # Errors
    ///
    /// As [`open_for`](Self::open_for).
    #[cfg(feature = "mmap")]
    pub fn open_mapped_read_only_for(dir: &Path, embedder: &dyn Embedder) -> Result<Self> {
        let index = Self::open_mapped_read_only(dir)?;
        index.check_agreement(embedder)?;
        Ok(index)
    }

    /// [`open`](Self::open) plus fingerprint / dim / metric agreement with `embedder` (FR-015).
    ///
    /// # Errors
    ///
    /// `Error::FingerprintMismatch` naming both fingerprints; `Error::Corrupt` on a dim or metric
    /// disagreement; otherwise as [`open`](Self::open).
    pub fn open_for(dir: &Path, embedder: &dyn Embedder) -> Result<Self> {
        let index = Self::open(dir)?;
        index.check_agreement(embedder)?;
        Ok(index)
    }

    /// [`open_mapped`](Self::open_mapped) plus the agreement checks of [`open_for`](Self::open_for).
    ///
    /// # Errors
    ///
    /// As [`open_for`](Self::open_for).
    #[cfg(feature = "mmap")]
    pub fn open_mapped_for(dir: &Path, embedder: &dyn Embedder) -> Result<Self> {
        let index = Self::open_mapped(dir)?;
        index.check_agreement(embedder)?;
        Ok(index)
    }

    /// The directory this index lives in.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The committed vector stored under `id`, exactly as it was added; `None` if `id` has no
    /// live row (pending changes are not visible, as with `search`).
    #[must_use]
    pub fn vector(&self, id: DocId) -> Option<Vec<f32>> {
        let r = self.live_row(id)?;
        Some(self.layout.row_at(self.rows.as_slice(), r).collect())
    }

    /// Whether this handle was opened read-only (`open_read_only*`): mutations are refused and
    /// the directory is never touched.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// What the manifest says: rows, live, dead, generation.
    #[must_use]
    pub fn stats(&self) -> DenseStats {
        DenseStats {
            rows: self.header.rows,
            live: self.header.live,
            dead: self.dead.len(),
            generation: self.header.generation,
        }
    }

    /// Make `commit` compact the row file when the dead-row share it leaves exceeds `share`
    /// (`0.0..=1.0`; `None`, the default, compacts only on [`compact`](Self::compact)).
    ///
    /// # Errors
    ///
    /// `Error::Schema` for a share outside `0.0..=1.0` or not finite.
    pub fn set_compaction_threshold(&mut self, share: Option<f32>) -> Result<()> {
        if let Some(s) = share
            && !(s.is_finite() && (0.0..=1.0).contains(&s))
        {
            return Err(schema_err(format!(
                "compaction threshold {s} is not in 0.0..=1.0"
            )));
        }
        self.compaction_threshold = share;
        Ok(())
    }

    /// Rewrite the row file with the live rows only — the pending changes folded in — in
    /// ascending id order, under the next generation: the manifest switches to it by one
    /// `rename` and the old row file is removed. A no-op when nothing is pending, nothing is
    /// dead and the rows already ascend.
    ///
    /// On a handle from `open_read_only*` this is `Error::Io` (permission denied, "read-only
    /// index"); on a directory that cannot be written the underlying `Error::Io` surfaces.
    ///
    /// # Errors
    ///
    /// `Error::Io`; `Error::Corrupt` if the generation counter cannot advance. A failure before
    /// the manifest is renamed leaves the previous state on disk and in this handle, pending
    /// changes included (the partial new file is removed, or swept at the next open). A failure
    /// *after* the rename — only the directory sync — leaves the switched state, adopted by
    /// this handle, and names itself as such.
    pub fn compact(&mut self) -> Result<()> {
        if self.read_only {
            return Err(read_only());
        }
        if self.pending.is_empty() && self.dead.is_empty() && self.ascending {
            return Ok(());
        }
        self.rewrite()
    }

    /// The rewrite protocol: committed live rows ⊕ pending, ascending by id, into a new
    /// generation; one manifest rename; nothing in memory changes before it. Used by `compact`
    /// and by `commit` when the configured dead-row share would be exceeded — so a commit is
    /// always one protocol, never a durable append followed by a separate compaction.
    fn rewrite(&mut self) -> Result<()> {
        let generation = self.header.generation.checked_add(1).ok_or_else(|| {
            corrupt(format!(
                "generation {} cannot advance; the row file's generation counter is exhausted",
                self.header.generation
            ))
        })?;
        // The new generation's row count, validated against the row-space limit before any
        // I/O: the committed live rows, minus those a pending change supersedes or deletes,
        // plus the pending inserts and replacements.
        let superseded = self
            .pending
            .keys()
            .filter(|id| self.live_row(**id).is_some())
            .count();
        let adds = self.pending.values().filter(|c| c.is_some()).count();
        let total = self.rows_by_id.len() - superseded + adds;
        row_space(0, total)?;
        let old_path = self.dir.join(format::row_file(self.header.generation));
        let bytes = self.rows.as_slice();
        let mut buf = Vec::with_capacity(total * self.layout.row_bytes);
        let mut rows_by_id = BTreeMap::new();
        let mut committed = self.rows_by_id.iter().peekable();
        let mut pending = self.pending.iter().peekable();
        let mut next = 0u32;
        let mut push = |id: u32, norm: f32, row: &[f32], rows_by_id: &mut BTreeMap<u32, u32>| {
            format::encode_row(&mut buf, id, norm, row);
            rows_by_id.insert(id, next);
            next += 1;
        };
        loop {
            let next_committed = committed.peek().map(|(id, r)| (**id, **r));
            let next_pending = pending.peek().map(|(id, change)| (id.0, change.as_ref()));
            match (next_committed, next_pending) {
                (None, None) => break,
                // A committed row with no pending change: kept.
                (Some((id, r)), np) if np.is_none_or(|(pid, _)| pid > id) => {
                    let r = r as usize;
                    let row: Vec<f32> = self.layout.row_at(bytes, r).collect();
                    push(id, self.layout.norm_at(bytes, r), &row, &mut rows_by_id);
                    committed.next();
                }
                // A pending change (a replacement, an insert, or a delete): the change wins.
                (nc, Some((pid, change))) => {
                    if let Some(v) = change {
                        push(
                            pid,
                            search::norm_f64(v.iter().copied()) as f32,
                            v,
                            &mut rows_by_id,
                        );
                    }
                    if nc.is_some_and(|(id, _)| id == pid) {
                        committed.next();
                    }
                    pending.next();
                }
                (Some(_), None) => unreachable!("covered by the first arm"),
            }
        }
        debug_assert_eq!(next as usize, total);
        let count = u64::from(next);
        let layout = Rows::new(next as usize, self.header.dim);
        let new_path = self.dir.join(format::row_file(generation));
        let header = Header {
            generation,
            rows: count,
            live: count,
            ..self.header.clone()
        };
        // Write the new generation, make its entry durable, bring it into memory; every
        // fallible step precedes the rename.
        let prepared = (|| -> Result<Bytes> {
            let mut file = std::fs::File::create(&new_path)?;
            file.write_all(&buf)?;
            file.sync_all()?;
            sync_dir(&self.dir)?;
            if layout.count == 0 {
                Ok(Bytes::Owned(Vec::new()))
            } else {
                bytes::read_prefix(&new_path, self.load_path, layout.len_bytes())
            }
        })();
        let rows = match prepared {
            Ok(rows) => rows,
            Err(e) => {
                let _ = std::fs::remove_file(&new_path);
                return Err(e);
            }
        };
        let switched = write_manifest(&self.dir, &header, &RoaringBitmap::new());
        if let Err(ManifestFailure::BeforeSwitch(e)) = switched {
            let _ = std::fs::remove_file(&new_path);
            return Err(e);
        }
        self.header = header;
        self.dead = RoaringBitmap::new();
        self.rows = rows;
        self.layout = layout;
        self.rows_by_id = rows_by_id;
        self.ascending = true;
        self.pending.clear();
        match switched {
            Ok(()) => {
                // The rename is durable, so the old generation can go; its removal need not
                // be durable — a survivor is swept at open.
                let _ = std::fs::remove_file(old_path);
                Ok(())
            }
            // The manifest is switched but its entry's durability is unconfirmed: keep the
            // old generation (harmless; swept at open) and say what happened.
            Err(ManifestFailure::AfterSwitch(e)) => Err(e),
            Err(ManifestFailure::BeforeSwitch(_)) => unreachable!("handled above"),
        }
    }

    /// Whether the committed rows are currently a memory map (feature `mmap`). An empty index
    /// is a heap buffer whatever the load path — a zero-length mapping does not exist — and the
    /// mapping is (re)made by the first commit that appends and by every compaction.
    #[cfg(feature = "mmap")]
    #[must_use]
    pub fn is_mapped(&self) -> bool {
        matches!(self.rows, Bytes::Mapped(_))
    }

    fn live_row(&self, id: DocId) -> Option<usize> {
        self.rows_by_id.get(&id.0).map(|&r| r as usize)
    }

    fn open_with(dir: &Path, load_path: LoadPath, read_only: bool) -> Result<Self> {
        let manifest = match std::fs::read(dir.join(MANIFEST)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && dir.join(V1_FILE).is_file() => {
                return Err(format::version_1_error());
            }
            Err(e) => return Err(e.into()),
        };
        let (header, dead) = format::decode_manifest(&manifest)?;
        let mut index = Self {
            dir: dir.to_path_buf(),
            header,
            load_path,
            rows: Bytes::Owned(Vec::new()),
            layout: Rows::new(0, 1),
            dead,
            rows_by_id: BTreeMap::new(),
            ascending: true,
            pending: BTreeMap::new(),
            compaction_threshold: None,
            read_only,
        };
        if !read_only {
            index.settle()?;
        }
        index.reload()?;
        Ok(index)
    }

    /// At a writable open only: a crashed append leaves a tail beyond the committed rows — cut
    /// it (best effort: a read-only file keeps it, and only the committed rows are ever read);
    /// stale generations and a manifest temporary are swept the same way. Nothing here touches
    /// a committed byte, and nothing is mapped yet. A read-only open skips this entirely: it
    /// holds no writer's role, so it alters nothing.
    fn settle(&self) -> Result<()> {
        let layout = Rows::new(self.header.rows as usize, self.header.dim);
        let path = self.dir.join(format::row_file(self.header.generation));
        let len = std::fs::metadata(&path)?.len();
        let committed = layout.len_bytes() as u64;
        if len < committed {
            return Err(corrupt(format!(
                "{} is {len} bytes, shorter than the {committed} bytes of {} committed rows",
                path.display(),
                self.header.rows
            )));
        }
        if len > committed
            && let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path)
        {
            let _ = file.set_len(committed);
        }
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let stale = name == MANIFEST_TMP
                    || format::row_file_generation(&name)
                        .is_some_and(|g| g != self.header.generation);
                if stale {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    /// (Re)read the committed rows from the manifest's generation and rebuild the id table.
    fn reload(&mut self) -> Result<()> {
        let layout = Rows::new(self.header.rows as usize, self.header.dim);
        let path = self.dir.join(format::row_file(self.header.generation));
        let rows = if layout.count == 0 {
            // Nothing to read, but the generation the manifest names must be there: a missing
            // row file is an incomplete directory, whatever the open's mode.
            std::fs::metadata(&path)?;
            Bytes::Owned(Vec::new())
        } else {
            bytes::read_prefix(&path, self.load_path, layout.len_bytes())?
        };
        let bytes = rows.as_slice();
        let mut rows_by_id: BTreeMap<u32, u32> = BTreeMap::new();
        let mut ascending = true;
        let mut last: Option<u32> = None;
        for r in 0..layout.count {
            let id = layout.id_at(bytes, r);
            if last.is_some_and(|l| l >= id) {
                ascending = false;
            }
            last = Some(id);
            if self.dead.contains(r as u32) {
                continue;
            }
            // One live row per id: a second one is a corrupt file, not a silent overwrite.
            if let Some(first) = rows_by_id.insert(id, r as u32) {
                return Err(corrupt(format!(
                    "{} has two live rows for id {id} (rows {first} and {r})",
                    path.display()
                )));
            }
        }
        self.rows = rows;
        self.layout = layout;
        self.rows_by_id = rows_by_id;
        self.ascending = ascending;
        Ok(())
    }

    fn check_agreement(&self, embedder: &dyn Embedder) -> Result<()> {
        if self.header.fingerprint != embedder.fingerprint() {
            return Err(Error::FingerprintMismatch {
                index: self.header.fingerprint.clone(),
                current: embedder.fingerprint().to_owned(),
            });
        }
        if self.header.dim != embedder.dim() {
            return Err(corrupt(format!(
                "index has dim {}, embedder has dim {}",
                self.header.dim,
                embedder.dim()
            )));
        }
        if self.metric() != embedder.metric() {
            return Err(corrupt(format!(
                "index uses {:?}, embedder uses {:?}",
                self.metric(),
                embedder.metric()
            )));
        }
        Ok(())
    }

    fn validate_vector(&self, id: DocId, v: &[f32]) -> Result<()> {
        if v.len() != self.header.dim {
            return Err(dim_mismatch(self.header.dim, v.len()));
        }
        if let Some(i) = v.iter().position(|x| !x.is_finite()) {
            return Err(schema_err(format!(
                "vector for {id} has a non-finite component at index {i}"
            )));
        }
        let norm = search::norm_f64(v.iter().copied());
        if self.metric() == Metric::Cosine && norm == 0.0 {
            return Err(schema_err(format!(
                "vector for {id} has zero norm; cosine similarity is undefined"
            )));
        }
        // The norm is persisted as f32; a finite vector such as [f32::MAX, f32::MAX] has a norm
        // that only fits f64, and storing it as +inf would silently score its own cosine as 0.
        if !(norm as f32).is_finite() {
            return Err(schema_err(format!(
                "vector for {id} has norm {norm:e}, which does not fit a finite f32"
            )));
        }
        Ok(())
    }
}

impl VectorIndex for FlatIndex {
    fn dim(&self) -> usize {
        self.header.dim
    }

    fn metric(&self) -> Metric {
        self.header.metric.into()
    }

    fn fingerprint(&self) -> &str {
        &self.header.fingerprint
    }

    fn add(&mut self, id: DocId, vector: &[f32]) -> Result<()> {
        self.validate_vector(id, vector)?;
        self.pending.insert(id, Some(vector.to_vec()));
        Ok(())
    }

    fn delete(&mut self, ids: &[DocId]) -> Result<()> {
        for &id in ids {
            self.pending.insert(id, None);
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if self.read_only {
            return Err(read_only());
        }
        if self.pending.is_empty() {
            return Ok(());
        }
        // 0. The dead-row share this commit would leave. Over the configured threshold, the
        //    commit *is* a rewrite — one protocol, one rename — never an append followed by a
        //    separate compaction that could fail after the append is durable.
        let adds = self.pending.values().filter(|c| c.is_some()).count();
        let superseded = self
            .pending
            .keys()
            .filter(|id| self.live_row(**id).is_some())
            .count();
        let rows_after = self.layout.count + adds;
        let dead_after = self.dead.len() + superseded as u64;
        if let Some(t) = self.compaction_threshold
            && rows_after > 0
            && (dead_after as f64 / rows_after as f64) > f64::from(t)
        {
            return self.rewrite();
        }
        // 1. Resolve the pending changes against the committed state, on copies: nothing in
        //    memory changes until the manifest is renamed, so a failed commit keeps every staged
        //    change for a retry.
        let mut dead = self.dead.clone();
        let mut buf = Vec::new();
        let mut appended: Vec<(u32, u32)> = Vec::new(); // (id, row)
        let mut deleted: Vec<u32> = Vec::new();
        let mut next_row = row_space(self.layout.count, adds)?;
        let last_id = (self.layout.count > 0).then(|| {
            self.layout
                .id_at(self.rows.as_slice(), self.layout.count - 1)
        });
        let mut ascending = self.ascending;
        for (id, change) in &self.pending {
            if let Some(r) = self.live_row(*id) {
                dead.insert(r as u32);
            }
            match change {
                Some(v) => {
                    let norm = search::norm_f64(v.iter().copied()) as f32;
                    format::encode_row(&mut buf, id.0, norm, v);
                    if appended.is_empty() && last_id.is_some_and(|l| l >= id.0) {
                        ascending = false;
                    }
                    appended.push((id.0, next_row));
                    next_row += 1;
                }
                None => deleted.push(id.0),
            }
        }
        // 2. Append the new rows — beyond every live mapping's end (ADR-0013) — and sync.
        let path = self.dir.join(format::row_file(self.header.generation));
        let committed_len = self.layout.len_bytes() as u64;
        if !buf.is_empty() {
            let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
            // A tail beyond the committed rows (a failed earlier attempt) would shift the row
            // offsets: cut it first. This handle's mapping, if any, ends at `committed_len`.
            file.set_len(committed_len)?;
            if let Err(e) = file.write_all(&buf).and_then(|()| file.sync_all()) {
                let _ = file.set_len(committed_len);
                return Err(e.into());
            }
        }
        // 3. Bring the appended rows into memory *before* the manifest switches: a buffer grows
        //    in place (undone by a truncate on failure), a mapping is made anew (nothing is
        //    read). Then the manifest — the truth — replaced atomically.
        let rows = u64::from(next_row);
        let header = Header {
            rows,
            live: rows - dead.len(),
            ..self.header.clone()
        };
        let layout = Rows::new(rows as usize, self.header.dim);
        let undo_file = |path: &Path| {
            if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
                let _ = file.set_len(committed_len);
            }
        };
        let remapped: Option<Bytes> = match self.load_path {
            LoadPath::Buffered => {
                if let Some(v) = self.rows.owned_mut() {
                    v.truncate(committed_len as usize);
                    v.extend_from_slice(&buf);
                }
                None
            }
            #[cfg(feature = "mmap")]
            LoadPath::Mmap if buf.is_empty() => None,
            #[cfg(feature = "mmap")]
            LoadPath::Mmap => match bytes::read_prefix(&path, self.load_path, layout.len_bytes()) {
                Ok(b) => Some(b),
                Err(e) => {
                    undo_file(&path);
                    return Err(e);
                }
            },
        };
        let switched = write_manifest(&self.dir, &header, &dead);
        if let Err(ManifestFailure::BeforeSwitch(e)) = switched {
            // The temporary mapping covers the appended bytes: it must be gone before they are
            // cut (the invariant in `bytes::map_readonly`).
            drop(remapped);
            if let Some(v) = self.rows.owned_mut() {
                v.truncate(committed_len as usize);
            }
            if !buf.is_empty() {
                undo_file(&path);
            }
            return Err(e);
        }
        // 4. The manifest is switched: the in-memory state follows it — nothing here can fail.
        if let Some(b) = remapped {
            self.rows = b;
        }
        debug_assert!(self.rows.as_slice().len() >= layout.len_bytes());
        self.header = header;
        self.dead = dead;
        self.layout = layout;
        self.ascending = ascending;
        self.pending.clear();
        for &id in &deleted {
            self.rows_by_id.remove(&id);
        }
        for &(id, r) in &appended {
            self.rows_by_id.insert(id, r);
        }
        match switched {
            Ok(()) => Ok(()),
            Err(ManifestFailure::AfterSwitch(e)) => Err(e),
            Err(ManifestFailure::BeforeSwitch(_)) => unreachable!("handled above"),
        }
    }

    fn search(&self, query: &[f32], allowed: Option<&DocSet>, k: usize) -> Result<Vec<Hit>> {
        let metric = self.metric();
        // Validation precedes the no-work shortcut on purpose: a malformed query is a caller
        // error whatever `k` is, and hiding it behind `k == 0` would let it surface later.
        let q = search::validate_query(query, self.header.dim, metric)?;
        if k == 0 || allowed.is_some_and(DocSet::is_empty) {
            return Ok(Vec::new());
        }
        let bytes = self.rows.as_slice();
        let layout = self.layout;
        let mut scored: Vec<(f32, u32)> = Vec::with_capacity(self.header.live as usize);
        for r in 0..layout.count {
            if self.dead.contains(r as u32) {
                continue;
            }
            let id = layout.id_at(bytes, r);
            if allowed.is_some_and(|set| !set.contains(DocId(id))) {
                continue;
            }
            let s = search::score(
                metric,
                &q,
                layout.row_at(bytes, r),
                layout.norm_at(bytes, r),
            );
            scored.push((s, id));
        }
        Ok(search::top_k(scored, k))
    }

    fn len(&self) -> u64 {
        self.header.live
    }
}

/// The refusal of a mutation on a read-only handle: the same shape the lexical stage uses.
fn read_only() -> Error {
    Error::Io(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "read-only index: opened with open_read_only, mutations are refused",
    ))
}

/// The first row index of a commit that appends `adds` rows to `committed` — or the row-space
/// exhaustion error, before any I/O: row indices are `u32` (the tombstone set's domain).
fn row_space(committed: usize, adds: usize) -> Result<u32> {
    let first = u32::try_from(committed).ok();
    let last = first.and_then(|f| f.checked_add(u32::try_from(adds).ok()?));
    match (first, last) {
        (Some(first), Some(_)) => Ok(first),
        _ => Err(Error::Io(std::io::Error::other(format!(
            "dense row space exhausted: {committed} committed rows plus {adds} would exceed {} — compact the index first",
            u32::MAX
        )))),
    }
}

/// Write a manifest to `manifest.bin.tmp`, sync, and `rename` it over `manifest.bin`.
///
/// The rename is the switch. A failure before it leaves the previous manifest; a failure after
/// it — the directory sync — leaves the new manifest in place with its entry's durability
/// unconfirmed. Callers roll back on the first and adopt the switched state on the second.
fn write_manifest(
    dir: &Path,
    header: &Header,
    dead: &RoaringBitmap,
) -> std::result::Result<(), ManifestFailure> {
    let before = |e: Error| ManifestFailure::BeforeSwitch(e);
    let encoded = format::encode_manifest(header, dead).map_err(before)?;
    let tmp = dir.join(MANIFEST_TMP);
    (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        std::fs::rename(&tmp, dir.join(MANIFEST))
    })()
    .map_err(|e| before(e.into()))?;
    sync_dir(dir).map_err(|e| {
        ManifestFailure::AfterSwitch(Error::Io(std::io::Error::other(format!(
            "the manifest is switched but the directory sync failed, so its entry's durability \
             is unconfirmed; the index state is the new one and coherent: {e}"
        ))))
    })
}

/// How a manifest write failed: before the rename (nothing on disk changed) or after it (the
/// manifest is the new one; only the directory sync failed).
enum ManifestFailure {
    BeforeSwitch(Error),
    AfterSwitch(Error),
}

impl ManifestFailure {
    fn into_error(self) -> Error {
        match self {
            Self::BeforeSwitch(e) | Self::AfterSwitch(e) => e,
        }
    }
}

/// Make the directory's entries durable (a rename, a new file): on POSIX the entry lives in
/// the directory, which is synced like any file. Without it a power loss can keep a renamed
/// manifest that names a row file whose entry never reached disk.
///
/// The power-loss ordering guarantee (ADR-0013) is made on the Unix targets the engine ships
/// to — macOS, iOS, Android, Linux. Elsewhere a directory cannot be opened for `fsync` and this
/// is a no-op: the crash-at-any-byte guarantee (the manifest is the truth, replaced by rename)
/// still holds, the ordering of entries across a power loss is the filesystem's.
fn sync_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn row_space_is_checked_before_any_io() {
        assert_eq!(row_space(0, 10).unwrap(), 0);
        assert_eq!(row_space(100, 0).unwrap(), 100);
        assert_eq!(row_space(u32::MAX as usize - 1, 1).unwrap(), u32::MAX - 1);
        assert!(matches!(row_space(u32::MAX as usize, 1), Err(Error::Io(_))));
        assert!(matches!(
            row_space(u32::MAX as usize - 1, 2),
            Err(Error::Io(_))
        ));
        assert!(matches!(row_space(usize::MAX, 0), Err(Error::Io(_))));
    }
}
