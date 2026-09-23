//! `xtriever-pipeline.json` — the hybrid index's identity and the last full commit (research D3).

use std::path::Path;

use serde::{Deserialize, Serialize};
use xtriever_core::{Error, FieldName, Result, Schema};

use crate::error::{corrupt, write_atomically};
use crate::rerank::RerankMode;
use crate::{FORMAT_VERSION, SPARSE_FORMAT_VERSION};

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
    /// read as `None` — an `Option` field needs no attribute for that (compact only on
    /// `merge`); the format version is unchanged.
    pub dense_compact_dead_share: Option<f32>,
    /// Feature 027: the sparse record, present exactly when the format version is
    /// [`SPARSE_FORMAT_VERSION`](crate::SPARSE_FORMAT_VERSION). Omitted, not `null`, when
    /// absent, so an index without the option writes the same bytes as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse: Option<crate::SparseRecord>,
    pub live_docs: u64,
    pub generation: u64,
}

impl Descriptor {
    pub fn write(&self, dir: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| corrupt(format!("cannot encode {FILE}: {e}")))?;
        write_atomically(&dir.join(FILE), &json)
    }

    /// Read and check the format version: 2 without a sparse record, 3 with one (Feature 027,
    /// ADR-0016); anything else is refused by name.
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
        match (d.format_version, d.sparse.is_some()) {
            (FORMAT_VERSION, false) | (SPARSE_FORMAT_VERSION, true) => Ok(d),
            (SPARSE_FORMAT_VERSION, false) => Err(corrupt(format!(
                "{} is format version {SPARSE_FORMAT_VERSION} but has no sparse record; a sparse \
                 index records its expansion",
                path.display()
            ))),
            (FORMAT_VERSION, true) => Err(corrupt(format!(
                "{} is format version {FORMAT_VERSION} but carries a sparse record; a sparse index \
                 is format version {SPARSE_FORMAT_VERSION}",
                path.display()
            ))),
            (version, _) => Err(corrupt(format!(
                "{} is format version {version}, this build reads {FORMAT_VERSION}, or \
                 {SPARSE_FORMAT_VERSION} for a sparse index; rebuild the index",
                path.display()
            ))),
        }
    }

    /// The lexical stage's schema this descriptor implies: the user's, plus the reserved
    /// `_sparse` field for a sparse index.
    pub fn lexical_schema(&self) -> Schema {
        crate::types::lexical_schema(&self.schema, self.sparse.as_ref().map(|r| r.boost))
    }

    /// The identities the index was created with must match what it is opened with (FR-006).
    pub fn check_identity(&self, lexical_schema: &Schema, fingerprint: &str) -> Result<()> {
        if self.embedder_fingerprint != fingerprint {
            return Err(Error::FingerprintMismatch {
                index: self.embedder_fingerprint.clone(),
                current: fingerprint.to_owned(),
            });
        }
        let expected = self.lexical_schema();
        if &expected != lexical_schema {
            return Err(corrupt(format!(
                "descriptor schema {expected:?} does not match the lexical index's schema {lexical_schema:?}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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
            sparse: None,
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

    fn record() -> crate::SparseRecord {
        crate::SparseRecord {
            scale: 10,
            boost: 1.0,
            field: crate::SPARSE_FIELD.to_owned(),
            encoder: "encoder@rev".to_owned(),
            tokenizer_sha256: "a".repeat(64),
            table_sha256: "b".repeat(64),
        }
    }

    /// Feature 027 (ADR-0016): a sparse index is format version 3 and carries its record.
    #[test]
    fn a_sparse_descriptor_round_trips_at_version_3() {
        let dir = tempfile::tempdir().unwrap();
        let sparse = Descriptor {
            format_version: crate::SPARSE_FORMAT_VERSION,
            sparse: Some(record()),
            ..sample()
        };
        sparse.write(dir.path()).unwrap();
        assert_eq!(Descriptor::read(dir.path()).unwrap(), sparse);
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(text.starts_with("{\n  \"format_version\": 3"), "{text}");
        assert!(text.contains("\"sparse\": {"), "{text}");
    }

    /// Without the option nothing changes on disk: no `sparse` key, version 2 (FR-002).
    #[test]
    fn a_descriptor_without_the_option_writes_no_sparse_key() {
        let dir = tempfile::tempdir().unwrap();
        sample().write(dir.path()).unwrap();
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(!text.contains("sparse"), "{text}");
    }

    /// The version and the record agree or the descriptor is corrupt: version 3 without a
    /// record, or 2 with one, is refused by name (research D8).
    #[test]
    fn version_and_sparse_record_must_agree() {
        let dir = tempfile::tempdir().unwrap();
        for (version, sparse) in [
            (crate::SPARSE_FORMAT_VERSION, None),
            (FORMAT_VERSION, Some(record())),
        ] {
            Descriptor {
                format_version: version,
                sparse,
                ..sample()
            }
            .write(dir.path())
            .unwrap();
            match Descriptor::read(dir.path()) {
                Err(Error::Corrupt(m)) => {
                    assert!(
                        m.contains(&version.to_string()) && m.contains("sparse"),
                        "{m}"
                    );
                }
                other => panic!("version {version}: expected Corrupt, got {other:?}"),
            }
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
