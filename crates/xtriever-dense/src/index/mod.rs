//! `FlatIndex`: a flat, exact `VectorIndex` over an append-only row file and an atomically
//! replaced manifest (Feature 024, ADR-0013; research D3–D7). `commit` appends, `compact`
//! rewrites under a new generation; a committed byte is never modified in place.

mod format;
mod search;

use std::collections::BTreeMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use roaring::RoaringBitmap;
use xtriever_core::{DocId, DocSet, Embedder, Error, Hit, Metric, Result, VectorIndex, fs};

use crate::FORMAT_VERSION;
use crate::LoadPath;
use crate::bytes::{self, Bytes};
use crate::error::{corrupt, dim_mismatch, schema_err};
use format::{Header, MANIFEST, MANIFEST_TMP, RowBytes, Rows, StagedRow, V1_FILE};

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
    /// Row ids strictly ascending in the file (a fresh or compacted generation appended to
    /// with higher ids only).
    pub ordered: bool,
}

/// The one definition of an acceptable dead-row share: `None`, or a finite value in
/// `0.0..=1.0`. `Error::Schema` otherwise — the pipeline's configuration check and the
/// descriptor check at open both call this, so the rule cannot drift between layers.
///
/// # Errors
///
/// `Error::Schema` naming the value.
pub fn validate_compaction_threshold(share: Option<f32>) -> Result<()> {
    if let Some(s) = share
        && !(s.is_finite() && (0.0..=1.0).contains(&s))
    {
        return Err(schema_err(format!(
            "compaction threshold {s} is not in 0.0..=1.0"
        )));
    }
    Ok(())
}

/// A flat, exact vector index: `vectors.<g>.bin` (rows, append-only) + `manifest.bin`.
///
/// `add` and `delete` stage changes in memory; `commit` appends the new rows, marks superseded
/// and deleted rows dead, and replaces the manifest by `rename`; `compact` writes the live rows
/// under a new generation and switches the manifest the same way. A reader (or a mapping) of
/// the previous state is never disturbed by either, and a crash at any point leaves either the
/// previous manifest or the new one — the previous committed state or the fully committed
/// new one, never a partial state.
///
/// One writer at a time: a handle checks at every commit that the manifest on disk is the one
/// it last saw and refuses (`Corrupt`) otherwise, so two writable handles cannot silently undo
/// each other's rows — but the precondition every open carries (no other writer while the
/// handle lives) is still the caller's.
pub struct FlatIndex {
    dir: PathBuf,
    header: Header,
    /// The validated row layout of `header` (`rows × dim`); switched together with it.
    layout: Rows,
    load_path: LoadPath,
    /// The row file's committed rows as loaded (read, or mapped) …
    rows: Bytes,
    /// … plus the rows appended in memory since (the buffered path: the matrix already in
    /// memory is never reallocated or copied by an append; a reopen or a compaction folds the
    /// tail back). Empty on the mapped path, which re-maps the committed prefix instead.
    tail: Vec<u8>,
    dead: RoaringBitmap,
    /// Live id → its row, built only when the file needs it — tombstones present, or ids not
    /// in order. An ordered, tombstone-free generation (fresh, compacted, or appended to with
    /// higher ids — the shipped case) has no table: `vector(id)` is a binary search over the
    /// row ids, and a read-only open touches nothing beyond the manifest.
    table: Option<BTreeMap<u32, u32>>,
    /// `Some(vector)` = add or replace, `None` = delete. Invisible until `commit`.
    /// Staged changes: `Some` an insert or replacement, already quantised and with its norm
    /// (once, at `add`, where it is validated — a quarter of the memory of the floats, and
    /// `commit` and `compact` only copy bytes); `None` a delete.
    pending: BTreeMap<DocId, Option<StagedRow>>,
    /// `commit` rewrites instead of appending when the dead share it would leave exceeds this
    /// (`None` = only on `compact`).
    compaction_threshold: Option<f32>,
    /// A read-only open: no truncation or sweep at open, every mutation refused.
    read_only: bool,
    /// A manifest rename whose directory sync failed: the state is switched but its entry's
    /// durability is unconfirmed. No later `commit` or `compact` returns success until a
    /// directory sync has succeeded.
    sync_pending: bool,
    /// Bytes this handle has written to the row files and manifests since it was opened —
    /// counted after each write succeeded (a rolled-back append still wrote its rows).
    written: u64,
    /// A writable handle on a table-free (ordered, tombstone-free) generation has verified the
    /// manifest's `ordered` claim against the row ids before its first write; a read-only
    /// handle trusts the manifest, as it trusts every other field.
    order_verified: bool,
}

impl std::fmt::Debug for FlatIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlatIndex")
            .field("dir", &self.dir)
            .field("header", &self.header)
            .field("load_path", &self.load_path)
            .field("dead", &self.dead.len())
            .field("table", &self.table.as_ref().map(BTreeMap::len))
            .field("pending", &self.pending.len())
            .field("read_only", &self.read_only)
            .field("sync_pending", &self.sync_pending)
            .finish_non_exhaustive()
    }
}

