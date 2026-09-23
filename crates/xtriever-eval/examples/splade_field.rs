//! `splade_field` — the sparse-field spike (Feature 027 candidate, after Feature 016): the v3
//! sparse encoder's document weights written into a second tantivy field of the
//! `lexical-baseline-v2` index, scored by BM25 beside `contents` at a swept boost.
//!
//! ```text
//! cargo run --release -p xtriever-eval --example splade_field -- --dataset D
//!     --sparse-dir DIR [--scale 10] [--boosts 0,0.1,0.2,0.3,0.5,0.7] [--qweights plain,idf]
//!     [--index-dir DIR] --runs-dir DIR [--chunk 2000] [--cache DIR]
//! ```
//!
//! `DIR/<dataset>/docs.jsonl` holds each document's `(token id, weight)` pairs and
//! `queries.jsonl` each judged query's `(token id, idf)` pairs — Feature 012's encodings of
//! `opensearch-neural-sparse-encoding-doc-v3-distill@babf71f3`, exported unchanged. A token
//! enters the field as the literal term `s<id>`, repeated `round(weight × scale)` times (Feature
//! 012's `bm25x`, whose scale was 100), so BM25 reads the weight as term frequency; the field's
//! analyzer is the unstemmed `standard`, which keeps `s<id>` whole.
//!
//! One index per `(dataset, scale)`; each `(query weighting, boost)` is a query-time choice:
//! `Bool { should: [Match(contents, text), Boost(Term(sparse, s<id>), b × q)…] }` with `q` = 1
//! (`plain`, Feature 012's choice) or the token's idf over the query's largest (`idf`). Boost 0
//! sends no sparse clause at all and must reproduce `lexical-baseline-v2`. Nothing here is the
//! engine's surface.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    clippy::print_stdout
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;
use xtriever_core::{
    AnalyzerId, FieldDef, FieldKind, FieldName, LexicalIndex, LexicalQuery, Value,
};
use xtriever_eval::dataset::{Dataset, Manifest};
use xtriever_eval::report::score;
use xtriever_eval::run::{EvalConfig, Run, build};
use xtriever_lexical::TantivyIndex;

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
struct DocRow {
    doc_id: String,
    terms: Vec<(u32, f32)>,
}

#[derive(Deserialize)]
struct QueryRow {
    query_id: String,
    terms: Vec<(u32, f32)>,
}

fn dir_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            total += if path.is_dir() {
                dir_bytes(&path)
            } else {
                path.metadata().map(|m| m.len()).unwrap_or(0)
            };
        }
    }
    total
}

