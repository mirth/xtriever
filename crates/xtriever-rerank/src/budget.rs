//! The budget loop (spec FR-007; research D6): score in input order, stop at the item or time
//! limit, `None` for every passage not reached. The crate's only clock use — `xtriever-rerank`
//! is a leaf crate, so `Instant` is permitted here (Principle III); the pipeline never reads a
//! clock and hands this stage the *remaining* time as `max_time`.

use std::time::Instant;

use xtriever_core::{Budget, Passage, Result};

/// Score `passages` against `query` with `scorer` in input order until `budget` is spent.
///
/// The time check precedes every pair, the first included, with `>=`: a zero limit of either
/// kind scores nothing; a limit the first pair fits — the check for pair 1 sees only the
/// nanoseconds since entry — always yields at least one score; no budget scores everything.
/// Public for the offline budget test only.
///
/// # Errors
///
/// The first `scorer` error aborts the call (the pipeline degrades on it).
#[doc(hidden)]
pub fn rerank_with(
    scorer: &dyn Fn(&str, &str) -> Result<f32>,
    query: &str,
    passages: &[Passage<'_>],
    budget: &Budget,
) -> Result<Vec<Option<f32>>> {
    let mut out = vec![None; passages.len()];
    let start = Instant::now();
    for (i, p) in passages.iter().enumerate() {
        if budget.max_items.is_some_and(|n| i >= n) {
            break;
        }
        if budget.max_time.is_some_and(|t| start.elapsed() >= t) {
            break;
        }
        out[i] = Some(scorer(query, p.text)?);
    }
    Ok(out)
}
