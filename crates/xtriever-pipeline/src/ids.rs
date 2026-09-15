//! `ids.json` — external `String` ↔ internal `DocId`, plus chunk provenance (005 research
//! D4; Feature 010 for the in-memory shape).
//!
//! Position = internal id; `null` = deleted. Ids are assigned in ingestion order and never
//! reused, so a stale stage row can never attach to a new document.
//!
//! **In memory** (010 research D2): every id's bytes once, in one arena, with a `(start, end)`
//! span per slot (an empty span is a deleted slot — empty ids are refused at assign); the
//! reverse lookup is a `hashbrown::HashTable<u32>` hashed and compared on the arena; chunk
//! provenance is one fixed-width slot per id, its parent interned in a second arena. About
//! 52 bytes per slot plus the ids' own bytes (contract §3), instead of three std collections
//! at ~200 bytes per slot. **On disk** nothing changed: `write` produces the same bytes as
//! before (contract §1); `read` streams the file into the arena through a serde visitor so
//! the transient never holds the old `Vec<Option<String>>` + `BTreeMap<String, _>` shape
//! (research D4).

use std::collections::BTreeMap;
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
use std::path::Path;

use hashbrown::HashTable;
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use xtriever_core::{ChunkInfo, DocId, Result};

use crate::FORMAT_VERSION;
use crate::error::{corrupt, schema_err, write_atomically};

pub(crate) const FILE: &str = "ids.json";

/// "No parent" in a chunk slot — the slot has no chunk provenance.
const NONE: u32 = u32::MAX;

/// The file's shape, used for writing only (byte-identical to the previous derive — the
/// chunk keys are decimal ids in *string* order because the map is keyed by `String`).
#[derive(Serialize)]
struct OnDisk<'a> {
    format_version: u32,
    external: Vec<Option<&'a str>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    chunks: BTreeMap<String, ChunkInfo>,
}

/// One slot's provenance: 32 bytes; `parent == NONE` means none. `Option<(u64, u64)>` is kept
/// as is so every `ChunkInfo` value round-trips (research D2).
#[derive(Debug, Clone, Copy)]
struct ChunkSlot {
    byte_range: Option<(u64, u64)>,
    parent: u32,
    ordinal: u32,
}

const NO_CHUNK: ChunkSlot = ChunkSlot {
    byte_range: None,
    parent: NONE,
    ordinal: 0,
};

#[derive(Debug, Clone)]
pub(crate) struct IdMap {
    /// Every slot's external id, concatenated, in slot order.
    id_bytes: Vec<u8>,
    /// `(start, end)` into `id_bytes` per slot; `start == end` is a deleted slot.
    spans: Vec<(u32, u32)>,
    /// The live slots, hashed and compared on their bytes in `id_bytes`.
    reverse: HashTable<u32>,
    /// Empty until the first chunk is assigned, then one slot per id.
    chunks: Vec<ChunkSlot>,
    /// Distinct parent ids, interned at first use; never removed while open.
    parent_bytes: Vec<u8>,
    /// `parents + 1` offsets into `parent_bytes`.
    parent_offsets: Vec<u32>,
    /// Parent index by parent bytes.
    parents: HashTable<u32>,
}

impl Default for IdMap {
    fn default() -> Self {
        Self {
            id_bytes: Vec::new(),
            spans: Vec::new(),
            reverse: HashTable::new(),
            chunks: Vec::new(),
            parent_bytes: Vec::new(),
            parent_offsets: vec![0],
            parents: HashTable::new(),
        }
    }
}

/// Deterministic, dependency-free; always over bytes (`str` hashes differently).
fn hash(bytes: &[u8]) -> u64 {
    BuildHasherDefault::<DefaultHasher>::default().hash_one(bytes)
}

fn span_bytes<'a>(id_bytes: &'a [u8], spans: &[(u32, u32)], slot: u32) -> &'a [u8] {
    spans
        .get(slot as usize)
        .and_then(|&(start, end)| id_bytes.get(start as usize..end as usize))
        .unwrap_or(&[])
}

fn parent_bytes_of<'a>(parent_bytes: &'a [u8], offsets: &[u32], idx: u32) -> &'a [u8] {
    let i = idx as usize;
    match (offsets.get(i), offsets.get(i + 1)) {
        (Some(&start), Some(&end)) => parent_bytes
            .get(start as usize..end as usize)
            .unwrap_or(&[]),
        _ => &[],
    }
}

impl IdMap {
    // ── lookups on the arenas ─────────────────────────────────────────────────────────────

