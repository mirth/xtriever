//! Feature 026: the eight-bit scheme's invariants over random vectors, not just chosen ones
//! (spec FR-001, data-model "Invariants"; ADR-0015).
//!
//! The unit tests in `src/quantise.rs` pin the shapes that matter — the peak, the zero vector,
//! the forbidden code, the half-way rounding, the denormal peak. These check that nothing else
//! drifts: that recovery stays within half a step everywhere, that the scheme is idempotent on
//! its own output, that the stored norm is the recovered vector's, and that the cosine between
//! a vector and its recovery stays above what the study measured, which is the property the
//! ranking actually depends on.
//!
//! These are property tests of the crate's own quantiser, reached through `support`'s wrappers;
//! the independent restatement of the scheme is `reference/dense_format3.py`, which mints and
//! checks the goldens the other suites replay.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use proptest::prelude::*;
use support::{quantise, recover, recovered_norm};

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        1.0
    } else {
        dot / (na * nb)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every component comes back within half a quantisation step.
    #[test]
    fn recovery_stays_within_half_a_step(vector in prop::collection::vec(-2.0f32..2.0, 1..=384)) {
        let (codes, scale) = quantise(&vector);
        for (original, recovered) in vector.iter().zip(recover(&codes, scale)) {
            prop_assert!((original - recovered).abs() <= scale / 2.0 + 1e-6);
        }
    }

    /// The scale is a normal positive number and no code is the value that would make negation
    /// asymmetric — including for vectors whose peak is below the floor (review finding 2).
    #[test]
    fn the_scale_is_normal_and_no_code_is_the_minimum(
        vector in prop_oneof![
            prop::collection::vec(-2.0f32..2.0, 1..=384),
            prop::collection::vec(-1e-36f32..1e-36, 1..=384),
        ],
    ) {
        let (codes, scale) = quantise(&vector);
        prop_assert!(scale.is_normal() && scale > 0.0, "{scale:e}");
        prop_assert!(codes.iter().all(|c| *c != i8::MIN));
        // Half a step, plus what f32 arithmetic adds: the quotient `v / scale` is rounded to
        // f32 before it is rounded to a code (a half-way quotient such as 126.5 lands on either
        // side of it), and `code × scale` rounds once more — a few ulps of the peak, not of the
        // scale. CI found the bound short by that much on `[-1.9813946, 1.9892262]`.
        let peak = vector.iter().fold(0.0f32, |p, v| p.max(v.abs()));
        let slack = 4.0 * f32::EPSILON * peak.max(scale);
        for (original, recovered) in vector.iter().zip(recover(&codes, scale)) {
            prop_assert!(
                (original - recovered).abs() <= scale / 2.0 + slack,
                "{original} recovered as {recovered}, step {scale}"
            );
        }
    }

    /// Quantising what the scheme produced changes nothing: a re-encoded index is the same index.
    #[test]
    fn the_scheme_is_idempotent(vector in prop::collection::vec(-2.0f32..2.0, 1..=384)) {
        let (codes, scale) = quantise(&vector);
        let (again, scale_again) = quantise(&recover(&codes, scale));
        prop_assert_eq!(codes, again);
        prop_assert!((scale - scale_again).abs() <= f32::EPSILON * scale.max(1.0));
    }

    /// The stored norm is the norm of what the row recovers to, so a row's cosine with itself
    /// in the scan's arithmetic is one (review finding 6).
    #[test]
    fn the_stored_norm_is_the_recovered_norm(vector in prop::collection::vec(-2.0f32..2.0, 1..=384)) {
        let (codes, scale) = quantise(&vector);
        let by_floats = support::norm(&recover(&codes, scale));
        let stored = recovered_norm(&codes, scale);
        prop_assert!((stored - by_floats).abs() <= 1e-4 * by_floats.max(1.0), "{stored} vs {by_floats}");
        let dot: i64 = codes.iter().map(|c| i64::from(*c) * i64::from(*c)).sum();
        let self_cosine = dot as f64 * f64::from(scale) * f64::from(scale) / (stored * stored);
        prop_assert!(dot == 0 || (self_cosine - 1.0).abs() < 1e-9, "{self_cosine}");
    }

    /// The direction survives to the degree the half-step bound implies — an invariant of the
    /// scheme, not a statistic of random vectors (review round 6, finding 5). With every
    /// component within `step / 2`, the error vector `e` has `‖e‖ ≤ (step / 2) · √dim`, and for
    /// `t = ‖e‖ / ‖v‖ < 1` the cosine between `v` and `v + e` is at least `(1 − t) / (1 + t)`.
    /// What real embeddings measure (0.99941 mean) is the study's number, not this test's.
    #[test]
    fn the_direction_survives_within_the_bound_the_step_implies(
        vector in prop::collection::vec(-1.0f32..1.0, 32..=384),
    ) {
        let (codes, scale) = quantise(&vector);
        let norm = support::norm(&vector);
        let worst_error = f64::from(scale) / 2.0 * (vector.len() as f64).sqrt();
        prop_assume!(worst_error < norm);
        let t = worst_error / norm;
        let bound = (1.0 - t) / (1.0 + t);
        let got = f64::from(cosine(&vector, &recover(&codes, scale)));
        prop_assert!(got >= bound - 1e-6, "cosine {got} below the bound {bound} (t = {t})");
    }
}
