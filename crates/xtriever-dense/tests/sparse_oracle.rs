//! Feature 027 (User Story 2): the sparse encoder's expansions and the query side against the
//! PyTorch reference, `reference/fixtures/027/` (written by `reference/gen_027_fixtures.py`).
//!
//! Tolerances come from the fixture's manifest (research D9): every weight within 1e-4 absolute;
//! the kept entries equal apart from reference weights below that; every term frequency at scale
//! 10 equal apart from entries whose reference `weight × 10` lies within 1e-3 of a half; every
//! query's terms equal exactly.
//!
//! Needs the pinned encoder (`scripts/fetch-model.sh --manifest
//! reference/models/manifest-sparse-doc-v3.json`), so every test is ignored by default:
//!
//!     cargo nextest run -p xtriever-dense --run-ignored all -E 'binary(sparse_oracle)'
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;
use xtriever_dense::LoadPath;
use xtriever_dense::model::{PINNED_SPARSE, SPARSE_IDENTITY};
use xtriever_dense::sparse::{Expansion, SparseEncoder, SparseQuery, field_text};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/027")
}

/// The git-ignored encoder directory, overridable with `XTRIEVER_SPARSE_MODEL_DIR`.
fn encoder_dir() -> PathBuf {
    std::env::var_os("XTRIEVER_SPARSE_MODEL_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference/models/opensearch-neural-sparse-encoding-doc-v3-distill")
        },
        PathBuf::from,
    )
}

fn load<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = fixtures_dir().join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[derive(Deserialize)]
struct Documents {
    max_tokens: usize,
    documents: Vec<Document>,
}

#[derive(Deserialize)]
struct Document {
    id: String,
    text: String,
    input_ids: Vec<u32>,
    untruncated_tokens: usize,
    truncated: bool,
}

#[derive(Deserialize)]
struct Expansions {
    expansions: BTreeMap<String, Vec<(u32, f32)>>,
}

#[derive(Deserialize)]
struct Queries {
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Query {
    id: String,
    text: String,
    terms: Vec<u32>,
}

#[derive(Deserialize)]
struct Manifest {
    model: String,
    tolerance: Tolerance,
}

#[derive(Deserialize)]
struct Tolerance {
    weight_abs: f64,
    scale: u32,
    half_margin: f64,
}

/// One encoder for the whole binary: it is 268 MB and takes seconds to build.
fn encoder() -> &'static SparseEncoder {
    static ENCODER: OnceLock<SparseEncoder> = OnceLock::new();
    ENCODER.get_or_init(|| {
        SparseEncoder::load(&encoder_dir(), LoadPath::Buffered).expect("load the sparse encoder")
    })
}

/// Occurrences per token id in a `_sparse` field value.
fn occurrences(text: &str) -> BTreeMap<u32, u32> {
    let mut counts = BTreeMap::new();
    for term in text.split(' ').filter(|t| !t.is_empty()) {
        let id: u32 = term
            .strip_prefix('s')
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("term {term:?} is not s<id>"));
        *counts.entry(id).or_insert(0) += 1;
    }
    counts
}

#[test]
fn fixture_names_the_pinned_encoder() {
    let manifest: Manifest = load("manifest.json");
    assert_eq!(
        manifest.model,
        format!("{}@{}", PINNED_SPARSE.repository, PINNED_SPARSE.revision)
    );
    let documents: Documents = load("documents.json");
    assert_eq!(documents.max_tokens, PINNED_SPARSE.max_tokens);
}

#[test]
#[ignore = "needs the sparse encoder"]
fn identity_is_the_pinned_one() {
    assert_eq!(encoder().identity(), SPARSE_IDENTITY);
}

#[test]
#[ignore = "needs the sparse encoder"]
fn tokenization_matches_the_reference() {
    let documents: Documents = load("documents.json");
    for doc in &documents.documents {
        assert_eq!(
            encoder().token_ids(&doc.text).unwrap(),
            doc.input_ids,
            "{}: token ids differ",
            doc.id
        );
    }
}

