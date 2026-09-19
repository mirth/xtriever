//! Dense format version 2 (Feature 024, ADR-0013; data-model "Dense directory"):
//!
//! ```text
//! manifest.bin
//!   magic          8 bytes  b"XTDENSE2"
//!   hdr_len        8 bytes  u64 LE
//!   header         JSON     {"format_version":2,"dim":..,"metric":"..","fingerprint":"..",
//!                            "generation":g,"rows":n,"live":m,"ordered":b,"tombstones_len":t}
//!   tombstones     t bytes  a `roaring` bitmap of dead row indices (portable serialisation)
//!
//! vectors.<g>.bin          n rows in commit order, each
//!   id             4 bytes  u32 LE
//!   norm           4 bytes  f32 LE   (Euclidean norm of the row; used by Cosine, written always)
//!   vector         dim × 4  f32 LE
//! ```
//!
//! `ordered` records whether the row file's ids are strictly ascending (a fresh or compacted
//! generation, appended to only with higher ids): with no tombstones, such a file needs no id
//! table — `vector(id)` is a binary search — so a read-only open of a shipped index touches
//! nothing beyond the manifest.
//!
//! The manifest is the truth: it is replaced atomically (written to `manifest.bin.tmp`, synced,
//! renamed) and its `rows` is the committed length of the row file — bytes beyond it are not
//! part of the index. Rows are only ever appended; a compaction writes a new generation and
//! the manifest switches to it. Decoding is `from_le_bytes` over `chunks_exact(4)` — no
//! transmute, no alignment requirement — so the same code reads a heap buffer and a memory map.
//!
//! Version 1 (`index.bin`, columnar: ids, norms, vectors) is not read; it is refused at open
//! naming both versions.

use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use xtriever_core::{Metric, Result};

use crate::FORMAT_VERSION;
use crate::error::corrupt;

const MAGIC: &[u8; 8] = b"XTDENSE2";
/// Every dense format's magic is `XTDENSE` followed by one version digit; a manifest whose
/// magic carries another digit is a *versioned* refusal naming both versions, not bad magic.
const MAGIC_PREFIX: &[u8; 7] = b"XTDENSE";

pub(crate) const MANIFEST: &str = "manifest.bin";
pub(crate) const MANIFEST_TMP: &str = "manifest.bin.tmp";
pub(crate) const V1_FILE: &str = "index.bin";

/// The row file of generation `g`.
pub(crate) fn row_file(generation: u64) -> String {
    format!("vectors.{generation}.bin")
}

/// The generation a row-file name carries, if it is one (`vectors.<g>.bin`).
pub(crate) fn row_file_generation(name: &str) -> Option<u64> {
    name.strip_prefix("vectors.")?
        .strip_suffix(".bin")?
        .parse()
        .ok()
}

/// The JSON header. Field order is the on-disk key order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Header {
    pub format_version: u32,
    pub dim: usize,
    pub metric: MetricName,
    pub fingerprint: String,
    pub generation: u64,
    pub rows: u64,
    pub live: u64,
    /// Ids strictly ascending in the row file. Absent in manifests written before the field
    /// (read as `false` — an id table is built, which is always correct).
    #[serde(default)]
    pub ordered: bool,
    pub tombstones_len: u64,
}

/// `Metric` as it is spelled in the header.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MetricName {
    Cosine,
    Dot,
    Euclidean,
}

impl From<Metric> for MetricName {
    fn from(m: Metric) -> Self {
        match m {
            Metric::Cosine => Self::Cosine,
            Metric::Dot => Self::Dot,
            Metric::Euclidean => Self::Euclidean,
        }
    }
}

impl From<MetricName> for Metric {
    fn from(m: MetricName) -> Self {
        match m {
            MetricName::Cosine => Self::Cosine,
            MetricName::Dot => Self::Dot,
            MetricName::Euclidean => Self::Euclidean,
        }
    }
}

/// The committed rows of a row file: how many, and how to read one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rows {
    pub count: usize,
    pub dim: usize,
    pub row_bytes: usize,
}

