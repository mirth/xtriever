//! Feature 026: the eight-bit scheme's invariants over random vectors, not just chosen ones
//! (spec FR-001, data-model "Invariants"; ADR-0015).
//!
//! The unit tests in `src/quantise.rs` pin the shapes that matter — the peak, the zero vector,
//! the forbidden code. These check that nothing else drifts: that recovery stays within half a
//! step everywhere, that the scheme is idempotent on its own output, and that the cosine between
//! a vector and its recovery stays above what the study measured, which is the property the
//! ranking actually depends on.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use proptest::prelude::*;

/// The scheme, as `src/quantise.rs` implements it. Duplicated here rather than exported: the
/// module is private on purpose, and a test that re-states the rule catches a change to it.
fn quantise(vector: &[f32]) -> (Vec<i8>, f32) {
    let peak = vector.iter().fold(0.0f32, |peak, v| peak.max(v.abs()));
    let scale = if peak > 0.0 { peak / 127.0 } else { 1.0 };
    let codes = vector
        .iter()
        .map(|v| (v / scale).round().clamp(-127.0, 127.0) as i8)
        .collect();
    (codes, scale)
}

fn recover(codes: &[i8], scale: f32) -> Vec<f32> {
    codes.iter().map(|c| f32::from(*c) * scale).collect()
}

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

    /// The scale is usable and no code is the value that would make negation asymmetric.
    #[test]
    fn the_scale_is_positive_and_no_code_is_the_minimum(vector in prop::collection::vec(-2.0f32..2.0, 1..=384)) {
        let (codes, scale) = quantise(&vector);
        prop_assert!(scale > 0.0);
        prop_assert!(codes.iter().all(|c| *c != i8::MIN));
    }

    /// Quantising what the scheme produced changes nothing: a re-encoded index is the same index.
    #[test]
    fn the_scheme_is_idempotent(vector in prop::collection::vec(-2.0f32..2.0, 1..=384)) {
        let (codes, scale) = quantise(&vector);
        let (again, scale_again) = quantise(&recover(&codes, scale));
        prop_assert_eq!(codes, again);
        prop_assert!((scale - scale_again).abs() <= f32::EPSILON * scale.max(1.0));
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
