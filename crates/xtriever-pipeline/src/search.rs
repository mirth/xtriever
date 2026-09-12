//! Search: filter once, lexical, dense, fuse — with degradation, budgets and explanation
//! (data-model "Search algorithm"; research D5–D8).

use std::collections::HashMap;
use std::time::Duration;

use xtriever_core::{
    DocSet, Error, Filter, Hit, LexicalIndex, LexicalQuery, Result, TextKind, VectorIndex,
};

use crate::fusion::fused_terms;
use crate::index::HybridIndex;
use crate::{
    Degradation, DegradeReason, HitExplain, HybridHit, Response, SearchOptions, StageReport,
};

/// Why the dense stage did not contribute — the internal form of [`Degradation`].
enum Skip {
    Error(Error),
    Budget { elapsed_ms: u64, limit_ms: u64 },
}

impl HybridIndex {
    /// Search with a free-text query: `LexicalQuery::Match` over every text field for the lexical
    /// stage, the embedded text for the dense stage.
    ///
    /// # Errors
    ///
    /// As [`search_lexical`](Self::search_lexical).
    pub fn search(
        &self,
        query: &str,
        filter: Option<&Filter>,
        k: usize,
        options: &SearchOptions<'_>,
    ) -> Result<Response> {
        self.search_with(
            &LexicalQuery::Match(None, query.to_owned()),
            query,
            filter,
            k,
            options,
        )
    }

    /// Search with a caller-built lexical query; the dense stage embeds `dense_text`.
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
        self.search_with(query, dense_text, filter, k, options)
    }

    fn search_with(
        &self,
        query: &LexicalQuery,
        dense_text: &str,
        filter: Option<&Filter>,
        k: usize,
        opts: &SearchOptions<'_>,
    ) -> Result<Response> {
        let time_limit_ignored = opts.budget.max_time.is_some() && opts.elapsed.is_none();
        let empty = |dense_ran: bool| Response {
            hits: Vec::new(),
            stages: StageReport {
                lexical_candidates: 0,
                dense_candidates: dense_ran.then_some(0),
                degraded: None,
                time_limit_ignored,
            },
        };
        if k == 0 {
            return Ok(empty(false));
        }
        // 2. Filter, resolved once; an empty set short-circuits both stages.
        let allowed: Option<DocSet> = match filter {
            Some(f) => Some(self.lexical.resolve_filter(f)?),
            None => None,
        };
        if allowed.as_ref().is_some_and(DocSet::is_empty) {
            return Ok(empty(true));
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

        // 7–8. Fuse, or degrade to the lexical list.
        let stages = StageReport {
            lexical_candidates: lexical.len(),
            dense_candidates: dense.as_ref().map(Vec::len),
            degraded: skip.map(|reason| Degradation {
                stage: "dense",
                reason,
            }),
            time_limit_ignored,
        };
        let hits = match &dense {
            Some(dense) => self.fuse(&lexical, dense, k, opts.explain)?,
            None => self.degraded(&lexical, k, opts.explain)?,
        };
        Ok(Response { hits, stages })
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

    fn fuse(
        &self,
        lexical: &[Hit],
        dense: &[Hit],
        k: usize,
        explain: bool,
    ) -> Result<Vec<HybridHit>> {
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
        let mut hits = Vec::with_capacity(k.min(lex_pos.len() + den_pos.len()));
        for (id, lex, den) in fused_terms(lexical, dense, self.config.rrf_k)
            .into_iter()
            .take(k)
        {
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
                }
            });
            hits.push(HybridHit {
                external_id: self.external_of(id)?.to_owned(),
                id,
                score,
                chunk: self.committed_ids.chunk(id).cloned(),
                explain,
            });
        }
        Ok(hits)
    }

    fn degraded(&self, lexical: &[Hit], k: usize, explain: bool) -> Result<Vec<HybridHit>> {
        let mut hits = Vec::with_capacity(k.min(lexical.len()));
        for (i, h) in lexical.iter().take(k).enumerate() {
            let score = f64::from(h.score);
            hits.push(HybridHit {
                external_id: self.external_of(h.id)?.to_owned(),
                id: h.id,
                score,
                chunk: self.committed_ids.chunk(h.id).cloned(),
                explain: explain.then(|| HitExplain {
                    bm25_score: Some(h.score),
                    bm25_rank: Some(i as u32 + 1),
                    dense_score: None,
                    dense_rank: None,
                    fused: score,
                }),
            });
        }
        Ok(hits)
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