    fn id_at(&self, slot: usize) -> Option<&str> {
        let &(start, end) = self.spans.get(slot)?;
        if start == end {
            return None;
        }
        // The arena only ever receives `&str` bytes at span boundaries, so this cannot fail.
        std::str::from_utf8(self.id_bytes.get(start as usize..end as usize)?).ok()
    }

    fn find_slot(&self, external: &str) -> Option<u32> {
        let bytes = external.as_bytes();
        self.reverse
            .find(hash(bytes), |&s| {
                span_bytes(&self.id_bytes, &self.spans, s) == bytes
            })
            .copied()
    }

    fn parent_at(&self, idx: u32) -> &str {
        std::str::from_utf8(parent_bytes_of(
            &self.parent_bytes,
            &self.parent_offsets,
            idx,
        ))
        .unwrap_or("")
    }

    // ── growth ────────────────────────────────────────────────────────────────────────────

    /// Append `external` as a new slot's bytes (no reverse entry yet).
    fn push_span(&mut self, external: &str) -> Result<u32> {
        let slot = u32::try_from(self.spans.len())
            .map_err(|_| corrupt("internal id space exhausted (u32)"))?;
        let start = self.id_bytes.len();
        let end =
            u32::try_from(start + external.len()).map_err(|_| corrupt("id bytes exceed u32"))?;
        self.id_bytes.extend_from_slice(external.as_bytes());
        // `start <= end`, so it fits too.
        self.spans.push((end - external.len() as u32, end));
        Ok(slot)
    }

    /// Insert a slot into the reverse table; the caller has checked it is absent.
    fn index_slot(&mut self, slot: u32) {
        let (id_bytes, spans) = (&self.id_bytes, &self.spans);
        let bytes = span_bytes(id_bytes, spans, slot);
        self.reverse
            .insert_unique(hash(bytes), slot, |&s| hash(span_bytes(id_bytes, spans, s)));
    }

    fn intern_parent(&mut self, parent: &str) -> Result<u32> {
        let bytes = parent.as_bytes();
        let h = hash(bytes);
        let found = self
            .parents
            .find(h, |&i| {
                parent_bytes_of(&self.parent_bytes, &self.parent_offsets, i) == bytes
            })
            .copied();
        if let Some(i) = found {
            return Ok(i);
        }
        let idx = u32::try_from(self.parent_offsets.len() - 1)
            .ok()
            .filter(|&i| i != NONE)
            .ok_or_else(|| corrupt("parent table exhausted (u32)"))?;
        let end = u32::try_from(self.parent_bytes.len() + bytes.len())
            .map_err(|_| corrupt("parent bytes exceed u32"))?;
        self.parent_bytes.extend_from_slice(bytes);
        self.parent_offsets.push(end);
        let (pb, po) = (&self.parent_bytes, &self.parent_offsets);
        self.parents
            .insert_unique(h, idx, |&i| hash(parent_bytes_of(pb, po, i)));
        Ok(idx)
    }

    fn set_chunk(&mut self, slot: u32, chunk: Option<&ChunkInfo>) -> Result<()> {
        match chunk {
            Some(c) => {
                let parent = self.intern_parent(&c.parent)?;
                let needed = (slot as usize + 1).max(self.spans.len());
                if self.chunks.len() < needed {
                    self.chunks.resize(needed, NO_CHUNK);
                }
                if let Some(s) = self.chunks.get_mut(slot as usize) {
                    *s = ChunkSlot {
                        byte_range: c.byte_range,
                        parent,
                        ordinal: c.ordinal,
                    };
                }
            }
            None => {
                if let Some(s) = self.chunks.get_mut(slot as usize) {
                    *s = NO_CHUNK;
                }
            }
        }
        Ok(())
    }

    // ── the operations (contract §4) ──────────────────────────────────────────────────────

    /// Assign (or reuse) the internal id for `external`; a replace overwrites the chunk entry.
    pub fn assign(&mut self, external: &str, chunk: Option<ChunkInfo>) -> Result<DocId> {
        if external.is_empty() {
            return Err(schema_err("external id must not be empty"));
        }
        let slot = match self.find_slot(external) {
            Some(slot) => slot,
            None => {
                let slot = self.push_span(external)?;
                self.index_slot(slot);
                slot
            }
        };
        self.set_chunk(slot, chunk.as_ref())?;
        Ok(DocId(slot))
    }

