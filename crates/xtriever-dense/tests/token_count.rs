//! Feature 008 D7: `MiniLmEmbedder::token_count` — the untruncated sequence length a text
//! would need, `[CLS]`/`[SEP]` included; content pieces are additive over whitespace. Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use xtriever_dense::{LoadPath, MiniLmEmbedder};

fn load(path: LoadPath) -> MiniLmEmbedder {
    MiniLmEmbedder::load(&support::model_dir(), path).expect("load pinned model")
}

#[test]
#[ignore = "needs the model"]
fn counts_include_the_special_tokens_and_never_truncate() {
    let paths = [
        LoadPath::Buffered,
        #[cfg(feature = "mmap")]
        LoadPath::Mmap,
    ];
    for path in paths {
        let e = load(path);
        assert_eq!(
            e.token_count("").unwrap(),
            2,
            "{path:?}: empty text is [CLS] [SEP]"
        );
        assert_eq!(e.token_count("hello world").unwrap(), 4, "{path:?}");
        let long = "word ".repeat(3_000);
        let n = e.token_count(&long).unwrap();
        assert!(n > 256, "{path:?}: 3,000 words counted as {n} — truncated");
        assert_eq!(n, 3_002);
    }
}

#[test]
#[ignore = "needs the model"]
fn content_pieces_are_additive_over_a_space() {
    let e = load(LoadPath::Buffered);
    let pairs = [
        ("Alan Turing", "was a mathematician."),
        ("東京は日本の首都です。", "人口が多い。"),
        (
            "https://publikationen.%5B%5Buni-tuebingen%5D%5D.de/xmlui",
            "bitstream",
        ),
        ("café résumé", "naïve"),
        ("Pneumonoultramicroscopicsilicovolcanoconiosis", "is long."),
        ("1,234.56", "km/h"),
        ("Mr.", "Smith"),
        ("ACGTACGTACGT", "ACGT"),
        ("(disambiguation)", "may refer to:"),
        ("x", "y"),
    ];
    for (a, b) in pairs {
        let joined = format!("{a} {b}");
        assert_eq!(
            e.token_count(a).unwrap() + e.token_count(b).unwrap() - 2,
            e.token_count(&joined).unwrap(),
            "{a:?} + {b:?}"
        );
    }
}

#[derive(serde::Deserialize)]
struct SetB {
    unit_costs: BTreeMap<String, usize>,
}

#[test]
#[ignore = "needs the model"]
fn matches_the_python_reference_unit_costs() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../reference/fixtures/008/chunk_b.json");
    let set: SetB = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let e = load(LoadPath::Buffered);
    assert!(set.unit_costs.len() > 500);
    for (unit, want) in &set.unit_costs {
        assert_eq!(e.token_count(unit).unwrap() - 2, *want, "unit {unit:?}");
    }
}
