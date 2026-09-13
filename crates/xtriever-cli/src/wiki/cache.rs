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
