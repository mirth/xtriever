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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use std::path::Path;

    fn repo() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    #[test]
    fn the_committed_manifest_parses_with_its_pins_and_rules() {
        let m = Manifest::load(&repo().join("reference/datasets/wiki-manifest.json")).unwrap();
        assert_eq!(m.edition, "simple");
        assert_eq!(m.snapshot_date, "2023-11-01");
        assert_eq!(m.parquet.bytes, 156_885_218);
        assert_eq!(
            m.parquet.sha256,
            "31bded16768a47c286becd292079122f5d7d4397a17b87d4250a00ccd581e6f0"
        );
        assert_eq!(m.parquet.rows, 241_787);
        assert_eq!(m.jsonl.file, "simple.jsonl");
        assert_eq!(m.jsonl.lines, 241_787);
        assert_eq!(m.jsonl.sha256.len(), 64, "the JSONL must be pinned");
        assert_eq!(m.exclusions.len(), 3);
        assert_eq!(m.licence.name, "CC BY-SA 4.0");
    }

    #[test]
    fn an_unknown_rule_kind_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let text = std::fs::read_to_string(repo().join("reference/datasets/wiki-manifest.json"))
            .unwrap()
            .replace("\"title_suffix\"", "\"title_regex\"");
        let path = dir.path().join("m.json");
        std::fs::write(&path, text).unwrap();
        let err = format!("{:#}", Manifest::load(&path).unwrap_err());
        assert!(err.contains("title_regex"), "{err}");
    }

    #[test]
    fn verify_file_names_the_path_and_both_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        std::fs::write(&path, b"hello").unwrap();
        let good = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        verify_file(&path, 5, good).unwrap();
        let err = verify_file(&path, 5, &"0".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("f.bin") && err.contains(good) && err.contains(&"0".repeat(64)),
            "{err}"
        );
        let err = verify_file(&path, 6, good).unwrap_err().to_string();
        assert!(err.contains("6") && err.contains("5"), "{err}");
    }
}
