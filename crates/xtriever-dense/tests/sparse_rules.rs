//! Feature 027 (User Story 2): the sparse module's rules that need no model — the `_sparse` field
//! text (research D4), the query-term rule (research D7) and the refusals. Runs in CI.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::Path;

use proptest::prelude::*;
use sha2::{Digest, Sha256};
use xtriever_core::Error;
use xtriever_dense::LoadPath;
use xtriever_dense::model::SPARSE_MODEL_NAME;
use xtriever_dense::sparse::{
    Expansion, MAX_SCALE, MAX_WEIGHT, SPECIAL_IDS, SparseEncoder, SparseQuery, VOCABULARY_SIZE,
    field_text,
};

fn expansion(entries: &[(u32, f32)]) -> Expansion {
    Expansion {
        entries: entries.to_vec(),
        truncated: false,
    }
}

#[test]
fn field_text_repeats_each_term_by_its_rounded_weight() {
    // 0.26 × 10 = 2.6 → 3; 0.04 × 10 = 0.4 → dropped; 1.0 × 10 → 10.
    let e = expansion(&[(5, 0.26), (7, 0.04), (42, 1.0)]);
    let mut want = vec!["s5"; 3];
    want.extend(vec!["s42"; 10]);
    assert_eq!(field_text(&e, 10).unwrap(), want.join(" "));
}

#[test]
fn field_text_rounds_halves_away_from_zero() {
    // Exact halves in binary: 0.25 × 2 = 0.5 → 1, 0.75 × 2 = 1.5 → 2, 1.25 × 2 = 2.5 → 3.
    let e = expansion(&[(1, 0.25), (2, 0.75), (3, 1.25)]);
    assert_eq!(field_text(&e, 2).unwrap(), "s1 s2 s2 s3 s3 s3");
}

#[test]
fn field_text_of_nothing_is_empty() {
    assert_eq!(field_text(&expansion(&[]), 10).unwrap(), "");
    assert_eq!(
        field_text(&expansion(&[(9, 0.01), (10, 0.049)]), 10).unwrap(),
        ""
    );
}

#[test]
fn field_text_uses_single_spaces_and_ascending_ids() {
    let text = field_text(&expansion(&[(3, 0.2), (200, 0.3), (30_521, 0.1)]), 10).unwrap();
    assert!(!text.starts_with(' ') && !text.ends_with(' ') && !text.contains("  "));
    assert_eq!(text, "s3 s3 s200 s200 s200 s30521");
}

fn refused(e: &Expansion, scale: u32) -> String {
    match field_text(e, scale) {
        Err(Error::Schema(message)) => message,
        other => panic!("expected Schema, got {other:?}"),
    }
}

/// Review finding: a caller-built expansion (an evaluation cache, `add_encoded`) is refused,
/// never repaired — a repeated or out-of-order id would silently change a term frequency.
#[test]
fn field_text_refuses_ids_that_are_not_strictly_ascending() {
    let m = refused(&expansion(&[(42, 0.3), (42, 0.3)]), 10);
    assert!(m.contains("entry 1") && m.contains("token 42"), "{m}");
    let m = refused(&expansion(&[(7, 0.3), (5, 0.3)]), 10);
    assert!(
        m.contains("token 5") && m.contains("follows token 7"),
        "{m}"
    );
}

#[test]
fn field_text_refuses_weights_the_encoder_cannot_produce() {
    for bad in [0.0, -0.5, f32::NAN, f32::INFINITY, MAX_WEIGHT + 0.001, 1e6] {
        let m = refused(&expansion(&[(1, 0.2), (9, bad)]), 10);
        assert!(m.contains("entry 1") && m.contains("token 9"), "{bad}: {m}");
    }
    assert!(field_text(&expansion(&[(1, MAX_WEIGHT)]), 10).is_ok());
}

