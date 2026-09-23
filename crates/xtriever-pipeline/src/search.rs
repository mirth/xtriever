//! Search: filter once, lexical, dense, fuse, re-rank — with degradation, budgets and
//! explanation (data-model "Search algorithm"; 005 research D5–D8; 006 research D8).

use std::collections::HashMap;
use std::time::Duration;

use xtriever_core::{
    Budget, DocId, DocSet, Error, Filter, Hit, LexicalIndex, LexicalQuery, Passage, Result,
    TextKind, VectorIndex,
};

use crate::fusion::fused_terms;
use crate::index::HybridIndex;
use crate::rerank::{RerankMode, order_head};
use crate::{
    Degradation, DegradeReason, HitExplain, HybridHit, RerankReport, Response, SearchOptions,
    StageReport,
};

/// Why an ML stage did not contribute — the internal form of [`DegradeReason`].
enum Skip {
    Error(Error),
    Budget { elapsed_ms: u64, limit_ms: u64 },
}

impl Skip {
    fn reason(self) -> DegradeReason {
        match self {
            Self::Error(e) => DegradeReason::StageError(e.to_string()),
            Self::Budget {
                elapsed_ms,
                limit_ms,
            } => DegradeReason::BudgetExceeded {
                elapsed_ms,
                limit_ms,
            },
        }
    }
}

/// Step 9's output: the ordered candidates (cut at `k`) with their re-rank scores, and the
/// stage report (`None` when the stage did not run).
/// Ordered candidates with their re-rank score and, under `Interpolate`, the combined score.
type Reranked = (
    Vec<(Candidate, Option<f32>, Option<f64>)>,
    Option<RerankReport>,
);

/// A fused candidate before hits are built: its id, fused score, explanation, and its passage
/// text once the re-rank step has read it (so a hit is never read twice).
struct Candidate {
    id: DocId,
    score: f64,
    explain: Option<HitExplain>,
    text: Option<String>,
}

impl HybridIndex {
    /// Search with a free-text query: `LexicalQuery::Match` over every text field for the lexical
    /// stage — on a sparse index, plus the query's expansion terms against `_sparse` (Feature
    /// 027) — and the embedded text for the dense stage.
    ///
    /// On a sparse index the expansion degrades like an ML stage (Principle VI): if the query
    /// side cannot tokenise the query, the text fields are searched alone and
    /// [`StageReport::sparse_skipped`](crate::StageReport::sparse_skipped) says why.
    ///
    /// # Errors
    ///
    /// As [`search_lexical`](Self::search_lexical); in strict mode, also the query side's
    /// `Error::Model` when it cannot tokenise the query.
    pub fn search(
        &self,
        query: &str,
        filter: Option<&Filter>,
        k: usize,
        options: &SearchOptions<'_>,
    ) -> Result<Response> {
        let (lexical, sparse_skipped) = lexical_query(
            &self.config.schema,
            self.sparse_query.as_ref(),
            query,
            options.strict,
        )?;
        self.search_with(&lexical, query, filter, k, options, sparse_skipped)
    }

    /// Search with a caller-built lexical query; the dense stage embeds `dense_text`. On a
    /// sparse index no expansion is added — a caller who wants it writes its `Term(_sparse, …)`
    /// clauses — and every `Match(None, …)` in the query means the caller's own text fields,
    /// never the reserved `_sparse` field. `explain`'s lexical score is the whole lexical
    /// score, any `_sparse` clauses included.
    ///
    /// # Errors
    ///
    /// Filter or lexical-stage errors in every mode; dense-stage errors and `BudgetExhausted`
    /// only in strict mode; `Corrupt` if a stage returns an id the id map does not know.
    pub fn search_lexical(
        &self,
        query: &LexicalQuery,
        dense_text: &str,
        filter: Option<&Filter>,
        k: usize,
        options: &SearchOptions<'_>,
    ) -> Result<Response> {
        match &self.sparse_query {
            Some(_) => self.search_with(
                &own_fields(&self.config.schema, query),
                dense_text,
                filter,
                k,
                options,
                None,
            ),
            None => self.search_with(query, dense_text, filter, k, options, None),
        }
    }

