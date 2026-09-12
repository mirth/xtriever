//! `index.bin`, format version 1 (data-model "On disk", research D8):
//!
//! ```text
//! magic      8 bytes  b"XTDENSE1"
//! hdr_len    8 bytes  u64 LE
//! header     JSON     {"format_version":1,"dim":..,"metric":"..","fingerprint":"..","count":n}
//! ids        n × u32 LE, strictly ascending
//! norms      n × f32 LE   (Euclidean norm of the row; used by Cosine, written for every metric)
//! vectors    n × dim × f32 LE, row i belongs to ids[i]
//! ```
//!
//! Decoding is `from_le_bytes` over `chunks_exact(4)` — no transmute, no alignment requirement —
//! so the same code reads a heap buffer and a memory map.

use serde::{Deserialize, Serialize};
use xtriever_core::{Metric, Result};

use crate::FORMAT_VERSION;
use crate::error::corrupt;

const MAGIC: &[u8; 8] = b"XTDENSE1";

/// The JSON header. Field order is the on-disk key order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Header {
    pub format_version: u32,
    pub dim: usize,
    pub metric: MetricName,
    pub fingerprint: String,
    pub count: u64,
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

/// Byte offsets of the three sections inside a decoded file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layout {
    pub count: usize,
    pub dim: usize,
    pub ids_at: usize,
    pub norms_at: usize,
    pub vectors_at: usize,
}

impl Layout {
    pub fn id_at(&self, bytes: &[u8], i: usize) -> u32 {
        let at = self.ids_at + i * 4;
        u32::from_le_bytes(four(bytes, at))
    }

    pub fn norm_at(&self, bytes: &[u8], i: usize) -> f32 {
        let at = self.norms_at + i * 4;
        f32::from_le_bytes(four(bytes, at))
    }

    pub fn row_at<'a>(&self, bytes: &'a [u8], i: usize) -> impl Iterator<Item = f32> + 'a {
        let at = self.vectors_at + i * self.dim * 4;
        bytes[at..at + self.dim * 4]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
    }
}

fn four(bytes: &[u8], at: usize) -> [u8; 4] {
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

/// Encode a generation. `ids` must be strictly ascending, `rows.len() == ids.len() * dim`.
pub(crate) fn encode(header: &Header, ids: &[u32], norms: &[f32], rows: &[f32]) -> Result<Vec<u8>> {
    let json =
        serde_json::to_vec(header).map_err(|e| corrupt(format!("cannot encode header: {e}")))?;
    let mut out = Vec::with_capacity(16 + json.len() + ids.len() * 8 + rows.len() * 4);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(json.len() as u64).to_le_bytes());
    out.extend_from_slice(&json);
    for id in ids {
        out.extend_from_slice(&id.to_le_bytes());
    }
    for n in norms {
        out.extend_from_slice(&n.to_le_bytes());
    }
    for x in rows {
        out.extend_from_slice(&x.to_le_bytes());
    }
    Ok(out)
}

