//! `beir` — evaluate the lexical stage on the pinned BEIR datasets.
//!
//! This is the only place that names `TantivyIndex` (a dev-dependency), so the `xtriever-eval`
//! library graph stays `std`-only. Subcommands (contract `eval-harness.md`):
//!
//! ```text
//! beir verify [--cache DIR] <dataset...>
//! beir run    --dataset D [--config lexical-baseline-v1] [--out F] [--export-run F] [--index-dir DIR] [--cache DIR]
//! beir delta  before.json... -- after.json...      (or two single files)
//! beir smoke  --dataset scifact --baseline F [--cache DIR]
//! ```
//!
//! Exit codes: 0 ok, 1 error, 2 smoke failed.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, bail};
use xtriever_core::LexicalIndex;
use xtriever_eval::dataset::{Dataset, Manifest};
use xtriever_eval::report::{EvalReport, delta, score, smoke};
use xtriever_eval::run::{EvalConfig, build, execute};
use xtriever_lexical::TantivyIndex;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Args {
    positional: Vec<String>,
    flags: std::collections::BTreeMap<String, String>,
}

fn parse(args: &[String]) -> Args {
    let mut positional = Vec::new();
    let mut flags = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--" {
            positional.push(args[i].clone()); // separator for `delta before... -- after...`
            i += 1;
        } else if let Some(name) = args[i].strip_prefix("--") {
            let value = args.get(i + 1).cloned().unwrap_or_default();
            flags.insert(name.to_owned(), value);
            i += 2;
        } else {
            positional.push(args[i].clone());
            i += 1;
        }
    }
    Args { positional, flags }
}

fn manifest() -> anyhow::Result<Manifest> {
    Ok(Manifest::load(
        &repo_root().join("reference/datasets/beir-manifest.json"),
    )?)
}

fn cache_dir(a: &Args) -> PathBuf {
    a.flags
        .get("cache")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("reference/datasets/beir"))
}

fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn config(a: &Args) -> anyhow::Result<EvalConfig> {
    match a
        .flags
        .get("config")
        .map(String::as_str)
        .unwrap_or("lexical-baseline-v1")
    {
        "lexical-baseline-v1" => Ok(EvalConfig::lexical_baseline_v1()),
        other => bail!("unknown configuration `{other}`; known: lexical-baseline-v1"),
    }
}

/// Index, retrieve and score one dataset. Returns the report.
fn evaluate(dataset: &str, a: &Args) -> anyhow::Result<EvalReport> {
    let m = manifest()?;
    let cfg = config(a)?;
    let ds =
        Dataset::load(&m, dataset, &cache_dir(a)).with_context(|| format!("loading {dataset}"))?;
    if !ds.dangling.queries.is_empty() || !ds.dangling.documents.is_empty() {
        eprintln!(
            "note: {dataset}: {} judged queries and {} judged documents are dangling",
            ds.dangling.queries.len(),
            ds.dangling.documents.len()
        );
    }
    let (schema, docs, ids) = build(&ds, &cfg)?;
    let keep;
    let index_dir = match a.flags.get("index-dir") {
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
    index.add(&docs)?;
    index.commit()?;
    let run = execute(&index, &ids, &ds, &cfg)?;
    if let Some(path) = a.flags.get("export-run") {
        run.export_jsonl(Path::new(path))?;
        eprintln!("exported run to {path}");
    }
    let report = score(&run, &ds, &git_head())?;
    eprintln!(
        "{dataset} {}: nDCG@10={:.6} Recall@100={:.6} (BEIR-rounded {} / {}) scored={} dropped_self_ids={}",
        cfg.name,
        report.mean_ndcg_10,
        report.mean_recall_100,
        report.beir_rounded.ndcg_10,
        report.beir_rounded.recall_100,
        report.scored_queries,
        report.dropped_identical
    );
    Ok(report)
}

fn read_report(path: &str) -> anyhow::Result<EvalReport> {
    Ok(serde_json::from_str(
        &std::fs::read_to_string(path).with_context(|| path.to_owned())?,
    )?)
}

fn write_report(path: &str, report: &EvalReport) -> anyhow::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(report)? + "\n")?;
    eprintln!("wrote {path}");
    Ok(())
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some((sub, rest)) = argv.split_first() else {
        eprintln!("usage: beir <verify|run|delta|smoke> ...");
        return ExitCode::from(1);
    };
    let a = parse(rest);
    let result: anyhow::Result<ExitCode> = (|| match sub.as_str() {
        "verify" => {
            let m = manifest()?;
            for name in &a.positional {
                let ds = Dataset::load(&m, name, &cache_dir(&a))?;
                println!(
                    "{name}: documents={} queries={} judged_queries={} judgement_pairs={} dangling_queries={} dangling_docs={}",
                    ds.counts.documents,
                    ds.counts.queries,
                    ds.counts.judged_queries,
                    ds.counts.judgement_pairs,
                    ds.dangling.queries.len(),
                    ds.dangling.documents.len()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        "run" => {
            let dataset = a.flags.get("dataset").context("--dataset is required")?;
            let report = evaluate(dataset, &a)?;
            if let Some(out) = a.flags.get("out") {
                write_report(out, &report)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        "delta" => {
            let mid = a.positional.iter().position(|p| p == "--");
            let (before, after): (Vec<&String>, Vec<&String>) = match mid {
                Some(i) => (
                    a.positional[..i].iter().collect(),
                    a.positional[i + 1..].iter().collect(),
                ),
                None if a.positional.len() == 2 => (vec![&a.positional[0]], vec![&a.positional[1]]),
                None => bail!(
                    "usage: beir delta before.json after.json  |  beir delta before... -- after..."
                ),
            };
            let before: Vec<EvalReport> = before
                .iter()
                .map(|p| read_report(p))
                .collect::<anyhow::Result<_>>()?;
            let after: Vec<EvalReport> = after
                .iter()
                .map(|p| read_report(p))
                .collect::<anyhow::Result<_>>()?;
            print!("{}", delta(&before, &after).to_markdown());
            Ok(ExitCode::SUCCESS)
        }
        "smoke" => {
            let dataset = a
                .flags
                .get("dataset")
                .map(String::as_str)
                .unwrap_or("scifact");
            let baseline = read_report(a.flags.get("baseline").context("--baseline is required")?)?;
            if baseline.dataset != dataset {
                bail!("baseline is for `{}`, not `{dataset}`", baseline.dataset);
            }
            let current = evaluate(dataset, &a)?;
            match smoke(&baseline, &current) {
                Ok(d) => {
                    print!("{}", d.to_markdown());
                    println!("eval-smoke: PASS");
                    Ok(ExitCode::SUCCESS)
                }
                Err(f) => {
                    println!("eval-smoke: FAIL — {f}");
                    Ok(ExitCode::from(2))
                }
            }
        }
        other => bail!("unknown subcommand `{other}`"),
    })();
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("beir: error: {e:#}");
            ExitCode::from(1)
        }
    }
}