/// The parts of a committed state that switch together.
struct Committed {
    header: Header,
    layout: Rows,
    rows: Bytes,
    tail: Vec<u8>,
    dead: RoaringBitmap,
    table: Option<BTreeMap<u32, u32>>,
}

impl FlatIndex {
    /// Create at `dir` (created if absent; must be empty). Writes an empty generation.
    ///
    /// # Errors
    ///
    /// `Error::Corrupt` if `dir` is not empty, `dim == 0` or `dim` is wider than the integer
    /// kernel can score (132,104); `Error::Io` otherwise. A failure leaves the directory empty,
    /// so a corrected retry succeeds.
    pub fn create(dir: &Path, dim: usize, metric: Metric, fingerprint: &str) -> Result<Self> {
        if dim == 0 {
            return Err(corrupt("dim must be at least 1"));
        }
        if dim > crate::quantise::MAX_DIM {
            return Err(corrupt(format!(
                "dim {dim} exceeds {}, the widest row the integer kernel can score",
                crate::quantise::MAX_DIM
            )));
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
            scheme: crate::quantise::SCHEME.to_owned(),
            dim,
            metric: metric.into(),
            fingerprint: fingerprint.to_owned(),
            generation: 0,
            rows: 0,
            live: 0,
            ordered: true,
            tombstones_len: 0,
        };
        let row_file = dir.join(format::row_file(0));
        let written = (|| -> std::result::Result<(), ManifestFailure> {
            let before = ManifestFailure::BeforeSwitch;
            (|| -> Result<()> {
                std::fs::File::create(&row_file)?.sync_all()?;
                fs::sync_dir(dir)?;
                Ok(())
            })()
            .map_err(before)?;
            write_manifest(dir, &header, &RoaringBitmap::new(), &mut 0)
        })();
        match written {
            Ok(()) => Self::open_with(dir, LoadPath::Buffered, false),
            Err(ManifestFailure::BeforeSwitch(e)) => {
                // Nothing usable exists: leave the directory as empty as it was found.
                let _ = std::fs::remove_file(&row_file);
                let _ = std::fs::remove_file(dir.join(MANIFEST_TMP));
                Err(e)
            }
            // The index is complete; only its entry's durability is unconfirmed — the handle
            // retries the sync before its first commit.
            Err(ManifestFailure::AfterSwitch(_)) => {
                let mut index = Self::open_with(dir, LoadPath::Buffered, false)?;
                index.sync_pending = true;
                Ok(index)
            }
        }
    }

