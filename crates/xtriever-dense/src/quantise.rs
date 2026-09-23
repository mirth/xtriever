//! Eight-bit quantisation of a dense vector (Feature 026, [ADR-0015]).
//!
//! One scale per vector, symmetric: `scale = max|component| / 127` and
//! `code = round(component / scale)`, rounding half away from zero. Recovery is `code × scale`,
//! and it is **approximate** — this module never claims otherwise, and the stage keeps no float
//! copy to fall back on (spec FR-003).
//!
//! The scale is floored at [`f32::MIN_POSITIVE`]: a peak so small that `peak / 127` would be a
//! denormal or zero (below about 1.5e-36) is quantised at the floor instead, so a stored scale
//! is always a normal, strictly positive number. Such a vector is zero at eight-bit precision
//! wherever its components are below half the floor, which is what a float would recover from
//! it too; `add` refuses it under Cosine, exactly as it refuses a zero vector.
//!
//! Measured on two datasets before the format changed: against the float ranking the scheme
//! costs 0.0006 nDCG@10 on SciFact and on NFCorpus, leaves Recall@100 unchanged, and agrees on
//! 99.5 % of the first hundred candidates (`reference/int8_vectors_study.py`).
//!
//! The module is `#[doc(hidden)] pub` so the crate's own test suites use this one spelling of
//! the scheme rather than a copy that would share its bugs; the *independent* restatement is
//! `reference/dense_format3.py`, which mints and checks every golden those suites replay.
//!
//! [ADR-0015]: https://github.com/mirth/xtriever/blob/main/docs/adr/0015-eight-bit-vectors-and-models.md

/// The name of this scheme, written into every manifest header so that reading code never has
/// to infer it: `i8` codes, one scale per vector, symmetric about zero.
pub(crate) const SCHEME: &str = "i8-symmetric-per-vector";

/// The largest magnitude a code may take. `-128` is never produced, so negating a code is exact.
pub(crate) const MAX_CODE: f32 = 127.0;

/// The widest vector [`dot`] can score without its `i32` accumulator overflowing:
/// `dim × 127 × 128 ≤ i32::MAX`. The engine never writes −128, but [`dot`] reads codes straight
/// from disk, where a corrupt or foreign byte can be, so the bound counts it (review round 2,
/// finding 3). An index is refused at create and at open beyond it.
pub(crate) const MAX_DIM: usize = (i32::MAX / (127 * 128)) as usize;

/// A quantised vector: the codes, and the scale that recovers them.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantised {
    /// One code per dimension, each in `-127..=127` as written; a reader takes a row byte of
    /// `0x80` as −128 (see `MAX_DIM`, crate-private).
    pub codes: Vec<i8>,
    /// A normal, strictly positive `f32`. Multiply a code by this to recover the component.
    pub scale: f32,
}

/// Quantise `vector`, whose components must be finite.
///
/// A vector of all zeros — which a degenerate embedding can be — gets scale `1.0` and all-zero
/// codes, so recovery gives back zeros instead of dividing by zero. A non-finite component is
/// a caller's bug — every caller validates first (`validate_vector`, `validate_query`) — and
/// would break the invariants below (an infinite peak makes an infinite scale, a NaN becomes
/// code 0), so it is asserted in debug builds rather than mapped to something plausible.
pub fn quantise(vector: &[f32]) -> Quantised {
    debug_assert!(
        vector.iter().all(|v| v.is_finite()),
        "quantise: a non-finite component; callers validate first"
    );
    let peak = vector.iter().fold(0.0f32, |peak, v| peak.max(v.abs()));
    let scale = if peak > 0.0 {
        // The floor keeps a denormal peak from storing a zero (or denormal) scale, which no
        // reader could tell from corruption.
        (peak / MAX_CODE).max(f32::MIN_POSITIVE)
    } else {
        1.0
    };
    let codes = vector
        .iter()
        .map(|v| {
            // `round` (half away from zero) then clamp: the peak component lands exactly on
            // ±127, and the clamp is a guard, not a path anything reaches with a floored scale.
            (v / scale).round().clamp(-MAX_CODE, MAX_CODE) as i8
        })
        .collect();
    Quantised { codes, scale }
}