    fn search_with(
        &self,
        query: &LexicalQuery,
        dense_text: &str,
        filter: Option<&Filter>,
        k: usize,
        opts: &SearchOptions<'_>,
        sparse_skipped: Option<DegradeReason>,
    ) -> Result<Response> {
        let time_limit_ignored = opts.budget.max_time.is_some() && opts.elapsed.is_none();
        // Neither stage runs on the two short-circuits: `dense_candidates` is `None` because
        // the stage did not run, and `degraded` is `None` because nothing was skipped for cause.
        let empty = || Response {
            hits: Vec::new(),
            stages: StageReport {
                lexical_candidates: 0,
                dense_candidates: None,
                degraded: None,
                rerank: None,
                time_limit_ignored,
                sparse_skipped: sparse_skipped.clone(),
            },
        };
        if k == 0 {
            return Ok(empty());
        }
        // 2. Filter, resolved once; an empty set short-circuits both stages.
        let allowed: Option<DocSet> = match filter {
            Some(f) => Some(self.lexical.resolve_filter(f)?),
            None => None,
        };
        if allowed.as_ref().is_some_and(DocSet::is_empty) {
            return Ok(empty());
        }
        let depth = opts.depth.unwrap_or(self.config.candidate_depth);

        // 3. Lexical (errors are errors in every mode).
        let lexical_filter = allowed
            .as_ref()
            .map(|set| Filter::Ids(set.iter().collect()));
        let lexical = self.lexical.search(query, lexical_filter.as_ref(), depth)?;

        // 4–6. Dense, guarded by the budget at check points A and B.
        let dense: std::result::Result<Vec<Hit>, Skip> = self.dense_candidates(
            dense_text,
            allowed.as_ref(),
            depth.min(opts.budget.max_items.unwrap_or(usize::MAX)),
            opts,
        );
        let (dense, skip) = match dense {
            Ok(hits) => (Some(hits), None),
            Err(Skip::Error(e)) if opts.strict => return Err(e),
            Err(Skip::Budget {
                elapsed_ms,
                limit_ms,
            }) if opts.strict => {
                return Err(Error::BudgetExhausted(format!(
                    "dense stage: {elapsed_ms} ms elapsed > {limit_ms} ms limit"
                )));
            }
            Err(Skip::Error(e)) => (None, Some(DegradeReason::StageError(e.to_string()))),
            Err(Skip::Budget {
                elapsed_ms,
                limit_ms,
            }) => (
                None,
                Some(DegradeReason::BudgetExceeded {
                    elapsed_ms,
                    limit_ms,
                }),
            ),
        };

        // 7–8. Fuse, or degrade to the lexical list — built to `max(k, d)` so the re-ranker
        // can promote a candidate from just below `k` (006 research D8).
        let rerank_depth = if self.reranker.is_some() {
            opts.rerank_depth.unwrap_or(self.config.rerank_depth)
        } else {
            0
        };
        // Feature 015: the order rule for the head — the caller's override or the recorded one.
        let rerank_mode = match opts.rerank_mode {
            Some(mode) => {
                mode.validate()?;
                mode
            }
            None => self.config.rerank_mode,
        };
        let list_len = k.max(rerank_depth);
        let candidates = match &dense {
            Some(dense) => self.fuse(&lexical, dense, list_len, opts.explain),
            None => self.degraded(&lexical, list_len, opts.explain),
        };

        // 9. Re-rank the first `d` candidates under the remaining budget.
        let (ordered, rerank) =
            self.rerank(candidates, dense_text, rerank_depth, rerank_mode, k, opts)?;

        let stages = StageReport {
            lexical_candidates: lexical.len(),
            dense_candidates: dense.as_ref().map(Vec::len),
            degraded: skip.map(|reason| Degradation {
                stage: "dense",
                reason,
            }),
            rerank,
            time_limit_ignored,
            sparse_skipped,
        };
        let mut hits = Vec::with_capacity(ordered.len());
        let mut rerank_rank = 0u32;
        for (c, rerank_score, combined) in ordered {
            let explain = c.explain.map(|mut e| {
                if rerank_score.is_some() {
                    rerank_rank += 1;
                    e.rerank_score = rerank_score;
                    e.rerank_rank = Some(rerank_rank);
                    e.rerank_combined = combined;
                }
                e
            });
            let text = match c.text {
                Some(text) => text,
                None => self.passages.read(c.id)?,
            };
            hits.push(HybridHit {
                external_id: self.external_of(c.id)?.to_owned(),
                id: c.id,
                score: c.score,
                rerank_score,
                text,
                chunk: self.committed_ids.chunk(c.id),
                explain,
            });
        }
        Ok(Response { hits, stages })
    }

