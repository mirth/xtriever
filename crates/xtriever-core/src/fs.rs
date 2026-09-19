//! The two durable-write primitives every stage's on-disk protocol is built from, so there is
//! one definition of "atomic replace" and one of "the directory entry is durable" across the
//! workspace (Feature 024 review): the pipeline's descriptor, id map and commit marker (its
//! creation and its removal both synced), the lexical descriptor and the dense manifest all go
//! through here.
//!
//! `std`-only, as everything in this crate.

use std::io::Write;
use std::path::Path;

/// Write `bytes` to `tmp`, sync it, and `rename` it over `path` — the target is never modified
/// in place, so a reader (or a mapping) of the previous file is undisturbed and a crash leaves
/// either the old file or the new one. The directory entry is *not* synced here; call
/// [`sync_dir`] when the protocol needs the rename to be durable across a power loss.
///
/// # Errors
///
/// `std::io::Error` from any step; a failure before the rename leaves `path` untouched (and
/// possibly a partial `tmp`, which the caller's next write replaces).
pub fn write_atomically(path: &Path, tmp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    {
        let mut file = std::fs::File::create(tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(tmp, path)
}

/// Make a directory's entries durable (a rename, a new file): on POSIX the entry lives in the
/// directory, which is synced like any file. Without it a power loss can keep a renamed file
/// whose entry never reached disk, or a file that names another whose entry did not.
///
/// The power-loss ordering guarantee is made on the Unix targets the engine ships to (macOS,
/// iOS, Android, Linux). Elsewhere a directory cannot be opened for `fsync` and this is a
/// documented no-op: the crash-at-any-byte guarantee of an atomic replace still holds, the
/// ordering of entries across a power loss is the filesystem's.
///
/// # Errors
///
/// `std::io::Error` if the directory cannot be opened or synced (Unix).
pub fn sync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

/// Open a directory for a later [`sync_dir`]-style `fsync` *before* the operation whose entry
/// it will make durable, so an unopenable directory fails the operation before anything is
/// renamed rather than after. `None` where directories cannot be opened (non-Unix).
///
/// # Errors
///
/// `std::io::Error` if the directory cannot be opened (Unix).
pub fn open_dir_for_sync(dir: &Path) -> std::io::Result<Option<std::fs::File>> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir).map(Some)
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_and_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let tmp = dir.path().join("f.tmp");
        write_atomically(&path, &tmp, b"1").unwrap();
        write_atomically(&path, &tmp, b"22").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"22");
        assert!(!tmp.exists());
        sync_dir(dir.path()).unwrap();
        let _ = open_dir_for_sync(dir.path()).unwrap();
    }
}