/// The Euclidean norm of the vector a quantised row recovers to: `sqrt(Σ code²) × scale`,
/// with the sum exact in integers and one multiply after the root.
///
/// This is the norm a row stores (Feature 026 review, finding 6): cosine then divides the dot
/// product of two quantised vectors by the norms of *those* vectors, so a row's cosine with
/// itself is one and no score exceeds one beyond the final rounding. The norm of the float
/// vector that was added is not kept, because that vector is not kept either.
pub fn norm(quantised: &Quantised) -> f64 {
    let sum_of_squares: i64 = quantised
        .codes
        .iter()
        .map(|c| i64::from(*c) * i64::from(*c))
        .sum();
    // `i64 → f64` is exact below 2^53, and `dim × 127²` is nowhere near it.
    (sum_of_squares as f64).sqrt() * f64::from(quantised.scale)
}

/// One code recovered as a float: `code × scale`, the one spelling of the recovery, used by
/// [`recover`], by the row reader (`Rows::recover`) and by the test suites.
#[inline]
pub fn recover_code(code: i8, scale: f32) -> f32 {
    f32::from(code) * scale
}

/// Recover a quantised vector as floats. For tests and for `vector(id)`; the scan never
/// materialises a row.
pub fn recover(quantised: &Quantised) -> Vec<f32> {
    quantised
        .codes
        .iter()
        .map(|c| recover_code(*c, quantised.scale))
        .collect()
}

