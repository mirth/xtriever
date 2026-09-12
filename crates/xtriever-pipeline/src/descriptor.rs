//! `xtriever-pipeline.json` — the hybrid index's identity and the last full commit (research D3).

use std::path::Path;

use serde::{Deserialize, Serialize};
use xtriever_core::{Error, FieldName, Result, Schema};

use crate::FORMAT_VERSION;
use crate::error::{corrupt, write_atomically};

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
                "{} is format version {}, this build reads {FORMAT_VERSION}",
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
        assert!(text.starts_with("{\n  \"format_version\": 1"), "{text}");
        std::fs::write(
            dir.path().join(FILE),
            text.replace("\"format_version\": 1", "\"format_version\": 7"),
        )
        .unwrap();
        assert!(
            matches!(Descriptor::read(dir.path()), Err(Error::Corrupt(m)) if m.contains('7') && m.contains('1'))
        );
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