/// Review round 2: an id no encoder output can hold — a special token, or one beyond the
/// vocabulary — is refused, not written as a term no query ever produces.
#[test]
fn field_text_refuses_special_and_out_of_vocabulary_ids() {
    for id in SPECIAL_IDS {
        let m = refused(&expansion(&[(id, 0.3)]), 10);
        assert!(
            m.contains(&format!("token {id}")) && m.contains("special"),
            "{m}"
        );
    }
    for id in [VOCABULARY_SIZE, u32::MAX] {
        let m = refused(&expansion(&[(1, 0.3), (id, 0.3)]), 10);
        assert!(m.contains("entry 1") && m.contains("vocabulary"), "{m}");
    }
    assert!(field_text(&expansion(&[(VOCABULARY_SIZE - 1, 0.3)]), 10).is_ok());
}

/// The special ids `validate` refuses are the pinned tokenizer's, as the reference recorded
/// them (`reference/fixtures/027/documents.json`); the encoder also refuses a tokenizer that
/// disagrees at load.
#[test]
fn special_ids_are_the_reference_tokenizers() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/027/documents.json");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let recorded: Vec<u32> = serde_json::from_value(doc["special_ids"].clone()).unwrap();
    assert_eq!(recorded, SPECIAL_IDS);
}

/// Review finding: no input may make the field text unboundedly long.
#[test]
fn field_text_refuses_a_scale_outside_its_range() {
    let e = expansion(&[(1, 0.5)]);
    for bad in [0, MAX_SCALE + 1, u32::MAX] {
        assert!(refused(&e, bad).contains(&bad.to_string()));
    }
    let longest = field_text(&expansion(&[(1, MAX_WEIGHT)]), MAX_SCALE).unwrap();
    assert_eq!(longest.split(' ').count(), 4_500);
}

/// The weight bound is the encoder's own: `ln(1 + ln(1 + x))` of the largest finite logit.
#[test]
fn max_weight_bounds_every_weight_the_encoder_can_produce() {
    let largest = f64::from(f32::MAX).ln_1p().ln_1p() as f32;
    assert!(largest <= MAX_WEIGHT, "{largest}");
    assert!(MAX_WEIGHT - largest < 0.01, "{largest}: the bound is loose");
}

fn counts(text: &str) -> BTreeMap<u32, u64> {
    let mut out = BTreeMap::new();
    for term in text.split(' ').filter(|t| !t.is_empty()) {
        *out.entry(term[1..].parse().unwrap()).or_insert(0) += 1;
    }
    out
}

proptest! {
    /// Every entry's occurrence count is `round(weight × scale)`, halves away from zero, for
    /// every scale from 1 to 100; zero-count entries are absent.
    #[test]
    fn field_text_counts_are_the_rounded_scaled_weights(
        weights in proptest::collection::btree_map(
            (0u32..VOCABULARY_SIZE).prop_filter("not special", |id| !SPECIAL_IDS.contains(id)),
            0.000_01f32..4.0,
            0..64,
        ),
        scale in 1u32..=100,
    ) {
        let e = expansion(&weights.iter().map(|(&i, &w)| (i, w)).collect::<Vec<_>>());
        let got = counts(&field_text(&e, scale).unwrap());
        let want: BTreeMap<u32, u64> = weights
            .iter()
            .filter_map(|(&i, &w)| {
                let n = (f64::from(w) * f64::from(scale)).round() as u64;
                (n > 0).then_some((i, n))
            })
            .collect();
        prop_assert_eq!(got, want);
    }
}

// ── the query side, on a synthetic tokenizer and table ────────────────────────────────────