/// The dot product of a quantised query and a quantised row.
///
/// The products accumulate in `i32`: 384 terms of at most `127 × 128` (the query never holds
/// −128; a row byte on disk might) reach about 6.2 million, four orders of magnitude inside the
/// type, and [`MAX_DIM`] is where the bound would fail.
pub(crate) fn dot(query: &Quantised, row_codes: &[u8], row_scale: f64) -> f64 {
    debug_assert_eq!(query.codes.len(), row_codes.len());
    let mut accumulator: i32 = 0;
    for (q, r) in query.codes.iter().zip(row_codes) {
        accumulator += i32::from(*q) * i32::from(*r as i8);
    }
    // The accumulation is exact; one multiply turns it into the score. Nothing rounds until
    // here, which is why identical rows give identical bits whatever order they are visited in.
    f64::from(accumulator) * f64::from(query.scale) * row_scale
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_peak_component_lands_on_the_largest_code() {
        let q = quantise(&[0.5, -1.0, 0.25]);
        assert_eq!(
            q.codes[1], -127,
            "the largest magnitude uses the whole range"
        );
        assert!((q.scale - 1.0 / 127.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rounding_is_half_away_from_zero() {
        // Half-way quotients: 0.5 → 1, 2.5 → 3, −1.5 → −2 (never to even). The reference
        // generator in `reference/gen_026_fixtures.py` restates this rule (review finding 1).
        let q = quantise(&[127.0, 0.5, 2.5, -1.5]);
        assert_eq!(q.scale, 1.0);
        assert_eq!(q.codes, vec![127, 1, 3, -2]);
    }

    #[test]
    fn no_code_is_ever_the_minimum_of_the_type() {
        // -128 would make negation asymmetric; the clamp keeps it out.
        for vector in [vec![-1.0, 1.0], vec![-0.001, 0.0005], vec![-3.4e30, 1.0]] {
            let q = quantise(&vector);
            assert!(
                q.codes.iter().all(|c| *c != i8::MIN),
                "{vector:?} produced {:?}",
                q.codes
            );
        }
    }

    #[test]
    fn a_zero_vector_is_not_a_division_by_zero() {
        let q = quantise(&[0.0, 0.0, 0.0]);
        assert_eq!(q.scale, 1.0, "a scale that recovers zeros");
        assert_eq!(q.codes, vec![0, 0, 0]);
        assert_eq!(recover(&q), vec![0.0, 0.0, 0.0]);
        assert_eq!(norm(&q), 0.0);
    }

    #[test]
    fn the_scale_is_always_a_normal_positive_number() {
        // Including a denormal peak, which `peak / 127` would underflow to zero (review
        // finding 2): the floor holds and the codes stay meaningful where they can be.
        for vector in [
            vec![0.0],
            vec![-1.0, -2.0],
            vec![1e-30, -1e-30],
            vec![1e-44, 0.0],
            vec![f32::MIN_POSITIVE, -f32::MIN_POSITIVE / 2.0],
        ] {
            let q = quantise(&vector);
            assert!(
                q.scale.is_normal() && q.scale > 0.0,
                "{vector:?}: {}",
                q.scale
            );
        }
        let floored = quantise(&[f32::MIN_POSITIVE, -f32::MIN_POSITIVE / 2.0, 1e-44]);
        assert_eq!(floored.scale, f32::MIN_POSITIVE);
        assert_eq!(
            floored.codes,
            vec![1, -1, 0],
            "half the floor rounds away from zero"
        );
        assert_eq!(
            quantise(&[1e-44, 0.0]).codes,
            vec![0, 0],
            "zero at eight-bit precision"
        );
    }

    #[test]
    fn recovery_is_within_half_a_step() {
        let vector: Vec<f32> = (0..384).map(|i| ((i as f32) / 384.0) - 0.5).collect();
        let q = quantise(&vector);
        let step = q.scale;
        for (original, recovered) in vector.iter().zip(recover(&q)) {
            assert!(
                (original - recovered).abs() <= step / 2.0 + f32::EPSILON,
                "{original} recovered as {recovered}, step {step}"
            );
        }
    }

    #[test]
    fn quantising_a_recovered_vector_changes_nothing() {
        let vector: Vec<f32> = (0..64)
            .map(|i| ((i * 37 % 101) as f32 / 101.0) - 0.5)
            .collect();
        let once = quantise(&vector);
        let twice = quantise(&recover(&once));
        assert_eq!(once, twice, "quantisation is idempotent on its own output");
    }

    #[test]
    fn the_norm_is_the_recovered_vectors_norm() {
        let vector: Vec<f32> = (0..384)
            .map(|i| ((i * 31 % 97) as f32 / 97.0) - 0.5)
            .collect();
        let q = quantise(&vector);
        let by_floats: f64 = recover(&q)
            .iter()
            .map(|x| f64::from(*x) * f64::from(*x))
            .sum::<f64>()
            .sqrt();
        assert!(
            (norm(&q) - by_floats).abs() < 1e-5,
            "{} vs {by_floats}",
            norm(&q)
        );
        // A row's cosine with itself, in the arithmetic the scan uses, is one.
        let raw: Vec<u8> = q.codes.iter().map(|c| *c as u8).collect();
        let cosine = dot(&q, &raw, f64::from(q.scale)) / (norm(&q) * norm(&q));
        assert!((cosine - 1.0).abs() < 1e-12, "{cosine}");
    }

    #[test]
    fn the_integer_dot_product_matches_the_recovered_one() {
        let a: Vec<f32> = (0..384)
            .map(|i| ((i * 31 % 97) as f32 / 97.0) - 0.5)
            .collect();
        let b: Vec<f32> = (0..384)
            .map(|i| ((i * 53 % 89) as f32 / 89.0) - 0.5)
            .collect();
        let (qa, qb) = (quantise(&a), quantise(&b));
        let raw: Vec<u8> = qb.codes.iter().map(|c| *c as u8).collect();
        let integer = dot(&qa, &raw, f64::from(qb.scale)) as f32;
        let recovered: f32 = recover(&qa)
            .iter()
            .zip(recover(&qb))
            .map(|(x, y)| x * y)
            .sum();
        assert!(
            (integer - recovered).abs() < 1e-4,
            "integer {integer} against recovered {recovered}"
        );
    }

    #[test]
    fn the_dimension_bound_is_where_the_accumulator_would_overflow() {
        assert_eq!(MAX_DIM, 132_104);
        // Against the worst byte a row file can hold, not the worst code the engine writes.
        assert!(i32::try_from(MAX_DIM as u64 * 127 * 128).is_ok());
        assert!(i32::try_from((MAX_DIM as u64 + 1) * 127 * 128).is_err());
        let query = Quantised {
            codes: vec![127; MAX_DIM],
            scale: 1.0,
        };
        let worst_row = vec![0x80u8; MAX_DIM]; // every byte −128
        assert_eq!(
            dot(&query, &worst_row, 1.0),
            -(MAX_DIM as f64) * 127.0 * 128.0
        );
    }
}
