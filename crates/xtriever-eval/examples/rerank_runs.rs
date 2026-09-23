//! `rerank_runs` — the engine's fusion and re-rank stage over exported first-stage runs, for the
//! expansion spikes (Feature 027 candidate).
//!
//! ```text
//! cargo run --release -p xtriever-eval --example rerank_runs -- --dataset D
//!     --lexical L.jsonl --dense D.jsonl --out R.jsonl [--scores CACHE.jsonl]
//!     [--depth N] [--alpha A] [--rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2-q8]
//! ```
//!
//! Each judged query's lexical and dense lists (external ids, rank order) are fused by the
//! pipeline's own `rrf` with `hybrid-rerank-v3`'s constant and depth (ties by `DocId`, which is
//! corpus order as the harness builds it), the first `depth` fused candidates are scored by the
//! pinned cross-encoder over the pipeline's stored passage (the `contents` text:
//! `title + " " + text`), and the list is ordered by the pipeline's own `order_interpolated`.
//! Every setting — the per-stage candidate depth, the fusion constant and depth, the re-rank
//! depth and α — is read from
//! `RerankConfig::hybrid_rerank_v3()`, so fed the v2 lexical run and the dense run it reproduces
//! `hybrid-rerank-v3`; `--depth` and `--alpha` override the last two, `--depth` within `1..=k`.
//!
//! Cross-encoder scores are cached per `(model, dataset, query, document)`: the model scores each pair
//! alone (Feature 006), so a cached score is the score, and a new setting pays only for the
//! pairs it adds. A line from another model or dataset is not reused; a line recording neither
//! is refused (the cache predates the key — delete it); a run naming a document the dataset
//! does not hold is refused; an unterminated tail from an interrupted run is
//! cut before the run appends; a cache that exists but cannot be read is an error, not an
//! empty cache; a non-finite score is refused rather than written; the cache is flushed
//! explicitly so a failed final write is reported.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    clippy::print_stdout
)]

// Shared helpers; each example uses a part of them.
#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;
use xtriever_core::{Budget, DocId, FieldName, Hit, Passage, Reranker, Value};
use xtriever_eval::dataset::{Dataset, Manifest};
use xtriever_eval::report::score;
use xtriever_eval::run::{RerankConfig, RerankMode, Run, build};
use xtriever_pipeline::{order_interpolated, rrf};
use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

use common::{flags, repo_root};

#[derive(Deserialize)]
struct RunRow {
    query_id: String,
    doc_ids: Vec<String>,
}

#[derive(Deserialize)]
struct Cached {
    m: Option<String>,
    ds: Option<String>,
    q: String,
    d: String,
    s: f32,
}

/// The cached scores `model` produced for `dataset`, keyed `(query, document)` — ids are only
/// unique within a dataset (SciFact's and FiQA's are both numeric). A missing file is an empty
/// cache; any other read failure is an error. Every record is written with its newline, so an
/// unterminated tail is an interrupted write: the file is cut back to its last complete line
/// before this run appends to it, and that pair is scored again.
fn load_cache(
    path: &str,
    model: &str,
    dataset: &str,
) -> anyhow::Result<BTreeMap<(String, String), f32>> {
    let mut cache = BTreeMap::new();
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(cache),
        Err(e) => return Err(e).with_context(|| format!("cannot read the score cache {path}")),
    };
    let complete = text.rfind('\n').map_or(0, |i| i + 1);
    let torn = complete < text.len();
    if torn {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)?
            .set_len(complete as u64)
            .with_context(|| format!("cannot cut the torn tail of {path}"))?;
    }
    let mut other = 0usize;
    for (n, line) in text[..complete].lines().enumerate() {
        let c: Cached =
            serde_json::from_str(line).with_context(|| format!("{path} line {}", n + 1))?;
        match (c.m.as_deref(), c.ds.as_deref()) {
            (Some(m), Some(ds)) if m == model && ds == dataset => {
                anyhow::ensure!(c.s.is_finite(), "{path} line {}: score {}", n + 1, c.s);
                cache.insert((c.q, c.d), c.s);
            }
            (Some(_), Some(_)) => other += 1,
            _ => anyhow::bail!(
                "{path} line {}: no model or dataset recorded; the cache predates its key, \
                 delete it",
                n + 1
            ),
        }
    }
    if other > 0 || torn {
        eprintln!(
            "{path}: {other} lines from another model or dataset not reused; torn tail cut: {torn}"
        );
    }
    Ok(cache)
}

