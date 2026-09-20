//! Eight-bit quantisation of a dense vector (Feature 026, [ADR-0015]).
//!
//! One scale per vector, symmetric: `scale = max|component| / 127` and
//! `code = round(component / scale)`. Recovery is `code × scale`, and it is **approximate** —
//! this module never claims otherwise, and the stage keeps no float copy to fall back on
//! (spec FR-003).
//!
//! Measured on two datasets before the format changed: against the float ranking the scheme
//! costs 0.0006 nDCG@10 on SciFact and on NFCorpus, leaves Recall@100 unchanged, and agrees on
//! 99.5 % of the first hundred candidates (`reference/int8_vectors_study.py`).
//!
//! [ADR-0015]: https://github.com/mirth/xtriever/blob/main/docs/adr/0015-eight-bit-vectors-and-models.md

/// The largest magnitude a code may take. `-128` is never produced, so negating a code is exact.
pub(crate) const MAX_CODE: f32 = 127.0;

/// A quantised vector: the codes, and the scale that recovers them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Quantised {
    /// One code per dimension, each in `-127..=127`.
    pub codes: Vec<i8>,
    /// Strictly positive. Multiply a code by this to recover the component.
    pub scale: f32,
}

/// Quantise `vector`.
///
/// A vector of all zeros — which a degenerate embedding can be — gets scale `1.0` and all-zero
/// codes, so recovery gives back zeros instead of dividing by zero.
pub(crate) fn quantise(vector: &[f32]) -> Quantised {
    let peak = vector.iter().fold(0.0f32, |peak, v| peak.max(v.abs()));
    let scale = if peak > 0.0 { peak / MAX_CODE } else { 1.0 };
    let codes = vector
        .iter()
        .map(|v| {
            // `round` then clamp: the peak component lands exactly on ±127, and a value that
            // rounds past it (only reachable through a denormal scale) is held there.
            (v / scale).round().clamp(-MAX_CODE, MAX_CODE) as i8
        })
        .collect();
    Quantised { codes, scale }
}

/// Recover a quantised vector as floats. For tests and for the reference oracle; the scan never
/// materialises a row.
#[cfg(test)]
pub(crate) fn recover(quantised: &Quantised) -> Vec<f32> {
    quantised
        .codes
        .iter()
        .map(|c| f32::from(*c) * quantised.scale)
        .collect()
}

/// The dot product of a quantised query and a quantised row.
///
/// The products accumulate in `i32`: 384 terms of at most `127 × 127` reach about 6.2 million,
/// four orders of magnitude inside the type, so no saturation handling is needed.
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
    }

    #[test]
    fn the_scale_is_always_positive() {
        for vector in [vec![0.0], vec![-1.0, -2.0], vec![1e-30, -1e-30]] {
            assert!(quantise(&vector).scale > 0.0, "{vector:?}");
        }
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
}
