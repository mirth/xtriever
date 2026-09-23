//! Feature 027 (User Story 3): the two sparse configurations and the sparse-weights cache
//! (contract `surfaces-and-eval.md`). Model-free; runs in CI.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use xtriever_eval::Error;
use xtriever_eval::run::{
    CachedExpansion, HybridConfig, RerankConfig, SparseCacheKey, SparseProgress, SparseSettings,
    read_sparse_weights, write_sparse_weights,
};

const DEFAULT: SparseSettings = SparseSettings {
    scale: 10,
    boost: 1.0,
};

/// `hybrid-sparse-v1` is `hybrid-baseline-v2` plus the option at the spike's settings — the
/// same fields, depths and fusion, so the comparison measures the option alone.
#[test]
fn hybrid_sparse_v1_is_hybrid_baseline_v2_plus_the_option() {
    let sparse = HybridConfig::hybrid_sparse_v1();
    assert_eq!(sparse.name, "hybrid-sparse-v1");
    assert_eq!(sparse.sparse, Some(DEFAULT));
    assert_eq!(
        HybridConfig {
            name: "hybrid-baseline-v2".into(),
            sparse: None,
            ..sparse.clone()
        },
        HybridConfig::hybrid_baseline_v2()
    );
    sparse.validate().unwrap();
}

/// `hybrid-sparse-rerank-v1` is `hybrid-rerank-v3` over `hybrid-sparse-v1`: the same re-rank
/// depth and interpolation.
#[test]
fn hybrid_sparse_rerank_v1_is_hybrid_rerank_v3_plus_the_option() {
    let sparse = RerankConfig::hybrid_sparse_rerank_v1();
    assert_eq!(sparse.name, "hybrid-sparse-rerank-v1");
    assert_eq!(sparse.hybrid, HybridConfig::hybrid_sparse_v1());
    let v3 = RerankConfig::hybrid_rerank_v3();
    assert_eq!(
        RerankConfig {
            name: v3.name.clone(),
            hybrid: v3.hybrid.clone(),
            ..sparse.clone()
        },
        v3
    );
    sparse.validate().unwrap();
}

/// An earlier recipe serialises exactly as before: no `sparse` key in its reports.
#[test]
fn a_recipe_without_the_option_serialises_without_it() {
    let json = serde_json::to_string(&HybridConfig::hybrid_baseline_v2()).unwrap();
    assert!(!json.contains("sparse"), "{json}");
}

/// The settings the pipeline would refuse are refused by the harness first, by value; the
/// upper bound is the dense crate's.
#[test]
fn invalid_settings_are_refused() {
    assert_eq!(
        SparseSettings::MAX_SCALE,
        xtriever_dense::sparse::MAX_SCALE,
        "the harness bound mirrors the dense crate's"
    );
    for (settings, needle) in [
        (
            SparseSettings {
                scale: 0,
                boost: 1.0,
            },
            "scale",
        ),
        (
            SparseSettings {
                scale: SparseSettings::MAX_SCALE + 1,
                boost: 1.0,
            },
            "scale",
        ),
        (
            SparseSettings {
                scale: 10,
                boost: 0.0,
            },
            "boost",
        ),
        (
            SparseSettings {
                scale: 10,
                boost: f32::NAN,
            },
            "boost",
        ),
    ] {
        let config = HybridConfig {
            sparse: Some(settings),
            ..HybridConfig::hybrid_sparse_v1()
        };
        match config.validate() {
            Err(Error::Run(m)) => assert!(m.contains(needle), "{settings:?}: {m}"),
            other => panic!("{settings:?}: expected Run, got {other:?}"),
        }
    }
}

fn key() -> SparseCacheKey {
    SparseCacheKey {
        format_version: SparseCacheKey::FORMAT_VERSION,
        dataset: "scifact".into(),
        encoder: "encoder@rev".into(),
        recipe: "lexical-baseline-v2".into(),
        corpus_sha256: "c".repeat(64),
        documents: 3,
    }
}