impl Rows {
    /// The layout of `count` rows of `dim`, or `None` if the byte arithmetic overflows.
    pub fn checked(count: usize, dim: usize) -> Option<Self> {
        let row_bytes = dim.checked_mul(4)?.checked_add(8)?;
        count.checked_mul(row_bytes)?;
        Some(Self {
            count,
            dim,
            row_bytes,
        })
    }

    /// The layout of `count` rows of `dim`, or the row-space error: the byte arithmetic must
    /// fit the platform and the row indices must fit `u32` (the tombstone set's domain). The
    /// one constructor every write path uses, before any I/O.
    pub fn for_count(count: usize, dim: usize) -> Result<Self> {
        if u32::try_from(count).is_err() {
            return Err(row_space_error(count, dim));
        }
        Self::checked(count, dim).ok_or_else(|| row_space_error(count, dim))
    }

    /// The committed length in bytes.
    pub fn len_bytes(&self) -> usize {
        self.count * self.row_bytes
    }

    pub fn id_at(&self, bytes: &[u8], r: usize) -> u32 {
        u32::from_le_bytes(four(bytes, r * self.row_bytes))
    }

    pub fn norm_at(&self, bytes: &[u8], r: usize) -> f32 {
        f32::from_le_bytes(four(bytes, r * self.row_bytes + 4))
    }

    pub fn row_at<'a>(&self, bytes: &'a [u8], r: usize) -> impl Iterator<Item = f32> + 'a {
        let at = r * self.row_bytes + 8;
        bytes[at..at + self.dim * 4]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
    }
}

