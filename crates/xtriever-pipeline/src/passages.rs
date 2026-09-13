//! `passages.bin` — every document's passage text, read on demand (research D7, ADR-0008).
//!
//! Layout (data-model "Passage Store"): magic `XTPASS01`, a `u64` LE header length, a JSON
//! header `{"format_version": 1, "count": N}`, `N + 1` `u64` LE byte offsets into the text block,
//! then the UTF-8 text block; `text(i) = block[off[i]..off[i+1]]`. Position = internal id, like
//! `ids.json`; a deleted or empty passage is an empty range. Only the offset table lives in
//! memory (8 bytes per id) — a hit's text is read from the file when a search needs it, so the
//! corpus text costs disk, never RSS. The file is only ever replaced whole (`.tmp` + `rename`).

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xtriever_core::{DocId, Result};

use crate::error::corrupt;

pub(crate) const FILE: &str = "passages.bin";
const MAGIC: &[u8; 8] = b"XTPASS01";
const STORE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    format_version: u32,
    count: u64,
}

/// The store handle: the offset table in memory, the text on disk, staged changes pending.
#[derive(Debug)]
pub(crate) struct PassageStore {
    path: PathBuf,
    /// `count + 1` offsets into the text block.
    offsets: Vec<u64>,
    /// Absolute file position of the text block.
    text_start: u64,
    /// Staged by `add`/`delete`, written by `commit`: `None` = deleted.
    pending: BTreeMap<u32, Option<String>>,
}

impl PassageStore {
    /// Write an empty store (count 0).
    pub fn create(dir: &Path) -> Result<Self> {
        let mut store = Self {
            path: dir.join(FILE),
            offsets: vec![0],
            text_start: 0,
            pending: BTreeMap::new(),
        };
        store.commit(0)?;
        Ok(store)
    }

