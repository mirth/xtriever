//! `LexicalQuery` → backend query translation (research D9; spec FR-005, FR-017, FR-018).

use tantivy::Index;
use tantivy::Term;
use tantivy::query::{
    BooleanQuery, BoostQuery, EmptyQuery, FuzzyTermQuery, Occur, PhraseQuery, Query, TermQuery,
};
use tantivy::schema::{Field, IndexRecordOption};
use tantivy::tokenizer::TokenStream;
use xtriever_core::{FieldKind, FieldName, LexicalQuery, Result};

use crate::error::{invalid_query, map};
use crate::schema::{FieldMap, MappedField};

/// Largest Levenshtein distance the backend's automaton builder supports (fuzzy_query.rs:113-126).
const MAX_FUZZY_DISTANCE: u8 = 2;

/// Analyze `text` with the field's own tokenizer; returns `(position, term)` pairs.
pub(crate) fn analyze(index: &Index, field: Field, text: &str) -> Result<Vec<(usize, String)>> {
    let mut analyzer = index.tokenizer_for_field(field).map_err(map)?;
    let mut stream = analyzer.token_stream(text);
    let mut out = Vec::new();
    while stream.advance() {
        let t = stream.token();
        out.push((t.position, t.text.clone()));
    }
    Ok(out)
}

/// Look a field up and require it to be indexed (FR-018).
fn lookup<'a>(fields: &'a FieldMap, name: &FieldName) -> Result<&'a MappedField> {
    let mf = fields.get(name)?;
    if !mf.indexed {
        return Err(invalid_query(format!(
            "field `{name}` is not indexed and cannot be queried"
        )));
    }
    Ok(mf)
}

fn require_text<'a>(fields: &'a FieldMap, name: &FieldName, what: &str) -> Result<&'a MappedField> {
    let mf = lookup(fields, name)?;
    if !mf.is_text() {
        return Err(invalid_query(format!(
            "{what} requires a text field; `{name}` is {:?}",
            mf.kind
        )));
    }
    Ok(mf)
}

fn require_term_field<'a>(
    fields: &'a FieldMap,
    name: &FieldName,
    what: &str,
) -> Result<&'a MappedField> {
    let mf = lookup(fields, name)?;
    if !matches!(mf.kind, FieldKind::Text(_) | FieldKind::Keyword) {
        return Err(invalid_query(format!(
            "{what} requires a text or keyword field; `{name}` is {:?}",
            mf.kind
        )));
    }
    Ok(mf)
}

/// Multiply by the field boost iff it is not neutral (FR-005).
fn boosted(q: Box<dyn Query>, boost: f32) -> Box<dyn Query> {
    if boost == 1.0 {
        q
    } else {
        Box::new(BoostQuery::new(q, boost))
    }
}

fn term_query(field: Field, text: &str) -> Box<dyn Query> {
    Box::new(TermQuery::new(
        Term::from_field_text(field, text),
        IndexRecordOption::WithFreqs,
    ))
}

/// `Match` on one text field: analyzed terms OR-ed, i.e. `Should` clauses whose scores sum.
fn match_one(index: &Index, mf: &MappedField, text: &str) -> Result<Box<dyn Query>> {
    let terms = analyze(index, mf.field, text)?;
    if terms.is_empty() {
        return Ok(Box::new(EmptyQuery));
    }
    let clauses = terms
        .iter()
        .map(|(_, t)| (Occur::Should, term_query(mf.field, t)))
        .collect();
    Ok(boosted(Box::new(BooleanQuery::new(clauses)), mf.boost))
}

pub(crate) fn translate(
    q: &LexicalQuery,
    fields: &FieldMap,
    index: &Index,
) -> Result<Box<dyn Query>> {
    Ok(match q {
        LexicalQuery::Match(Some(name), text) => {
            let mf = require_text(fields, name, "Match")?;
            match_one(index, mf, text)?
        }
        LexicalQuery::Match(None, text) => {
            // Every indexed text field, each under its own boost, all OR-ed (FR-017 sum semantics).
            let mut clauses = Vec::new();
            for name in fields.text_fields() {
                let mf = fields.get(name)?;
                let sub = match_one(index, mf, text)?;
                clauses.push((Occur::Should, sub));
            }
            if clauses.is_empty() {
                Box::new(EmptyQuery)
            } else {
                Box::new(BooleanQuery::new(clauses))
            }
        }
        LexicalQuery::Phrase(name, text, slop) => {
            let mf = require_text(fields, name, "Phrase")?;
            let terms = analyze(index, mf.field, text)?;
            match terms.len() {
                0 => Box::new(EmptyQuery),
                // The backend's PhraseQuery constructors panic below two terms (D9).
                1 => boosted(term_query(mf.field, &terms[0].1), mf.boost),
                _ => {
                    let with_offsets = terms
                        .iter()
                        .map(|(pos, t)| (*pos, Term::from_field_text(mf.field, t)))
                        .collect();
                    boosted(
                        Box::new(PhraseQuery::new_with_offset_and_slop(with_offsets, *slop)),
                        mf.boost,
                    )
                }
            }
        }
        LexicalQuery::Term(name, text) => {
            let mf = require_term_field(fields, name, "Term")?;
            boosted(term_query(mf.field, text), mf.boost)
        }
        LexicalQuery::Fuzzy(name, text, distance) => {
            let mf = require_term_field(fields, name, "Fuzzy")?;
            if *distance > MAX_FUZZY_DISTANCE {
                return Err(invalid_query(format!(
                    "Fuzzy distance {distance} exceeds the maximum of {MAX_FUZZY_DISTANCE}"
                )));
            }
            // `false`: a transposition costs two edits — strict Levenshtein (spec FR-017).
            let fq = FuzzyTermQuery::new(Term::from_field_text(mf.field, text), *distance, false);
            boosted(Box::new(fq), mf.boost)
        }
        LexicalQuery::Bool {
            must,
            should,
            must_not,
        } => {
            // No clauses, or only `MustNot` clauses, match nothing — the backend's semantics.
            let mut clauses = Vec::with_capacity(must.len() + should.len() + must_not.len());
            for (occur, group) in [
                (Occur::Must, must),
                (Occur::Should, should),
                (Occur::MustNot, must_not),
            ] {
                for sub in group {
                    clauses.push((occur, translate(sub, fields, index)?));
                }
            }
            if clauses.is_empty() {
                Box::new(EmptyQuery)
            } else {
                Box::new(BooleanQuery::new(clauses))
            }
        }
        LexicalQuery::Boost(inner, factor) => {
            Box::new(BoostQuery::new(translate(inner, fields, index)?, *factor))
        }
    })
}
