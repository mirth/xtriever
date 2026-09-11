//! `spike_query` — run one keyword query and return the top `k` hits.

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::{Index, TantivyDocument};

use crate::ffi::{RankedHit, SpikeError};
use crate::spike::index::{FIELD_EXTERNAL_ID, FIELD_TEXT, build_schema, stored_external_id};

/// Search `index_dir` for `query`, returning at most `k` hits, highest score first.
///
/// # Errors
///
/// [`SpikeError::IndexIo`] if the index cannot be opened or read; [`SpikeError::QueryParse`] if the
/// query cannot be parsed, **or** if it matches nothing — see below.
pub fn run(index_dir: &str, query: &str, k: u32) -> Result<Vec<RankedHit>, SpikeError> {
    let io_err = |e: &dyn std::fmt::Display| SpikeError::IndexIo {
        path: index_dir.to_owned(),
        message: e.to_string(),
    };
    let parse_err = |e: &dyn std::fmt::Display| SpikeError::QueryParse {
        message: e.to_string(),
    };

    let schema = build_schema();
    let text_field = schema.get_field(FIELD_TEXT).map_err(|e| io_err(&e))?;
    let id_field = schema
        .get_field(FIELD_EXTERNAL_ID)
        .map_err(|e| io_err(&e))?;

    let index = Index::open_in_dir(index_dir).map_err(|e| io_err(&e))?;
    let reader = index.reader().map_err(|e| io_err(&e))?;
    let searcher = reader.searcher();

    let parser = QueryParser::for_index(&index, vec![text_field]);
    let parsed = parser.parse_query(query).map_err(|e| parse_err(&e))?;

    let limit = usize::try_from(k).map_err(|e| parse_err(&e))?;
    if limit == 0 {
        return Err(parse_err(&"k must be greater than zero"));
    }

    // `order_by_score` is BM25, and it documents its tie-break as ascending `DocAddress` —
    // segment-ordinal-major, not `DocId`. Identical while the index has one segment, which the
    // single-threaded writer guarantees; `IndexOutcome::segment_count` is what proves it did.
    let collected = searcher
        .search(&parsed, &TopDocs::with_limit(limit).order_by_score())
        .map_err(|e| io_err(&e))?;

    // An empty ranking is a failure, not a vacuous pass. The fixture query is constructed to match
    // at least one document, so emptiness means analysis dropped the query terms — exactly the
    // silent-success case the spec's edge cases call out.
    if collected.is_empty() {
        return Err(SpikeError::QueryParse {
            message: format!(
                "query {query:?} matched no documents; analysis probably dropped its terms"
            ),
        });
    }

    let mut hits = Vec::with_capacity(collected.len());
    for (score, address) in collected {
        let document: TantivyDocument = searcher.doc(address).map_err(|e| io_err(&e))?;
        let external_id = stored_external_id(&document, id_field).ok_or_else(|| {
            io_err(&format!(
                "document at segment {} doc {} has no stored {FIELD_EXTERNAL_ID}",
                address.segment_ord, address.doc_id
            ))
        })?;
        hits.push(RankedHit {
            external_id,
            score,
            segment_ord: address.segment_ord,
            doc_id: address.doc_id,
        });
    }
    Ok(hits)
}