/// Validate a file's bytes and return its header and section layout (`Corrupt` on any problem).
pub(crate) fn decode(bytes: &[u8]) -> Result<(Header, Layout)> {
    let Some(magic) = bytes.get(..8) else {
        return Err(corrupt(format!(
            "index.bin is {} bytes, shorter than the magic",
            bytes.len()
        )));
    };
    if magic != MAGIC {
        return Err(corrupt(format!(
            "index.bin magic is {magic:?}, expected {MAGIC:?}"
        )));
    }
    let Some(len_bytes) = bytes.get(8..16) else {
        return Err(corrupt("index.bin has no header length"));
    };
    let hdr_len = usize::try_from(u64::from_le_bytes(
        len_bytes
            .try_into()
            .map_err(|_| corrupt("index.bin header length is unreadable"))?,
    ))
    .map_err(|_| corrupt("index.bin header length does not fit this platform"))?;
    let Some(json) = 16usize
        .checked_add(hdr_len)
        .and_then(|end| bytes.get(16..end))
    else {
        return Err(corrupt(format!(
            "index.bin header length {hdr_len} exceeds the file ({} bytes)",
            bytes.len()
        )));
    };
    let header: Header =
        serde_json::from_slice(json).map_err(|e| corrupt(format!("index.bin header: {e}")))?;
    if header.format_version != FORMAT_VERSION {
        return Err(corrupt(format!(
            "index.bin is format version {}, this build reads {FORMAT_VERSION}",
            header.format_version
        )));
    }
    let count = usize::try_from(header.count).map_err(|_| {
        corrupt(format!(
            "index.bin count {} does not fit this platform",
            header.count
        ))
    })?;
    if header.dim == 0 {
        return Err(corrupt("index.bin dim is 0"));
    }
    // Every offset comes from the untrusted header: checked arithmetic, so an absurd `count` or
    // `dim` is `Corrupt`, never an overflow.
    let too_large = || {
        corrupt(format!(
            "index.bin count {count} × dim {} does not fit this platform",
            header.dim
        ))
    };
    let column = count.checked_mul(4).ok_or_else(too_large)?;
    let ids_at = 16 + hdr_len; // bounded by `bytes.len()` above
    let norms_at = ids_at.checked_add(column).ok_or_else(too_large)?;
    let vectors_at = norms_at.checked_add(column).ok_or_else(too_large)?;
    let rows = count
        .checked_mul(header.dim)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(too_large)?;
    let expected_len = vectors_at.checked_add(rows).ok_or_else(too_large)?;
    if bytes.len() != expected_len {
        return Err(corrupt(format!(
            "index.bin is {} bytes, expected {expected_len} for count {count} × dim {}",
            bytes.len(),
            header.dim
        )));
    }
    let layout = Layout {
        count,
        dim: header.dim,
        ids_at,
        norms_at,
        vectors_at,
    };
    let mut prev: Option<u32> = None;
    for i in 0..count {
        let id = layout.id_at(bytes, i);
        if prev.is_some_and(|p| p >= id) {
            return Err(corrupt(format!(
                "index.bin ids are not strictly ascending at row {i}"
            )));
        }
        prev = Some(id);
    }
    Ok((header, layout))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let header = Header {
            format_version: FORMAT_VERSION,
            dim: 2,
            metric: MetricName::Dot,
            fingerprint: "fp".into(),
            count: 2,
        };
        let bytes = encode(&header, &[3, 9], &[1.0, 2.0], &[1.0, 0.0, 0.0, 2.0]).unwrap();
        let (h, layout) = decode(&bytes).unwrap();
        assert_eq!(h, header);
        assert_eq!(layout.id_at(&bytes, 1), 9);
        assert_eq!(layout.norm_at(&bytes, 1), 2.0);
        assert_eq!(layout.row_at(&bytes, 1).collect::<Vec<_>>(), vec![0.0, 2.0]);
        assert_eq!(
            serde_json::to_string(&header).unwrap(),
            "{\"format_version\":1,\"dim\":2,\"metric\":\"dot\",\"fingerprint\":\"fp\",\"count\":2}"
        );
    }

    #[test]
    fn absurd_count_or_dim_is_corrupt_not_a_panic() {
        for (count, dim) in [
            (u64::MAX, 1usize),
            (1 << 40, usize::MAX),
            (usize::MAX as u64, 4),
        ] {
            let header = Header {
                format_version: FORMAT_VERSION,
                dim,
                metric: MetricName::Dot,
                fingerprint: String::new(),
                count,
            };
            let bytes = encode(&header, &[], &[], &[]).unwrap();
            assert!(
                matches!(decode(&bytes), Err(xtriever_core::Error::Corrupt(_))),
                "{count} × {dim}"
            );
        }
        // A header length past the end of the file, including one that would overflow `16 + len`.
        let mut bytes = b"XTDENSE1".to_vec();
        bytes.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            decode(&bytes),
            Err(xtriever_core::Error::Corrupt(_))
        ));
    }

    #[test]
    fn unordered_ids_are_corrupt() {
        let header = Header {
            format_version: FORMAT_VERSION,
            dim: 1,
            metric: MetricName::Dot,
            fingerprint: String::new(),
            count: 2,
        };
        let bytes = encode(&header, &[5, 5], &[1.0, 1.0], &[1.0, 1.0]).unwrap();
        assert!(decode(&bytes).is_err());
    }
}
