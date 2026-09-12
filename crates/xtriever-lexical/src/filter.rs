//! `Filter` → `DocSet` evaluation (research D10; spec FR-019–FR-023).
//!
//! Leaves are answered by the backend (`TermQuery`, `TermSetQuery`, `RangeQuery`, `ExistsQuery`)
//! collected into a `DocSet`; combinators are `roaring` set operations on `xtriever_core::DocSet`.
//! Every leaf runs through the `Searcher`, so deleted documents never appear (FR-020).

use std::ops::Bound;

use tantivy::collector::DocSetCollector;
use tantivy::query::{ExistsQuery, Query, RangeQuery, TermQuery, TermSetQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{DocAddress, Searcher, Term};
use xtriever_core::{DocSet, FieldKind, FieldName, Filter, Result, Value};

use crate::error::{invalid_query, map};
use crate::schema::{FieldMap, ID_FIELD, MappedField, len_field_name};
use crate::search::doc_ids;

fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Text(_) => "Text",
        Value::Keyword(_) => "Keyword",
        Value::U64(_) => "U64",
        Value::I64(_) => "I64",
        Value::F64(_) => "F64",
        Value::Bool(_) => "Bool",
        Value::DateMillis(_) => "DateMillis",
    }
}

/// Look up a filterable field: it must exist, be indexed, and (for everything but `Exists`) be a
/// metadata kind — `Eq`/`In`/`Range` on analyzed text is an invalid query (D10).
fn lookup<'a>(fields: &'a FieldMap, name: &FieldName, what: &str) -> Result<&'a MappedField> {
    let mf = fields.get(name)?;
    if !mf.indexed {
        return Err(invalid_query(format!(
            "field `{name}` is not indexed and cannot be filtered"
        )));
    }
    if what != "Exists" && mf.is_text() {
        return Err(invalid_query(format!(
            "{what} is not defined on analyzed text field `{name}`; only Exists is"
        )));
    }
    Ok(mf)
}

/// Build the backend term for a value, checking the value's variant against the field's kind.
fn term(mf: &MappedField, name: &FieldName, value: &Value) -> Result<Term> {
    Ok(match (&mf.kind, value) {
        (FieldKind::Keyword, Value::Keyword(s)) => Term::from_field_text(mf.field, s),
        (FieldKind::U64, Value::U64(v)) => Term::from_field_u64(mf.field, *v),
        (FieldKind::I64, Value::I64(v)) | (FieldKind::DateMillis, Value::DateMillis(v)) => {
            Term::from_field_i64(mf.field, *v)
        }
        (FieldKind::F64, Value::F64(v)) => Term::from_field_f64(mf.field, *v),
        (FieldKind::Bool, Value::Bool(v)) => Term::from_field_bool(mf.field, *v),
        _ => {
            return Err(invalid_query(format!(
                "field `{name}` is {:?}; filter value is {}",
                mf.kind,
                kind_name(value)
            )));
        }
    })
}

fn collect(searcher: &Searcher, q: &dyn Query) -> Result<DocSet> {
    let addrs = searcher.search(q, &DocSetCollector).map_err(map)?;
    let mut addrs: Vec<DocAddress> = addrs.into_iter().collect();
    addrs.sort_unstable();
    Ok(doc_ids(searcher, &addrs)?.into_iter().collect())
}

fn exists(searcher: &Searcher, name: &str) -> Result<DocSet> {
    collect(searcher, &ExistsQuery::new(name.to_owned(), false))
}

/// All live documents: every document carries the id column.
fn alive(searcher: &Searcher) -> Result<DocSet> {
    exists(searcher, ID_FIELD)
}

pub(crate) fn resolve(filter: &Filter, fields: &FieldMap, searcher: &Searcher) -> Result<DocSet> {
    match filter {
        Filter::Eq(name, value) => {
            let mf = lookup(fields, name, "Eq")?;
            collect(
                searcher,
                &TermQuery::new(term(mf, name, value)?, IndexRecordOption::Basic),
            )
        }
        Filter::In(name, values) => {
            let mf = lookup(fields, name, "In")?;
            let terms = values
                .iter()
                .map(|v| term(mf, name, v))
                .collect::<Result<Vec<_>>>()?;
            if terms.is_empty() {
                return Ok(DocSet::new());
            }
            collect(searcher, &TermSetQuery::new(terms))
        }
        Filter::Range(name, None, None) => {
            // `RangeQuery` panics with no bound set; a fully open range is existence (D10).
            let mf = lookup(fields, name, "Range")?;
            exists(searcher, &backend_name(mf, name))
        }
        Filter::Range(name, lo, hi) => {
            let mf = lookup(fields, name, "Range")?;
            if mf.kind == FieldKind::Bool {
                // The backend's fast-field range weight accepts u64/i64/f64/date terms only.
                return Err(invalid_query(format!(
                    "Range is not defined on bool field `{name}`; use Eq"
                )));
            }
            let bound = |v: &Option<Value>| -> Result<Bound<Term>> {
                Ok(match v {
                    Some(v) => Bound::Included(term(mf, name, v)?),
                    None => Bound::Unbounded,
                })
            };
            collect(searcher, &RangeQuery::new(bound(lo)?, bound(hi)?))
        }
        Filter::Exists(name) => {
            let mf = lookup(fields, name, "Exists")?;
            exists(searcher, &backend_name(mf, name))
        }
        Filter::And(subs) => {
            let mut acc = alive(searcher)?;
            for sub in subs {
                acc = acc.intersection(&resolve(sub, fields, searcher)?);
            }
            Ok(acc)
        }
        Filter::Or(subs) => {
            let mut acc = DocSet::new();
            for sub in subs {
                acc = acc.union(&resolve(sub, fields, searcher)?);
            }
            Ok(acc)
        }
        Filter::Not(sub) => Ok(alive(searcher)?.difference(&resolve(sub, fields, searcher)?)),
        Filter::Ids(ids) => {
            let alive = alive(searcher)?;
            Ok(ids
                .iter()
                .copied()
                .filter(|id| alive.contains(*id))
                .collect())
        }
    }
}

/// The backend column that carries existence for a field: text fields have no fast column of
/// their own, so their hidden length column stands in (D7, D10).
fn backend_name<'a>(mf: &MappedField, name: &'a FieldName) -> std::borrow::Cow<'a, str> {
    if mf.len_field.is_some() {
        len_field_name(name).into()
    } else {
        name.0.as_str().into()
    }
}
