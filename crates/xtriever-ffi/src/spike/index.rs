//! `spike_index` — build a tantivy index from a document corpus.
//!
//! The single most important line in this file is the writer construction. See [`run`].

use tantivy::schema::{
    IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::{Index, TantivyDocument, doc};

use crate::ffi::{IndexOutcome, SpikeDocument, SpikeError};

/// Field name holding the caller's opaque external identifier.
pub(crate) const FIELD_EXTERNAL_ID: &str = "external_id";
/// Field name holding the document body.
pub(crate) const FIELD_TEXT: &str = "text";

/// Per-thread memory arena for the writer.
///
/// tantivy's own minimum, `MEMORY_BUDGET_NUM_BYTES_MIN` in
/// `tantivy-0.26.2/src/indexer/index_writer.rs:32`, which is `MARGIN_IN_BYTES * 15`. It is `pub` in
/// a private module and therefore not nameable from here, so the value is repeated with that
/// citation. Deliberately the *minimum*: the arena counts toward `phys_footprint`, which this spike
/// exists to measure, and a larger budget would inflate the number for no benefit at 1,000
/// documents.
const WRITER_MEMORY_BUDGET: usize = 15_000_000;

/// The schema both indexing and querying agree on.
///
/// `text` is tokenized with positions (BM25 needs frequencies; positions cost little here and keep
/// the door open for phrase queries). `external_id` is `STRING | STORED` — untokenized, so it round
/// trips exactly, and stored so a hit can be resolved back to the caller's identifier.
pub(crate) fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field(
        FIELD_TEXT,
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ),
    );
    builder.add_text_field(FIELD_EXTERNAL_ID, STRING | STORED);
    builder.build()
}

fn io_err(path: &str, e: &dyn std::fmt::Display) -> SpikeError {
    SpikeError::IndexIo {
        path: path.to_owned(),
        message: e.to_string(),
    }
}

/// Index `documents` into a fresh index at `index_dir`.
///
/// # Errors
///
/// [`SpikeError::IndexIo`] if the directory cannot be created, written, or committed.
pub fn run(index_dir: &str, documents: &[SpikeDocument]) -> Result<IndexOutcome, SpikeError> {
    let schema = build_schema();
    let text_field = schema
        .get_field(FIELD_TEXT)
        .map_err(|e| io_err(index_dir, &e))?;
    let id_field = schema
        .get_field(FIELD_EXTERNAL_ID)
        .map_err(|e| io_err(index_dir, &e))?;

    let index =
        Index::create_in_dir(index_dir, schema.clone()).map_err(|e| io_err(index_dir, &e))?;

    // ONE worker thread, not `Index::writer`, which picks `min(available_parallelism(), 8)`.
    // This is a correctness requirement, not tuning: each indexing thread builds its own segment,
    // so a multi-threaded writer assigns `DocId`s and segments unreproducibly. Since the collector
    // breaks score ties by ascending `DocAddress = (segment_ord, doc_id)`, the golden ranking would
    // then differ between a developer Mac and an iPhone for reasons that have nothing to do with
    // iOS (research D5).
    let mut writer = index
        .writer_with_num_threads::<TantivyDocument>(1, WRITER_MEMORY_BUDGET)
        .map_err(|e| io_err(index_dir, &e))?;

    // Insertion order is part of the contract: it is what makes `DocId` assignment deterministic.
    for document in documents {
        writer
            .add_document(doc!(
                text_field => document.text.clone(),
                id_field => document.external_id.clone(),
            ))
            .map_err(|e| io_err(index_dir, &e))?;
    }
    writer.commit().map_err(|e| io_err(index_dir, &e))?;

    let reader = index.reader().map_err(|e| io_err(index_dir, &e))?;
    let searcher = reader.searcher();

    let documents_indexed = u32::try_from(documents.len()).map_err(|e| io_err(index_dir, &e))?;
    let segment_count =
        u32::try_from(searcher.segment_readers().len()).map_err(|e| io_err(index_dir, &e))?;

    Ok(IndexOutcome {
        documents_indexed,
        segment_count,
    })
}

/// Read a stored `external_id` back out of a retrieved document.
pub(crate) fn stored_external_id(
    document: &TantivyDocument,
    field: tantivy::schema::Field,
) -> Option<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
}