#[test]
#[ignore = "needs the sparse encoder"]
fn weights_match_the_reference() {
    let documents: Documents = load("documents.json");
    let expected: Expansions = load("expansions.json");
    let manifest: Manifest = load("manifest.json");
    let tol = &manifest.tolerance;

    let mut worst = (0.0_f64, String::new());
    for doc in &documents.documents {
        let got = encoder().encode(&doc.text).unwrap();
        assert_eq!(
            got.truncated, doc.truncated,
            "{}: truncated ({} tokens untruncated)",
            doc.id, doc.untruncated_tokens
        );
        assert!(
            got.entries.windows(2).all(|w| w[0].0 < w[1].0),
            "{}: entries not strictly ascending by id",
            doc.id
        );
        assert!(
            got.entries.iter().all(|&(_, w)| w > 0.0 && w.is_finite()),
            "{}: a kept weight is not finite and positive",
            doc.id
        );

        let ours: BTreeMap<u32, f32> = got.entries.iter().copied().collect();
        let theirs: BTreeMap<u32, f32> = expected.expansions[&doc.id].iter().copied().collect();
        for (&id, &want) in &theirs {
            match ours.get(&id) {
                Some(&have) => {
                    let diff = (f64::from(have) - f64::from(want)).abs();
                    if diff > worst.0 {
                        worst = (diff, format!("{} token {id}", doc.id));
                    }
                    assert!(
                        diff <= tol.weight_abs,
                        "{} token {id}: weight {have} vs reference {want} (|Δ| {diff:e})",
                        doc.id
                    );
                }
                None => assert!(
                    f64::from(want) < tol.weight_abs,
                    "{} token {id}: kept by the reference at {want}, missing here",
                    doc.id
                ),
            }
        }
        for (&id, &have) in &ours {
            if !theirs.contains_key(&id) {
                assert!(
                    f64::from(have) < tol.weight_abs,
                    "{} token {id}: kept here at {have}, absent from the reference",
                    doc.id
                );
            }
        }
    }
    eprintln!("largest weight difference: {:e} ({})", worst.0, worst.1);
}

#[test]
#[ignore = "needs the sparse encoder"]
fn term_frequencies_match_the_reference_at_the_default_scale() {
    let documents: Documents = load("documents.json");
    let expected: Expansions = load("expansions.json");
    let manifest: Manifest = load("manifest.json");
    let tol = &manifest.tolerance;

    for doc in &documents.documents {
        let reference = Expansion {
            entries: expected.expansions[&doc.id].clone(),
            truncated: doc.truncated,
        };
        let ours = occurrences(&field_text(
            &encoder().encode(&doc.text).unwrap(),
            tol.scale,
        ));
        let theirs = occurrences(&field_text(&reference, tol.scale));
        let boundary: BTreeMap<u32, bool> = reference
            .entries
            .iter()
            .map(|&(id, w)| {
                let scaled = f64::from(w) * f64::from(tol.scale);
                (id, (scaled - scaled.floor() - 0.5).abs() < tol.half_margin)
            })
            .collect();
        let ids: std::collections::BTreeSet<u32> =
            ours.keys().chain(theirs.keys()).copied().collect();
        for id in ids {
            if boundary.get(&id).copied().unwrap_or(false) {
                continue;
            }
            assert_eq!(
                ours.get(&id),
                theirs.get(&id),
                "{} token {id}: term frequency at scale {}",
                doc.id,
                tol.scale
            );
        }
    }
}

#[test]
#[ignore = "needs the sparse encoder"]
fn query_terms_match_the_reference_exactly() {
    let queries: Queries = load("queries.json");
    let dir = encoder_dir();
    let side = SparseQuery::open(
        &dir.join(PINNED_SPARSE.files[1].name),
        &dir.join(PINNED_SPARSE.files[3].name),
        PINNED_SPARSE.files[1].sha256,
        PINNED_SPARSE.files[3].sha256,
    )
    .unwrap();
    for q in &queries.queries {
        assert_eq!(
            side.terms(&q.text).unwrap(),
            q.terms,
            "{}: {:?}",
            q.id,
            q.text
        );
    }
}

#[test]
#[ignore = "needs the sparse encoder"]
fn encoding_is_deterministic() {
    use sha2::{Digest, Sha256};

    let documents: Documents = load("documents.json");
    let run = || {
        let mut hasher = Sha256::new();
        for doc in &documents.documents {
            let e = encoder().encode(&doc.text).unwrap();
            hasher.update([u8::from(e.truncated)]);
            for (id, w) in e.entries {
                hasher.update(id.to_le_bytes());
                hasher.update(w.to_bits().to_le_bytes());
            }
        }
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let first = run();
    assert_eq!(first, run(), "two encodings of the fixtures differ");
    // Compared across thread counts by hand (quickstart §1): run once more with
    // RAYON_NUM_THREADS=1 and --no-capture, and compare this line.
    eprintln!(
        "expansion digest: {first} (threads: {})",
        xtriever_dense::MiniLmEmbedder::thread_count()
    );
}
