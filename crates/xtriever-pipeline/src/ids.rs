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

    /// The assigned-id space (deleted slots included) — the passage store's slot count.
    pub fn len(&self) -> usize {
        self.external.len()
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
        // Feature 010 (T008): ids that are prefixes of each other, non-ASCII, a quote, and both
        // byte-range shapes must survive the round trip — compared through the accessors for
        // every slot, not through private fields.
        assert_eq!(m.assign("12", None).unwrap(), DocId(3));
        assert_eq!(
            m.assign("123", Some(chunk("p", 7, Some((3, 9))))).unwrap(),
            DocId(4)
        );
        assert_eq!(m.assign("café \"x\"", None).unwrap(), DocId(5));
        assert_eq!(m.internal("12"), Some(DocId(3)));
        assert_eq!(m.internal("123"), Some(DocId(4)));
        assert_eq!(m.internal("1"), None);
        assert_eq!(m.internal("1234"), None);
        let dir = tempfile::tempdir().unwrap();
        m.write(dir.path()).unwrap();
        let back = IdMap::read(dir.path()).unwrap();
        assert_same(&m, &back);
        assert_eq!(chunk_of(&back, 1), Some(chunk("p", 1, None)));
        assert_eq!(chunk_of(&back, 4), Some(chunk("p", 7, Some((3, 9)))));
        assert_eq!(chunk_of(&back, 0), None);
        assert_eq!(chunk_of(&back, 3), None);
    }

    fn chunk(parent: &str, ordinal: u32, byte_range: Option<(u64, u64)>) -> ChunkInfo {
        ChunkInfo {
            parent: parent.into(),
            ordinal,
            byte_range,
        }
    }

    fn chunk_of(m: &IdMap, id: u32) -> Option<ChunkInfo> {
        m.chunk(DocId(id)).map(|c| c.to_owned())
    }

    /// Equality through the public accessors over the whole id space.
    fn assert_same(a: &IdMap, b: &IdMap) {
        assert_eq!(a.len(), b.len());
        assert_eq!(a.live(), b.live());
        for i in 0..a.len() as u32 {
            assert_eq!(a.external(DocId(i)), b.external(DocId(i)), "slot {i}");
            assert_eq!(chunk_of(a, i), chunk_of(b, i), "chunk {i}");
            if let Some(ext) = a.external(DocId(i)) {
                assert_eq!(b.internal(ext), Some(DocId(i)), "reverse {ext}");
            }
        }
    }

    // ── Feature 010: what the map costs (research D7; contract §3) ──────────────────────────

    /// The bound every read must meet: the ids' and parents' own bytes plus a fixed overhead
    /// per slot and per parent (data-model "Derived bounds").
    fn held_bound(id_bytes: usize, parent_bytes: usize, slots: usize, parents: usize) -> usize {
        id_bytes + parent_bytes + 52 * slots + 16 * parents + 65_536
    }

    /// A map in the Wikipedia shape: `"{page}#{ordinal}"` ids, one to three chunks per page
    /// (deterministic LCG), every chunk with a byte range. Returns the map and the byte totals.
    fn synthetic_wikipedia_shape(passages: usize) -> (IdMap, usize, usize, usize) {
        let mut m = IdMap::default();
        let (mut id_bytes, mut parent_bytes, mut parents) = (0, 0, 0);
        let mut page: u64 = 1_000;
        let mut lcg: u64 = 0x0010_0010;
        while m.len() < passages {
            lcg = lcg
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let ordinals = 1 + ((lcg >> 33) % 3) as u32;
            let parent = page.to_string();
            parent_bytes += parent.len();
            parents += 1;
            for ordinal in 0..ordinals {
                if m.len() == passages {
                    break;
                }
                let ext = format!("{parent}#{ordinal}");
                id_bytes += ext.len();
                let ord = u64::from(ordinal);
                m.assign(
                    &ext,
                    Some(chunk(&parent, ordinal, Some((ord * 900, ord * 900 + 880)))),
                )
                .unwrap();
            }
            page += 1 + (lcg >> 40) % 5;
        }
        (m, id_bytes, parent_bytes, parents)
    }

    /// The byte totals of a map already read (for the `XTRIEVER_IDS_JSON` mode).
    fn totals(m: &IdMap) -> (usize, usize, usize) {
        let mut id_bytes = 0;
        let mut parents = std::collections::HashSet::new();
        for i in 0..m.len() as u32 {
            id_bytes += m.external(DocId(i)).map_or(0, str::len);
            if let Some(c) = chunk_of(m, i) {
                parents.insert(c.parent);
            }
        }
        let parent_bytes = parents.iter().map(String::len).sum();
        (id_bytes, parent_bytes, parents.len())
    }

    /// SC-003 / SC-004: what `read` holds afterwards and what it touches on the way, against
    /// the bounds — on a synthetic 100k-passage map, or on the file `XTRIEVER_IDS_JSON` names
    /// (the 008 index's, for the report's before/after lines). Prints one line either way.
    #[test]
    fn id_map_cost_per_passage() {
        let alloc = crate::test_alloc();
        let tmp = tempfile::tempdir().unwrap();
        let (dir, expected): (std::path::PathBuf, Option<(usize, usize, usize)>) =
            match std::env::var_os("XTRIEVER_IDS_JSON") {
                Some(file) => (
                    std::path::Path::new(&file).parent().unwrap().to_path_buf(),
                    None,
                ),
                None => {
                    let (m, id_bytes, parent_bytes, parents) = synthetic_wikipedia_shape(100_000);
                    m.write(tmp.path()).unwrap();
                    (
                        tmp.path().to_path_buf(),
                        Some((id_bytes, parent_bytes, parents)),
                    )
                }
            };
        let file_len = std::fs::metadata(dir.join(FILE)).unwrap().len() as usize;

        let base = alloc.current_usage();
        alloc.reset_peak_usage();
        let started = std::time::Instant::now();
        let m = IdMap::read(&dir).unwrap();
        let read_ms = started.elapsed().as_millis();
        let held = alloc.current_usage() - base;
        let peak = alloc.peak_usage() - base;

        let slots = m.len();
        let (id_bytes, parent_bytes, parents) = expected.unwrap_or_else(|| totals(&m));
        let bound = held_bound(id_bytes, parent_bytes, slots, parents);
        let transient_bound = bound + file_len + 16 * 1024 * 1024;
        println!(
            "id_map_cost: passages={slots} id_bytes={id_bytes} parent_bytes={parent_bytes} parents={parents} \
             held={held} peak={peak} per_passage={:.1} read_ms={read_ms} bound={bound} transient_bound={transient_bound} file={file_len}",
            held as f64 / slots as f64
        );
        drop(m);
        assert!(
            held <= bound,
            "held {held} B exceeds the bound {bound} B ({:.1} B/passage)",
            held as f64 / slots as f64
        );
        assert!(
            peak <= transient_bound,
            "peak during read {peak} B exceeds the transient bound {transient_bound} B"
        );
    }

    /// Contract §1 oracle at full scale: the 008 index's `ids.json` (sha256 `20028054…`)
    /// reproduces byte for byte from read → write. No-op unless `XTRIEVER_IDS_JSON` is set.
    #[test]
    fn full_file_write_back_is_byte_identical() {
        use sha2::{Digest, Sha256};
        let Some(file) = std::env::var_os("XTRIEVER_IDS_JSON") else {
            return;
        };
        let file = std::path::PathBuf::from(file);
        let m = IdMap::read(file.parent().unwrap()).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        m.write(tmp.path()).unwrap();
        let original = std::fs::read(&file).unwrap();
        let written = std::fs::read(tmp.path().join(FILE)).unwrap();
        let hex: String = Sha256::digest(&written)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        println!(
            "full_file_write_back: {} bytes, sha256 {hex}",
            written.len()
        );
        assert_eq!(written.len(), original.len());
        assert!(
            written == original,
            "write-back differs from the original file"
        );
    }

    // ── Feature 010: refusals (contract §2) ──────────────────────────────────────────────────

    fn read_text(text: &str) -> Result<IdMap> {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(FILE), text).unwrap();
        IdMap::read(tmp.path())
    }

    fn corrupt_message(r: Result<IdMap>) -> String {
        match r {
            Err(xtriever_core::Error::Corrupt(m)) => m,
            other => panic!("expected Error::Corrupt, got {other:?}"),
        }
    }

    #[test]
    fn refuses_wrong_format_version() {
        let msg = corrupt_message(read_text(r#"{"format_version":3,"external":["a"]}"#));
        assert!(
            msg.contains("is format version 3, this build reads 2"),
            "{msg}"
        );
    }

    #[test]
    fn refuses_duplicate_external() {
        let msg = corrupt_message(read_text(
            r#"{"format_version":2,"external":["a",null,"a"]}"#,
        ));
        assert!(
            msg.contains("external id \"a\" appears twice in ids.json"),
            "{msg}"
        );
    }

    #[test]
    fn refuses_non_numeric_chunk_key() {
        let msg = corrupt_message(read_text(
            r#"{"format_version":2,"external":["a"],"chunks":{"x":{"parent":"p","ordinal":0,"byte_range":null}}}"#,
        ));
        assert!(
            msg.contains("chunk key \"x\" is not an internal id"),
            "{msg}"
        );
    }

    #[test]
    fn refuses_chunk_key_without_slot() {
        let msg = corrupt_message(read_text(
            r#"{"format_version":2,"external":["a","b","c"],"chunks":{"7":{"parent":"p","ordinal":0,"byte_range":null}}}"#,
        ));
        assert!(msg.contains("chunk key 7 has no slot in ids.json"), "{msg}");
    }

    #[test]
    fn accepts_any_member_order() {
        let canonical = read_text(
            r#"{"format_version":2,"external":["a",null,"c"],"chunks":{"2":{"parent":"p","ordinal":4,"byte_range":[1,2]}}}"#,
        )
        .unwrap();
        let reordered = read_text(
            r#"{"chunks":{"2":{"parent":"p","ordinal":4,"byte_range":[1,2]}},"external":["a",null,"c"],"format_version":2}"#,
        )
        .unwrap();
        assert_same(&canonical, &reordered);
        assert_eq!(chunk_of(&reordered, 2), Some(chunk("p", 4, Some((1, 2)))));
        assert_eq!(reordered.external(DocId(1)), None);
        assert_eq!(reordered.live(), 2);
    }

    #[test]
    fn ignores_unknown_members() {
        let m =
            read_text(r#"{"format_version":2,"note":{"any":[1,2]},"external":["a"],"extra":1}"#)
                .unwrap();
        assert_eq!(m.internal("a"), Some(DocId(0)));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn refuses_repeated_member() {
        let msg = corrupt_message(read_text(
            r#"{"format_version":2,"external":["a"],"external":["b"]}"#,
        ));
        assert!(msg.contains("duplicate field"), "{msg}");
    }
}