    /// Open, reading the committed rows into memory.
    ///
    /// # Errors
    ///
    /// `Error::Io` if a file cannot be read; `Error::Corrupt` if it is not a version-3 index
    /// (a version-1 `index.bin` or a version-2 manifest is named as such, with the
    /// instruction to rebuild — Feature 026, ADR-0015).
    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_with(dir, LoadPath::Buffered, false)
    }

    /// Open read-only, reading the committed rows into memory: nothing in the directory is
    /// touched — no crashed tail is cut, no stale generation swept — and every mutation
    /// (`add`, `delete`, `commit`, `compact`, `set_compaction_threshold`) is refused with
    /// `Error::Io` "read-only index". For a directory that is read-only by construction (an app bundle) or one
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

    /// The committed vector stored under `id`, **as the stored row recovers it**: each
    /// component within half a quantisation step of what was added, the step being the row's
    /// scale — `max|component| / 127`, floored at `f32::MIN_POSITIVE` — because a row is
    /// eight-bit codes and one scale (Feature 026, ADR-0015) and the floats are not kept. For a
    /// vector whose peak is below `127 × f32::MIN_POSITIVE` the floor is the step, so components
    /// below half of it come back as exactly zero. `Ok(None)` if `id` has no live row (pending
    /// changes are not visible, as with `search`).
    ///
    /// # Errors
    ///
    /// `Error::Corrupt` if the row's scale or norm is one this engine never writes — the same
    /// check the scan makes, so a damaged row is refused here rather than recovered as NaN.
    pub fn vector(&self, id: DocId) -> Result<Option<Vec<f32>>> {
        let Some(r) = self.live_row(id) else {
            return Ok(None);
        };
        let (scale, _) = self.checked_row(r, id.0, self.least_norm())?;
        Ok(Some(
            Rows::recover(self.layout.codes_at(self.bytes(), r), scale).collect(),
        ))
    }

    /// Whether this handle was opened read-only (`open_read_only*`): every mutation is refused
    /// and the directory is never touched.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Bytes this handle has written to the row files and manifests since it was opened, each
    /// write counted once it succeeded: every appended row (a later rollback does not uncount
    /// it), every manifest temporary, every compaction's new generation — nothing else is ever
    /// written. A successful commit's share of it is its write volume.
    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        self.written
    }

    /// What the manifest says: rows, live, dead, generation.
    #[must_use]
    pub fn stats(&self) -> DenseStats {
        DenseStats {
            rows: self.header.rows,
            live: self.header.live,
            dead: self.dead.len(),
            generation: self.header.generation,
            ordered: self.header.ordered,
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

    /// Whether the last manifest switch's directory sync failed (`commit` / `compact` returned
    /// the durability-unconfirmed error): the state on disk is the new one; the next `commit`
    /// or `compact` retries the sync first and succeeds only once it has. Per handle: a
    /// dropped handle drops the obligation, which is why the directory is opened for the sync
    /// *before* the rename — an unopenable directory fails before the switch.
    #[must_use]
    pub fn is_sync_pending(&self) -> bool {
        self.sync_pending
    }

    /// Make `commit` rewrite the row file (a compaction with the changes folded in) when the
    /// dead-row share the commit would leave exceeds `share` (`0.0..=1.0`; `None`, the default,
    /// compacts only on [`compact`](Self::compact)).
    ///
    /// # Errors
    ///
    /// `Error::Schema` for a share outside `0.0..=1.0` or not finite; `Error::Io` on a
    /// read-only handle.
    pub fn set_compaction_threshold(&mut self, share: Option<f32>) -> Result<()> {
        if self.read_only {
            return Err(Error::read_only());
        }
        validate_compaction_threshold(share)?;
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
    /// `Error::Io`; `Error::Corrupt` if the generation counter cannot advance or the manifest
    /// on disk is not the one this handle last saw (another writer). A failure before the
    /// manifest is renamed leaves the previous state on disk and in this handle, pending
    /// changes included (the partial new file is removed, or swept at the next open). A failure
    /// *after* the rename — only the directory sync — leaves the switched state, adopted by
    /// this handle, and names itself as such.
    pub fn compact(&mut self) -> Result<()> {
        if self.read_only {
            return Err(Error::read_only());
        }
        self.confirm_sync()?;
        // Even a no-op answers for the state on disk: a stale handle must not report success
        // over another writer's changes.
        self.verify_unchanged()?;
        if self.pending.is_empty() && self.dead.is_empty() && self.header.ordered {
            return Ok(());
        }
        self.rewrite()
    }

    /// The rewrite protocol: committed live rows ⊕ pending, ascending by id, streamed into a
    /// new generation; one manifest rename; nothing in memory changes before it. Used by
    /// `compact` and by `commit` when the configured dead-row share would be exceeded — so a
    /// commit is always one protocol, never a durable append followed by a separate compaction.
    fn rewrite(&mut self) -> Result<()> {
        self.verify_unchanged()?;
        self.verify_order()?;
        let generation = self.header.generation.checked_add(1).ok_or_else(|| {
            corrupt(format!(
                "generation {} cannot advance; the row file's generation counter is exhausted",
                self.header.generation
            ))
        })?;
        // The new generation's row count, validated before any I/O: the committed live rows,
        // minus those a pending change supersedes or deletes, plus the pending inserts.
        let (adds, superseded) = self.pending_counts();
        let total = self.header.live as usize - superseded + adds;
        let layout = Rows::for_count(total, self.header.dim)?;
        let old_path = self.dir.join(format::row_file(self.header.generation));
        let new_path = self.dir.join(format::row_file(generation));
        let header = Header {
            generation,
            rows: layout.count as u64,
            live: layout.count as u64,
            ordered: true,
            ..self.header.clone()
        };
        // Stream the merged rows to the new file — a kept row is its committed bytes copied
        // as they are — keeping a copy in memory only on the buffered path (the memory the new
        // state needs anyway); the mapped path maps the file afterwards.
        let prepared = (|| -> Result<Bytes> {
            let file = std::fs::File::create(&new_path)?;
            let mut out = std::io::BufWriter::new(file);
            let mut mem: Option<Vec<u8>> = matches!(self.load_path, LoadPath::Buffered)
                .then(|| Vec::with_capacity(layout.len_bytes()));
            let mut row_buf = Vec::with_capacity(layout.row_bytes);
            let mut emitted = 0usize;
            let bytes = self.bytes();
            let old = self.layout;
            for row in self.merged_rows() {
                row_buf.clear();
                match row {
                    Merged::Kept(r) => {
                        row_buf.extend_from_slice(old.row_bytes_at(bytes, r));
                    }
                    Merged::Pending(id, v) => {
                        format::encode_row(&mut row_buf, id, v);
                    }
                }
                out.write_all(&row_buf)?;
                if let Some(m) = &mut mem {
                    m.extend_from_slice(&row_buf);
                }
                emitted += 1;
            }
            debug_assert_eq!(emitted, total);
            let file = out
                .into_inner()
                .map_err(std::io::IntoInnerError::into_error)?;
            self.written += layout.len_bytes() as u64;
            file.sync_all()?;
            // The new generation's entry must be durable before a manifest names it.
            fs::sync_dir(&self.dir)?;
            let rows = match mem {
                Some(m) => Bytes::Owned(m),
                None if layout.count == 0 => Bytes::Owned(Vec::new()),
                None => bytes::read_prefix(&new_path, self.load_path, layout.len_bytes())?,
            };
            Ok(rows)
        })();
        let rows = match prepared {
            Ok(v) => v,
            Err(e) => {
                let _ = std::fs::remove_file(&new_path);
                return Err(e);
            }
        };
        let switched =
            match write_manifest(&self.dir, &header, &RoaringBitmap::new(), &mut self.written) {
                Ok(()) => Ok(()),
                Err(ManifestFailure::BeforeSwitch(e)) => {
                    let _ = std::fs::remove_file(&new_path);
                    return Err(e);
                }
                Err(ManifestFailure::AfterSwitch(e)) => Err(e),
            };
        // The manifest is switched: only infallible adoption follows.
        self.adopt(Committed {
            header,
            layout,
            rows,
            tail: Vec::new(),
            dead: RoaringBitmap::new(),
            table: None,
        });
        self.order_verified = true; // ascending by construction
        self.pending.clear();
        if switched.is_ok() {
            // The rename is durable, so the old generation can go; its removal need not be
            // durable — a survivor is swept at open.
            let _ = std::fs::remove_file(old_path);
        } else {
            // Switched, but its entry's durability is unconfirmed: keep the old generation
            // (harmless; swept at open) and retry the sync before any later success.
            self.sync_pending = true;
        }
        switched
    }

    /// The committed live rows ⊕ the pending changes, in ascending id order: a kept committed
    /// row, or a pending vector (a replacement or insert); deletes and superseded rows are
    /// skipped.
    fn merged_rows(&self) -> impl Iterator<Item = Merged<'_>> + '_ {
        let bytes = self.bytes();
        let layout = self.layout;
        let committed: Box<dyn Iterator<Item = (u32, usize)> + '_> = match &self.table {
            Some(t) => Box::new(t.iter().map(|(&id, &r)| (id, r as usize))),
            // Ordered and tombstone-free: the file order is the id order.
            None => Box::new((0..layout.count).map(move |r| (layout.id_at(bytes, r), r))),
        };
        let pending = self.pending.iter().map(|(id, c)| (id.0, c.as_ref()));
        Merge {
            committed: committed.peekable(),
            pending: pending.peekable(),
        }
    }

    /// Pending inserts/replacements, and pending ids that supersede or delete a live row.
    fn pending_counts(&self) -> (usize, usize) {
        let adds = self.pending.values().filter(|c| c.is_some()).count();
        let superseded = self
            .pending
            .keys()
            .filter(|id| self.live_row(**id).is_some())
            .count();
        (adds, superseded)
    }

    /// The manifest on disk must be the one this handle last saw — generation, rows, live
    /// count *and* the tombstone set (a delete-only commit changes only the last two): another
    /// writable handle's commit in between would otherwise be silently undone, its deletes
    /// resurrected or its rows cut in place.
    fn verify_unchanged(&self) -> Result<()> {
        let manifest = std::fs::read(self.dir.join(MANIFEST))?;
        let (header, _, dead) = format::decode_manifest(&manifest)?;
        if header.generation != self.header.generation
            || header.rows != self.header.rows
            || header.live != self.header.live
            || dead != self.dead
        {
            return Err(corrupt(format!(
                "the index was changed by another writer (manifest generation {} rows {} live \
                 {}, this handle saw generation {} rows {} live {}); reopen before writing",
                header.generation,
                header.rows,
                header.live,
                self.header.generation,
                self.header.rows,
                self.header.live
            )));
        }
        Ok(())
    }

    /// Before a writable handle builds on a table-free generation — `live_row` by binary
    /// search, the rewrite's file-order merge — the manifest's `ordered` claim is checked once
    /// against the row ids: strictly ascending, hence unique. A manifest that lies is
    /// `Corrupt` here, before anything is written; the cost (one pass over the ids) falls on
    /// the first write of a handle, never on a read-only open — the mapped-open goal.
    fn verify_order(&mut self) -> Result<()> {
        if self.order_verified || self.table.is_some() {
            return Ok(());
        }
        let bytes = self.bytes();
        let layout = self.layout;
        for r in 1..layout.count {
            if layout.id_at(bytes, r - 1) >= layout.id_at(bytes, r) {
                return Err(corrupt(format!(
                    "{} is not ordered at row {r} although its manifest says so",
                    self.dir
                        .join(format::row_file(self.header.generation))
                        .display()
                )));
            }
        }
        self.order_verified = true;
        Ok(())
    }

    /// Retry a directory sync an earlier switch left unconfirmed; nothing else proceeds until
    /// it has succeeded, so a later success never hides an unconfirmed one.
    fn confirm_sync(&mut self) -> Result<()> {
        if self.sync_pending {
            fs::sync_dir(&self.dir)?;
            self.sync_pending = false;
        }
        Ok(())
    }

    /// Switch the in-memory state to a committed one — the one place it changes.
    fn adopt(&mut self, c: Committed) {
        debug_assert!(c.rows.as_slice().len() + c.tail.len() >= c.layout.len_bytes());
        self.header = c.header;
        self.layout = c.layout;
        self.rows = c.rows;
        self.tail = c.tail;
        self.dead = c.dead;
        self.table = c.table;
    }

    /// The committed rows: the loaded segment plus the appended tail.
    fn bytes(&self) -> RowBytes<'_> {
        RowBytes {
            base: self.rows.as_slice(),
            tail: &self.tail,
        }
    }

    fn live_row(&self, id: DocId) -> Option<usize> {
        match &self.table {
            Some(t) => t.get(&id.0).map(|&r| r as usize),
            None => {
                // Ordered, tombstone-free: binary search over the row ids.
                let bytes = self.bytes();
                let (mut lo, mut hi) = (0usize, self.layout.count);
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    match self.layout.id_at(bytes, mid).cmp(&id.0) {
                        std::cmp::Ordering::Less => lo = mid + 1,
                        std::cmp::Ordering::Greater => hi = mid,
                        std::cmp::Ordering::Equal => return Some(mid),
                    }
                }
                None
            }
        }
    }

    fn open_with(dir: &Path, load_path: LoadPath, read_only: bool) -> Result<Self> {
        let manifest = match std::fs::read(dir.join(MANIFEST)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && dir.join(V1_FILE).is_file() => {
                return Err(format::version_1_error());
            }
            Err(e) => return Err(e.into()),
        };
        let (header, layout, dead) = format::decode_manifest(&manifest)?;
        if !read_only {
            settle(dir, &header, &layout)?;
        }
        let committed = load(dir, header, layout, dead, load_path)?;
        let mut index = Self {
            dir: dir.to_path_buf(),
            header: committed.header.clone(),
            layout: committed.layout,
            load_path,
            rows: Bytes::Owned(Vec::new()),
            tail: Vec::new(),
            dead: RoaringBitmap::new(),
            table: None,
            pending: BTreeMap::new(),
            compaction_threshold: None,
            read_only,
            sync_pending: false,
            written: 0,
            order_verified: false,
        };
        index.adopt(committed);
        Ok(index)
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

    /// Validate `v` for this index and return it as the row it will be stored as.
    fn validate_vector(&self, id: DocId, v: &[f32]) -> Result<StagedRow> {
        if v.len() != self.header.dim {
            return Err(dim_mismatch(self.header.dim, v.len()));
        }
        if let Some(i) = v.iter().position(|x| !x.is_finite()) {
            return Err(schema_err(format!(
                "vector for {id} has a non-finite component at index {i}"
            )));
        }
        // What gets stored is the quantised row, and it is that row's norm that is written and
        // that cosine divides by (ADR-0015). A zero vector, and a vector whose every component
        // is below half the scale floor, both quantise to all-zero codes: one check covers both,
        // and it is the stored row that cosine cannot point with.
        let staged = StagedRow::new(v);
        let norm = f64::from(staged.norm);
        if self.metric() == Metric::Cosine && norm == 0.0 {
            return Err(schema_err(format!(
                "vector for {id} has zero norm at eight-bit precision; cosine similarity is \
                 undefined"
            )));
        }
        // The norm is persisted as f32; a finite vector such as [f32::MAX, f32::MAX] has a norm
        // that only fits f64, and storing it as +inf would silently score its own cosine as 0.
        if !staged.norm.is_finite() {
            return Err(schema_err(format!(
                "vector for {id} has norm {:e}, which does not fit a finite f32",
                crate::quantise::norm(&staged.quantised)
            )));
        }
        Ok(staged)
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
        if self.read_only {
            return Err(Error::read_only());
        }
        let staged = self.validate_vector(id, vector)?;
        self.pending.insert(id, Some(staged));
        Ok(())
    }

    fn delete(&mut self, ids: &[DocId]) -> Result<()> {
        if self.read_only {
            return Err(Error::read_only());
        }
        for &id in ids {
            self.pending.insert(id, None);
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if self.read_only {
            return Err(Error::read_only());
        }
        self.confirm_sync()?;
        // Even a no-op answers for the state on disk (see `compact`).
        self.verify_unchanged()?;
        if self.pending.is_empty() {
            return Ok(());
        }
        self.verify_order()?;
        // 0. The dead-row share this commit would leave. Over the configured threshold, the
        //    commit *is* a rewrite — one protocol, one rename — never an append followed by a
        //    separate compaction that could fail after the append is durable.
        let (adds, superseded) = self.pending_counts();
        let rows_after = self.layout.count + adds;
        let dead_after = self.dead.len() + superseded as u64;
        // The share is compared in its own precision (`f32`), so "more than the share" means the
        // same at every boundary whatever the share's binary representation: 7 of 10 at 0.7
        // and 6 of 10 at 0.6 are both *at* the share, not over it.
        if let Some(t) = self.compaction_threshold
            && rows_after > 0
            && (dead_after as f32 / rows_after as f32) > t
        {
            return self.rewrite();
        }
        let layout = Rows::for_count(rows_after, self.header.dim)?;
        // 1. Resolve the pending changes against the committed state, on copies: nothing in
        //    memory changes until the manifest is renamed, so a failed commit keeps every staged
        //    change for a retry.
        let mut dead = self.dead.clone();
        let mut buf = Vec::with_capacity(adds * layout.row_bytes);
        let mut appended: Vec<(u32, u32)> = Vec::new(); // (id, row)
        let mut removed: Vec<u32> = Vec::new(); // ids whose live row is superseded or deleted
        let mut next_row = self.layout.count as u32;
        let last_id =
            (self.layout.count > 0).then(|| self.layout.id_at(self.bytes(), self.layout.count - 1));
        let mut ordered = self.header.ordered;
        for (id, change) in &self.pending {
            if let Some(r) = self.live_row(*id) {
                dead.insert(r as u32);
                removed.push(id.0);
            }
            if let Some(v) = change {
                format::encode_row(&mut buf, id.0, v);
                if appended.is_empty() && last_id.is_some_and(|l| l >= id.0) {
                    ordered = false;
                }
                appended.push((id.0, next_row));
                next_row += 1;
            }
        }
        // 2. Append the new rows — beyond every live mapping's end (ADR-0013) — and sync. A
        //    read+write handle, not an append-only one: the truncation of a stale tail needs
        //    write access to the end of file on every platform.
        let path = self.dir.join(format::row_file(self.header.generation));
        let committed_len = self.layout.len_bytes() as u64;
        if !buf.is_empty() {
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)?;
            let len = file.metadata()?.len();
            if len < committed_len {
                return Err(corrupt(format!(
                    "{} is {len} bytes, shorter than the {committed_len} committed",
                    path.display()
                )));
            }
            // A tail beyond the committed rows (a failed earlier attempt) is *overwritten*, not
            // cut: the append starts at the committed length, which every mapping ends at and
            // every reader stops at, so the tail is unobservable — and a truncation is not
            // portable while a mapping is open (Windows refuses `SetEndOfFile` on a mapped
            // file whatever the range). Whatever is left beyond the new length after a
            // shorter append is ignored the same way and cut at the next writable open.
            file.seek(SeekFrom::Start(committed_len))?;
            let wrote = file.write_all(&buf);
            if wrote.is_ok() {
                self.written += buf.len() as u64;
            }
            if let Err(e) = wrote.and_then(|()| file.sync_all()) {
                // Best effort: the partial tail is unobservable either way.
                let _ = file.set_len(committed_len);
                return Err(e.into());
            }
        }
        // 3. Bring the appended rows into memory *before* the manifest switches: a buffer grows
        //    in place (undone by a truncate on failure), a mapping is made anew over the new
        //    committed length. Then the manifest — the truth — replaced atomically.
        let rows_total = u64::from(next_row);
        let header = Header {
            rows: rows_total,
            live: rows_total - dead.len(),
            ordered,
            ..self.header.clone()
        };
        // Best effort, never relied on: the appended tail lies beyond every mapping and beyond
        // the manifest's rows, so a tail that cannot be cut (a mapping open on Windows) is
        // simply overwritten by the next append.
        let undo_file = |path: &Path| {
            if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
                let _ = file.set_len(committed_len);
            }
        };
        // The mapped path maps the new committed prefix before the switch (a mapping is lazy:
        // nothing is read); the buffered path appends to the in-memory tail *after* the
        // switch, infallibly — the matrix already in memory is never reallocated or copied.
        let remapped: Option<Bytes> = match self.load_path {
            LoadPath::Buffered => None,
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
        // The table the new state needs, built from the old state (infallible: an ordered,
        // tombstone-free file has unique ids) before the switch, updated after it.
        let mut table = if dead.is_empty() && ordered {
            None
        } else {
            Some(self.table.clone().unwrap_or_else(|| self.build_table()))
        };
        let switched = match write_manifest(&self.dir, &header, &dead, &mut self.written) {
            Ok(()) => Ok(()),
            Err(ManifestFailure::BeforeSwitch(e)) => {
                // The temporary mapping covers the appended bytes: it must be gone before they
                // are cut (the invariant in `bytes::map_readonly`).
                drop(remapped);
                if !buf.is_empty() {
                    undo_file(&path);
                }
                return Err(e);
            }
            Err(ManifestFailure::AfterSwitch(e)) => Err(e),
        };
        // 4. The manifest is switched: only infallible adoption follows.
        if let Some(t) = &mut table {
            for id in &removed {
                t.remove(id);
            }
            for &(id, r) in &appended {
                t.insert(id, r);
            }
        }
        let (rows, tail) = match remapped {
            Some(b) => (b, Vec::new()),
            None => {
                let mut tail = std::mem::take(&mut self.tail);
                tail.extend_from_slice(&buf);
                (
                    std::mem::replace(&mut self.rows, Bytes::Owned(Vec::new())),
                    tail,
                )
            }
        };
        self.adopt(Committed {
            header,
            layout,
            rows,
            tail,
            dead,
            table,
        });
        self.pending.clear();
        if switched.is_err() {
            self.sync_pending = true;
        }
        switched
    }

    fn search(&self, query: &[f32], allowed: Option<&DocSet>, k: usize) -> Result<Vec<Hit>> {
        let metric = self.metric();
        // Validation precedes the no-work shortcut on purpose: a malformed query is a caller
        // error whatever `k` is, and hiding it behind `k == 0` would let it surface later.
        let q = search::validate_query(query, self.header.dim, metric)?;
        if k == 0 || allowed.is_some_and(DocSet::is_empty) {
            return Ok(Vec::new());
        }
        // The metric was decided by `validate_query`; each arm is one loop over the rows with
        // nothing to decide per row (review finding 5), and the row's codes, scale and norm
        // arrive decoded once by `scan`.
        let scored = match &q {
            search::Query::Dot(codes) => self.scan(allowed, |row_codes, scale, _| {
                search::dot_score(codes, row_codes, scale)
            }),
            search::Query::Cosine { codes, norm } => {
                self.scan(allowed, |row_codes, scale, row_norm| {
                    search::cosine_score(codes, *norm, row_codes, scale, row_norm)
                })
            }
            search::Query::Euclidean(vector) => self.scan(allowed, |row_codes, scale, _| {
                search::euclidean_score(vector, Rows::recover(row_codes, scale))
            }),
        }?;
        Ok(search::top_k(scored, k))
    }

    fn len(&self) -> u64 {
        self.header.live
    }
}

