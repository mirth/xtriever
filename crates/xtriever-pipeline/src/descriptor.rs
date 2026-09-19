//! `xtriever-pipeline.json` — the hybrid index's identity and the last full commit (research D3).

use std::path::Path;

use serde::{Deserialize, Serialize};
use xtriever_core::{Error, FieldName, Result, Schema};

use crate::FORMAT_VERSION;
use crate::error::{corrupt, write_atomically};
use crate::rerank::RerankMode;

pub(crate) const FILE: &str = "xtriever-pipeline.json";

/// Key order is the on-disk order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Descriptor {
    pub format_version: u32,
    pub schema: Schema,
    pub embedder_fingerprint: String,
    pub dense_fields: Vec<FieldName>,
    pub candidate_depth: usize,
    pub rrf_k: u32,
    /// Feature 006 (format version 2): the default re-rank depth.
    pub rerank_depth: usize,
    /// Feature 015: how the re-ranked head is ordered. Absent in indexes written before it,
    /// which read as the default (`Interpolate { alpha: 0.5 }`, ADR-0012); the format version
    /// is unchanged.
    #[serde(default)]
    pub rerank_mode: RerankMode,
    /// Feature 024: the dense compaction share. Absent in indexes written before it, which
    /// read as `None` (compact only on `merge`); the format version is unchanged.
    #[serde(default)]
    pub dense_compact_dead_share: Option<f32>,
    pub live_docs: u64,
    pub generation: u64,
}

impl Descriptor {
    pub fn write(&self, dir: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| corrupt(format!("cannot encode {FILE}: {e}")))?;
        write_atomically(&dir.join(FILE), &json)
    }

    /// Read and check the format version.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                corrupt(format!("{} is missing: not a hybrid index", path.display()))
            } else {
                Error::Io(e)
            }
        })?;
        let d: Self = serde_json::from_str(&text)
            .map_err(|e| corrupt(format!("{} is not a valid descriptor: {e}", path.display())))?;
        if d.format_version != FORMAT_VERSION {
            return Err(corrupt(format!(
                "{} is format version {}, this build reads {FORMAT_VERSION}; rebuild the index",
                path.display(),
                d.format_version
            )));
        }
        Ok(d)
    }

    /// The identities the index was created with must match what it is opened with (FR-006).
    pub fn check_identity(&self, lexical_schema: &Schema, fingerprint: &str) -> Result<()> {
        if self.embedder_fingerprint != fingerprint {
            return Err(Error::FingerprintMismatch {
                index: self.embedder_fingerprint.clone(),
                current: fingerprint.to_owned(),
            });
        }
        if &self.schema != lexical_schema {
            return Err(corrupt(format!(
                "descriptor schema {:?} does not match the lexical index's schema {:?}",
                self.schema, lexical_schema
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample() -> Descriptor {
        Descriptor {
            format_version: FORMAT_VERSION,
            schema: Schema::default(),
            embedder_fingerprint: "fp".into(),
            dense_fields: vec![],
            candidate_depth: 100,
            rrf_k: 60,
            rerank_depth: 20,
            rerank_mode: RerankMode::default(),
            dense_compact_dead_share: None,
            live_docs: 0,
            generation: 0,
        }
    }

    #[test]
    fn round_trip_and_version_check() {
        let dir = tempfile::tempdir().unwrap();
        sample().write(dir.path()).unwrap();
        assert_eq!(Descriptor::read(dir.path()).unwrap(), sample());
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(text.starts_with("{\n  \"format_version\": 2"), "{text}");
        for old in ["1", "7"] {
            std::fs::write(
                dir.path().join(FILE),
                text.replace(
                    "\"format_version\": 2",
                    &format!("\"format_version\": {old}"),
                ),
            )
            .unwrap();
            assert!(
                matches!(Descriptor::read(dir.path()), Err(Error::Corrupt(m)) if m.contains(old) && m.contains('2') && m.contains("rebuild"))
            );
        }
    }

    #[test]
    fn identity_checks() {
        let d = sample();
        assert!(matches!(
            d.check_identity(&Schema::default(), "other"),
            Err(Error::FingerprintMismatch { .. })
        ));
        let other = Schema { fields: vec![] };
        d.check_identity(&other, "fp").unwrap();
    }
}
