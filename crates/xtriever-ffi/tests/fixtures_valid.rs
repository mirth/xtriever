//! Fixture integrity — the discriminator that makes "fails for the right reason" mechanical.
//!
//! This file is deliberately **not** gated on the `spike` feature and must be **GREEN** at the
//! T017 checkpoint. Its whole purpose is to separate the two failure causes:
//!
//! * these tests green + the others red  =>  the oracles are sound and the implementation is
//!   missing, which is exactly what FR-013 requires;
//! * these tests red                     =>  the fixtures are broken, and any red elsewhere is
//!   uninterpretable.
//!
//! It also makes the goldens tamper-evident: `manifest.json` records a SHA-256 per file, so
//! hand-editing a fixture to make a test pass turns this red and names the file (FR-028).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use sha2_lite::sha256_hex;
use support::{as_f32_vec, as_u32_vec, fixtures_dir, load};

/// Minimal SHA-256 so the manifest check needs no extra dependency in the iOS graph.
mod sha2_lite {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];

    /// Hex-encoded SHA-256 of `data`.
    pub fn sha256_hex(data: &[u8]) -> String {
        let mut h: [u32; 8] = [
            0x6a09_e667,
            0xbb67_ae85,
            0x3c6e_f372,
            0xa54f_f53a,
            0x510e_527f,
            0x9b05_688c,
            0x1f83_d9ab,
            0x5be0_cd19,
        ];
        let mut msg = data.to_vec();
        let bit_len = (data.len() as u64) * 8;
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bit_len.to_be_bytes());

        for block in msg.chunks_exact(64) {
            let mut w = [0u32; 64];
            for (i, c) in block.chunks_exact(4).enumerate() {
                w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }
            let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
            let (mut e, mut f, mut g, mut hh) = (h[4], h[5], h[6], h[7]);
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let t1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let t2 = s0.wrapping_add(maj);
                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(t1);
                d = c;
                c = b;
                b = a;
                a = t1.wrapping_add(t2);
            }
            for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
                *slot = slot.wrapping_add(v);
            }
        }
        h.iter().map(|w| format!("{w:08x}")).collect()
    }
}

#[test]
fn manifest_hashes_match_every_fixture() {
    let manifest = load("manifest.json");
    let files = manifest["files"].as_object().expect("files map");
    assert!(!files.is_empty(), "manifest records no files");
    for (name, expected) in files {
        let bytes = std::fs::read(fixtures_dir().join(name)).expect("fixture readable");
        let actual = sha256_hex(&bytes);
        let expected = expected.as_str().expect("hash string");
        if actual == expected {
            continue;
        }

        // Before blaming the fixture, check the boring cause. Git on Windows rewrites LF to CRLF
        // on checkout unless `.gitattributes` says otherwise, which changes the bytes without
        // changing the content — and the first time this fired it sent the investigation looking
        // for a hand-edited golden that did not exist.
        let normalized = sha256_hex(
            &bytes
                .iter()
                .copied()
                .filter(|b| *b != b'\r')
                .collect::<Vec<u8>>(),
        );
        assert_ne!(
            normalized, expected,
            "{name} differs from its recorded hash ONLY by line endings — the working copy has \
             CRLF where the fixture is LF. This is Git translating on checkout, not a bad fixture. \
             Check that .gitattributes marks `reference/fixtures/** -text`, then re-checkout."
        );
        panic!(
            "{name} does not match its recorded hash (got {actual}, expected {expected}) and the \
             difference is not line endings — a fixture was hand-edited, or it was regenerated \
             without updating the manifest"
        );
    }
}

#[test]
fn corpus_is_well_formed() {
    let corpus = load("corpus.json");
    let docs = corpus["documents"].as_array().expect("documents");
    assert_eq!(docs.len(), 1000, "corpus must hold exactly 1,000 documents");

    let mut ids = std::collections::HashSet::new();
    for doc in docs {
        let id = doc["external_id"].as_str().expect("external_id");
        let text = doc["text"].as_str().expect("text");
        assert!(
            !id.is_empty() && ids.insert(id),
            "external_id must be unique and non-empty"
        );
        assert!(!text.is_empty(), "document text must be non-empty");
        assert!(
            text.is_ascii(),
            "corpus is ASCII-only so tokenization is not entangled with normalization"
        );
    }
    assert!(!corpus["query"].as_str().expect("query").is_empty());
    assert!(!corpus["sentence"].as_str().expect("sentence").is_empty());
    assert!(
        corpus["matching_documents"]
            .as_u64()
            .expect("matching_documents")
            >= 1,
        "the query must match at least one document, or an empty ranking would pass vacuously"
    );
}

