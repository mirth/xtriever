//! nDCG@k and Recall@k over an **ordered** id list, and the per-query aggregation, under the
//! conventions of `pytrec_eval` behind BEIR's wrapper (research D3):
//!
//! | situation | rule |
//! |---|---|
//! | gain | linear: `grade / log2(rank + 1)`; ideal DCG from all grades > 0 sorted descending, cut at `k` |
//! | recall | relevant (grade > 0) among the first `k` distinct ids ÷ all relevant |
//! | a repeated id | counts once, at its first rank |
//! | no relevant document | 0.0 (never NaN), and the query **is** counted in the mean |
//! | judged query with an empty result list | 0.0, counted |
//! | query in the run but not judged | ignored, counted as `unjudged_queries` |
//! | judged query absent from the run | excluded from the mean, counted as `not_retrieved_queries` |
//! | a result whose id equals the query id | dropped before scoring and counted (BEIR's `ignore_identical_ids`) |
//! | ties | none: the list order **is** the ranking; nothing here re-sorts |
//! | mean | per-query values summed in ascending query-id order (FR-004) |

use std::collections::{BTreeMap, BTreeSet};

use crate::dataset::Qrels;

/// First `k` distinct ids, in order.
fn head<'a>(ranked: &[&'a str], k: usize) -> Vec<&'a str> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(k.min(ranked.len()));
    for id in ranked {
        if out.len() == k {
            break;
        }
        if seen.insert(*id) {
            out.push(*id);
        }
    }
    out
}

fn discount(rank_from_one: usize) -> f64 {
    ((rank_from_one + 1) as f64).log2()
}

/// nDCG at `k` for one ranked list. `grades` is the query's judgements (grade 0 = non-relevant).
pub fn ndcg_at(ranked: &[&str], grades: &BTreeMap<String, u32>, k: usize) -> f64 {
    let mut ideal: Vec<u32> = grades.values().copied().filter(|g| *g > 0).collect();
    if ideal.is_empty() {
        return 0.0;
    }
    ideal.sort_unstable_by(|a, b| b.cmp(a));
    let idcg: f64 = ideal
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, g)| f64::from(*g) / discount(i + 1))
        .sum();
    let dcg: f64 = head(ranked, k)
        .iter()
        .enumerate()
        .map(|(i, id)| f64::from(grades.get(*id).copied().unwrap_or(0)) / discount(i + 1))
        .sum();
    dcg / idcg
}

/// Recall at `k`: relevant documents among the first `k` distinct ids ÷ all relevant documents.
pub fn recall_at(ranked: &[&str], grades: &BTreeMap<String, u32>, k: usize) -> f64 {
    let relevant = grades.values().filter(|g| **g > 0).count();
    if relevant == 0 {
        return 0.0;
    }
    let hit = head(ranked, k)
        .iter()
        .filter(|id| grades.get(**id).is_some_and(|g| *g > 0))
        .count();
    hit as f64 / relevant as f64
}

/// Per-query metrics and their means.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryMetrics {
    /// `query id → (ndcg@10, recall@100)` for every scored query, sorted by id.
    pub per_query: BTreeMap<String, (f64, f64)>,
    /// Judged queries with a (possibly empty) result list — the mean's denominator.
    pub scored_queries: u32,
    /// Queries in the run but not in the qrels: ignored.
    pub unjudged_queries: u32,
    /// Judged queries absent from the run: excluded from the mean.
    pub not_retrieved_queries: u32,
    /// Scored queries with no grade > 0: counted as 0.0.
    pub no_relevant_queries: u32,
    /// Results removed because the document id equalled the query id.
    pub dropped_identical: u32,
    /// Mean nDCG@10 over `scored_queries`, summed in ascending query-id order.
    pub mean_ndcg_10: f64,
    /// Mean Recall@100, likewise.
    pub mean_recall_100: f64,
}

/// Score every query of `run` against `qrels`.
pub fn score_queries(run: &BTreeMap<String, Vec<String>>, qrels: &Qrels) -> QueryMetrics {
    let mut per_query = BTreeMap::new();
    let mut unjudged = 0u32;
    let mut no_relevant = 0u32;
    let mut dropped_identical = 0u32;
    for (q, ids) in run {
        let Some(grades) = qrels.judged(q) else {
            unjudged += 1;
            continue;
        };
        if !grades.values().any(|g| *g > 0) {
            no_relevant += 1;
        }
        let before = ids.len();
        let refs: Vec<&str> = ids
            .iter()
            .map(String::as_str)
            .filter(|id| *id != q)
            .collect();
        dropped_identical += (before - refs.len()) as u32;
        per_query.insert(
            q.clone(),
            (ndcg_at(&refs, grades, 10), recall_at(&refs, grades, 100)),
        );
    }
    let not_retrieved = qrels
        .grades
        .keys()
        .filter(|q| !run.contains_key(*q))
        .count() as u32;
    let n = per_query.len();
    // BTreeMap iterates in ascending key order: the fixed summation order FR-004 requires.
    let (sum_n, sum_r) = per_query
        .values()
        .fold((0.0f64, 0.0f64), |(a, b), (n, r)| (a + n, b + r));
    let (mean_ndcg_10, mean_recall_100) = if n == 0 {
        (0.0, 0.0)
    } else {
        (sum_n / n as f64, sum_r / n as f64)
    };
    QueryMetrics {
        per_query,
        scored_queries: n as u32,
        unjudged_queries: unjudged,
        not_retrieved_queries: not_retrieved,
        no_relevant_queries: no_relevant,
        dropped_identical,
        mean_ndcg_10,
        mean_recall_100,
    }
}
