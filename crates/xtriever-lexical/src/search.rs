//! Top-k collection with the tie-break inside the collector (research D11, D12 as revised).
//!
//! The sort key is `(score, Reverse(DocId))`, read per candidate from the hidden id column, so the
//! backend's own top-k keeps `(score DESC, DocId ASC)` across segments **including at the
//! k-boundary**. The backend's default tie-break — ascending `DocAddress` — is segment-ordinal-major,
//! and segment ordinals for equal-size segments come from a `HashMap` iteration order (tantivy
//! `segment_updater.rs:406-407`, `segment_register.rs:18`), i.e. they are random per process. Keying
//! on `DocId` removes that source of non-determinism entirely (Principle VI, FR-013/FR-014).
//!
//! Both the filtered and unfiltered paths use this same collector, so they share one collection
//! strategy and produce bit-identical scores (FR-022): a plain `TopDocs` would use block-WAND
//! pruning while a wrapped one cannot, and the two sum BM25 clauses in different orders.

use std::cmp::Reverse;
use std::sync::Arc;

use roaring::RoaringBitmap;
use tantivy::collector::{Collector, FilterCollector, TopDocs};
use tantivy::query::Query;
use tantivy::{DocAddress, DocId as BackendDocId, Score, Searcher, SegmentReader};
use xtriever_core::{DocId, Hit, Result};

use crate::error::{corrupt, map};
use crate::schema::ID_FIELD;

/// `(score, Reverse(id))`: larger is better, so equal scores order by ascending id.
type SortKey = (Score, Reverse<u64>);

/// Map backend addresses back to caller `DocId`s through the hidden id column (used by filters).
pub(crate) fn doc_ids(searcher: &Searcher, addrs: &[DocAddress]) -> Result<Vec<DocId>> {
    let columns: Vec<_> = searcher
        .segment_readers()
        .iter()
        .map(|r| r.fast_fields().u64(ID_FIELD).map_err(map))
        .collect::<Result<_>>()?;
    addrs
        .iter()
        .map(|addr| {
            let col = columns
                .get(addr.segment_ord as usize)
                .ok_or_else(|| corrupt("segment ordinal out of range"))?;
            let id = col
                .first(addr.doc_id)
                .ok_or_else(|| corrupt(format!("document {addr:?} has no {ID_FIELD}")))?;
            to_doc_id(id)
        })
        .collect()
}

fn to_doc_id(id: u64) -> Result<DocId> {
    u32::try_from(id)
        .map(DocId)
        .map_err(|_| corrupt(format!("{ID_FIELD} value {id} exceeds u32")))
}

fn keyed_top_k(k: usize) -> impl Collector<Fruit = Vec<(SortKey, DocAddress)>> {
    TopDocs::with_limit(k).tweak_score(move |segment: &SegmentReader| {
        // A missing column would mean an index not written by this crate; `u64::MAX` then sorts
        // such a document last among ties rather than panicking inside the collector.
        let ids = segment
            .fast_fields()
            .u64(ID_FIELD)
            .ok()
            .map(|c| c.first_or_default_col(u64::MAX));
        move |doc: BackendDocId, score: Score| -> SortKey {
            let id = ids.as_ref().map_or(u64::MAX, |c| c.get_val(doc));
            (score, Reverse(id))
        }
    })
}

/// Top-`k` by `(score DESC, DocId ASC)`. With a filter, only documents whose `DocId` is in the
/// bitmap are collected; the same keyed collector decides the k-boundary either way.
pub(crate) fn top_k(
    searcher: &Searcher,
    query: &dyn Query,
    k: usize,
    filter: Option<Arc<RoaringBitmap>>,
) -> Result<Vec<Hit>> {
    if k == 0 {
        return Ok(Vec::new()); // `TopDocs::with_limit(0)` panics (D11)
    }
    let collected: Vec<(SortKey, DocAddress)> = match filter {
        None => searcher.search(query, &keyed_top_k(k)).map_err(map)?,
        Some(bitmap) => {
            let pred = move |xid: u64| u32::try_from(xid).is_ok_and(|x| bitmap.contains(x));
            let collector = FilterCollector::new(ID_FIELD.to_owned(), pred, keyed_top_k(k));
            searcher.search(query, &collector).map_err(map)?
        }
    };
    let mut hits = collected
        .into_iter()
        .map(|((score, Reverse(id)), _)| {
            Ok(Hit {
                id: to_doc_id(id)?,
                score,
            })
        })
        .collect::<Result<Vec<Hit>>>()?;
    // The collector already orders this way; the explicit sort keeps the contract local and
    // independent of the collector's tie rules (ADR-0005). `total_cmp` needs no unwrap.
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    Ok(hits)
}
