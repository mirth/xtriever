//! Property tests for the chunker (contracts/chunker.md "Properties"): tiling, bound,
//! determinism, text idempotence — for generated bodies and budgets, cost = words.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use xtriever_analysis::chunk::chunk;

fn word_cost(s: &str) -> usize {
    s.split_whitespace().count()
}

fn body() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        3 => "[a-z]{1,8}",
        1 => "[A-Z][a-z]{1,5}[.!?]",
        1 => Just(" ".to_owned()),
        1 => Just("\n".to_owned()),
        1 => Just("\n\n".to_owned()),
        1 => Just("\t".to_owned()),
        1 => Just("東京は日本の首都です。".to_owned()),
        1 => Just("café".to_owned()),
        1 => Just("\u{3000}".to_owned()),
    ];
    proptest::collection::vec(piece, 0..40).prop_map(|v| v.join(" "))
}

fn is_ws(c: char) -> bool {
    c.is_whitespace()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn tiling(body in body(), budget in 1usize..=64) {
        let passages = chunk(&body, budget, &word_cost);
        let mut covered = vec![false; body.len()];
        let mut last_end = 0u64;
        for p in &passages {
            let (s, e) = p.byte_range;
            prop_assert!(s < e, "empty range {:?}", p.byte_range);
            prop_assert!(s >= last_end, "ranges must be increasing and non-overlapping");
            prop_assert!(body.is_char_boundary(s as usize) && body.is_char_boundary(e as usize));
            for b in s..e { covered[b as usize] = true; }
            last_end = e;
        }
        for (i, c) in body.char_indices() {
            if !is_ws(c) {
                prop_assert!(covered[i], "non-whitespace byte {i} ({c:?}) not covered");
            }
        }
    }

    #[test]
    fn bound(body in body(), budget in 1usize..=64) {
        for p in chunk(&body, budget, &word_cost) {
            prop_assert!(p.cost <= budget || word_cost(&p.text) == 1, "{:?} cost {} > {}", p.text, p.cost, budget);
        }
    }

    #[test]
    fn determinism(body in body(), budget in 1usize..=64) {
        let a = chunk(&body, budget, &word_cost);
        let b = chunk(&body, budget, &word_cost);
        prop_assert_eq!(a, b);
    }

    #[test]
    fn text_idempotence_modulo_the_paragraph_joiner(body in body(), budget in 1usize..=64) {
        for p in chunk(&body, budget, &word_cost) {
            if p.cost > budget { continue; } // a lone over-budget fragment re-splits, by design
            let again = chunk(&p.text, budget, &word_cost);
            prop_assert_eq!(again.len(), 1, "re-chunking {:?} gave {} passages", p.text, again.len());
            prop_assert_eq!(&again[0].text, &p.text.replace('\n', " "));
        }
    }
}

#[test]
fn budget_zero_and_empty_body_give_nothing() {
    assert!(chunk("anything at all", 0, &word_cost).is_empty());
    assert!(chunk("", 5, &word_cost).is_empty());
    assert!(chunk("   \n\n \t ", 5, &word_cost).is_empty());
}