fn four(bytes: &[u8], at: usize) -> [u8; 4] {
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

/// Append one row to `out`.
pub(crate) fn encode_row(out: &mut Vec<u8>, id: u32, norm: f32, vector: &[f32]) {
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&norm.to_le_bytes());
    for x in vector {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

/// Encode a manifest: the header (with `tombstones_len` set here) and the tombstone set.
pub(crate) fn encode_manifest(header: &Header, dead: &RoaringBitmap) -> Result<Vec<u8>> {
    let mut tombstones = Vec::with_capacity(dead.serialized_size());
    dead.serialize_into(&mut tombstones)
        .map_err(|e| corrupt(format!("cannot encode tombstones: {e}")))?;
    let header = Header {
        tombstones_len: tombstones.len() as u64,
        ..header.clone()
    };
    let json =
        serde_json::to_vec(&header).map_err(|e| corrupt(format!("cannot encode header: {e}")))?;
    let mut out = Vec::with_capacity(16 + json.len() + tombstones.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(json.len() as u64).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&tombstones);
    Ok(out)
}

/// The refusal of a layout that does not fit: an `Io` error (the caller's disk state is fine;
/// the request is what cannot be served) naming the limit and the remedy.
pub(crate) fn row_space_error(count: usize, dim: usize) -> xtriever_core::Error {
    xtriever_core::Error::Io(std::io::Error::other(format!(
        "dense row space exhausted: {count} rows of dim {dim} exceed this platform's limits \
         ({} rows, {} bytes) — compact the index first",
        u32::MAX,
        usize::MAX
    )))
}

/// Validate a manifest's bytes and return its header, its row layout (validated) and its
/// tombstone set (`Corrupt` on any problem, naming both versions when the version is not this
/// build's).
pub(crate) fn decode_manifest(bytes: &[u8]) -> Result<(Header, Rows, RoaringBitmap)> {
    let Some(magic) = bytes.get(..8) else {
        return Err(corrupt(format!(
            "{MANIFEST} is {} bytes, shorter than the magic",
            bytes.len()
        )));
    };
    if magic != MAGIC
        && let [prefix @ .., digit] = magic
        && prefix == MAGIC_PREFIX
        && digit.is_ascii_digit()
    {
        if *digit == b'1' {
            return Err(version_1_error());
        }
        return Err(corrupt(format!(
            "dense index is format version {}, this build reads {FORMAT_VERSION}",
            char::from(*digit)
        )));
    }
    if magic != MAGIC {
        return Err(corrupt(format!(
            "{MANIFEST} magic is {magic:?}, expected {MAGIC:?}"
        )));
    }
    let Some(len_bytes) = bytes.get(8..16) else {
        return Err(corrupt(format!("{MANIFEST} has no header length")));
    };
    let hdr_len =
        usize::try_from(u64::from_le_bytes(len_bytes.try_into().map_err(|_| {
            corrupt(format!("{MANIFEST} header length is unreadable"))
        })?))
        .map_err(|_| {
            corrupt(format!(
                "{MANIFEST} header length does not fit this platform"
            ))
        })?;
    let Some(json) = 16usize
        .checked_add(hdr_len)
        .and_then(|end| bytes.get(16..end))
    else {
        return Err(corrupt(format!(
            "{MANIFEST} header length {hdr_len} exceeds the file ({} bytes)",
            bytes.len()
        )));
    };
    let header: Header =
        serde_json::from_slice(json).map_err(|e| corrupt(format!("{MANIFEST} header: {e}")))?;
    if header.format_version != FORMAT_VERSION {
        return Err(corrupt(format!(
            "dense index is format version {}, this build reads {FORMAT_VERSION}",
            header.format_version
        )));
    }
    if header.dim == 0 {
        return Err(corrupt(format!("{MANIFEST} dim is 0")));
    }
    if header.rows > u64::from(u32::MAX) {
        return Err(corrupt(format!(
            "{MANIFEST} rows {} exceeds the row-index range",
            header.rows
        )));
    }
    // The row layout comes from the untrusted header: checked arithmetic, so an absurd `dim`
    // or `rows` is `Corrupt`, never an overflow.
    let layout = Rows::checked(header.rows as usize, header.dim).ok_or_else(|| {
        corrupt(format!(
            "{MANIFEST} rows {} × dim {} does not fit this platform",
            header.rows, header.dim
        ))
    })?;
    let tombstones_at = 16 + hdr_len;
    let tombstones_len = usize::try_from(header.tombstones_len).map_err(|_| {
        corrupt(format!(
            "{MANIFEST} tombstones length does not fit this platform"
        ))
    })?;
    let Some(tombstones) = tombstones_at
        .checked_add(tombstones_len)
        .and_then(|end| bytes.get(tombstones_at..end))
    else {
        return Err(corrupt(format!(
            "{MANIFEST} tombstones length {tombstones_len} exceeds the file ({} bytes)",
            bytes.len()
        )));
    };
    if bytes.len() != tombstones_at + tombstones_len {
        return Err(corrupt(format!(
            "{MANIFEST} has {} trailing bytes",
            bytes.len() - tombstones_at - tombstones_len
        )));
    }
    let dead = RoaringBitmap::deserialize_from(tombstones)
        .map_err(|e| corrupt(format!("{MANIFEST} tombstones: {e}")))?;
    if dead.len() > header.rows {
        return Err(corrupt(format!(
            "{MANIFEST} has {} tombstones for {} rows",
            dead.len(),
            header.rows
        )));
    }
    if let Some(max) = dead.max()
        && u64::from(max) >= header.rows
    {
        return Err(corrupt(format!(
            "{MANIFEST} tombstone {max} is beyond the {} committed rows",
            header.rows
        )));
    }
    if header.live != header.rows - dead.len() {
        return Err(corrupt(format!(
            "{MANIFEST} live {} is not rows {} minus {} tombstones",
            header.live,
            header.rows,
            dead.len()
        )));
    }
    Ok((header, layout, dead))
}

/// The refusal of a version-1 directory (`index.bin`, Feature 004–023).
pub(crate) fn version_1_error() -> xtriever_core::Error {
    corrupt(format!(
        "dense index is format version 1 ({V1_FILE}); this build reads {FORMAT_VERSION} \
         ({MANIFEST}) — rebuild the index (Feature 024, ADR-0013)"
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn header(rows: u64, live: u64) -> Header {
        Header {
            format_version: FORMAT_VERSION,
            dim: 2,
            metric: MetricName::Dot,
            fingerprint: "fp".into(),
            generation: 3,
            rows,
            live,
            ordered: true,
            tombstones_len: 0,
        }
    }

    #[test]
    fn manifest_round_trip() {
        let mut dead = RoaringBitmap::new();
        dead.insert(1);
        let bytes = encode_manifest(&header(3, 2), &dead).unwrap();
        let (h, layout, d) = decode_manifest(&bytes).unwrap();
        assert_eq!(layout, Rows::checked(3, 2).unwrap());
        assert!(h.ordered);
        assert_eq!(h.rows, 3);
        assert_eq!(h.live, 2);
        assert_eq!(h.generation, 3);
        assert_eq!(h.tombstones_len as usize, dead.serialized_size());
        assert_eq!(d, dead);
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.starts_with("{\"format_version\":2,\"dim\":2,\"metric\":\"dot\",\"fingerprint\":\"fp\",\"generation\":3,\"rows\":3,\"live\":2,\"ordered\":true,\"tombstones_len\":"), "{json}");
    }

    #[test]
    fn rows_read_back_what_encode_row_wrote() {
        let mut out = Vec::new();
        encode_row(&mut out, 3, 1.0, &[1.0, 0.0]);
        encode_row(&mut out, 9, 2.0, &[0.0, 2.0]);
        let rows = Rows::for_count(2, 2).unwrap();
        assert_eq!(rows.len_bytes(), out.len());
        assert!(Rows::for_count(usize::MAX, 4).is_err());
        assert!(Rows::for_count(1, usize::MAX).is_err());
        assert_eq!(rows.id_at(&out, 1), 9);
        assert_eq!(rows.norm_at(&out, 1), 2.0);
        assert_eq!(rows.row_at(&out, 1).collect::<Vec<_>>(), vec![0.0, 2.0]);
        assert_eq!(row_file(7), "vectors.7.bin");
        assert_eq!(row_file_generation("vectors.7.bin"), Some(7));
        assert_eq!(row_file_generation("vectors.x.bin"), None);
        assert_eq!(row_file_generation("manifest.bin"), None);
    }

    #[test]
    fn inconsistent_manifests_are_corrupt_not_a_panic() {
        let dead = RoaringBitmap::new();
        // live ≠ rows − dead
        let bytes = encode_manifest(&header(3, 1), &dead).unwrap();
        assert!(matches!(
            decode_manifest(&bytes),
            Err(xtriever_core::Error::Corrupt(_))
        ));
        // a tombstone beyond the rows
        let mut far = RoaringBitmap::new();
        far.insert(10);
        let bytes = encode_manifest(&header(3, 2), &far).unwrap();
        assert!(matches!(
            decode_manifest(&bytes),
            Err(xtriever_core::Error::Corrupt(_))
        ));
        // a header length past the end of the file, including one that would overflow `16 + len`.
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            decode_manifest(&bytes),
            Err(xtriever_core::Error::Corrupt(_))
        ));
        // rows beyond u32
        let bytes = encode_manifest(
            &header(u64::from(u32::MAX) + 1, u64::from(u32::MAX) + 1),
            &dead,
        )
        .unwrap();
        assert!(matches!(
            decode_manifest(&bytes),
            Err(xtriever_core::Error::Corrupt(_))
        ));
        // a future versioned magic names both versions
        let mut v3 = b"XTDENSE3".to_vec();
        v3.extend_from_slice(&0u64.to_le_bytes());
        let msg = match decode_manifest(&v3).unwrap_err() {
            xtriever_core::Error::Corrupt(m) => m,
            other => panic!("{other:?}"),
        };
        assert!(msg.contains("version 3") && msg.contains('2'), "{msg}");
        // version 1 magic
        let mut v1 = b"XTDENSE1".to_vec();
        v1.extend_from_slice(&0u64.to_le_bytes());
        let msg = match decode_manifest(&v1).unwrap_err() {
            xtriever_core::Error::Corrupt(m) => m,
            other => panic!("{other:?}"),
        };
        assert!(msg.contains("version 1") && msg.contains('2'), "{msg}");
    }
}