    /// Remove `external`; the slot stays reserved. `None` if unknown.
    pub fn remove(&mut self, external: &str) -> Option<DocId> {
        let bytes = external.as_bytes();
        let (id_bytes, spans) = (&self.id_bytes, &self.spans);
        let slot = match self
            .reverse
            .find_entry(hash(bytes), |&s| span_bytes(id_bytes, spans, s) == bytes)
        {
            Ok(entry) => entry.remove().0,
            Err(_) => return None,
        };
        if let Some(span) = self.spans.get_mut(slot as usize) {
            *span = (span.0, span.0);
        }
        if let Some(s) = self.chunks.get_mut(slot as usize) {
            *s = NO_CHUNK;
        }
        Some(DocId(slot))
    }

    pub fn external(&self, id: DocId) -> Option<&str> {
        self.id_at(id.0 as usize)
    }

    pub fn internal(&self, external: &str) -> Option<DocId> {
        self.find_slot(external).map(DocId)
    }

    /// The chunk provenance of a slot, if any (owned: the parent is copied out of the arena).
    pub fn chunk(&self, id: DocId) -> Option<ChunkInfo> {
        let slot = self.chunks.get(id.0 as usize)?;
        (slot.parent != NONE).then(|| ChunkInfo {
            parent: self.parent_at(slot.parent).to_owned(),
            ordinal: slot.ordinal,
            byte_range: slot.byte_range,
        })
    }

    /// The assigned-id space (deleted slots included) — the passage store's slot count.
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn live(&self) -> u64 {
        self.reverse.len() as u64
    }

    // ── the file ──────────────────────────────────────────────────────────────────────────

    pub fn write(&self, dir: &Path) -> Result<()> {
        let chunks = self
            .chunks
            .iter()
            .enumerate()
            .filter(|(_, s)| s.parent != NONE)
            .map(|(i, s)| {
                (
                    i.to_string(),
                    ChunkInfo {
                        parent: self.parent_at(s.parent).to_owned(),
                        ordinal: s.ordinal,
                        byte_range: s.byte_range,
                    },
                )
            })
            .collect();
        let on_disk = OnDisk {
            format_version: FORMAT_VERSION,
            external: (0..self.spans.len()).map(|i| self.id_at(i)).collect(),
            chunks,
        };
        let json = serde_json::to_vec(&on_disk)
            .map_err(|e| corrupt(format!("cannot encode {FILE}: {e}")))?;
        write_atomically(&dir.join(FILE), &json)
    }

    /// Read `<dir>/ids.json` straight into the arenas (research D4). Refusals: contract §2.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(FILE);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| corrupt(format!("cannot read {}: {e}", path.display())))?;
        let invalid = |e: serde_json::Error| {
            corrupt(format!("{} is not a valid id map: {e}", path.display()))
        };
        let mut map = IdMap::default();
        let mut de = serde_json::Deserializer::from_str(&text);
        let format_version = FileSeed { map: &mut map }
            .deserialize(&mut de)
            .map_err(invalid)?;
        de.end().map_err(invalid)?;
        drop(text);
        if format_version != FORMAT_VERSION {
            return Err(corrupt(format!(
                "{} is format version {format_version}, this build reads {FORMAT_VERSION}",
                path.display()
            )));
        }
        map.finish_read()
    }

    /// After the parse: the reverse table (duplicates refused), the chunk vector reconciled
    /// with the slot count, capacities trimmed.
    fn finish_read(mut self) -> Result<Self> {
        let live = self.spans.iter().filter(|(s, e)| s != e).count();
        self.reverse = HashTable::with_capacity(live);
        for slot in 0..self.spans.len() {
            let Some(ext) = self.id_at(slot) else {
                continue;
            };
            if self.find_slot(ext).is_some() {
                return Err(corrupt(format!(
                    "external id {ext:?} appears twice in {FILE}"
                )));
            }
            // `slot < spans.len() <= u32::MAX + 1`; the push already bounded it.
            self.index_slot(slot as u32);
        }
        if self.chunks.len() > self.spans.len() {
            if let Some((key, _)) = self
                .chunks
                .iter()
                .enumerate()
                .skip(self.spans.len())
                .find(|(_, s)| s.parent != NONE)
            {
                return Err(corrupt(format!("chunk key {key} has no slot in {FILE}")));
            }
            self.chunks.truncate(self.spans.len());
        } else if !self.chunks.is_empty() {
            self.chunks.resize(self.spans.len(), NO_CHUNK);
        }
        self.id_bytes.shrink_to_fit();
        self.spans.shrink_to_fit();
        self.chunks.shrink_to_fit();
        self.parent_bytes.shrink_to_fit();
        self.parent_offsets.shrink_to_fit();
        Ok(self)
    }
}