fn main() -> anyhow::Result<()> {
    let f = flags();
    let get = |k: &str| f.get(k).map(String::as_str);
    let dataset = get("dataset").context("--dataset")?;
    let sparse_dir = PathBuf::from(get("sparse-dir").context("--sparse-dir")?).join(dataset);
    let runs_dir = PathBuf::from(get("runs-dir").context("--runs-dir")?);
    std::fs::create_dir_all(&runs_dir)?;
    let scale: f32 = get("scale").and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let chunk: usize = get("chunk").and_then(|s| s.parse().ok()).unwrap_or(2000);
    let boosts: Vec<f32> = get("boosts")
        .unwrap_or("0,0.1,0.2,0.3,0.5,0.7")
        .split(',')
        .map(|s| s.parse().expect("boost"))
        .collect();
    let qweights: Vec<String> = get("qweights")
        .unwrap_or("plain,idf")
        .split(',')
        .map(str::to_owned)
        .collect();
    let cache = get("cache").map_or_else(
        || repo_root().join("reference/datasets/beir"),
        PathBuf::from,
    );

    let manifest = Manifest::load(&repo_root().join("reference/datasets/beir-manifest.json"))?;
    let ds = Dataset::load(&manifest, dataset, &cache)?;
    let cfg = EvalConfig::lexical_baseline_v2();
    let (mut schema, docs, ids) = build(&ds, &cfg)?;
    let sparse = FieldName::from("sparse");
    let contents = FieldName::from("contents");
    schema.fields.push(FieldDef {
        name: sparse.clone(),
        kind: FieldKind::Text(AnalyzerId("standard".into())),
        indexed: true,
        stored: false,
        boost: 1.0,
    });

    let mut weights: BTreeMap<String, Vec<(u32, f32)>> = BTreeMap::new();
    for line in std::fs::read_to_string(sparse_dir.join("docs.jsonl"))?.lines() {
        let r: DocRow = serde_json::from_str(line)?;
        weights.insert(r.doc_id, r.terms);
    }
    let mut qterms: BTreeMap<String, Vec<(u32, f32)>> = BTreeMap::new();
    for line in std::fs::read_to_string(sparse_dir.join("queries.jsonl"))?.lines() {
        let r: QueryRow = serde_json::from_str(line)?;
        qterms.insert(r.query_id, r.terms);
    }

    let keep;
    let index_dir = match get("index-dir") {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
            dir
        }
        None => {
            keep = tempfile::tempdir()?;
            keep.path().join("idx")
        }
    };
    let mut index = TantivyIndex::create(&index_dir, schema)?;
    let (mut tokens, mut missing) = (0u64, 0usize);
    // In chunks: at scale 100 a document's field is thousands of terms, and the whole corpus's
    // text at once would not be small.
    for part in docs.chunks(chunk) {
        let mut batch = part.to_vec();
        for doc in &mut batch {
            let ext = ids.external(doc.id).context("unknown DocId")?;
            let mut text = String::new();
            match weights.get(ext) {
                Some(terms) => {
                    for &(id, w) in terms {
                        let reps = (w * scale).round() as usize;
                        for _ in 0..reps {
                            text.push('s');
                            text.push_str(&id.to_string());
                            text.push(' ');
                        }
                        tokens += reps as u64;
                    }
                }
                None => missing += 1,
            }
            doc.fields.insert(sparse.clone(), Value::Text(text));
        }
        index.add(&batch)?;
    }
    index.commit()?;
    let bytes = dir_bytes(&index_dir);
    eprintln!(
        "{dataset}: scale {scale}; {} documents, {:.0} sparse tokens each, {missing} without an encoding; index {bytes} bytes",
        docs.len(),
        tokens as f64 / docs.len() as f64
    );

    let texts: BTreeMap<&str, &str> = ds
        .queries
        .queries
        .iter()
        .map(|(id, t)| (id.as_str(), t.as_str()))
        .collect();
    for qw in &qweights {
        for &b in &boosts {
            let mut results = BTreeMap::new();
            for query_id in ds.qrels.grades.keys() {
                let Some(text) = texts.get(query_id.as_str()) else {
                    continue;
                };
                let mut should = vec![LexicalQuery::Match(
                    Some(contents.clone()),
                    (*text).to_owned(),
                )];
                if b > 0.0
                    && let Some(terms) = qterms.get(query_id)
                {
                    let top = terms.iter().map(|t| t.1).fold(f32::MIN, f32::max);
                    for &(id, idf) in terms {
                        let q = if qw == "idf" && top > 0.0 {
                            idf / top
                        } else {
                            1.0
                        };
                        let term = LexicalQuery::Term(sparse.clone(), format!("s{id}"));
                        should.push(LexicalQuery::Boost(Box::new(term), b * q));
                    }
                }
                let query = LexicalQuery::Bool {
                    must: vec![],
                    should,
                    must_not: vec![],
                };
                let hits = index.search(&query, None, cfg.k)?;
                let mut external = Vec::with_capacity(hits.len());
                for hit in hits {
                    external.push(ids.external(hit.id).context("unknown DocId")?.to_owned());
                }
                results.insert(query_id.clone(), external);
            }
            let name = format!("splade-field-s{scale}-{qw}-b{b}");
            let run = Run {
                config: name.clone(),
                dataset: ds.name.clone(),
                results,
                unjudged_queries: 0,
            };
            run.export_jsonl(&runs_dir.join(format!("{dataset}-s{scale}-{qw}-b{b}.jsonl")))?;
            let report = score(&run, &ds, "spike")?;
            eprintln!(
                "{dataset} {name}: nDCG@10={:.6} Recall@100={:.6}",
                report.mean_ndcg_10, report.mean_recall_100
            );
        }
    }
    Ok(())
}