impl FlatIndex {
    /// Row `r`'s stored scale and norm, checked: a scale that is not a normal positive number,
    /// or a norm that is not finite (or, under Cosine, not positive), was never written by
    /// this engine, and rather than score or recover it — a NaN would make the order arbitrary
    /// — the reader refuses the row file as corrupt (contract "Refusals"). The check lives at
    /// the readers rather than at open because an open reads nothing beyond the manifest
    /// (Feature 024); the scan and `vector` are the first readers of a row.
    ///
    /// The codes are **not** checked: the format carries no checksum, so a damaged code byte
    /// is indistinguishable from a written one whatever its value, and a check for the one
    /// value the engine never writes (`0x80`, −128) would cost a pass over every row's codes
    /// to catch one damage pattern in 256. The reader takes the byte as −128; the dimension
    /// bound (`quantise::MAX_DIM`) counts it so the accumulator cannot overflow; the contract
    /// says so.
    ///
    /// The norm is checked only under Cosine, the one metric that reads it: under Dot and
    /// Euclidean a damaged norm changes no score, and refusing the row there would turn one
    /// bit flip into an outage for the whole index rather than degrading (Principle VI; review
    /// round 6, finding 7). The scale is read by every metric and is checked for every one.
    fn checked_row(&self, r: usize, id: u32, least_norm: Option<f32>) -> Result<(f32, f32)> {
        let bytes = self.bytes();
        let scale = self.layout.scale_at(bytes, r);
        let norm = self.layout.norm_at(bytes, r);
        let norm_ok = least_norm.is_none_or(|least| norm.is_finite() && norm >= least);
        if !(scale.is_normal() && scale > 0.0 && norm_ok) {
            return Err(corrupt(format!(
                "{} row {r} (id {id}) has scale {scale:e} and norm {norm:e}, which this engine \
                 never writes",
                format::row_file(self.header.generation)
            )));
        }
        Ok((scale, norm))
    }

