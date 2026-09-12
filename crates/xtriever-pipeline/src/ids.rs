//! `ids.json` — external `String` ↔ internal `DocId`, plus chunk provenance (research D4).
//!
//! Position = internal id; `null` = deleted. Ids are assigned in ingestion order and never
//! reused, so a stale stage row can never attach to a new document.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};
use xtriever_core::{ChunkInfo, DocId, Result};

use crate::FORMAT_VERSION;
use crate::error::{corrupt, schema_err, write_atomically};

pub(crate) const FILE: &str = "ids.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnDisk {
    format_version: u32,
    external: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    chunks: BTreeMap<String, ChunkInfo>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct IdMap {
    external: Vec<Option<String>>,
    chunks: BTreeMap<u32, ChunkInfo>,
    reverse: HashMap<String, u32>,
}

impl IdMap {
    /// Assign (or reuse) the internal id for `external`; a replace overwrites the chunk entry.
    pub fn assign(&mut self, external: &str, chunk: Option<ChunkInfo>) -> Result<DocId> {
        if external.is_empty() {
            return Err(schema_err("external id must not be empty"));
        }
        let id = match self.reverse.get(external) {
            Some(&id) => id,
            None => {
                let id = u32::try_from(self.external.len())
                    .map_err(|_| corrupt("internal id space exhausted (u32)"))?;
                self.external.push(Some(external.to_owned()));
                self.reverse.insert(external.to_owned(), id);
                id
            }
        };
        match chunk {
            Some(c) => {
                self.chunks.insert(id, c);
            }
            None => {
                self.chunks.remove(&id);
            }
        }
        Ok(DocId(id))
    }

    /// Remove `external`; the slot stays reserved. `None` if unknown.
    pub fn remove(&mut self, external: &str) -> Option<DocId> {
        let id = self.reverse.remove(external)?;
        self.external[id as usize] = None;
        self.chunks.remove(&id);
        Some(DocId(id))
    }

    pub fn external(&self, id: DocId) -> Option<&str> {
        self.external.get(id.0 as usize).and_then(|s| s.as_deref())
    }

    pub fn internal(&self, external: &str) -> Option<DocId> {
        self.reverse.get(external).map(|&id| DocId(id))
    }

    pub fn chunk(&self, id: DocId) -> Option<&ChunkInfo> {
        self.chunks.get(&id.0)
    }

    pub fn live(&self) -> u64 {
        self.reverse.len() as u64
    }

    pub fn write(&self, dir: &Path) -> Result<()> {
        let on_disk = OnDisk {
            format_version: FORMAT_VERSION,
            external: self.external.clone(),
            chunks: self
                .chunks
                .iter()
                .map(|(id, c)| (id.to_string(), c.clone()))
                .collect(),
        };
        let json = serde_json::to_vec(&on_disk)
            .map_err(|e| corrupt(format!("cannot encode {FILE}: {e}")))?;
        write_atomically(&dir.join(FILE), &json)
    }

    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(FILE);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| corrupt(format!("cannot read {}: {e}", path.display())))?;
        let on_disk: OnDisk = serde_json::from_str(&text)
            .map_err(|e| corrupt(format!("{} is not a valid id map: {e}", path.display())))?;
        if on_disk.format_version != FORMAT_VERSION {
            return Err(corrupt(format!(
                "{} is format version {}, this build reads {FORMAT_VERSION}",
                path.display(),
                on_disk.format_version
            )));
        }
        let mut reverse = HashMap::with_capacity(on_disk.external.len());
        for (i, ext) in on_disk.external.iter().enumerate() {
            if let Some(ext) = ext {
                let id = u32::try_from(i).map_err(|_| corrupt("id map exceeds u32"))?;
                if reverse.insert(ext.clone(), id).is_some() {
                    return Err(corrupt(format!(
                        "external id {ext:?} appears twice in {FILE}"
                    )));
                }
            }
        }
        let mut chunks = BTreeMap::new();
        for (id, c) in on_disk.chunks {
            let id: u32 = id
                .parse()
                .map_err(|_| corrupt(format!("chunk key {id:?} is not an internal id")))?;
            chunks.insert(id, c);
        }
        Ok(Self {
            external: on_disk.external,
            chunks,
            reverse,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn assign_reuse_remove_never_reuse_and_round_trip() {
        let mut m = IdMap::default();
        assert_eq!(m.assign("a", None).unwrap(), DocId(0));
        assert_eq!(
            m.assign(
                "b",
                Some(ChunkInfo {
                    parent: "p".into(),
                    ordinal: 1,
                    byte_range: None
                })
            )
            .unwrap(),
            DocId(1)
        );
        assert_eq!(m.assign("a", None).unwrap(), DocId(0), "reused");
        assert_eq!(m.live(), 2);
        assert_eq!(m.remove("a"), Some(DocId(0)));
        assert_eq!(m.remove("a"), None);
        assert_eq!(
            m.assign("c", None).unwrap(),
            DocId(2),
            "slot 0 is never reused"
        );
        assert_eq!(m.external(DocId(0)), None);
        assert_eq!(m.internal("c"), Some(DocId(2)));
        assert_eq!(m.chunk(DocId(1)).map(|c| c.ordinal), Some(1));
        assert!(m.assign("", None).is_err());
        let dir = tempfile::tempdir().unwrap();
        m.write(dir.path()).unwrap();
        let back = IdMap::read(dir.path()).unwrap();
        assert_eq!(back.external, m.external);
        assert_eq!(back.reverse, m.reverse);
        assert_eq!(back.chunks, m.chunks);
    }
}
