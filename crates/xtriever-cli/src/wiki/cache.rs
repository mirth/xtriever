//! The sharded, resumable embedding cache (data-model "Embedding cache"; research D9).
//!
//! `<root>/<fp16>/shard-NNNNN.f32` holds `count × dim` little-endian `f32`s; the sidecar
//! `shard-NNNNN.json` carries the content key (SHA-256 over the length-prefixed passage
//! texts), count, dim and embedder fingerprint. A shard is
//! a hit only when every one of those matches and the vectors file has exactly the right
//! length; anything else is a miss, never an error. Both files are written to `.tmp` and
//! renamed, sidecar last, so a crash leaves at worst a `.tmp` nobody reads.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The cache directory for one embedder: `root/<first 16 hex of sha256(fingerprint)>`.
#[must_use]
pub fn cache_dir_for(root: &Path, embedder_fingerprint: &str) -> PathBuf {
    let hex: String = Sha256::digest(embedder_fingerprint.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    root.join(&hex[..16])
}

/// The content key of a shard: SHA-256 over each passage text preceded by its byte length
/// (`u64` little-endian) — length-prefixed so no joiner can be confused with content.
#[must_use]
pub fn shard_key(texts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for t in texts {
        hasher.update((t.len() as u64).to_le_bytes());
        hasher.update(t.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
struct Sidecar {
    key: String,
    count: usize,
    dim: usize,
    embedder_fingerprint: String,
}

fn paths(dir: &Path, shard: usize) -> (PathBuf, PathBuf) {
    (
        dir.join(format!("shard-{shard:05}.f32")),
        dir.join(format!("shard-{shard:05}.json")),
    )
}

/// The shard's vectors if — and only if — everything matches.
#[must_use]
pub fn read_shard(
    dir: &Path,
    shard: usize,
    key: &str,
    count: usize,
    dim: usize,
    embedder_fingerprint: &str,
) -> Option<Vec<f32>> {
    let (vectors, sidecar) = paths(dir, shard);
    let want = Sidecar {
        key: key.to_owned(),
        count,
        dim,
        embedder_fingerprint: embedder_fingerprint.to_owned(),
    };
    let have: Sidecar = serde_json::from_slice(&std::fs::read(sidecar).ok()?).ok()?;
    if have != want {
        return None;
    }
    let bytes = std::fs::read(vectors).ok()?;
    if bytes.len() != count * dim * 4 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

/// Write a shard atomically (vectors first, then the sidecar).
///
/// # Errors
///
/// I/O errors, with the path; a `vectors` length other than `count × dim`.
pub fn write_shard(
    dir: &Path,
    shard: usize,
    key: &str,
    count: usize,
    dim: usize,
    embedder_fingerprint: &str,
    vectors: &[f32],
) -> anyhow::Result<()> {
    anyhow::ensure!(
        vectors.len() == count * dim,
        "shard {shard}: {} floats for {count} × {dim}",
        vectors.len()
    );
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let (vectors_path, sidecar_path) = paths(dir, shard);
    let mut bytes = Vec::with_capacity(vectors.len() * 4);
    for v in vectors {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    write_atomic(&vectors_path, &bytes)?;
    let sidecar = Sidecar {
        key: key.to_owned(),
        count,
        dim,
        embedder_fingerprint: embedder_fingerprint.to_owned(),
    };
    write_atomic(&sidecar_path, &serde_json::to_vec(&sidecar)?)?;
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} → {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const FP: &str = "sentence-transformers/all-MiniLM-L6-v2@abc;dim=384";

    #[test]
    fn a_written_shard_reads_back_bit_identical() {
        let dir = tempfile::tempdir().unwrap();
        let key = shard_key(&["one", "two"]);
        let vectors: Vec<f32> = (0..2 * 3).map(|i| (i as f32) * 0.5 - 1.0).collect();
        write_shard(dir.path(), 7, &key, 2, 3, FP, &vectors).unwrap();
        assert_eq!(
            read_shard(dir.path(), 7, &key, 2, 3, FP),
            Some(vectors.clone())
        );
        let bits: Vec<u32> = read_shard(dir.path(), 7, &key, 2, 3, FP)
            .unwrap()
            .iter()
            .map(|f| f.to_bits())
            .collect();
        assert_eq!(
            bits,
            vectors.iter().map(|f| f.to_bits()).collect::<Vec<_>>()
        );
        assert!(dir.path().join("shard-00007.f32").exists());
        assert!(dir.path().join("shard-00007.json").exists());
        assert!(!dir.path().join("shard-00007.f32.tmp").exists());
    }

    #[test]
    fn every_mismatch_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let key = shard_key(&["a", "b", "c"]);
        let vectors = vec![1.0f32; 3 * 2];
        write_shard(dir.path(), 0, &key, 3, 2, FP, &vectors).unwrap();
        assert!(read_shard(dir.path(), 0, &key, 3, 2, FP).is_some());
        assert!(
            read_shard(dir.path(), 0, &shard_key(&["a", "b", "x"]), 3, 2, FP).is_none(),
            "key"
        );
        assert!(read_shard(dir.path(), 0, &key, 2, 2, FP).is_none(), "count");
        assert!(read_shard(dir.path(), 0, &key, 3, 3, FP).is_none(), "dim");
        assert!(
            read_shard(dir.path(), 0, &key, 3, 2, "other-model").is_none(),
            "fingerprint"
        );
        assert!(
            read_shard(dir.path(), 1, &key, 3, 2, FP).is_none(),
            "shard number"
        );
        // Truncated vectors file: a miss, not a panic.
        std::fs::write(dir.path().join("shard-00000.f32"), [0u8; 4]).unwrap();
        assert!(
            read_shard(dir.path(), 0, &key, 3, 2, FP).is_none(),
            "length"
        );
    }

    #[test]
    fn a_leftover_tmp_is_ignored_and_the_key_is_content_addressed() {
        let dir = tempfile::tempdir().unwrap();
        let key = shard_key(&["x"]);
        std::fs::write(dir.path().join("shard-00002.f32.tmp"), [0u8; 8]).unwrap();
        std::fs::write(dir.path().join("shard-00002.json.tmp"), b"{}").unwrap();
        assert!(read_shard(dir.path(), 2, &key, 1, 2, FP).is_none());
        assert_eq!(shard_key(&["x"]), shard_key(&["x"]));
        assert_ne!(
            shard_key(&["x", "y"]),
            shard_key(&["xy"]),
            "texts are length-prefixed"
        );
        assert_ne!(
            shard_key(&["x", "y"]),
            shard_key(&["x\0y"]),
            "a NUL inside a text is content, not a boundary"
        );
        assert_eq!(shard_key(&[]).len(), 64);
    }

    #[test]
    fn the_cache_directory_is_keyed_by_the_embedder_fingerprint() {
        let root = std::path::Path::new("/cache");
        let a = cache_dir_for(root, FP);
        let b = cache_dir_for(root, "different");
        assert_ne!(a, b);
        assert_eq!(a.parent(), Some(root));
        assert_eq!(a.file_name().unwrap().len(), 16);
    }
}
