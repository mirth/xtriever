//! The re-rank ordering rule (research D8; spec FR-011, FR-012): scored candidates first by
//! `(score DESC, id ASC)`, then every unscored candidate — not reached by the budget, or beyond
//! the re-rank depth — in fused order, cut at `k`. One rule covers "not reached" and "not
//! selected": whatever the re-ranker vouched for comes first, and no candidate disappears.

use xtriever_core::DocId;

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