/// A word-level tokenizer with BERT's five special tokens and a few words.
const TOKENIZER: &str = r#"{
  "version": "1.0",
  "truncation": null,
  "padding": null,
  "added_tokens": [
    {"id": 0, "content": "[PAD]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true},
    {"id": 1, "content": "[UNK]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true},
    {"id": 2, "content": "[CLS]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true},
    {"id": 3, "content": "[SEP]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true},
    {"id": 4, "content": "[MASK]", "single_word": false, "lstrip": false, "rstrip": false, "normalized": false, "special": true}
  ],
  "normalizer": {"type": "Lowercase"},
  "pre_tokenizer": {"type": "Whitespace"},
  "post_processor": null,
  "decoder": null,
  "model": {
    "type": "WordLevel",
    "vocab": {"[PAD]": 0, "[UNK]": 1, "[CLS]": 2, "[SEP]": 3, "[MASK]": 4, "the": 5, "cat": 6, "dog": 7, "fish": 8, "zebra": 9},
    "unk_token": "[UNK]"
  }
}"#;

/// Every token has an entry; `the` and `zebra` have none worth keeping.
const TABLE: &str = r#"{"[PAD]": 1.0, "[UNK]": 1.0, "[CLS]": 1.0, "[SEP]": 1.0, "[MASK]": 1.0,
  "the": 0.0, "cat": 2.5, "dog": 1.5, "fish": 0.75, "zebra": 0.0}"#;

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn write_side(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let tokenizer = dir.join("tokenizer.json");
    let table = dir.join("query-table.json");
    std::fs::write(&tokenizer, TOKENIZER).unwrap();
    std::fs::write(&table, TABLE).unwrap();
    (tokenizer, table)
}

fn open_side(dir: &Path) -> SparseQuery {
    let (tokenizer, table) = write_side(dir);
    SparseQuery::open(
        &tokenizer,
        &table,
        &sha256(TOKENIZER.as_bytes()),
        &sha256(TABLE.as_bytes()),
    )
    .unwrap()
}

#[test]
fn query_terms_are_distinct_positive_non_special_and_ascending() {
    let tmp = tempfile::tempdir().unwrap();
    let side = open_side(tmp.path());
    // fish(8) dog(7) cat(6) repeated; `the` and `zebra` have no positive entry; `[CLS]` and
    // `[SEP]` are special; `unicorn` is unknown, so `[UNK]`, also special.
    assert_eq!(
        side.terms("[CLS] fish dog the Cat cat zebra unicorn DOG [SEP]")
            .unwrap(),
        vec![6, 7, 8]
    );
    assert_eq!(side.terms("").unwrap(), Vec::<u32>::new());
    assert_eq!(side.terms("the zebra").unwrap(), Vec::<u32>::new());
}

#[test]
fn query_side_refuses_a_tokenizer_whose_hash_differs() {
    let tmp = tempfile::tempdir().unwrap();
    let (tokenizer, table) = write_side(tmp.path());
    let wrong = sha256(b"something else");
    let err = SparseQuery::open(&tokenizer, &table, &wrong, &sha256(TABLE.as_bytes())).unwrap_err();
    match err {
        Error::Corrupt(message) => {
            assert!(message.contains("tokenizer.json"), "{message}");
            assert!(message.contains(&wrong), "{message}");
            assert!(message.contains(&sha256(TOKENIZER.as_bytes())), "{message}");
        }
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

#[test]
fn query_side_refuses_a_table_whose_hash_differs() {
    let tmp = tempfile::tempdir().unwrap();
    let (tokenizer, table) = write_side(tmp.path());
    let wrong = sha256(b"another table");
    let err =
        SparseQuery::open(&tokenizer, &table, &sha256(TOKENIZER.as_bytes()), &wrong).unwrap_err();
    match err {
        Error::Corrupt(message) => {
            assert!(message.contains("query-table.json"), "{message}");
            assert!(message.contains(&wrong), "{message}");
            assert!(message.contains(&sha256(TABLE.as_bytes())), "{message}");
        }
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

#[test]
fn encoder_refuses_a_missing_directory_by_file() {
    let tmp = tempfile::tempdir().unwrap();
    let err = SparseEncoder::load(&tmp.path().join("absent"), LoadPath::Buffered).unwrap_err();
    match err {
        Error::Model { model, message } => {
            assert_eq!(model, SPARSE_MODEL_NAME);
            assert!(message.contains("config.json"), "{message}");
        }
        other => panic!("expected Model, got {other:?}"),
    }
}

#[test]
fn encoder_refuses_a_file_of_the_wrong_size_before_parsing_it() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("config.json"), "{}").unwrap();
    let err = SparseEncoder::load(tmp.path(), LoadPath::Buffered).unwrap_err();
    match err {
        Error::Model { model, message } => {
            assert_eq!(model, SPARSE_MODEL_NAME);
            assert!(
                message.contains("config.json") && message.contains("2 bytes"),
                "{message}"
            );
        }
        other => panic!("expected Model, got {other:?}"),
    }
}