    /// The least norm a row of this index may carry, for the metric that reads it: a cosine
    /// row's norm is at least its scale (one code is ±127), so at least the floor. `None` under
    /// Dot and Euclidean, which never read the norm and so never check it.
    fn least_norm(&self) -> Option<f32> {
        (self.metric() == Metric::Cosine).then_some(f32::MIN_POSITIVE)
    }

    /// Every live, allowed row as `(score_row(codes, scale, norm), id)`, with the row's scale
    /// and norm decoded and checked once ([`Self::checked_row`]) and handed to the scorer with
    /// the codes; everything loop-invariant is decided out here.
    fn scan(
        &self,
        allowed: Option<&DocSet>,
        mut score_row: impl FnMut(&[u8], f32, f32) -> f32,
    ) -> Result<Vec<(f32, u32)>> {
        let bytes = self.bytes();
        let layout = self.layout;
        let no_dead = self.dead.is_empty();
        let least_norm = self.least_norm();
        let mut scored: Vec<(f32, u32)> = Vec::with_capacity(self.header.live as usize);
        for r in 0..layout.count {
            if !no_dead && self.dead.contains(r as u32) {
                continue;
            }
            let id = layout.id_at(bytes, r);
            if allowed.is_some_and(|set| !set.contains(DocId(id))) {
                continue;
            }
            let (scale, norm) = self.checked_row(r, id, least_norm)?;
            scored.push((score_row(layout.codes_at(bytes, r), scale, norm), id));
        }
        Ok(scored)
    }

