//! The re-rank ordering rules.
//!
//! **Replace** (Feature 006, research D8; spec FR-011, FR-012): scored candidates first by
//! `(score DESC, id ASC)`, then every unscored candidate — not reached by the budget, or beyond
//! the re-rank depth — in fused order, cut at `k`. One rule covers "not reached" and "not
//! selected": whatever the re-ranker vouched for comes first, and no candidate disappears.
//!
//! **Interpolate** (Feature 015, the default; ADR-0012): the scored head is ordered by
//! `(1 − α)·minmax(fused score) + α·minmax(cross-encoder score)`, both min-max normalised over
//! the head (a constant column, or a single candidate, contributes zeros), ties by fused
//! position; the rest follows in fused order. Measured in Feature 014 over the three BEIR sets:
//! at α 0.5 and depth 20 it scores 0.7207 / 0.3622 / 0.3910 nDCG@10 against replace-order's
//! 0.6954 / 0.3609 / 0.3742 — the cross-encoder's *scores* carry information its *order* alone
//! throws away.

use serde::{Deserialize, Serialize};
use xtriever_core::{DocId, Error, Result};

/// How the re-ranked head is ordered (Feature 015).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankMode {
    /// The cross-encoder's order replaces the fused order within the head (Feature 006).
    Replace,
    /// The head is ordered by `(1 − alpha)·minmax(fused) + alpha·minmax(cross-encoder)`,
    /// ties by fused position. `alpha` must be finite and within `[0, 1]`.
    Interpolate {
        /// Weight of the cross-encoder term; 0 keeps the fused order, 1 follows the
        /// cross-encoder (ties by fused position rather than by id).
        alpha: f64,
    },
}

impl Default for RerankMode {
    /// `Interpolate { alpha: 0.5 }` — the configuration Feature 014 chose under its fixed rule.
    fn default() -> Self {
        Self::Interpolate { alpha: 0.5 }
    }
}

impl RerankMode {
    /// `alpha` finite and within `[0, 1]`; `Replace` has nothing to check.
    ///
    /// # Errors
    ///
    /// `Error::Schema` naming the offending value.
    pub fn validate(&self) -> Result<()> {
        match *self {
            Self::Replace => Ok(()),
            Self::Interpolate { alpha } if alpha.is_finite() && (0.0..=1.0).contains(&alpha) => {
                Ok(())
            }
            Self::Interpolate { alpha } => Err(Error::Schema(format!(
                "rerank alpha must be within [0, 1], got {alpha}"
            ))),
        }
    }
}

/// Order `fused` (ids with their fused scores) under the re-ranker's `scores` (one per leading
/// candidate, `None` = not scored); returns `(id, fused score, re-rank score)` — at most `k`.
/// Scores must be finite (the caller validates); public so tests and the harness can call the
/// exact rule the index uses.
#[must_use]
pub fn order_reranked(
    fused: &[(DocId, f64)],
    scores: &[Option<f32>],
    k: usize,
) -> Vec<(DocId, f64, Option<f32>)> {
    let mut scored: Vec<(DocId, f64, Option<f32>)> = fused
        .iter()
        .zip(scores)
        .filter_map(|(&(id, fused_score), s)| s.map(|s| (id, fused_score, Some(s))))
        .collect();
    scored.sort_by(|a, b| {
        let (sa, sb) = (a.2.unwrap_or(f32::NAN), b.2.unwrap_or(f32::NAN));
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    let unscored = fused
        .iter()
        .enumerate()
        .filter(|(i, _)| !scores.get(*i).is_some_and(Option::is_some))
        .map(|(_, &(id, fused_score))| (id, fused_score, None));
    scored.into_iter().chain(unscored).take(k).collect()
}

/// Per-column min-max to `[0, 1]`; a constant column (or a single value) is all zeros.
fn minmax(values: &[f64]) -> Vec<f64> {
    let (lo, hi) = values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    if values.is_empty() || hi == lo {
        return vec![0.0; values.len()];
    }
    values.iter().map(|&v| (v - lo) / (hi - lo)).collect()
}

/// Order `fused` under the interpolating rule (Feature 015): the scored head by
/// `(combined DESC, fused position ASC)` with `combined = (1 − alpha)·minmax(fused score) +
/// alpha·minmax(score)`, then every unscored candidate in fused order, cut at `k`. Returns
/// `(id, fused score, re-rank score, combined score)` — the last two `Some` exactly for the
/// head. Scores must be finite and `alpha` valid (the caller validates); public so tests and
/// the harness can call the exact rule the index uses.
#[must_use]
pub fn order_interpolated(
    fused: &[(DocId, f64)],
    scores: &[Option<f32>],
    k: usize,
    alpha: f64,
) -> Vec<(DocId, f64, Option<f32>, Option<f64>)> {
    // The head: (fused position, id, fused score, cross-encoder score).
    let head: Vec<(usize, DocId, f64, f32)> = fused
        .iter()
        .zip(scores)
        .enumerate()
        .filter_map(|(pos, (&(id, fused_score), s))| s.map(|s| (pos, id, fused_score, s)))
        .collect();
    let f = minmax(&head.iter().map(|h| h.2).collect::<Vec<_>>());
    let c = minmax(&head.iter().map(|h| f64::from(h.3)).collect::<Vec<_>>());
    // Head positions sorted by (combined DESC, fused position ASC).
    let combined: Vec<f64> = (0..head.len())
        .map(|j| (1.0 - alpha) * f[j] + alpha * c[j])
        .collect();
    let mut order: Vec<usize> = (0..head.len()).collect();
    order.sort_by(|&a, &b| {
        combined[b]
            .partial_cmp(&combined[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(head[a].0.cmp(&head[b].0))
    });
    let unscored = fused
        .iter()
        .enumerate()
        .filter(|(i, _)| !scores.get(*i).is_some_and(Option::is_some))
        .map(|(_, &(id, fused_score))| (id, fused_score, None, None));
    order
        .into_iter()
        .map(|j| {
            let (_, id, fused_score, s) = head[j];
            (id, fused_score, Some(s), Some(combined[j]))
        })
        .chain(unscored)
        .take(k)
        .collect()
}

/// The rule the index applies for `mode`, in the four-tuple shape (`Replace` carries no
/// combined score).
pub(crate) fn order_head(
    mode: RerankMode,
    fused: &[(DocId, f64)],
    scores: &[Option<f32>],
    k: usize,
) -> Vec<(DocId, f64, Option<f32>, Option<f64>)> {
    match mode {
        RerankMode::Replace => order_reranked(fused, scores, k)
            .into_iter()
            .map(|(id, fused_score, s)| (id, fused_score, s, None))
            .collect(),
        RerankMode::Interpolate { alpha } => order_interpolated(fused, scores, k, alpha),
    }
}
