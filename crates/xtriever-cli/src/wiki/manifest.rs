//! The pinned snapshot manifest (`reference/datasets/wiki-manifest.json`; data-model "Snapshot
//! manifest") and file verification — size **and** SHA-256, both hashes in the error.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use super::rules::Rule;

/// The whole manifest.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Manifest {
    /// Free text.
    pub note: String,
    /// Wikipedia edition (`simple`).
    pub edition: String,
    /// Snapshot date (`YYYY-MM-DD`).
    pub snapshot_date: String,
    /// Where the snapshot comes from.
    pub source: String,
    /// The text licence.
    pub licence: Licence,
    /// The pinned parquet.
    pub parquet: Parquet,
    /// The pinned derived JSONL.
    pub jsonl: Jsonl,
    /// Exclusion rules, applied in order.
    pub exclusions: Vec<Rule>,
}

/// Licence name and URL.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Licence {
    /// e.g. `CC BY-SA 4.0`.
    pub name: String,
    /// The licence text's URL.
    pub url: String,
}

/// The pinned parquet file.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Parquet {
    /// Download URL.
    pub url: String,
    /// Exact size.
    pub bytes: u64,
    /// Lower-case hex SHA-256.
    pub sha256: String,
    /// Row count.
    pub rows: u64,
}

/// The pinned JSONL derived from the parquet.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Jsonl {
    /// File name under the snapshot directory.
    pub file: String,
    /// Exact size.
    pub bytes: u64,
    /// Lower-case hex SHA-256.
    pub sha256: String,
    /// Line (article) count.
    pub lines: u64,
    /// The converter script, repo-relative.
    pub converter: String,
    /// The converter's SHA-256 at pinning time.
    pub converter_sha256: String,
}

impl Manifest {
    /// Read and parse; an unknown rule kind is an error naming it.
    ///
    /// # Errors
    ///
    /// I/O or JSON errors, with the path.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading manifest {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing manifest {}", path.display()))
    }
}

/// Streaming SHA-256 of a file, lower-case hex.
///
/// # Errors
///
/// I/O errors, with the path.
pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file =
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("reading {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Verify a file's size and SHA-256; the error names the path and both values of both.
///
/// # Errors
///
/// A mismatch or an I/O error.
pub fn verify_file(path: &Path, bytes: u64, sha256: &str) -> anyhow::Result<()> {
    let have_bytes = std::fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .len();
    let have_sha = sha256_file(path)?;
    if have_bytes != bytes || have_sha != sha256 {
        bail!(
            "{} does not match the manifest\n  expected {bytes} bytes sha256={sha256}\n  actual   {have_bytes} bytes sha256={have_sha}",
            path.display()
        );
    }
    Ok(())
}