    /// Live id → row from the current rows and tombstones. Infallible here: it is only built
    /// from a state whose ids were validated at open (duplicates are `Corrupt` there).
    fn build_table(&self) -> BTreeMap<u32, u32> {
        let bytes = self.bytes();
        (0..self.layout.count)
            .filter(|&r| !self.dead.contains(r as u32))
            .map(|r| (self.layout.id_at(bytes, r), r as u32))
            .collect()
    }
}

/// A row of the merged (committed ⊕ pending) sequence a rewrite emits.
enum Merged<'a> {
    Kept(usize),
    Pending(u32, &'a StagedRow),
}

/// The ascending merge of the committed live rows with the pending changes.
struct Merge<C, P>
where
    C: Iterator<Item = (u32, usize)>,
    P: Iterator,
{
    committed: std::iter::Peekable<C>,
    pending: std::iter::Peekable<P>,
}

impl<'a, C, P> Iterator for Merge<C, P>
where
    C: Iterator<Item = (u32, usize)>,
    P: Iterator<Item = (u32, Option<&'a StagedRow>)>,
{
    type Item = Merged<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.committed.peek().copied();
            let p = self.pending.peek().copied();
            match (c, p) {
                (None, None) => return None,
                (Some((_, r)), None) => {
                    self.committed.next();
                    return Some(Merged::Kept(r));
                }
                (None, Some((id, change))) => {
                    self.pending.next();
                    if let Some(v) = change {
                        return Some(Merged::Pending(id, v));
                    }
                }
                (Some((cid, r)), Some((pid, change))) => match cid.cmp(&pid) {
                    std::cmp::Ordering::Less => {
                        self.committed.next();
                        return Some(Merged::Kept(r));
                    }
                    std::cmp::Ordering::Greater => {
                        self.pending.next();
                        if let Some(v) = change {
                            return Some(Merged::Pending(pid, v));
                        }
                    }
                    // The pending change wins over the committed row for the same id.
                    std::cmp::Ordering::Equal => {
                        self.committed.next();
                        self.pending.next();
                        if let Some(v) = change {
                            return Some(Merged::Pending(pid, v));
                        }
                    }
                },
            }
        }
    }
}

