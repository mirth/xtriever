//! Exact scoring and the total `(score DESC, id ASC)` order (spec FR-010–FR-013; research D7, D9).
//!
//! Scores accumulate in `f64` in index order and are rounded to `f32` once: a product of two
//! `f32` is exact in `f64`, so the result agrees with a `float64` oracle to the final rounding, and
//! identical rows give identical bits whatever order they are visited in. A scalar loop — the
//! compiler does not reorder float reductions — with no threads and no SIMD (FR-027).

use std::cmp::Ordering;

use xtriever_core::{DocId, Hit, Metric, Result};

use crate::error::{dim_mismatch, invalid_query};

/// A query checked for width and finiteness, with its norm precomputed.
pub(crate) struct Query<'a> {
    pub vector: &'a [f32],
    pub norm: f64,
}

/// Validate a query for `dim` and `metric` (`DimensionMismatch`, `InvalidQuery`).
pub(crate) fn validate_query<'a>(q: &'a [f32], dim: usize, metric: Metric) -> Result<Query<'a>> {
    if q.len() != dim {
        return Err(dim_mismatch(dim, q.len()));
    }
    if let Some(i) = q.iter().position(|x| !x.is_finite()) {
        return Err(invalid_query(format!(
            "query has a non-finite component at index {i}"
        )));
    }
    let norm = norm_f64(q.iter().copied());
    if metric == Metric::Cosine && norm == 0.0 {
        return Err(invalid_query(
            "query has zero norm; cosine similarity is undefined",
        ));
    }
    Ok(Query { vector: q, norm })
}

/// Euclidean norm in `f64` from `f32` components.
pub(crate) fn norm_f64(xs: impl Iterator<Item = f32>) -> f64 {
    xs.map(|x| {
        let x = f64::from(x);
        x * x
    })
    .sum::<f64>()
    .sqrt()
}

/// One row's score under `metric`, rounded to `f32` once.
pub(crate) fn score(
    metric: Metric,
    q: &Query<'_>,
    row: impl Iterator<Item = f32>,
    row_norm: f32,
) -> f32 {
    let value = match metric {
        Metric::Cosine => {
            let dot = dot_f64(q.vector, row);
            dot / (q.norm * f64::from(row_norm))
        }
        Metric::Dot => dot_f64(q.vector, row),
        Metric::Euclidean => {
            let sum: f64 = q
                .vector
                .iter()
                .zip(row)
                .map(|(&a, b)| {
                    let d = f64::from(a) - f64::from(b);
                    d * d
                })
                .sum();
            -sum.sqrt()
        }
    };
    // `as` is the one conversion here; f64 → f32 rounds to nearest, which is the intent.
    value as f32
}

fn dot_f64(q: &[f32], row: impl Iterator<Item = f32>) -> f64 {
    q.iter()
        .zip(row)
        .map(|(&a, b)| f64::from(a) * f64::from(b))
        .sum()
}

/// Sort `(score, id)` pairs into `(score DESC, id ASC)` and keep the first `k`.
///
/// `partial_cmp` rather than `total_cmp`: scores are finite by construction, and `-0.0` must tie
/// `+0.0` (broken by id) exactly as the oracle orders them.
pub(crate) fn top_k(mut scored: Vec<(f32, u32)>, k: usize) -> Vec<Hit> {
    scored.sort_unstable_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    scored.truncate(k);
    scored
        .into_iter()
        .map(|(score, id)| Hit {
            id: DocId(id),
            score,
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn order_is_score_desc_then_id_asc_with_signed_zero_tied() {
        let hits = top_k(vec![(0.5, 9), (-0.0, 7), (0.5, 2), (0.0, 3), (1.0, 8)], 10);
        let ids: Vec<u32> = hits.iter().map(|h| h.id.0).collect();
        assert_eq!(ids, vec![8, 2, 9, 3, 7]);
        assert_eq!(top_k(vec![(1.0, 1), (2.0, 2)], 0).len(), 0);
    }

    #[test]
    fn cosine_of_identical_vectors_is_one() {
        let v = [0.3f32, -0.4, 0.5];
        let q = validate_query(&v, 3, Metric::Cosine).unwrap();
        let s = score(
            Metric::Cosine,
            &q,
            v.iter().copied(),
            norm_f64(v.iter().copied()) as f32,
        );
        assert!((s - 1.0).abs() < 1e-6);
    }
}
