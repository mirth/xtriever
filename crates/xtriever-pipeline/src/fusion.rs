//! Reciprocal rank fusion (research D5): `Σ 1 / (rrf_k + rank)` over the stages, in `f64`,
//! lexical term first, then dense — the same order the Python oracle adds in, so equal rank
//! pairs give bit-identical sums on both sides.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use xtriever_core::{DocId, Hit};

/// Reciprocal rank fusion over two ranked lists: ranks are 1-based positions in each list, a
/// document in one list only contributes that one term, order is `(score DESC, DocId ASC)`,
/// at most `k` results. Scores on the hits are ignored — only positions matter.
#[must_use]
pub fn rrf(lexical: &[Hit], dense: &[Hit], rrf_k: u32, k: usize) -> Vec<(DocId, f64)> {
    fused_terms(lexical, dense, rrf_k)
        .into_iter()
        .take(k)
        .map(|(id, lex, den)| (id, lex + den))
        .collect()
}

/// The per-document terms, already in fused order: `(id, lexical term, dense term)`.
pub(crate) fn fused_terms(lexical: &[Hit], dense: &[Hit], rrf_k: u32) -> Vec<(DocId, f64, f64)> {
    let mut terms: BTreeMap<u32, (f64, f64)> = BTreeMap::new();
    for (rank, hit) in lexical.iter().enumerate() {
        terms.entry(hit.id.0).or_default().0 = term(rrf_k, rank);
    }
    for (rank, hit) in dense.iter().enumerate() {
        terms.entry(hit.id.0).or_default().1 = term(rrf_k, rank);
    }
    let mut out: Vec<(DocId, f64, f64)> = terms
        .into_iter()
        .map(|(id, (lex, den))| (DocId(id), lex, den))
        .collect();
    out.sort_by(|a, b| {
        let (sa, sb) = (a.1 + a.2, b.1 + b.2);
        sb.partial_cmp(&sa)
            .unwrap_or(Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    out
}

fn term(rrf_k: u32, rank0: usize) -> f64 {
    // 1-based rank; `f64::from(u32)` and `as f64` on a small usize are both exact here.
    1.0 / (f64::from(rrf_k) + (rank0 + 1) as f64)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn hits(ids: &[u32]) -> Vec<Hit> {
        ids.iter()
            .map(|&i| Hit {
                id: DocId(i),
                score: 0.0,
            })
            .collect()
    }

    #[test]
    fn k_zero_and_empty_lists() {
        assert!(rrf(&hits(&[1, 2]), &hits(&[2]), 60, 0).is_empty());
        assert!(rrf(&[], &[], 60, 10).is_empty());
        let one = rrf(&hits(&[7]), &[], 60, 10);
        assert_eq!(one, vec![(DocId(7), 1.0 / 61.0)]);
    }
}