// ── the streaming reader (research D4) ───────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum Field {
    FormatVersion,
    External,
    Chunks,
    #[serde(other)]
    Other,
}

const FIELDS: &[&str] = &["format_version", "external", "chunks"];

/// The top-level object; yields `format_version`, fills the map through the seeds below.
struct FileSeed<'m> {
    map: &'m mut IdMap,
}

impl<'de> DeserializeSeed<'de> for FileSeed<'_> {
    type Value = u32;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<u32, D::Error> {
        d.deserialize_struct("OnDisk", FIELDS, self)
    }
}

impl<'de> Visitor<'de> for FileSeed<'_> {
    type Value = u32;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an id map object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> std::result::Result<u32, A::Error> {
        let mut version = None;
        let (mut seen_external, mut seen_chunks) = (false, false);
        while let Some(key) = access.next_key::<Field>()? {
            match key {
                Field::FormatVersion => {
                    if version.is_some() {
                        return Err(de::Error::duplicate_field("format_version"));
                    }
                    version = Some(access.next_value::<u32>()?);
                }
                Field::External => {
                    if seen_external {
                        return Err(de::Error::duplicate_field("external"));
                    }
                    seen_external = true;
                    access.next_value_seed(ExternalSeq { map: self.map })?;
                }
                Field::Chunks => {
                    if seen_chunks {
                        return Err(de::Error::duplicate_field("chunks"));
                    }
                    seen_chunks = true;
                    access.next_value_seed(ChunkMap { map: self.map })?;
                }
                Field::Other => {
                    access.next_value::<IgnoredAny>()?;
                }
            }
        }
        if !seen_external {
            return Err(de::Error::missing_field("external"));
        }
        version.ok_or_else(|| de::Error::missing_field("format_version"))
    }
}

/// `"external": [ "id" | null, … ]` — each entry straight into the arena.
struct ExternalSeq<'m> {
    map: &'m mut IdMap,
}

impl<'de> DeserializeSeed<'de> for ExternalSeq<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for ExternalSeq<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an array of external ids or nulls")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> std::result::Result<(), A::Error> {
        while access
            .next_element_seed(PushId { map: self.map })?
            .is_some()
        {}
        Ok(())
    }
}

/// One entry of `external`: a string (pushed) or `null` (an empty span).
struct PushId<'m> {
    map: &'m mut IdMap,
}

impl<'de> DeserializeSeed<'de> for PushId<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_option(self)
    }
}

impl<'de> Visitor<'de> for PushId<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an external id or null")
    }

    fn visit_none<E: de::Error>(self) -> std::result::Result<(), E> {
        self.map
            .push_span("")
            .map(|_| ())
            .map_err(de::Error::custom)
    }

    fn visit_unit<E: de::Error>(self) -> std::result::Result<(), E> {
        self.visit_none()
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_str(self)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<(), E> {
        if v.is_empty() {
            return Err(de::Error::custom(format!("empty external id in {FILE}")));
        }
        self.map.push_span(v).map(|_| ()).map_err(de::Error::custom)
    }
}

/// `"chunks": { "<id>": ChunkInfo, … }` — parsed into the slot vector, parents interned.
struct ChunkMap<'m> {
    map: &'m mut IdMap,
}

impl<'de> DeserializeSeed<'de> for ChunkMap<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ChunkMap<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an object of chunk provenance keyed by internal id")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> std::result::Result<(), A::Error> {
        // With `external` already read (the order `write` produces), the vector is sized
        // once; otherwise it grows and `finish_read` reconciles it.
        if self.map.chunks.is_empty() && !self.map.spans.is_empty() {
            self.map.chunks.reserve_exact(self.map.spans.len());
        }
        while let Some(key) = access.next_key_seed(ChunkKey)? {
            let chunk: ChunkInfo = access.next_value()?;
            self.map
                .set_chunk(key, Some(&chunk))
                .map_err(de::Error::custom)?;
        }
        Ok(())
    }
}

/// A chunk key: the decimal internal id, parsed without an allocation.
struct ChunkKey;

impl<'de> DeserializeSeed<'de> for ChunkKey {
    type Value = u32;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<u32, D::Error> {
        d.deserialize_str(self)
    }
}

impl Visitor<'_> for ChunkKey {
    type Value = u32;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a decimal internal id")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<u32, E> {
        v.parse()
            .map_err(|_| de::Error::custom(format!("chunk key {v:?} is not an internal id")))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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
