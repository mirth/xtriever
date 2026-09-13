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