    /// Step 9: check point C, passages from the store, the remaining budget, validation, the
    /// ordering rule. Returns the ordered candidates (cut at `k`) with their re-rank scores.
    fn rerank(
        &self,
        mut candidates: Vec<Candidate>,
        query: &str,
        depth: usize,
        mode: RerankMode,
        k: usize,
        opts: &SearchOptions<'_>,
    ) -> Result<Reranked> {
        let plain = |candidates: Vec<Candidate>, report| {
            let ordered = candidates
                .into_iter()
                .take(k)
                .map(|c| (c, None, None))
                .collect();
            Ok((ordered, report))
        };
        let (Some(reranker), true) = (self.reranker.as_ref(), depth > 0 && !candidates.is_empty())
        else {
            return plain(candidates, None);
        };
        let skipped = |reason: DegradeReason| RerankReport {
            candidates: 0,
            scored: 0,
            skipped: Some(reason),
        };
        // Check point C: before spending anything on the re-rank stage.
        match check_budget(opts) {
            Ok(()) => {}
            Err(Skip::Budget {
                elapsed_ms,
                limit_ms,
            }) if opts.strict => {
                return Err(Error::BudgetExhausted(format!(
                    "rerank stage: {elapsed_ms} ms elapsed > {limit_ms} ms limit"
                )));
            }
            Err(skip) => return plain(candidates, Some(skipped(skip.reason()))),
        }
        // Read the texts once; they stay on the candidates for the hits (review round 1 #1).
        let n = depth.min(candidates.len());
        for c in &mut candidates[..n] {
            c.text = Some(self.passages.read(c.id)?);
        }
        let passages: Vec<Passage<'_>> = candidates[..n]
            .iter()
            .map(|c| Passage {
                id: c.id,
                text: c.text.as_deref().unwrap_or_default(),
            })
            .collect();
        // The re-ranker measures its own time; it receives what is left of the caller's limit.
        let remaining = match (opts.budget.max_time, opts.elapsed) {
            (Some(limit), Some(elapsed)) => Some(limit.saturating_sub(elapsed())),
            _ => None,
        };
        let budget = Budget {
            max_time: remaining,
            max_items: opts.budget.max_items,
        };
        let scores = match reranker.rerank(query, &passages, &budget) {
            Ok(scores) => scores,
            Err(e) if opts.strict => return Err(e),
            Err(e) => {
                return plain(
                    candidates,
                    Some(skipped(DegradeReason::StageError(e.to_string()))),
                );
            }
        };
        // A wrong-length or non-finite result is a defect, not a stage failure (FR-014).
        if scores.len() != passages.len() {
            return Err(Error::Model {
                model: reranker.model_id().to_owned(),
                message: format!(
                    "returned {} scores for {} passages",
                    scores.len(),
                    passages.len()
                ),
            });
        }
        if let Some(bad) = scores.iter().flatten().find(|s| !s.is_finite()) {
            return Err(Error::Model {
                model: reranker.model_id().to_owned(),
                message: format!("returned a non-finite score {bad}"),
            });
        }
        let report = RerankReport {
            candidates: passages.len(),
            scored: scores.iter().flatten().count(),
            skipped: None,
        };
        let fused: Vec<(DocId, f64)> = candidates.iter().map(|c| (c.id, c.score)).collect();
        let order = order_head(mode, &fused, &scores, k);
        let mut by_id: HashMap<u32, Candidate> =
            candidates.into_iter().map(|c| (c.id.0, c)).collect();
        let ordered = order
            .into_iter()
            .filter_map(|(id, _, s, combined)| by_id.remove(&id.0).map(|c| (c, s, combined)))
            .collect();
        Ok((ordered, Some(report)))
    }

    fn dense_candidates(
        &self,
        text: &str,
        allowed: Option<&DocSet>,
        depth: usize,
        opts: &SearchOptions<'_>,
    ) -> std::result::Result<Vec<Hit>, Skip> {
        // Check point A: before spending anything on the dense stage.
        check_budget(opts)?;
        let mut vectors = self
            .embedder
            .embed(&[text], TextKind::Query)
            .map_err(Skip::Error)?;
        let vector = vectors.pop().ok_or_else(|| {
            Skip::Error(Error::Model {
                model: "embedder".into(),
                message: "returned no vector for the query".into(),
            })
        })?;
        let hits = self
            .dense
            .search(&vector, allowed, depth)
            .map_err(Skip::Error)?;
        // Check point B: a budget is a promise about the result, not the attempt.
        check_budget(opts)?;
        Ok(hits)
    }

