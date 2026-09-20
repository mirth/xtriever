//! Exact scoring and the total `(score DESC, id ASC)` order (spec FR-010–FR-013; research D7, D9).
//!
//! Since Feature 026 the stored rows are eight-bit codes with a scale (ADR-0015), so "exact"
//! means exact for what is stored: the dot product accumulates in `i32` — no rounding at all —
//! and one multiply by the two scales turns it into a score. Cosine divides that by the norms of
//! the two *quantised* vectors (the row's is stored beside it, the query's is computed once), so
//! it is the cosine of what is actually compared and a row's cosine with itself is one. Identical
//! rows give identical bits whatever order they are visited in, as before. The Euclidean path
//! recovers the row as floats and accumulates in `f64`, because a distance is not a dot product.
//!
//! The metric is decided once per search, not once per row: [`validate_query`] returns the
//! query already in the form its metric scores with, and the index runs one loop per form.
//!
//! A scalar loop — the compiler does not reorder float reductions — with no threads and no SIMD
//! (FR-027).

use std::cmp::Ordering;

use xtriever_core::{DocId, Hit, Metric, Result};

use crate::error::{dim_mismatch, invalid_query};
use crate::quantise::{self, Quantised};

/// A query checked for width and finiteness, in the form its metric scores with.
pub(crate) enum Query<'a> {
    /// `Metric::Dot`: the query quantised with the rows' scheme.
    Dot(Quantised),
    /// `Metric::Cosine`: the quantised query and the norm of what it recovers to.
    Cosine { codes: Quantised, norm: f64 },
    /// `Metric::Euclidean`: the floats as given; rows are recovered to floats to match.
    Euclidean(&'a [f32]),
}

/// Validate a query for `dim` and `metric` (`DimensionMismatch`, `InvalidQuery`), and put it
/// in the form the metric scores with.
pub(crate) fn validate_query<'a>(q: &'a [f32], dim: usize, metric: Metric) -> Result<Query<'a>> {
    if q.len() != dim {
        return Err(dim_mismatch(dim, q.len()));
    }
    if let Some(i) = q.iter().position(|x| !x.is_finite()) {
        return Err(invalid_query(format!(
            "query has a non-finite component at index {i}"
        )));
    }
    Ok(match metric {
        Metric::Dot => Query::Dot(quantise::quantise(q)),
        Metric::Cosine => {
            if norm_f64(q.iter().copied()) == 0.0 {
                return Err(invalid_query(
                    "query has zero norm; cosine similarity is undefined",
                ));
            }
            let codes = quantise::quantise(q);
            let norm = quantise::norm(&codes);
            if norm == 0.0 {
                // Every component below half the scale floor: nothing to point with.
                return Err(invalid_query(
                    "query is zero at eight-bit precision; cosine similarity is undefined",
                ));
            }
            Query::Cosine { codes, norm }
        }
        Metric::Euclidean => Query::Euclidean(q),
    })
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

/// One row's dot-product score from its stored codes and scale, rounded to `f32` once.
pub(crate) fn dot_score(query: &Quantised, row_codes: &[u8], row_scale: f32) -> f32 {
    // `as` is the one conversion here; f64 → f32 rounds to nearest, which is the intent.
    quantise::dot(query, row_codes, f64::from(row_scale)) as f32
}

/// One row's cosine score: the quantised dot product over the two quantised norms.
pub(crate) fn cosine_score(
    query: &Quantised,
    query_norm: f64,
    row_codes: &[u8],
    row_scale: f32,
    row_norm: f32,
) -> f32 {
    let dot = quantise::dot(query, row_codes, f64::from(row_scale));
    (dot / (query_norm * f64::from(row_norm))) as f32
}

/// One row's Euclidean score, `-distance`, over the recovered components.
pub(crate) fn euclidean_score(query: &[f32], row: impl Iterator<Item = f32>) -> f32 {
    let sum: f64 = query
        .iter()
        .zip(row)
        .map(|(&a, b)| {
            let d = f64::from(a) - f64::from(b);
            d * d
        })
        .sum();
    (-sum.sqrt()) as f32
}

/// Sort `(score, id)` pairs into `(score DESC, id ASC)` and keep the first `k`.
///
/// `partial_cmp` rather than `total_cmp`: scores are finite by construction (the scan refuses a
/// row whose scale or norm is not), and `-0.0` must tie `+0.0` (broken by id) exactly as the
/// oracle orders them.
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
    fn cosine_of_a_row_with_itself_is_one() {
        // Review finding 6: with the float norm this was 1.000026 for [1.0, 0.006]. Over the
        // quantised norms it is one to the last bit of the f64 division.
        for v in [vec![0.3f32, -0.4, 0.5], vec![1.0, 0.006]] {
            let Query::Cosine { codes, norm } =
                validate_query(&v, v.len(), Metric::Cosine).unwrap()
            else {
                panic!("cosine query")
            };
            let raw: Vec<u8> = codes.codes.iter().map(|c| *c as u8).collect();
            let s = cosine_score(&codes, norm, &raw, codes.scale, norm as f32);
            assert!((s - 1.0).abs() <= f32::EPSILON, "{v:?}: {s}");
        }
    }

    #[test]
    fn a_query_that_is_zero_at_eight_bit_precision_is_invalid_under_cosine() {
        let tiny = [1e-44f32, 0.0];
        assert!(matches!(
            validate_query(&tiny, 2, Metric::Cosine),
            Err(xtriever_core::Error::InvalidQuery(_))
        ));
        // The same vector is a legitimate (zero) dot-product query.
        assert!(matches!(
            validate_query(&tiny, 2, Metric::Dot),
            Ok(Query::Dot(_))
        ));
    }
}
