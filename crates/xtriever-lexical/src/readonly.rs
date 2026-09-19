//! A read-only view of an on-disk index directory (Feature 008, research D11).
//!
//! tantivy takes one lock on the read path: `IndexReader` creation acquires `META_LOCK`
//! (`reader/mod.rs:194` in 0.26.2) by creating `.tantivy-meta.lock` through
//! `Directory::open_write` (`directory/directory.rs:74-87`) and deleting it on drop. The lock
//! exists so a concurrent *writer's* garbage collection cannot delete segment files while a
//! reader opens them. An index in a directory nobody can write — an iOS app bundle — has no
//! such writer, so the lock guards nothing and its file cannot be created (007 report F-001).
//!
//! [`ReadOnlyDirectory`] delegates every read to tantivy's own [`MmapDirectory`], answers
//! `acquire_lock` with a lock that holds nothing, and refuses every write with an I/O error
//! whose message is `read-only index`. It is a wrapper, not a directory implementation.

use std::fmt;
use std::io;
use std::path::Path;
use std::sync::Arc;

use tantivy::directory::error::{DeleteError, OpenReadError, OpenWriteError};
use tantivy::directory::{
    Directory, DirectoryLock, FileHandle, Lock, MmapDirectory, WatchCallback, WatchHandle, WritePtr,
};

/// The refusal every layer shares: `xtriever_core::error::read_only_io_error` (permission
/// denied, "read-only index").
pub(crate) fn read_only_io_error() -> io::Error {
    xtriever_core::error::read_only_io_error()
}

/// [`MmapDirectory`] for reads; no lock file; every write refused.
#[derive(Clone)]
pub(crate) struct ReadOnlyDirectory {
    inner: MmapDirectory,
}

impl ReadOnlyDirectory {
    pub(crate) fn open(dir: &Path) -> Result<Self, tantivy::TantivyError> {
        Ok(Self {
            inner: MmapDirectory::open(dir)?,
        })
    }
}

impl fmt::Debug for ReadOnlyDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadOnlyDirectory")
            .field("inner", &self.inner)
            .finish()
    }
}

impl Directory for ReadOnlyDirectory {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        self.inner.get_file_handle(path)
    }

    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        Err(DeleteError::IoError {
            io_error: Arc::new(read_only_io_error()),
            filepath: path.to_path_buf(),
        })
    }

    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        self.inner.exists(path)
    }

    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        Err(OpenWriteError::wrap_io_error(
            read_only_io_error(),
            path.to_path_buf(),
        ))
    }

    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        self.inner.atomic_read(path)
    }

    fn atomic_write(&self, _path: &Path, _data: &[u8]) -> io::Result<()> {
        Err(read_only_io_error())
    }

    fn sync_directory(&self) -> io::Result<()> {
        // Nothing was written, so there is nothing to sync; refusing here would fail a reader.
        Ok(())
    }

    /// The lock holds nothing (see the module doc): `DirectoryLock: From<Box<T>>` for any
    /// `T: Send + Sync + 'static` (`directory/directory.rs:49-53`).
    fn acquire_lock(
        &self,
        _lock: &Lock,
    ) -> Result<DirectoryLock, tantivy::directory::error::LockError> {
        Ok(DirectoryLock::from(Box::new(())))
    }

    fn watch(&self, watch_callback: WatchCallback) -> tantivy::Result<WatchHandle> {
        self.inner.watch(watch_callback)
    }
}
