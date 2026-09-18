//! Bringing a read-only file into memory: buffered by default, memory-mapped behind the `mmap`
//! feature. Both the weight loader and the vector index go through here, so this module holds
//! the crate's **only** hand-written `unsafe` block (ADR-0007; constitution v1.2.0, Principle VII).

use std::path::Path;

use xtriever_core::Result;

use crate::LoadPath;

/// The bytes of a file, however they were obtained.
#[derive(Debug)]
pub(crate) enum Bytes {
    /// `std::fs::read` — a heap buffer.
    Owned(Vec<u8>),
    /// A read-only mapping of the file (feature `mmap`).
    #[cfg(feature = "mmap")]
    Mapped(memmap2::Mmap),
}

impl Bytes {
    pub(crate) fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(v) => v.as_slice(),
            #[cfg(feature = "mmap")]
            Self::Mapped(m) => &m[..],
        }
    }

    /// The heap buffer, if this is one (a mapping cannot grow in place).
    pub(crate) fn owned_mut(&mut self) -> Option<&mut Vec<u8>> {
        match self {
            Self::Owned(v) => Some(v),
            #[cfg(feature = "mmap")]
            Self::Mapped(_) => None,
        }
    }
}

/// Read `path` through `load_path`, whole.
pub(crate) fn read(path: &Path, load_path: LoadPath) -> Result<Bytes> {
    match load_path {
        LoadPath::Buffered => Ok(Bytes::Owned(std::fs::read(path)?)),
        #[cfg(feature = "mmap")]
        LoadPath::Mmap => {
            let file = std::fs::File::open(path)?;
            Ok(Bytes::Mapped(map_readonly(&file, None)?))
        }
    }
}

/// Read exactly the first `len` bytes of `path` through `load_path` (`len > 0`): the committed
/// rows of a vector file, so a mapping never covers bytes a later append or a crashed-tail
/// truncation may touch. `Error::Corrupt` if the file is shorter than `len`.
pub(crate) fn read_prefix(path: &Path, load_path: LoadPath, len: usize) -> Result<Bytes> {
    let actual = std::fs::metadata(path)?.len();
    if actual < len as u64 {
        return Err(crate::error::corrupt(format!(
            "{} is {actual} bytes, shorter than the {len} committed",
            path.display()
        )));
    }
    match load_path {
        LoadPath::Buffered => {
            let mut v = std::fs::read(path)?;
            v.truncate(len);
            Ok(Bytes::Owned(v))
        }
        #[cfg(feature = "mmap")]
        LoadPath::Mmap => {
            let file = std::fs::File::open(path)?;
            Ok(Bytes::Mapped(map_readonly(&file, Some(len))?))
        }
    }
}

/// Map `file` read-only. The one `unsafe` block in `xtriever-dense`.
///
/// `memmap2::MmapOptions::map` is `unsafe` because the borrow checker cannot see external
/// modification of the underlying file: if another process truncated or rewrote it, the mapped
/// slice would alias changing or unmapped memory. No code in any process can *enforce* that
/// precondition on a shared filesystem — which is why the mapped paths are opt-in (`mmap`) and
/// their public constructors state it as the caller's obligation, the same contract every
/// mmap-backed store (tantivy's `MmapDirectory` included) offers behind a safe API.
#[cfg(feature = "mmap")]
#[allow(unsafe_code)]
pub(crate) fn map_readonly(
    file: &std::fs::File,
    len: Option<usize>,
) -> std::io::Result<memmap2::Mmap> {
    // SAFETY (ADR-0007 condition 2, as amended by ADR-0013). Requirement: the mapped bytes are
    // not modified or truncated for the lifetime of the map. What this crate guarantees: it
    // never modifies a mapped byte — the weights are opened read-only after
    // `model::verify_files`; a row file `vectors.<g>.bin` is mapped over exactly its committed
    // rows (`len` = the manifest's `rows × row_bytes`, never the file's length), and the crate
    // only ever *extends* it past that committed length (`FlatIndex::commit`) or *replaces* it
    // by `rename` (`FlatIndex::compact`), so an inode that has been mapped keeps its mapped
    // bytes until the last reference is dropped. The truncations this crate performs — cutting
    // a crashed append's tail at open or before an append — touch only bytes beyond the
    // committed length, which no mapping covers. What this crate cannot guarantee and documents as the caller's precondition on
    // `LoadPath::Mmap`, `FlatIndex::open_mapped` and `FlatIndex::open_mapped_for`: that no
    // *other* process modifies or truncates the file meanwhile. The handle is read-only, so
    // nothing through it can write.
    let mut options = memmap2::MmapOptions::new();
    if let Some(len) = len {
        options.len(len);
    }
    unsafe { options.map(file) }
}
