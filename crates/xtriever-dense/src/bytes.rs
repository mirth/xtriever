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
}

/// Read `path` through `load_path`.
pub(crate) fn read(path: &Path, load_path: LoadPath) -> Result<Bytes> {
    match load_path {
        LoadPath::Buffered => Ok(Bytes::Owned(std::fs::read(path)?)),
        #[cfg(feature = "mmap")]
        LoadPath::Mmap => {
            let file = std::fs::File::open(path)?;
            Ok(Bytes::Mapped(map_readonly(&file)?))
        }
    }
}

/// Map `file` read-only. The one `unsafe` block in `xtriever-dense`.
///
/// `memmap2::MmapOptions::map` is `unsafe` because the borrow checker cannot see external
/// modification of the underlying file: if another process truncated or rewrote it, the mapped
/// slice would alias changing or unmapped memory.
#[cfg(feature = "mmap")]
#[allow(unsafe_code)]
pub(crate) fn map_readonly(file: &std::fs::File) -> std::io::Result<memmap2::Mmap> {
    // SAFETY: the mapped file is never modified or truncated for the lifetime of the map
    // (ADR-0007 condition 2). Two files ever reach this function: the pinned model weights,
    // which are a read-only, hash-verified resource (`model::verify_files` runs first), and the
    // vector index's `index.bin`, which this crate only ever *replaces* — `FlatIndex::commit`
    // writes `index.bin.tmp` in full and `rename`s it over the old name, so an inode that has been
    // mapped keeps its bytes until the last reference is dropped and is never written in place.
    // Both invariants are properties of this crate's own code, not of the caller.
    unsafe { memmap2::MmapOptions::new().map(file) }
}
