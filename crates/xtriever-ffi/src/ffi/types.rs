//! Wire types crossing to Swift.
//!
//! These are **provisional** (FR-031). They exist to prove the three operations' data can cross the
//! language boundary — not to propose an API. Do not depend on them from later specs, do not extend
//! them for convenience, and do not defend their shape in review as a design decision.

// See the crate-root comment and ADR-0003: the uniffi derives emit `unsafe impl`.
#![allow(unsafe_code)]

/// One input document: an opaque external id plus its text.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SpikeDocument {
    /// Caller-owned identifier. Opaque to every backend (Principle V) — it is round-tripped
    /// through a stored field and never seen as an internal id.
    pub external_id: String,
    /// The document body.
    pub text: String,
}

/// One search result.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RankedHit {
    /// The `SpikeDocument::external_id` this hit resolves to.
    pub external_id: String,
    /// BM25 score. `k1 = 1.2` and `b = 0.75` are private constants in tantivy 0.26.2 and are not
    /// configurable, so no scoring configuration can drift between host and device.
    pub score: f32,
    /// Segment ordinal. Diagnostic only: tantivy breaks score ties by ascending
    /// `DocAddress = (segment_ord, doc_id)`, not by `DocId`. When a host/device ranking mismatch
    /// happens, this is the difference between "iOS scores differently" and "iOS assigned documents
    /// to segments differently". A production surface would not expose it.
    pub segment_ord: u32,
    /// tantivy's internal dense `u32` document id within its segment. Diagnostic, as above.
    pub doc_id: u32,
}

/// Outcome of an indexing run.
#[derive(Debug, Clone, uniffi::Record)]
pub struct IndexOutcome {
    /// How many documents were committed.
    pub documents_indexed: u32,
    /// How many segments the index ended up with.
    ///
    /// Expected to be `1`. Anything higher means the writer's memory arena flushed mid-run, and the
    /// golden ranking's tie-break order is then not comparable between host and device at all — the
    /// harness should say so rather than report a mismatch.
    pub segment_count: u32,
}

/// Which weight-loading path to use, so the two can be measured against each other.
///
/// This parameter exists solely because of [`ADR-0002`]: whether memory-mapped weights keep 87.1 MiB
/// of fp32 parameters out of `phys_footprint` is the largest single lever on the 300 MB verdict, and
/// it is not knowable from documentation.
///
/// [`ADR-0002`]: ../../../docs/adr/0002-unsafe-mmap-safetensors-measurement.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LoadPath {
    /// Safe `VarBuilder::from_buffered_safetensors` — reads the file onto the heap. Primary path.
    Buffered,
    /// `unsafe VarBuilder::from_mmaped_safetensors` — the single ADR-0002 exemption. Secondary,
    /// measured. If it disagrees with [`LoadPath::Buffered`] by even one bit, it is a finding and
    /// gets deleted rather than accommodated.
    Mmapped,
}
