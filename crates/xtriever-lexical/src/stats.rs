//! Live-only corpus statistics (research D7, D8; spec FR-024–FR-027).
//!
//! The backend's own numbers are deletion-inclusive until a merge — `TermInfo::doc_freq` and
//! `total_num_tokens` both count deleted documents, and BM25 divides by `max_doc`. This module
//! therefore walks postings and the hidden length columns over *alive* documents instead.

use std::collections::BTreeMap;

use tantivy::postings::Postings;
use tantivy::schema::IndexRecordOption;
use tantivy::{DocSet as _, Searcher, TERMINATED, Term};
use xtriever_core::{FieldKind, FieldName, IndexStats, Result, TermStats};

use crate::error::{invalid_query, map};
use crate::schema::{FieldMap, len_field_name};

pub(crate) fn term_stats(
    searcher: &Searcher,
    fields: &FieldMap,
    name: &FieldName,
    term: &str,
) -> Result<Option<TermStats>> {
    let mf = fields.get(name)?;
    if !mf.indexed {
        return Err(invalid_query(format!("field `{name}` is not indexed")));
    }
    if !matches!(mf.kind, FieldKind::Text(_) | FieldKind::Keyword) {
        return Err(invalid_query(format!(
            "term statistics require a text or keyword field; `{name}` is {:?}",
            mf.kind
        )));
    }
    let term = Term::from_field_text(mf.field, term);
    let mut seen = false;
    let mut stats = TermStats::default();
    for reader in searcher.segment_readers() {
        let inverted = reader.inverted_index(mf.field).map_err(map)?;
        let Some(mut postings) = inverted.read_postings(&term, IndexRecordOption::WithFreqs)?
        else {
            continue;
        };
        seen = true;
        let alive = reader.alive_bitset();
        let mut doc = postings.doc();
        while doc != TERMINATED {
            if alive.is_none_or(|a| a.is_alive(doc)) {
                stats.doc_freq += 1;
                stats.total_term_freq += u64::from(postings.term_freq());
            }
            doc = postings.advance();
        }
    }
    // Unseen in every segment ⇒ `None`; seen but only in deleted documents ⇒ `Some(0, 0)` (FR-026).
    Ok(seen.then_some(stats))
}

pub(crate) fn stats(searcher: &Searcher, fields: &FieldMap) -> Result<IndexStats> {
    let mut avg_field_len = BTreeMap::new();
    for name in fields.text_fields() {
        let column_name = len_field_name(name);
        let (mut sum, mut count) = (0u64, 0u64);
        for reader in searcher.segment_readers() {
            let Some(column) = reader
                .fast_fields()
                .column_opt::<u64>(&column_name)
                .map_err(map)?
            else {
                continue;
            };
            for doc in reader.doc_ids_alive() {
                if let Some(len) = column.first(doc) {
                    sum += len;
                    count += 1;
                }
            }
        }
        if count > 0 {
            avg_field_len.insert(name.clone(), sum as f32 / count as f32);
        }
    }
    Ok(IndexStats {
        num_docs: searcher.num_docs(),
        avg_field_len,
    })
}