    fn fuse(&self, lexical: &[Hit], dense: &[Hit], len: usize, explain: bool) -> Vec<Candidate> {
        let lex_pos: HashMap<u32, (u32, f32)> = lexical
            .iter()
            .enumerate()
            .map(|(i, h)| (h.id.0, (i as u32 + 1, h.score)))
            .collect();
        let den_pos: HashMap<u32, (u32, f32)> = dense
            .iter()
            .enumerate()
            .map(|(i, h)| (h.id.0, (i as u32 + 1, h.score)))
            .collect();
        fused_terms(lexical, dense, self.config.rrf_k)
            .into_iter()
            .take(len)
            .map(|(id, lex, den)| {
                let score = lex + den;
                let explain = explain.then(|| {
                    let l = lex_pos.get(&id.0);
                    let d = den_pos.get(&id.0);
                    HitExplain {
                        bm25_score: l.map(|x| x.1),
                        bm25_rank: l.map(|x| x.0),
                        dense_score: d.map(|x| x.1),
                        dense_rank: d.map(|x| x.0),
                        fused: score,
                        rerank_score: None,
                        rerank_rank: None,
                        rerank_combined: None,
                    }
                });
                Candidate {
                    id,
                    score,
                    explain,
                    text: None,
                }
            })
            .collect()
    }

    fn degraded(&self, lexical: &[Hit], len: usize, explain: bool) -> Vec<Candidate> {
        lexical
            .iter()
            .take(len)
            .enumerate()
            .map(|(i, h)| {
                let score = f64::from(h.score);
                Candidate {
                    id: h.id,
                    score,
                    explain: explain.then(|| HitExplain {
                        bm25_score: Some(h.score),
                        bm25_rank: Some(i as u32 + 1),
                        dense_score: None,
                        dense_rank: None,
                        fused: score,
                        rerank_score: None,
                        rerank_rank: None,
                        rerank_combined: None,
                    }),
                    text: None,
                }
            })
            .collect()
    }
}

/// `Match(Some(f), text)` for each of the user's indexed text fields, in schema order.
fn field_matches(schema: &xtriever_core::Schema, text: &str) -> Vec<LexicalQuery> {
    schema
        .fields
        .iter()
        .filter(|f| f.indexed && matches!(f.kind, xtriever_core::FieldKind::Text(_)))
        .map(|f| LexicalQuery::Match(Some(f.name.clone()), text.to_owned()))
        .collect()
}

/// A sparse index's free-text query (Feature 027 research D7): the text over the user's text
/// fields, spelled out — `Match(None, …)` would also run it through `_sparse`'s analyzer — and
/// one `Term(_sparse, "s<id>")` per kept query token, ascending; the field's boost applies to
/// those terms. With no terms it is the text fields alone.
fn sparse_query(schema: &xtriever_core::Schema, text: &str, terms: &[u32]) -> LexicalQuery {
    let field = xtriever_core::FieldName::from(crate::SPARSE_FIELD);
    let mut should = field_matches(schema, text);
    should.extend(
        terms
            .iter()
            .map(|id| LexicalQuery::Term(field.clone(), format!("s{id}"))),
    );
    LexicalQuery::Bool {
        must: vec![],
        should,
        must_not: vec![],
    }
}

/// The query `search` sends for free text. Without a sparse side, `Match(None, text)` exactly
/// as before (FR-002). With one, [`sparse_query`] — and when the query side cannot tokenise the
/// text the expansion degrades rather than failing the search: no terms, and the reason; in
/// strict mode, the error.
pub(crate) fn lexical_query(
    schema: &xtriever_core::Schema,
    sparse: Option<&xtriever_dense::sparse::SparseQuery>,
    text: &str,
    strict: bool,
) -> Result<(LexicalQuery, Option<DegradeReason>)> {
    let Some(sparse) = sparse else {
        return Ok((LexicalQuery::Match(None, text.to_owned()), None));
    };
    match sparse.terms(text) {
        Ok(terms) => Ok((sparse_query(schema, text, &terms), None)),
        Err(e) if strict => Err(e),
        Err(e) => Ok((
            sparse_query(schema, text, &[]),
            Some(DegradeReason::StageError(e.to_string())),
        )),
    }
}

