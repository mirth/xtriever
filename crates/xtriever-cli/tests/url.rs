//! `wiki::url::derive_url` — the article URL from the title (contracts/artefact.md; research
//! D6: verified exact against every article's `url` in the snapshot).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use xtriever_cli::wiki::url::derive_url;

#[test]
fn derives_the_snapshot_urls() {
    let base = "https://simple.wikipedia.org/wiki/";
    let cases = [
        ("April", "April"),
        ("Alan Turing", "Alan%20Turing"),
        ("Church (building)", "Church%20%28building%29"),
        ("Dutton's Speedwords", "Dutton%27s%20Speedwords"),
        ("AC/DC", "AC/DC"),
        ("Biel/Bienne", "Biel/Bienne"),
        ("Alliance 90/The Greens", "Alliance%2090/The%20Greens"),
        ("Café", "Caf%C3%A9"),
        ("a~b_c-d.e", "a~b_c-d.e"),
        ("東京", "%E6%9D%B1%E4%BA%AC"),
        ("100% sure?", "100%25%20sure%3F"),
    ];
    for (title, want) in cases {
        assert_eq!(derive_url(title), format!("{base}{want}"), "{title:?}");
    }
}
