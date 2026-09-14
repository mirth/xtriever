//! Golden test for the chunker (specs/008-wiki-corpus/contracts/chunker.md): the Python
//! reference's passages, byte for byte — text, byte range and cost — under two cost models.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use xtriever_analysis::chunk::chunk;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/008")
}

#[derive(Deserialize)]
struct Golden {
    text: String,
    byte_range: (u64, u64),
    cost: usize,
}

#[derive(Deserialize)]
struct CaseA {
    name: String,
    budget: usize,
    body: String,
    passages: Vec<Golden>,
}

#[derive(Deserialize)]
struct SetB {
    articles: Vec<ArticleB>,
    unit_costs: BTreeMap<String, usize>,
}

#[derive(Deserialize)]
struct ArticleB {
    id: String,
    title: String,
    budget: usize,
    body: String,
    passages: Vec<Golden>,
}

#[derive(Deserialize)]
struct Manifest {
    files: BTreeMap<String, String>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn verified(name: &str) -> String {
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("manifest.json")).unwrap())
            .unwrap();
    let bytes = std::fs::read(fixtures().join(name)).unwrap();
    assert_eq!(
        sha256_hex(&bytes),
        manifest.files[name],
        "{name} does not match reference/fixtures/008/manifest.json — regenerate, do not edit"
    );
    String::from_utf8(bytes).unwrap()
}

fn assert_same(label: &str, got: &[xtriever_analysis::chunk::Passage], want: &[Golden]) {
    let got_view: Vec<(&str, (u64, u64), usize)> = got
        .iter()
        .map(|p| (p.text.as_str(), p.byte_range, p.cost))
        .collect();
    let want_view: Vec<(&str, (u64, u64), usize)> = want
        .iter()
        .map(|p| (p.text.as_str(), p.byte_range, p.cost))
        .collect();
    assert_eq!(got_view, want_view, "{label}");
}

#[test]
fn set_a_word_cost_matches_the_reference_byte_for_byte() {
    let cases: Vec<CaseA> = serde_json::from_str(&verified("chunk_a.json")).unwrap();
    assert_eq!(cases.len(), 48);
    let cost = |s: &str| s.split_whitespace().count();
    for case in &cases {
        let got = chunk(&case.body, case.budget, &cost);
        assert_same(
            &format!("{} @ budget {}", case.name, case.budget),
            &got,
            &case.passages,
        );
    }
}

#[test]
fn set_b_word_piece_cost_matches_the_reference_byte_for_byte() {
    let set: SetB = serde_json::from_str(&verified("chunk_b.json")).unwrap();
    assert_eq!(set.articles.len(), 9);
    let cost = |s: &str| {
        *set.unit_costs.get(s).unwrap_or_else(|| {
            panic!("the Rust chunker priced a unit the reference never did: {s:?}")
        })
    };
    for article in &set.articles {
        let got = chunk(&article.body, article.budget, &cost);
        assert_same(
            &format!("{} ({})", article.title, article.id),
            &got,
            &article.passages,
        );
    }
}