/// A caller's query on a sparse index: every `Match(None, text)` in it — "all the text fields"
/// — becomes the caller's own text fields spelled out, so the reserved `_sparse` field, which
/// the caller never declared, is never searched with analysed text. Everything else is kept as
/// built; the scores of the spelled-out fields are those `Match(None, …)` gives on an index
/// without the option.
pub(crate) fn own_fields(schema: &xtriever_core::Schema, query: &LexicalQuery) -> LexicalQuery {
    match query {
        LexicalQuery::Match(None, text) => sparse_query(schema, text, &[]),
        LexicalQuery::Bool {
            must,
            should,
            must_not,
        } => {
            let each = |qs: &[LexicalQuery]| qs.iter().map(|q| own_fields(schema, q)).collect();
            LexicalQuery::Bool {
                must: each(must),
                should: each(should),
                must_not: each(must_not),
            }
        }
        LexicalQuery::Boost(inner, factor) => {
            LexicalQuery::Boost(Box::new(own_fields(schema, inner)), *factor)
        }
        other => other.clone(),
    }
}

/// The time check at a check point: only when both a limit and a time source exist.
fn check_budget(opts: &SearchOptions<'_>) -> std::result::Result<(), Skip> {
    if let (Some(limit), Some(elapsed)) = (opts.budget.max_time, opts.elapsed) {
        let now: Duration = elapsed();
        if now > limit {
            return Err(Skip::Budget {
                elapsed_ms: now.as_millis().try_into().unwrap_or(u64::MAX),
                limit_ms: limit.as_millis().try_into().unwrap_or(u64::MAX),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use sha2::{Digest, Sha256};
    use xtriever_core::{AnalyzerId, FieldDef, FieldKind, FieldName, LexicalQuery, Schema};
    use xtriever_dense::sparse::SparseQuery;

    use super::{lexical_query, own_fields};
    use crate::DegradeReason;

    fn field(name: &str, kind: FieldKind, indexed: bool) -> FieldDef {
        FieldDef {
            name: FieldName::from(name),
            kind,
            indexed,
            stored: false,
            boost: 1.0,
        }
    }

    /// Two indexed text fields, a keyword and an unindexed text field, in that order.
    fn schema() -> Schema {
        let text = || FieldKind::Text(AnalyzerId("standard".into()));
        Schema {
            fields: vec![
                field("title", text(), true),
                field("tag", FieldKind::Keyword, true),
                field("body", text(), true),
                field("note", text(), false),
            ],
        }
    }

    /// A word-level tokenizer with BERT's special tokens; `the` has no positive table entry.
    const TOKENIZER: &str = r#"{"version": "1.0", "truncation": null, "padding": null,
      "added_tokens": [
        {"id": 0, "content": "[PAD]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true},
        {"id": 1, "content": "[UNK]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true},
        {"id": 2, "content": "[CLS]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true},
        {"id": 3, "content": "[SEP]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true},
        {"id": 4, "content": "[MASK]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true}],
      "normalizer": {"type": "Lowercase"}, "pre_tokenizer": {"type": "Whitespace"},
      "post_processor": null, "decoder": null,
      "model": {"type": "WordLevel", "unk_token": "[UNK]",
        "vocab": {"[PAD]": 0, "[UNK]": 1, "[CLS]": 2, "[SEP]": 3, "[MASK]": 4, "the": 5, "cat": 6, "dog": 7}}}"#;
    const TABLE: &str = r#"{"[PAD]": 1, "[UNK]": 1, "[CLS]": 1, "[SEP]": 1, "[MASK]": 1,
      "the": 0.0, "cat": 2.0, "dog": 1.0}"#;

    fn sha256(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn side(dir: &std::path::Path) -> SparseQuery {
        std::fs::write(dir.join("tokenizer.json"), TOKENIZER).unwrap();
        std::fs::write(dir.join("query-table.json"), TABLE).unwrap();
        SparseQuery::open(
            &dir.join("tokenizer.json"),
            &dir.join("query-table.json"),
            &sha256(TOKENIZER.as_bytes()),
            &sha256(TABLE.as_bytes()),
        )
        .unwrap()
    }

    /// A tokenizer that fails on any word it does not know: its unknown token is not in its
    /// vocabulary.
    const FAILING_TOKENIZER: &str = r#"{"version": "1.0", "truncation": null, "padding": null,
      "added_tokens": [], "normalizer": null, "pre_tokenizer": {"type": "Whitespace"},
      "post_processor": null, "decoder": null,
      "model": {"type": "WordLevel", "unk_token": "[UNK]", "vocab": {"cat": 0, "dog": 1}}}"#;
    const FAILING_TABLE: &str = r#"{"cat": 2.0, "dog": 1.0}"#;

    /// Review round 5 (Principle VI): a query the query side cannot tokenise degrades to the
    /// text fields alone, with the reason; strict mode returns the error.
    #[test]
    fn a_query_the_query_side_cannot_tokenise_degrades_to_the_text_fields() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("t.json"), FAILING_TOKENIZER).unwrap();
        std::fs::write(tmp.path().join("q.json"), FAILING_TABLE).unwrap();
        let sparse = SparseQuery::open(
            &tmp.path().join("t.json"),
            &tmp.path().join("q.json"),
            &sha256(FAILING_TOKENIZER.as_bytes()),
            &sha256(FAILING_TABLE.as_bytes()),
        )
        .unwrap();

        let (query, skipped) = lexical_query(&schema(), Some(&sparse), "cat zebra", false).unwrap();
        assert_eq!(
            query,
            LexicalQuery::Bool {
                must: vec![],
                should: vec![
                    LexicalQuery::Match(Some(FieldName::from("title")), "cat zebra".to_owned()),
                    LexicalQuery::Match(Some(FieldName::from("body")), "cat zebra".to_owned()),
                ],
                must_not: vec![],
            }
        );
        assert!(matches!(skipped, Some(DegradeReason::StageError(_))));
        assert!(lexical_query(&schema(), Some(&sparse), "cat zebra", true).is_err());
        // A query it can tokenise is not degraded.
        let (_, skipped) = lexical_query(&schema(), Some(&sparse), "cat dog", false).unwrap();
        assert_eq!(skipped, None);
    }

    /// Without the option the query is exactly what it was before Feature 027 (FR-002).
    #[test]
    fn without_the_option_the_query_is_match_over_every_field() {
        assert_eq!(
            lexical_query(&schema(), None, "dog cat", false).unwrap(),
            (LexicalQuery::Match(None, "dog cat".to_owned()), None)
        );
    }

    /// Research D7: the user's indexed text fields spelled out in schema order — so the query
    /// text never reaches `_sparse` through its analyzer — then one `Term` per kept token,
    /// ascending.
    #[test]
    fn with_the_option_the_query_adds_one_term_per_kept_token() {
        let tmp = tempfile::tempdir().unwrap();
        let sparse = side(tmp.path());
        let matches = |text: &str| {
            vec![
                LexicalQuery::Match(Some(FieldName::from("title")), text.to_owned()),
                LexicalQuery::Match(Some(FieldName::from("body")), text.to_owned()),
            ]
        };
        let term = |id: u32| LexicalQuery::Term(FieldName::from("_sparse"), format!("s{id}"));

        let mut should = matches("dog the cat dog");
        should.extend([term(6), term(7)]);
        assert_eq!(
            lexical_query(&schema(), Some(&sparse), "dog the cat dog", false).unwrap(),
            (
                LexicalQuery::Bool {
                    must: vec![],
                    should,
                    must_not: vec![],
                },
                None
            )
        );
        // No kept token: the fields alone, still spelled out.
        assert_eq!(
            lexical_query(&schema(), Some(&sparse), "the", false).unwrap(),
            (
                LexicalQuery::Bool {
                    must: vec![],
                    should: matches("the"),
                    must_not: vec![],
                },
                None
            )
        );
    }

    /// Review round 6: on a sparse index a caller's `Match(None, …)` means the caller's text
    /// fields, wherever it sits in the query; everything else is kept as built.
    #[test]
    fn a_callers_match_over_every_field_means_the_callers_fields() {
        let fields = |text: &str| LexicalQuery::Bool {
            must: vec![],
            should: vec![
                LexicalQuery::Match(Some(FieldName::from("title")), text.to_owned()),
                LexicalQuery::Match(Some(FieldName::from("body")), text.to_owned()),
            ],
            must_not: vec![],
        };
        let kept = LexicalQuery::Phrase(FieldName::from("body"), "cat dog".to_owned(), 1);
        let term = LexicalQuery::Term(FieldName::from("_sparse"), "s6".to_owned());
        let query = LexicalQuery::Bool {
            must: vec![LexicalQuery::Match(None, "cat".to_owned())],
            should: vec![
                LexicalQuery::Boost(Box::new(LexicalQuery::Match(None, "dog".to_owned())), 2.0),
                kept.clone(),
                term.clone(),
            ],
            must_not: vec![LexicalQuery::Match(None, "fish".to_owned())],
        };
        assert_eq!(
            own_fields(&schema(), &query),
            LexicalQuery::Bool {
                must: vec![fields("cat")],
                should: vec![
                    LexicalQuery::Boost(Box::new(fields("dog")), 2.0),
                    kept,
                    term
                ],
                must_not: vec![fields("fish")],
            }
        );
    }
}
