//! `wiki::rules` — the manifest's exclusion rules, applied in order, first match wins (spec
//! FR-003; research D3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use xtriever_cli::wiki::rules::{Rule, excluded_by};

fn rules() -> Vec<Rule> {
    vec![
        Rule::TitleSuffix {
            value: " (disambiguation)".to_owned(),
        },
        Rule::LeadContains {
            value: "may refer to".to_owned(),
            within_chars: 300,
        },
        Rule::LeadContains {
            value: "may mean".to_owned(),
            within_chars: 300,
        },
    ]
}

#[test]
fn title_suffix_is_rule_zero() {
    assert_eq!(
        excluded_by(&rules(), "Flame (disambiguation)", "Flame may refer to:"),
        Some(0)
    );
    assert_eq!(excluded_by(&rules(), "Flame", "Fire is hot."), None);
}

#[test]
fn lead_contains_counts_characters_not_bytes_and_stops_at_the_window() {
    let r = rules();
    let at_250 = format!("{}may refer to: things", "é".repeat(250));
    assert_eq!(
        excluded_by(&r, "X", &at_250),
        Some(1),
        "250 chars in (500 bytes) is inside 300 chars"
    );
    let at_350 = format!("{}may refer to: things", "a".repeat(350));
    assert_eq!(excluded_by(&r, "X", &at_350), None);
    assert_eq!(
        excluded_by(&r, "X", "Foo may mean several things."),
        Some(2)
    );
    // Straddling the window boundary counts if the phrase starts inside it.
    let straddle = format!("{}may refer to", "a".repeat(290));
    assert_eq!(excluded_by(&r, "X", &straddle), Some(1));
}

#[test]
fn the_first_matching_rule_is_the_one_counted() {
    assert_eq!(
        excluded_by(&rules(), "Bank (disambiguation)", "Bank may mean:"),
        Some(0)
    );
    assert_eq!(
        excluded_by(&rules(), "Bank", "Bank may refer to: it may mean:"),
        Some(1)
    );
}

#[test]
fn rules_round_trip_through_the_manifest_json() {
    let json = r#"[
        { "rule": "title_suffix", "value": " (disambiguation)" },
        { "rule": "lead_contains", "value": "may refer to", "within_chars": 300 }
    ]"#;
    let parsed: Vec<Rule> = serde_json::from_str(json).unwrap();
    assert_eq!(parsed[..], rules()[..2]);
}
