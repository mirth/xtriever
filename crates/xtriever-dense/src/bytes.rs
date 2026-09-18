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
/// slice would alias changing or unmapped memory. No code in any process can *enforce* that
/// precondition on a shared filesystem — which is why the mapped paths are opt-in (`mmap`) and
/// their public constructors state it as the caller's obligation, the same contract every
/// mmap-backed store (tantivy's `MmapDirectory` included) offers behind a safe API.
#[cfg(feature = "mmap")]
#[allow(unsafe_code)]
pub(crate) fn map_readonly(file: &std::fs::File) -> std::io::Result<memmap2::Mmap> {
    // SAFETY (ADR-0007 condition 2, as amended by ADR-0013). Requirement: the mapped bytes are
    // not modified or truncated for the lifetime of the map. What this crate guarantees: it
    // never modifies a mapped byte — the weights are opened read-only after
    // `model::verify_files`; the row file `vectors.<g>.bin` is only ever *extended* by
    // `FlatIndex::commit` (an append past every live mapping's end — a mapping covers the file's
    // length at map time and nothing before that changes) or *replaced* by `FlatIndex::compact`
    // (a new generation written in full, the manifest switched by `rename`), so an inode that
    // has been mapped keeps its mapped bytes until the last reference is dropped. The one
    // truncation this crate performs — cutting a crashed append's tail at open — happens before
    // the opening handle maps anything and only on bytes beyond every manifest's committed
    // length. What this crate cannot guarantee and documents as the caller's precondition on
    // `LoadPath::Mmap`, `FlatIndex::open_mapped` and `FlatIndex::open_mapped_for`: that no
    // *other* process modifies or truncates the file meanwhile. The handle is read-only, so
    // nothing through it can write.
    unsafe { memmap2::MmapOptions::new().map(file) }
}