/// At a writable open only: a crashed append leaves a tail beyond the committed rows — cut it
/// (best effort: a read-only file keeps it, and only the committed rows are ever read); stale
/// generations and a manifest temporary are swept the same way. Nothing here touches a
/// committed byte, and nothing is mapped yet. A read-only open skips this entirely: it holds
/// no writer's role, so it alters nothing.
fn settle(dir: &Path, header: &Header, layout: &Rows) -> Result<()> {
    let path = dir.join(format::row_file(header.generation));
    let len = std::fs::metadata(&path)?.len();
    let committed = layout.len_bytes() as u64;
    if len < committed {
        return Err(corrupt(format!(
            "{} is {len} bytes, shorter than the {committed} bytes of {} committed rows",
            path.display(),
            header.rows
        )));
    }
    if len > committed
        && let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path)
    {
        let _ = file.set_len(committed);
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let stale = name == MANIFEST_TMP
                || format::row_file_generation(&name).is_some_and(|g| g != header.generation);
            if stale {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

/// Read (or map) the committed rows of the manifest's generation and, when the file needs one,
/// build the id table: a second live row for an id is `Corrupt`.
fn load(
    dir: &Path,
    header: Header,
    layout: Rows,
    dead: RoaringBitmap,
    load_path: LoadPath,
) -> Result<Committed> {
    let path = dir.join(format::row_file(header.generation));
    let rows = if layout.count == 0 {
        // Nothing to read, but the generation the manifest names must be there: a missing row
        // file is an incomplete directory, whatever the open's mode.
        std::fs::metadata(&path)?;
        Bytes::Owned(Vec::new())
    } else {
        bytes::read_prefix(&path, load_path, layout.len_bytes())?
    };
    // `ordered` is trusted here like every other manifest field (a read-only open touches
    // nothing beyond the manifest); a writable handle verifies it before its first write.
    let table = if dead.is_empty() && header.ordered {
        None
    } else {
        let bytes = RowBytes::whole(rows.as_slice());
        let mut table = BTreeMap::new();
        for r in 0..layout.count {
            if dead.contains(r as u32) {
                continue;
            }
            let id = layout.id_at(bytes, r);
            if let Some(first) = table.insert(id, r as u32) {
                return Err(corrupt(format!(
                    "{} has two live rows for id {id} (rows {first} and {r})",
                    path.display()
                )));
            }
        }
        Some(table)
    };
    Ok(Committed {
        header,
        layout,
        rows,
        tail: Vec::new(),
        dead,
        table,
    })
}

/// Write a manifest to `manifest.bin.tmp`, sync, `rename` it over `manifest.bin`, and sync
/// the directory through a handle opened *before* the rename — so an unopenable directory is
/// a failure before the switch, and only an `fsync` error on an open handle can follow it.
///
/// The rename is the switch. A failure before it leaves the previous manifest; a failure after
/// it leaves the new manifest in place with its entry's durability unconfirmed. Callers roll
/// back on the first and adopt the switched state on the second.
fn write_manifest(
    dir: &Path,
    header: &Header,
    dead: &RoaringBitmap,
    written: &mut u64,
) -> std::result::Result<(), ManifestFailure> {
    let before = ManifestFailure::BeforeSwitch;
    let encoded = format::encode_manifest(header, dead).map_err(before)?;
    let dir_handle = fs::open_dir_for_sync(dir).map_err(|e| before(e.into()))?;
    let tmp = dir.join(MANIFEST_TMP);
    (|| -> std::io::Result<()> {
        {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(&encoded)?;
            *written += encoded.len() as u64;
            file.sync_all()?;
        }
        std::fs::rename(&tmp, dir.join(MANIFEST))
    })()
    .map_err(|e| before(e.into()))?;
    if let Some(handle) = dir_handle {
        handle.sync_all().map_err(|e| {
            ManifestFailure::AfterSwitch(Error::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "the manifest is switched but the directory sync failed, so its entry's \
                     durability is unconfirmed; the index state is the new one and coherent: {e}"
                ),
            )))
        })?;
    }
    Ok(())
}

/// How a manifest write failed: before the rename (nothing on disk changed) or after it (the
/// manifest is the new one; only the directory sync failed).
enum ManifestFailure {
    BeforeSwitch(Error),
    AfterSwitch(Error),
}