/// A key matches only itself: every field, changed alone, is a miss naming that field.
#[test]
fn the_cache_key_misses_on_any_difference() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        key().mismatch(tmp.path()).is_some(),
        "no key file is a miss"
    );
    key().write(tmp.path()).unwrap();
    assert_eq!(key().mismatch(tmp.path()), None);
    let changed = [
        (
            "format_version",
            SparseCacheKey {
                format_version: 1,
                ..key()
            },
        ),
        (
            "recipe",
            SparseCacheKey {
                recipe: "lexical-baseline-v1".into(),
                ..key()
            },
        ),
        (
            "dataset",
            SparseCacheKey {
                dataset: "fiqa".into(),
                ..key()
            },
        ),
        (
            "encoder",
            SparseCacheKey {
                encoder: "other@rev".into(),
                ..key()
            },
        ),
        (
            "corpus_sha256",
            SparseCacheKey {
                corpus_sha256: "d".repeat(64),
                ..key()
            },
        ),
        (
            "documents",
            SparseCacheKey {
                documents: 4,
                ..key()
            },
        ),
    ];
    for (field, other) in changed {
        let m = other.mismatch(tmp.path()).expect("a miss");
        assert!(m.contains(field), "{field}: {m}");
    }
}

fn cached(entries: Vec<(u32, f32)>, truncated: bool) -> CachedExpansion {
    CachedExpansion { entries, truncated }
}

/// The weights and truncation flags round-trip bit for bit, empty expansions included; a file
/// with another count, or cut short, or with bytes after the last document, or a flag that is
/// neither 0 nor 1, is refused.
#[test]
fn the_weights_round_trip_and_refuse_a_damaged_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("weights.bin");
    let expansions = vec![
        cached(
            vec![(1_037, 0.25_f32), (2_003, 1.059_535), (30_521, 7.7e-5)],
            true,
        ),
        cached(vec![], false),
        cached(vec![(999, f32::from_bits(0x3f80_0001))], false),
    ];
    write_sparse_weights(&path, &expansions).unwrap();
    let back = read_sparse_weights(&path, 3).unwrap();
    assert_eq!(back.len(), 3);
    for (a, b) in expansions.iter().zip(&back) {
        assert_eq!(a.truncated, b.truncated);
        assert_eq!(a.entries.len(), b.entries.len());
        for (&(ia, wa), &(ib, wb)) in a.entries.iter().zip(&b.entries) {
            assert_eq!((ia, wa.to_bits()), (ib, wb.to_bits()));
        }
    }
    // Per document: a 4-byte count, a 1-byte flag, 8 bytes per pair — 29, then 5, then 13.
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 47);

    let mut flagged = std::fs::read(&path).unwrap();
    flagged[4] = 2;
    std::fs::write(&path, &flagged).unwrap();
    assert!(
        matches!(read_sparse_weights(&path, 3), Err(Error::Run(m)) if m.contains("flag 2")),
        "a flag that is neither 0 nor 1"
    );
    write_sparse_weights(&path, &expansions).unwrap();

    assert!(
        matches!(read_sparse_weights(&path, 2), Err(Error::Run(_))),
        "trailing document"
    );
    assert!(
        matches!(read_sparse_weights(&path, 4), Err(Error::Run(_))),
        "missing document"
    );
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() - 3]).unwrap();
    assert!(
        matches!(read_sparse_weights(&path, 3), Err(Error::Run(_))),
        "cut short"
    );
}

/// Review round 5: an interrupted encode resumes from its last synced point, which a
/// progress record names; a directory that never reached one has none.
#[test]
fn the_progress_record_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(SparseProgress::read(tmp.path()).unwrap(), None);
    let progress = SparseProgress {
        documents: 3_000,
        bytes: 5_784_320,
    };
    progress.write(tmp.path()).unwrap();
    assert_eq!(SparseProgress::read(tmp.path()).unwrap(), Some(progress));
    assert!(!tmp.path().join("progress.json.tmp").exists());
}
