//! The article URL from its title (contracts/artefact.md; research D6 — verified exact against
//! every `url` in the snapshot): percent-encode the title's UTF-8 bytes except the unreserved
//! set and `/`.

const BASE: &str = "https://simple.wikipedia.org/wiki/";

/// `https://simple.wikipedia.org/wiki/` + the percent-encoded title.
#[must_use]
pub fn derive_url(title: &str) -> String {
    let mut out = String::with_capacity(BASE.len() + title.len() * 3);
    out.push_str(BASE);
    for byte in title.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

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
}
