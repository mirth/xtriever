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
        for (original, recovered) in vector.iter().zip(recover(&codes, scale)) {
            prop_assert!((original - recovered).abs() <= scale / 2.0 + f32::EPSILON * scale);
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

    /// The direction survives, which is what a cosine ranking depends on. The bound is loose
    /// against what was measured on real embeddings (0.99941 mean) because a random vector is a
    /// harder case than a trained one.
    #[test]
    fn the_direction_survives(vector in prop::collection::vec(-1.0f32..1.0, 32..=384)) {
        prop_assume!(vector.iter().any(|v| v.abs() > 1e-3));
        let (codes, scale) = quantise(&vector);
        prop_assert!(cosine(&vector, &recover(&codes, scale)) > 0.999);
    }
}