fn load_run(path: &str) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let mut out = BTreeMap::new();
    for line in std::fs::read_to_string(path)
        .with_context(|| path.to_owned())?
        .lines()
    {
        let r: RunRow = serde_json::from_str(line)?;
        out.insert(r.query_id, r.doc_ids);
    }
    Ok(out)
}

fn main() -> anyhow::Result<()> {
    let f = flags(&[
        "dataset",
        "lexical",
        "dense",
        "out",
        "scores",
        "depth",
        "alpha",
        "rerank-model-dir",
    ])?;
    let get = |k: &str| f.get(k).map(String::as_str);
    let dataset = get("dataset").context("--dataset")?;
    let v3 = RerankConfig::hybrid_rerank_v3();
    let RerankMode::Interpolate { alpha: v3_alpha } = v3.mode else {
        anyhow::bail!("hybrid-rerank-v3 no longer interpolates; this harness reproduces it")
    };
    let depth: usize = match get("depth") {
        Some(s) => s.parse().with_context(|| format!("--depth {s}"))?,
        None => v3.rerank_depth,
    };
    let alpha: f64 = match get("alpha") {
        Some(s) => s.parse().with_context(|| format!("--alpha {s}"))?,
        None => v3_alpha,
    };
    let config = RerankConfig {
        rerank_depth: depth,
        mode: RerankMode::Interpolate { alpha },
        ..v3
    };
    // The same checks the harness applies: 1 ≤ depth ≤ k, and a valid fused recipe.
    config.validate()?;
    anyhow::ensure!(
        (0.0..=1.0).contains(&alpha),
        "--alpha {alpha} is outside [0, 1]"
    );
    let hybrid = &config.hybrid;
    let lexical = load_run(get("lexical").context("--lexical")?)?;
    let dense = load_run(get("dense").context("--dense")?)?;
    let out = get("out").context("--out")?;
    let model_dir = get("rerank-model-dir").map_or_else(
        || repo_root().join("reference/models/ms-marco-MiniLM-L-6-v2-q8"),
        PathBuf::from,
    );

    let manifest = Manifest::load(&repo_root().join("reference/datasets/beir-manifest.json"))?;
    let ds = Dataset::load(
        &manifest,
        dataset,
        &repo_root().join("reference/datasets/beir"),
    )?;
    let (_, docs, ids) = build(&ds, &hybrid.lexical)?;
    let contents = FieldName::from("contents");
    // DocId → the passage the pipeline stores for it (its one dense field, `contents`).
    let mut passage: BTreeMap<u32, String> = BTreeMap::new();
    for doc in &docs {
        if let Some(Value::Text(t)) = doc.fields.get(&contents) {
            passage.insert(doc.id.0, t.clone());
        }
    }
    let internal: BTreeMap<&str, DocId> = ids
        .0
        .iter()
        .enumerate()
        .map(|(i, e)| (e.as_str(), DocId(i as u32)))
        .collect();
    // A run from another dataset, or from a corpus that has since changed, names documents
    // this corpus does not hold; fusing what remains would print quietly wrong numbers.
    for (flag, run) in [("--lexical", &lexical), ("--dense", &dense)] {
        let unknown: Vec<&str> = run
            .values()
            .flatten()
            .map(String::as_str)
            .filter(|e| !internal.contains_key(e))
            .collect();
        if let Some(first) = unknown.first() {
            anyhow::bail!(
                "{flag} names {} documents {dataset} does not hold (first: {first:?}); is it \
                 this dataset's run?",
                unknown.len()
            );
        }
    }
    let hits = |list: &[String]| -> Vec<Hit> {
        list.iter()
            .filter_map(|e| internal.get(e.as_str()))
            .map(|&id| Hit { id, score: 0.0 })
            .collect()
    };

    let reranker = MiniLmCrossEncoder::load(&model_dir, LoadPath::Mmap)?;
    let model = reranker.model_id().to_owned();
    let mut cache = match get("scores") {
        Some(path) => load_cache(path, &model, dataset)?,
        None => BTreeMap::new(),
    };
    let mut cache_out = match get("scores") {
        Some(path) => Some(std::io::BufWriter::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
        )),
        None => None,
    };
    let budget = Budget {
        max_time: None,
        max_items: None,
    };

    let texts: BTreeMap<&str, &str> = ds
        .queries
        .queries
        .iter()
        .map(|(id, t)| (id.as_str(), t.as_str()))
        .collect();
    let (mut reused, mut scored) = (0usize, 0usize);
    let mut results = BTreeMap::new();
    for query_id in ds.qrels.grades.keys() {
        let Some(text) = texts.get(query_id.as_str()) else {
            continue;
        };
        // Each stage contributes its first `candidate_depth` hits, as the pipeline fuses them.
        let mut lex = hits(lexical.get(query_id).map_or(&[][..], Vec::as_slice));
        let mut den = hits(dense.get(query_id).map_or(&[][..], Vec::as_slice));
        lex.truncate(hybrid.candidate_depth);
        den.truncate(hybrid.candidate_depth);
        let fused = rrf(&lex, &den, hybrid.rrf_k, hybrid.k);
        let n = depth.min(fused.len());
        let mut scores: Vec<Option<f32>> = vec![None; fused.len()];
        let mut missing: Vec<usize> = Vec::new();
        for (j, &(id, _)) in fused[..n].iter().enumerate() {
            let ext = ids.external(id).context("unknown DocId")?.to_owned();
            match cache.get(&(query_id.clone(), ext)) {
                Some(&s) => {
                    scores[j] = Some(s);
                    reused += 1;
                }
                None => missing.push(j),
            }
        }
        if !missing.is_empty() {
            let passages: Vec<Passage<'_>> = missing
                .iter()
                .map(|&j| Passage {
                    id: fused[j].0,
                    text: passage.get(&fused[j].0.0).map_or("", String::as_str),
                })
                .collect();
            let got = reranker.rerank(text, &passages, &budget)?;
            for (&j, s) in missing.iter().zip(got) {
                let s = s.context("the cross-encoder left a pair unscored without a budget")?;
                anyhow::ensure!(s.is_finite(), "the cross-encoder returned {s}");
                scores[j] = Some(s);
                scored += 1;
                let ext = ids
                    .external(fused[j].0)
                    .context("unknown DocId")?
                    .to_owned();
                if let Some(w) = cache_out.as_mut() {
                    writeln!(
                        w,
                        "{}",
                        serde_json::json!({ "m": model, "ds": dataset, "q": query_id, "d": ext, "s": s })
                    )?;
                }
                cache.insert((query_id.clone(), ext), s);
            }
        }
        let ordered = order_interpolated(&fused, &scores, hybrid.k, alpha);
        let mut external = Vec::with_capacity(ordered.len());
        for (id, ..) in ordered {
            external.push(ids.external(id).context("unknown DocId")?.to_owned());
        }
        results.insert(query_id.clone(), external);
    }
    // Flushed explicitly: a drop would discard a failed final write, and the cache would end
    // torn or short while the run reported success.
    if let Some(mut w) = cache_out {
        w.flush().context("cannot flush the score cache")?;
    }
    let run = Run {
        config: format!("rerank-d{depth}-a{alpha}"),
        dataset: ds.name.clone(),
        results,
        unjudged_queries: 0,
    };
    run.export_jsonl(Path::new(out))?;
    let report = score(&run, &ds, "spike")?;
    eprintln!(
        "{dataset} re-ranked: nDCG@10={:.6} Recall@100={:.6} (pairs scored {scored}, reused {reused})",
        report.mean_ndcg_10, report.mean_recall_100
    );
    Ok(())
}
