//! US3 scenarios 1–4 (offline) — the dense configuration, passage building, `execute_dense`
//! against stub stages, and the embedding cache key (spec FR-019–FR-021).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Mutex;

use xtriever_core::{DocId, DocSet, Embedder, Hit, Metric, Result, TextKind, Vector, VectorIndex};
use xtriever_eval::dataset::Dataset;
use xtriever_eval::run::{DenseConfig, EmbeddingCacheKey, IdMap, build_passages, execute_dense};

fn mini() -> (tempfile::TempDir, Dataset) {
    let dir = tempfile::tempdir().unwrap();
    let (manifest, _) = support::synthetic_dataset(dir.path());
    let ds = Dataset::load(&manifest, "mini", dir.path()).unwrap();
    (dir, ds)
}

/// Records every call; returns a fixed vector whose first component encodes the text length.
struct StubEmbedder {
    calls: Mutex<Vec<(String, TextKind)>>,
}

impl Embedder for StubEmbedder {
    fn dim(&self) -> usize {
        4
    }
    fn metric(&self) -> Metric {
        Metric::Cosine
    }
    fn fingerprint(&self) -> &str {
        "stub-fp"
    }
    fn max_input_tokens(&self) -> Option<usize> {
        Some(8)
    }
    fn embed(&self, texts: &[&str], kind: TextKind) -> Result<Vec<Vector>> {
        let mut calls = self.calls.lock().unwrap();
        Ok(texts
            .iter()
            .map(|t| {
                calls.push(((*t).to_owned(), kind));
                vec![t.len() as f32, 1.0, 0.0, 0.0]
            })
            .collect())
    }
}

/// Returns canned hits: ids 3, 0, 1 (in that order), truncated to `k`; records `k`.
struct StubIndex {
    ks: Mutex<Vec<usize>>,
}

impl VectorIndex for StubIndex {
    fn dim(&self) -> usize {
        4
    }
    fn metric(&self) -> Metric {
        Metric::Cosine
    }
    fn fingerprint(&self) -> &str {
        "stub-fp"
    }
    fn add(&mut self, _: DocId, _: &[f32]) -> Result<()> {
        Ok(())
    }
    fn delete(&mut self, _: &[DocId]) -> Result<()> {
        Ok(())
    }
    fn commit(&mut self) -> Result<()> {
        Ok(())
    }
    fn search(&self, _: &[f32], _: Option<&DocSet>, k: usize) -> Result<Vec<Hit>> {
        self.ks.lock().unwrap().push(k);
        let hits = [
            Hit {
                id: DocId(3),
                score: 0.9,
            },
            Hit {
                id: DocId(0),
                score: 0.5,
            },
            Hit {
                id: DocId(1),
                score: 0.1,
            },
        ];
        Ok(hits.into_iter().take(k).collect())
    }
    fn len(&self) -> u64 {
        4
    }
}

#[test]
fn dense_baseline_v1_is_the_recipe_and_validates_k() {
    let cfg = DenseConfig::dense_baseline_v1();
    assert_eq!(cfg.name, "dense-baseline-v1");
    assert_eq!(cfg.k, 100);
    assert!(cfg.passage.title_then_text);
    assert_eq!(cfg.passage.separator, " ");
    assert!(cfg.passage.omit_empty_title);
    cfg.validate().unwrap();
    let mut low = cfg.clone();
    low.k = 10;
    assert!(low.validate().is_err(), "k < 100 cannot report Recall@100");
    let (_dir, ds) = mini();
    assert!(build_passages(&ds, &low).is_err());
}

#[test]
fn passages_join_title_and_text_and_map_ids_in_corpus_order() {
    let (_dir, ds) = mini();
    let cfg = DenseConfig::dense_baseline_v1();
    let (passages, ids) = build_passages(&ds, &cfg).unwrap();
    assert_eq!(passages.len(), 4);
    assert_eq!(passages[0], "Quantum lattice quantum lattice photon");
    assert_eq!(
        passages[1], "river valley harbor",
        "empty title omitted, no leading separator"
    );
    assert_eq!(passages[2], "Piano piano harp quantum");
    assert_eq!(ids.external(DocId(0)), Some("d1"));
    assert_eq!(ids.external(DocId(3)), Some("q2"));
    assert_eq!(ids.external(DocId(4)), None);
    let mut no_title = cfg.clone();
    no_title.passage.title_then_text = false;
    let (text_only, _) = build_passages(&ds, &no_title).unwrap();
    assert_eq!(text_only[0], "quantum lattice photon");
}

#[test]
fn execute_dense_embeds_every_judged_query_as_query_and_maps_hits_in_order() {
    let (_dir, ds) = mini();
    let cfg = DenseConfig::dense_baseline_v1();
    let ids = IdMap(ds.corpus.ids.clone());
    let embedder = StubEmbedder {
        calls: Mutex::new(Vec::new()),
    };
    let index = StubIndex {
        ks: Mutex::new(Vec::new()),
    };
    let run = execute_dense(&embedder, &index, &ids, &ds, &cfg).unwrap();

    assert_eq!(run.config, "dense-baseline-v1");
    assert_eq!(run.dataset, "mini");
    let calls = embedder.calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "only the two judged queries");
    assert!(calls.iter().all(|(_, kind)| *kind == TextKind::Query));
    assert_eq!(calls[0].0, "quantum lattice");
    assert!(index.ks.lock().unwrap().iter().all(|&k| k == 100));
    // Order preserved, external ids mapped, raw (the identical-id rule is applied at scoring).
    assert_eq!(run.results["q1"], vec!["q2", "d1", "d2"]);
    assert_eq!(run.results["q2"], vec!["q2", "d1", "d2"]);
    assert_eq!(run.unjudged_queries, 0);
}

#[test]
fn cache_key_matches_only_when_every_field_agrees() {
    let dir = tempfile::tempdir().unwrap();
    let key = EmbeddingCacheKey {
        format_version: 1,
        config: "dense-baseline-v1".into(),
        dataset: "scifact".into(),
        embedder_fingerprint: "fp-a".into(),
        corpus_sha256: "abc".into(),
        documents: 5183,
    };
    assert!(!key.matches(dir.path()), "nothing written yet");
    key.write(dir.path()).unwrap();
    assert!(dir.path().join("cache.json").is_file());
    assert!(key.matches(dir.path()));

    for (label, other) in [
        (
            "config",
            EmbeddingCacheKey {
                config: "other".into(),
                ..key.clone()
            },
        ),
        (
            "dataset",
            EmbeddingCacheKey {
                dataset: "nfcorpus".into(),
                ..key.clone()
            },
        ),
        (
            "fingerprint",
            EmbeddingCacheKey {
                embedder_fingerprint: "fp-b".into(),
                ..key.clone()
            },
        ),
        (
            "corpus hash",
            EmbeddingCacheKey {
                corpus_sha256: "abd".into(),
                ..key.clone()
            },
        ),
        (
            "documents",
            EmbeddingCacheKey {
                documents: 5184,
                ..key.clone()
            },
        ),
        (
            "format",
            EmbeddingCacheKey {
                format_version: 2,
                ..key.clone()
            },
        ),
    ] {
        assert!(
            !other.matches(dir.path()),
            "{label} differs but the cache matched"
        );
    }
    std::fs::write(dir.path().join("cache.json"), "not json").unwrap();
    assert!(
        !key.matches(dir.path()),
        "unparseable key is a miss, not an error"
    );
}
