//! Core schema → backend schema mapping, the analyzer table, and the on-disk descriptor
//! (research D4, D5, D6, D7, D13; data-model `FieldMap`, `MappedField`, `Descriptor`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tantivy::schema::{
    Field, INDEXED, IndexRecordOption, NumericOptions, STORED, STRING, SchemaBuilder,
    TextFieldIndexing, TextOptions,
};
use xtriever_core::{AnalyzerId, FieldKind, FieldName, Result, Schema};

use crate::error::{corrupt, schema_err, unknown_field};

/// Analyzer ids recognised in `FieldKind::Text(..)` (FR-006). Both map onto tokenizer chains the
/// backend registers by default, so the crate registers none of its own (research D5).
pub const ANALYZERS: &[&str] = &["standard", "standard_en"];

/// Reserved prefix for the crate's hidden backend fields (data-model "Hidden backend fields").
pub(crate) const RESERVED_PREFIX: &str = "__xt_";
/// Hidden u64 column holding the caller's `DocId` (identity, replace, delete, filtering).
pub(crate) const ID_FIELD: &str = "__xt_id";
/// Name of the on-disk descriptor (FR-012).
pub(crate) const DESCRIPTOR_FILE: &str = "xtriever-lexical.json";
/// The one on-disk format version this crate reads and writes.
pub(crate) const FORMAT_VERSION: u32 = 1;

fn tokenizer_name(id: &AnalyzerId) -> Option<&'static str> {
    match id.0.as_str() {
        "standard" => Some("default"), // SimpleTokenizer → RemoveLongFilter(40) → LowerCaser
        "standard_en" => Some("en_stem"), // …→ Stemmer(English)
        _ => None,
    }
}

pub(crate) fn len_field_name(name: &FieldName) -> String {
    format!("{RESERVED_PREFIX}len_{name}")
}

/// One user field, mapped.
#[derive(Debug, Clone)]
pub(crate) struct MappedField {
    pub field: Field,
    pub kind: FieldKind,
    pub indexed: bool,
    pub boost: f32,
    /// Hidden exact-token-count column; `Some` iff `kind` is `Text` and the field is indexed.
    pub len_field: Option<Field>,
}

impl MappedField {
    pub fn is_text(&self) -> bool {
        matches!(self.kind, FieldKind::Text(_))
    }
}

/// Immutable mapping from the core schema to backend fields, built once at create/open.
#[derive(Debug, Clone)]
pub(crate) struct FieldMap {
    by_name: BTreeMap<FieldName, MappedField>,
    id_field: Field,
    /// Indexed `Text` fields in schema order — the iteration order for `Match(None, …)`.
    text_fields: Vec<FieldName>,
    backend: tantivy::schema::Schema,
}

