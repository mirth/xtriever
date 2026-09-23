//! `rerank_runs` — the engine's fusion and re-rank stage over exported first-stage runs, for the
//! expansion spikes (Feature 027 candidate).
//!
//! ```text
//! cargo run --release -p xtriever-eval --example rerank_runs -- --dataset D
//!     --lexical L.jsonl --dense D.jsonl --out R.jsonl [--scores CACHE.jsonl]
//!     [--depth 20] [--alpha 0.5] [--rerank-model-dir reference/models/ms-marco-MiniLM-L-6-v2-q8]
//! ```
//!
//! Each judged query's lexical and dense lists (external ids, rank order) are fused by the
//! pipeline's own `rrf` (k 60, depth 100, ties by `DocId`, which is corpus order as the harness
//! builds it), the first `depth` fused candidates are scored by the pinned cross-encoder over the
//! pipeline's stored passage (the `contents` text: `title + " " + text`), and the list is ordered
//! by the pipeline's own `order_interpolated` — `hybrid-rerank-v3`'s rule. Fed the v2 lexical run
//! and the dense run, it must reproduce `hybrid-rerank-v3`. Cross-encoder scores are cached per
//! `(query, document)`: the model scores each pair alone (Feature 006), so a cached score is the
//! score, and a new setting pays only for the pairs it adds.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    clippy::print_stdout
)]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;
use xtriever_core::{Budget, DocId, FieldName, Hit, Passage, Reranker, Value};
use xtriever_eval::dataset::{Dataset, Manifest};
use xtriever_eval::report::score;
use xtriever_eval::run::{EvalConfig, Run, build};
use xtriever_pipeline::{order_interpolated, rrf};
use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn flags() -> BTreeMap<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        if let Some(name) = args[i].strip_prefix("--") {
            out.insert(
                name.to_owned(),
                args.get(i + 1).cloned().unwrap_or_default(),
            );
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

#[derive(Deserialize)]
struct RunRow {
    query_id: String,
    doc_ids: Vec<String>,
}

#[derive(Deserialize)]
struct Cached {
    q: String,
    d: String,
    s: f32,
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
    let f = flags();
    let get = |k: &str| f.get(k).map(String::as_str);
    let dataset = get("dataset").context("--dataset")?;
    let depth: usize = get("depth").and_then(|s| s.parse().ok()).unwrap_or(20);
    let alpha: f64 = get("alpha").and_then(|s| s.parse().ok()).unwrap_or(0.5);
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
    let cfg = EvalConfig::lexical_baseline_v2();
    let (_, docs, ids) = build(&ds, &cfg)?;
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
    let hits = |list: &[String]| -> Vec<Hit> {
        list.iter()
            .filter_map(|e| internal.get(e.as_str()))
            .map(|&id| Hit { id, score: 0.0 })
            .collect()
    };

    let mut cache: BTreeMap<(String, String), f32> = BTreeMap::new();
    if let Some(path) = get("scores")
        && let Ok(text) = std::fs::read_to_string(path)
    {
        for line in text.lines() {
            let c: Cached = serde_json::from_str(line)?;
            cache.insert((c.q, c.d), c.s);
        }
    }
    let mut cache_out = match get("scores") {
        Some(path) => Some(std::io::BufWriter::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
        )),
        None => None,
    };
    let reranker = MiniLmCrossEncoder::load(&model_dir, LoadPath::Mmap)?;
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
        let lex = hits(lexical.get(query_id).map_or(&[][..], Vec::as_slice));
        let den = hits(dense.get(query_id).map_or(&[][..], Vec::as_slice));
        let fused = rrf(&lex, &den, 60, 100);
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
                        serde_json::json!({ "q": query_id, "d": ext, "s": s })
                    )?;
                }
                cache.insert((query_id.clone(), ext), s);
            }
        }
        let ordered = order_interpolated(&fused, &scores, cfg.k, alpha);
        let mut external = Vec::with_capacity(ordered.len());
        for (id, ..) in ordered {
            external.push(ids.external(id).context("unknown DocId")?.to_owned());
        }
        results.insert(query_id.clone(), external);
    }
    drop(cache_out);
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
