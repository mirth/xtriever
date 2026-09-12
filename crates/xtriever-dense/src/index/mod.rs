//! `FlatIndex`: a flat, exact `VectorIndex` persisted as one `index.bin` per generation
//! (spec FR-009–FR-017; research D7–D9).

mod format;
mod search;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use xtriever_core::{DocId, DocSet, Embedder, Error, Hit, Metric, Result, VectorIndex};

use crate::FORMAT_VERSION;
use crate::LoadPath;
use crate::bytes::{self, Bytes};
use crate::error::{corrupt, dim_mismatch, schema_err};
use format::{Header, Layout};

const FILE: &str = "index.bin";
const TMP: &str = "index.bin.tmp";

/// The committed generation: the file's bytes (owned or mapped) plus its decoded layout.
#[derive(Debug)]
struct Generation {
    bytes: Bytes,
    layout: Layout,
}

/// A flat, exact vector index persisted as one `index.bin` per generation.
///
/// `add` and `delete` stage changes in memory; `commit` writes a whole new file and replaces the
/// old one by `rename`, so a reader (or a mapping) of the previous generation is never disturbed
/// and a crash mid-commit leaves the previous generation intact.
#[derive(Debug)]
pub struct FlatIndex {
    dir: PathBuf,
    header: Header,
    load_path: LoadPath,
    committed: Generation,
    /// `Some(vector)` = add or replace, `None` = delete. Invisible until `commit`.
    pending: BTreeMap<DocId, Option<Vec<f32>>>,
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
        std::fs::create_dir_all(dir)?;
        if std::fs::read_dir(dir)?.next().is_some() {
            return Err(corrupt(format!("{} is not empty", dir.display())));
        }
        let header = Header {
            format_version: FORMAT_VERSION,
            dim,
            metric: metric.into(),
            fingerprint: fingerprint.to_owned(),
            count: 0,
        };
        write_generation(dir, &header, &[], &[], &[])?;
        Self::open_with(dir, LoadPath::Buffered)
    }

    /// Open, reading the current generation into memory.
    ///
    /// # Errors
    ///
    /// `Error::Io` if the file cannot be read; `Error::Corrupt` if it is not a version-1 index.
    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_with(dir, LoadPath::Buffered)
    }

    /// Open, mapping the current generation read-only (feature `mmap`, ADR-0007).
    ///
    /// **Precondition the caller owns**: no other process may modify or truncate
    /// `dir/index.bin` while this handle lives. This crate itself never does — `commit` only ever
    /// replaces the file by `rename` — but a mapping cannot defend against external writers,
    /// which is why this constructor exists only behind the opt-in feature. See
    /// [`crate::LoadPath::Mmap`].
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    #[cfg(feature = "mmap")]
    pub fn open_mapped(dir: &Path) -> Result<Self> {
        Self::open_with(dir, LoadPath::Mmap)
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

    /// The committed vector stored under `id`, exactly as it was added; `None` if `id` is not a
    /// committed row (pending changes are not visible, as with `search`).
    #[must_use]
    pub fn vector(&self, id: DocId) -> Option<Vec<f32>> {
        let bytes = self.committed.bytes.as_slice();
        let layout = self.committed.layout;
        // ids are strictly ascending: binary search on the id column.
        let (mut lo, mut hi) = (0usize, layout.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match layout.id_at(bytes, mid).cmp(&id.0) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(layout.row_at(bytes, mid).collect()),
            }
        }
        None
    }

    fn open_with(dir: &Path, load_path: LoadPath) -> Result<Self> {
        let committed = read_generation(dir, load_path)?;
        let (header, _) = format::decode(committed.bytes.as_slice())?;
        Ok(Self {
            dir: dir.to_path_buf(),
            header,
            load_path,
            committed,
            pending: BTreeMap::new(),
        })
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
        if self.pending.is_empty() {
            return Ok(());
        }
        // Merge committed ⊕ pending in ascending id order into a new generation. `pending` is
        // borrowed, not taken, so a failed commit keeps the staged changes (see the end).
        let bytes = self.committed.bytes.as_slice();
        let layout = self.committed.layout;
        let dim = layout.dim;
        let mut ids: Vec<u32> = Vec::with_capacity(layout.count + self.pending.len());
        let mut norms: Vec<f32> = Vec::with_capacity(ids.capacity());
        let mut rows: Vec<f32> = Vec::with_capacity(ids.capacity() * dim);
        let mut push = |id: u32, row: &mut dyn Iterator<Item = f32>, norm: Option<f32>| {
            let start = rows.len();
            rows.extend(row);
            let norm =
                norm.unwrap_or_else(|| search::norm_f64(rows[start..].iter().copied()) as f32);
            ids.push(id);
            norms.push(norm);
        };
        let mut pending = self.pending.iter().peekable();
        for i in 0..layout.count {
            let id = layout.id_at(bytes, i);
            // Emit every pending id below this committed id (pure inserts).
            while let Some((pid, change)) = pending.peek() {
                if pid.0 >= id {
                    break;
                }
                if let Some(v) = change {
                    push(pid.0, &mut v.iter().copied(), None);
                }
                pending.next();
            }
            match pending.peek() {
                Some((pid, change)) if pid.0 == id => {
                    // Replaced or deleted: the pending entry wins.
                    if let Some(v) = change {
                        push(id, &mut v.iter().copied(), None);
                    }
                    pending.next();
                }
                _ => push(
                    id,
                    &mut layout.row_at(bytes, i),
                    Some(layout.norm_at(bytes, i)),
                ),
            }
        }
        for (pid, change) in pending {
            if let Some(v) = change {
                push(pid.0, &mut v.iter().copied(), None);
            }
        }
        let header = Header {
            count: ids.len() as u64,
            ..self.header.clone()
        };
        write_generation(&self.dir, &header, &ids, &norms, &rows)?;
        self.committed = read_generation(&self.dir, self.load_path)?;
        self.header = header;
        // Only now: a failure above leaves every staged change in place for a retry.
        self.pending.clear();
        Ok(())
    }

    fn search(&self, query: &[f32], allowed: Option<&DocSet>, k: usize) -> Result<Vec<Hit>> {
        let metric = self.metric();
        // Validation precedes the no-work shortcut on purpose: a malformed query is a caller
        // error whatever `k` is, and hiding it behind `k == 0` would let it surface later.
        let q = search::validate_query(query, self.header.dim, metric)?;
        if k == 0 || allowed.is_some_and(DocSet::is_empty) {
            return Ok(Vec::new());
        }
        let bytes = self.committed.bytes.as_slice();
        let layout = self.committed.layout;
        let mut scored: Vec<(f32, u32)> = Vec::with_capacity(layout.count);
        for i in 0..layout.count {
            let id = layout.id_at(bytes, i);
            if allowed.is_some_and(|set| !set.contains(DocId(id))) {
                continue;
            }
            let s = search::score(
                metric,
                &q,
                layout.row_at(bytes, i),
                layout.norm_at(bytes, i),
            );
            scored.push((s, id));
        }
        Ok(search::top_k(scored, k))
    }

    fn len(&self) -> u64 {
        self.header.count
    }
}

/// Write a generation to `index.bin.tmp`, sync, and `rename` it over `index.bin`.
fn write_generation(
    dir: &Path,
    header: &Header,
    ids: &[u32],
    norms: &[f32],
    rows: &[f32],
) -> Result<()> {
    let encoded = format::encode(header, ids, norms, rows)?;
    let tmp = dir.join(TMP);
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, dir.join(FILE))?;
    Ok(())
}

fn read_generation(dir: &Path, load_path: LoadPath) -> Result<Generation> {
    let bytes = bytes::read(&dir.join(FILE), load_path)?;
    let (_, layout) = format::decode(bytes.as_slice())?;
    Ok(Generation { bytes, layout })
}