#[test]
fn tokens_are_well_formed() {
    let tokens = load("tokens.json");
    let ids = as_u32_vec(&tokens["input_ids"]);
    let mask = as_u32_vec(&tokens["attention_mask"]);
    let types = as_u32_vec(&tokens["token_type_ids"]);

    assert_eq!(
        ids.len(),
        256,
        "truncation/padding must be overridden to 256, not tokenizer.json's 128"
    );
    assert_eq!(mask.len(), 256);
    assert_eq!(types.len(), 256);
    assert!(
        types.iter().all(|&t| t == 0),
        "single-segment sentence must have all-zero token_type_ids"
    );
    assert_eq!(ids[0], 101, "[CLS] must lead");
    assert!(ids.contains(&102), "[SEP] must be present");
    assert!(mask.contains(&1), "at least one real token");
}

#[test]
fn embedding_is_well_formed() {
    let embedding = load("embedding.json");
    let vector = as_f32_vec(&embedding["vector"]);
    assert_eq!(
        vector.len(),
        384,
        "all-MiniLM-L6-v2 produces 384 dimensions"
    );
    assert!(
        vector.iter().all(|v| v.is_finite()),
        "no NaN or infinity in the golden vector"
    );

    let norm: f64 = vector
        .iter()
        .map(|v| f64::from(*v) * f64::from(*v))
        .sum::<f64>()
        .sqrt();
    assert!(
        (norm - 1.0).abs() < 1e-5,
        "golden embedding must be L2-normalized, got norm {norm}"
    );

    assert!((embedding["cosine_min"].as_f64().expect("cosine_min") - 0.9999).abs() < 1e-12);
    assert!((embedding["max_abs_diff"].as_f64().expect("max_abs_diff") - 1e-3).abs() < 1e-12);
}

#[test]
fn bm25_reference_is_well_formed() {
    let bm25 = load("bm25_reference.json");
    assert_eq!(bm25["provenance"].as_str(), Some("python-independent-bm25"));

    let hits = bm25["hits"].as_array().expect("hits");
    assert_eq!(hits.len(), bm25["k"].as_u64().expect("k") as usize);

    let scores: Vec<f64> = hits
        .iter()
        .map(|h| h["score"].as_f64().expect("score"))
        .collect();
    assert!(
        scores.windows(2).all(|w| w[0] > w[1]),
        "hits must be strictly descending"
    );

    let tol = bm25["score_rel_tol"].as_f64().expect("score_rel_tol");
    let gap = bm25["min_top_k_score_gap"]
        .as_f64()
        .expect("min_top_k_score_gap");
    assert!(
        gap > tol * scores[0],
        "adjacent top-k scores ({gap}) are closer than the comparison tolerance, so their \
         order is float noise rather than a meaningful oracle"
    );
    assert!(
        bm25["corpus_stats"]["docs_with_lossy_fieldnorm"]
            .as_u64()
            .expect("lossy count")
            > 0,
        "no document has a lossy fieldnorm, so the u8 quantization transcription is never exercised"
    );
}

#[test]
fn model_manifest_pins_the_exact_weights() {
    let model = load("model.json");
    assert_eq!(model["weights_bytes"].as_u64(), Some(90_868_376));
    assert_eq!(
        model["dtype"].as_str(),
        Some("f32"),
        "FR-033 pins as-published 32-bit weights"
    );
    assert_eq!(
        model["revision"].as_str(),
        Some("1110a243fdf4706b3f48f1d95db1a4f5529b4d41")
    );
    assert_eq!(model["max_sequence_length"].as_u64(), Some(256));
    assert_eq!(model["embedding_dim"].as_u64(), Some(384));
    assert_eq!(model["weights_sha256"].as_str().expect("sha256").len(), 64);
}

#[test]
fn ranking_is_either_a_placeholder_or_fully_minted() {
    let ranking = load("ranking.json");
    let status = ranking["status"].as_str().expect("status");
    let hits = ranking["hits"].as_array().expect("hits");
    let k = ranking["k"].as_u64().expect("k") as usize;

    match status {
        // PR 1a/1b: the host-minted ranking cannot exist until spike_index/spike_query land.
        // Enforcing emptiness here is what stops the placeholder ever satisfying a real test.
        "pending_host_mint" => assert!(hits.is_empty(), "a pending ranking must carry no hits"),
        "minted" => {
            assert_eq!(hits.len(), k, "a minted ranking must carry exactly k hits");
            for hit in hits {
                assert!(hit["external_id"].is_string());
                assert!(hit["score"].is_number());
                assert!(hit["segment_ord"].is_number() && hit["doc_id"].is_number());
            }
        }
        other => panic!("unknown ranking status {other:?}"),
    }
    assert_eq!(
        ranking["writer_threads"].as_u64(),
        Some(1),
        "the golden is only reproducible at one writer thread"
    );
}