impl FieldMap {
    /// Build the backend schema, validating per data-model "Validation at construction".
    pub fn build(schema: &Schema) -> Result<Self> {
        let mut builder = SchemaBuilder::default();
        let mut by_name = BTreeMap::new();
        let mut text_fields = Vec::new();

        for def in &schema.fields {
            let name = &def.name;
            if by_name.contains_key(name) {
                return Err(schema_err(format!("field `{name}`: declared twice")));
            }
            if name.0.starts_with(RESERVED_PREFIX) {
                return Err(schema_err(format!(
                    "field `{name}`: names starting with `{RESERVED_PREFIX}` are reserved"
                )));
            }
            let is_text = matches!(def.kind, FieldKind::Text(_));
            if def.boost != 1.0 && !is_text {
                return Err(schema_err(format!(
                    "field `{name}`: boost {} is only meaningful on text fields (kind is {:?})",
                    def.boost, def.kind
                )));
            }
            let mut len_field = None;
            let field = match &def.kind {
                FieldKind::Text(analyzer) => {
                    let tok = tokenizer_name(analyzer).ok_or_else(|| {
                        schema_err(format!(
                            "field `{name}`: unknown analyzer `{}`; known: {}",
                            analyzer.0,
                            ANALYZERS.join(", ")
                        ))
                    })?;
                    let mut opts = TextOptions::default();
                    if def.indexed {
                        opts = opts.set_indexing_options(
                            TextFieldIndexing::default()
                                .set_tokenizer(tok)
                                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
                        );
                        // Exact per-document token count for live-only statistics (D7); doubles as
                        // the existence marker for `Exists` on text fields (D10).
                        len_field = Some(
                            builder.add_u64_field(&len_field_name(name), tantivy::schema::FAST),
                        );
                        text_fields.push(name.clone());
                    }
                    if def.stored {
                        opts = opts.set_stored();
                    }
                    builder.add_text_field(&name.0, opts)
                }
                FieldKind::Keyword => {
                    let mut opts = if def.indexed {
                        STRING | tantivy::schema::FAST
                    } else {
                        TextOptions::default()
                    };
                    if def.stored {
                        opts = opts.set_stored();
                    }
                    builder.add_text_field(&name.0, opts)
                }
                FieldKind::U64 => builder.add_u64_field(&name.0, numeric(def.indexed, def.stored)),
                // DateMillis is stored as i64: the backend's date type truncates indexed values to
                // seconds (research D6), which would silently break millisecond equality.
                FieldKind::I64 | FieldKind::DateMillis => {
                    builder.add_i64_field(&name.0, numeric(def.indexed, def.stored))
                }
                FieldKind::F64 => builder.add_f64_field(&name.0, numeric(def.indexed, def.stored)),
                FieldKind::Bool => {
                    builder.add_bool_field(&name.0, numeric(def.indexed, def.stored))
                }
            };
            by_name.insert(
                name.clone(),
                MappedField {
                    field,
                    kind: def.kind.clone(),
                    indexed: def.indexed,
                    boost: def.boost,
                    len_field,
                },
            );
        }
        let id_field = builder.add_u64_field(ID_FIELD, INDEXED | tantivy::schema::FAST);
        Ok(Self {
            by_name,
            id_field,
            text_fields,
            backend: builder.build(),
        })
    }

    pub fn get(&self, name: &FieldName) -> Result<&MappedField> {
        self.by_name.get(name).ok_or_else(|| unknown_field(name))
    }

    pub fn id_field(&self) -> Field {
        self.id_field
    }

    pub fn text_fields(&self) -> &[FieldName] {
        &self.text_fields
    }

    pub fn backend(&self) -> &tantivy::schema::Schema {
        &self.backend
    }
}

fn numeric(indexed: bool, stored: bool) -> NumericOptions {
    let mut opts = NumericOptions::default();
    if indexed {
        opts = opts | INDEXED | tantivy::schema::FAST;
    }
    if stored {
        opts = opts | STORED;
    }
    opts
}

/// What an index directory records about itself (FR-012). The schema carries the analyzer ids,
/// so an analyzer change is a schema mismatch — the lexical analogue of the embedder fingerprint.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Descriptor {
    pub format_version: u32,
    pub schema: Schema,
}

impl Descriptor {
    pub fn new(schema: Schema) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            schema,
        }
    }

    /// Write, sync, rename, sync the directory (`xtriever_core::fs`) so a crash never leaves a
    /// half-written descriptor and a power loss keeps the renamed one.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let tmp = dir.join(format!("{DESCRIPTOR_FILE}.tmp"));
        let body = serde_json::to_vec_pretty(self)
            .map_err(|e| corrupt(format!("descriptor encode: {e}")))?;
        xtriever_core::fs::write_atomically(&dir.join(DESCRIPTOR_FILE), &tmp, &body)?;
        xtriever_core::fs::sync_dir(dir)?;
        Ok(())
    }

    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(DESCRIPTOR_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(corrupt(format!(
                    "not an xtriever-lexical index: missing {}",
                    path.display()
                )));
            }
            Err(e) => return Err(e.into()),
        };
        let d: Descriptor = serde_json::from_str(&text)
            .map_err(|e| corrupt(format!("descriptor {}: {e}", path.display())))?;
        if d.format_version != FORMAT_VERSION {
            return Err(corrupt(format!(
                "index format version {} is not supported (this crate reads version {FORMAT_VERSION})",
                d.format_version
            )));
        }
        Ok(d)
    }
}