    /// Open and validate: magic, header, offsets, and the count against the id map's length.
    pub fn open(dir: &Path, expected_count: usize) -> Result<Self> {
        let path = dir.join(FILE);
        let bad = |what: String| corrupt(format!("{}: {what}", path.display()));
        let mut file = std::fs::File::open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                corrupt(format!(
                    "{} is missing: not a format version {} hybrid index; rebuild the index",
                    path.display(),
                    crate::FORMAT_VERSION
                ))
            } else {
                xtriever_core::Error::Io(e)
            }
        })?;
        let file_len = file.metadata()?.len();
        let mut magic = [0u8; 8];
        file.read_exact(&mut magic)
            .map_err(|e| bad(format!("cannot read magic: {e}")))?;
        if &magic != MAGIC {
            return Err(bad("bad magic: not a passage store".into()));
        }
        let mut len_bytes = [0u8; 8];
        file.read_exact(&mut len_bytes)
            .map_err(|e| bad(format!("cannot read header length: {e}")))?;
        let header_len = u64::from_le_bytes(len_bytes);
        let header_usize = usize::try_from(header_len)
            .ok()
            .filter(|&n| n <= 1 << 20)
            .ok_or_else(|| bad(format!("implausible header length {header_len}")))?;
        let mut header_bytes = vec![0u8; header_usize];
        file.read_exact(&mut header_bytes)
            .map_err(|e| bad(format!("cannot read header: {e}")))?;
        let header: Header = serde_json::from_slice(&header_bytes)
            .map_err(|e| bad(format!("header is not valid JSON: {e}")))?;
        if header.format_version != STORE_FORMAT_VERSION {
            return Err(bad(format!(
                "store format version {}, this build reads {STORE_FORMAT_VERSION}",
                header.format_version
            )));
        }
        let count = usize::try_from(header.count)
            .map_err(|_| bad(format!("implausible count {}", header.count)))?;
        if count != expected_count {
            return Err(bad(format!(
                "passage store count {count} does not match the id map length {expected_count}"
            )));
        }
        let table_len = (count as u64 + 1)
            .checked_mul(8)
            .ok_or_else(|| bad("offset table overflows".into()))?;
        let text_start = 16u64
            .checked_add(header_len)
            .and_then(|n| n.checked_add(table_len))
            .ok_or_else(|| bad("offset table overflows".into()))?;
        if text_start > file_len {
            return Err(bad(format!(
                "offset table ends at {text_start} beyond the file length {file_len}"
            )));
        }
        let mut table = vec![0u8; table_len as usize];
        file.read_exact(&mut table)
            .map_err(|e| bad(format!("cannot read offset table: {e}")))?;
        let offsets: Vec<u64> = table
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
            .collect();
        if offsets.first() != Some(&0) || offsets.windows(2).any(|w| w[0] > w[1]) {
            return Err(bad("offset table is not monotone from 0".into()));
        }
        let block_len = offsets.last().copied().unwrap_or(0);
        if text_start
            .checked_add(block_len)
            .is_none_or(|end| end != file_len)
        {
            return Err(bad(format!(
                "text block of {block_len} bytes does not end at the file length {file_len}"
            )));
        }
        Ok(Self {
            path,
            offsets,
            text_start,
            pending: BTreeMap::new(),
        })
    }

    /// The number of slots (the assigned-id space) in the committed generation.
    pub fn count(&self) -> usize {
        self.offsets.len() - 1
    }

    /// Read one passage from the committed generation.
    pub fn read(&self, id: DocId) -> Result<String> {
        let i = id.0 as usize;
        let (Some(&start), Some(&end)) = (self.offsets.get(i), self.offsets.get(i + 1)) else {
            return Err(corrupt(format!(
                "stage returned internal id {id} unknown to the passage store"
            )));
        };
        let len = usize::try_from(end - start)
            .map_err(|_| corrupt(format!("{}: passage {id} is too long", self.path.display())))?;
        if len == 0 {
            return Ok(String::new());
        }
        let mut file = std::fs::File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.text_start + start))?;
        let mut bytes = vec![0u8; len];
        file.read_exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(|e| {
            corrupt(format!(
                "{}: passage {id} is not UTF-8: {e}",
                self.path.display()
            ))
        })
    }

    /// Stage a passage (`None` = deleted) for the next commit.
    pub fn stage(&mut self, id: u32, text: Option<String>) {
        self.pending.insert(id, text);
    }

    /// Write the next generation — `next_len` slots: pending texts, else the previous
    /// generation's bytes, else empty — to `passages.bin.tmp`, sync, rename, reload.
    pub fn commit(&mut self, next_len: usize) -> Result<()> {
        let tmp = self.path.with_extension("bin.tmp");
        let previous = if self.count() > 0 && self.text_start > 0 {
            Some(std::fs::File::open(&self.path)?)
        } else {
            None
        };
        let header = serde_json::to_vec(&Header {
            format_version: STORE_FORMAT_VERSION,
            count: next_len as u64,
        })
        .map_err(|e| corrupt(format!("cannot encode {FILE} header: {e}")))?;

        // Two passes over the slots: sizes for the offset table, then the bytes.
        let mut offsets: Vec<u64> = Vec::with_capacity(next_len + 1);
        offsets.push(0);
        let mut total = 0u64;
        for i in 0..next_len {
            let len = match self.pending.get(&(i as u32)) {
                Some(Some(text)) => text.len() as u64,
                Some(None) => 0,
                None => self.previous_len(i),
            };
            total = total
                .checked_add(len)
                .ok_or_else(|| corrupt(format!("{FILE}: text block overflows")))?;
            offsets.push(total);
        }

        let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        out.write_all(MAGIC)?;
        out.write_all(&(header.len() as u64).to_le_bytes())?;
        out.write_all(&header)?;
        for off in &offsets {
            out.write_all(&off.to_le_bytes())?;
        }
        let mut copy = Vec::new();
        for i in 0..next_len {
            match self.pending.get(&(i as u32)) {
                Some(Some(text)) => out.write_all(text.as_bytes())?,
                Some(None) => {}
                None => {
                    if let Some(prev) = previous.as_ref() {
                        let len = self.previous_len(i);
                        if len > 0 {
                            let mut reader = prev;
                            reader.seek(SeekFrom::Start(self.text_start + self.offsets[i]))?;
                            copy.clear();
                            copy.resize(len as usize, 0);
                            reader.read_exact(&mut copy)?;
                            out.write_all(&copy)?;
                        }
                    }
                }
            }
        }
        out.flush()?;
        out.into_inner()
            .map_err(|e| xtriever_core::Error::Io(e.into_error()))?
            .sync_all()?;
        std::fs::rename(&tmp, &self.path)?;

        self.text_start = 16 + header.len() as u64 + (next_len as u64 + 1) * 8;
        self.offsets = offsets;
        self.pending.clear();
        Ok(())
    }

    fn previous_len(&self, i: usize) -> u64 {
        match (self.offsets.get(i), self.offsets.get(i + 1)) {
            (Some(&a), Some(&b)) => b - a,
            _ => 0,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_replace_delete_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = PassageStore::create(dir.path()).unwrap();
        assert_eq!(store.count(), 0);
        store.stage(0, Some("alpha".into()));
        store.stage(1, Some("".into()));
        store.stage(2, Some("gamma δ".into()));
        store.commit(3).unwrap();
        assert_eq!(store.read(DocId(0)).unwrap(), "alpha");
        assert_eq!(store.read(DocId(1)).unwrap(), "");
        assert_eq!(store.read(DocId(2)).unwrap(), "gamma δ");
        assert!(store.read(DocId(3)).is_err());

        // Replace 0, delete 2, add 3; 1 is carried over from the previous generation.
        store.stage(0, Some("ALPHA".into()));
        store.stage(2, None);
        store.stage(3, Some("delta".into()));
        store.commit(4).unwrap();
        let reopened = PassageStore::open(dir.path(), 4).unwrap();
        assert_eq!(reopened.read(DocId(0)).unwrap(), "ALPHA");
        assert_eq!(reopened.read(DocId(1)).unwrap(), "");
        assert_eq!(reopened.read(DocId(2)).unwrap(), "");
        assert_eq!(reopened.read(DocId(3)).unwrap(), "delta");
        assert!(PassageStore::open(dir.path(), 5).is_err());
        assert!(!dir.path().join("passages.bin.tmp").exists());
    }
}
