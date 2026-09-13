//! Bringing the weight file into memory: buffered by default, memory-mapped behind the `mmap`
//! feature. This module holds the crate's **only** hand-written `unsafe` block (ADR-0009;
//! constitution v1.3.0, Principle VII) — the same block as `xtriever-dense`'s (ADR-0007).

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

/// Map `file` read-only. The one `unsafe` block in `xtriever-rerank`.
///
/// `memmap2::MmapOptions::map` is `unsafe` because the borrow checker cannot see external
/// modification of the underlying file: if another process truncated or rewrote it, the mapped
/// slice would alias changing or unmapped memory. No code in any process can *enforce* that
/// precondition on a shared filesystem — which is why the mapped path is opt-in (`mmap`) and
/// `LoadPath::Mmap` states it as the caller's obligation, the same contract every mmap-backed
/// store offers behind a safe API.
#[cfg(feature = "mmap")]
#[allow(unsafe_code)]
pub(crate) fn map_readonly(file: &std::fs::File) -> std::io::Result<memmap2::Mmap> {
    // SAFETY (ADR-0009; ADR-0007 condition 2). Requirement: the mapped file is not modified or
    // truncated for the lifetime of the map. What this crate guarantees: the weights are a
    // read-only resource, opened after `model::verify_files` has checked their size and hash,
    // and nothing in this crate ever writes them. What this crate cannot guarantee and
    // documents as the caller's precondition on `LoadPath::Mmap`: that no *other* process
    // modifies or truncates the file meanwhile. The handle is read-only, so nothing through it
    // can write.
    unsafe { memmap2::MmapOptions::new().map(file) }
}
