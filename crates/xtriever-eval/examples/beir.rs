//! `beir` — evaluate the lexical stage (Feature 003) or the dense stage (Feature 004) on the
//! pinned BEIR datasets.
//!
//! This is the only place that names `TantivyIndex` and `MiniLmEmbedder` / `FlatIndex`
//! (dev-dependencies), so the `xtriever-eval` library graph stays `std`-only. Subcommands
//! (contracts `eval-harness.md`, `dense-stage.md`):
//!
//! ```text
//! beir verify [--cache DIR] <dataset...>
//! beir run    --dataset D [--config lexical-baseline-v1] [--out F] [--export-run F] [--index-dir DIR] [--cache DIR]
//! beir run    --dataset D --config dense-baseline-v1 [--model-dir M] [--cache-dir C] [--load-path buffered|mmap] [--out F] [--export-run F]
//! beir run    --dataset D --config hybrid-baseline-v1 [--model-dir M] [--cache-dir C] [--index-dir DIR] [--load-path P] [--out F] [--export-run F] [--export-explain F]
//! beir compare a.json b.json                        (cross-configuration table, no ADR line)
//! beir delta  before.json... -- after.json...      (or two single files; same configuration only)
//! beir smoke  --dataset scifact --baseline F [--cache DIR]
//! beir model-memory [--model-dir M] --load-path buffered|mmap
//! beir export-vectors --dataset D [--cache-dir C] --sample N --out F
//! ```
//!
//! Exit codes: 0 ok, 1 error, 2 smoke failed. Timings go to stderr only, never into a report,
//! so two runs of the same configuration are byte-identical (spec 004 SC-006).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use std::io::Write;
use std::time::Instant;

use anyhow::{Context, bail};
use xtriever_core::{DocId, Embedder, LexicalIndex, TextKind, VectorIndex};
use xtriever_dense::{FlatIndex, LoadPath, MiniLmEmbedder};
use xtriever_eval::dataset::{Dataset, Manifest};
use xtriever_eval::report::{EvalReport, StageInfo, compare, delta, score, smoke};
use xtriever_eval::run::{
    DenseConfig, EmbeddingCacheKey, EvalConfig, HybridConfig, build, build_external,
    build_passages, execute, execute_dense, execute_external,
};
use xtriever_lexical::TantivyIndex;
use xtriever_pipeline::{HybridIndex, SearchOptions, SourceDocument};

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

enum Config {
    Lexical(EvalConfig),
    Dense(DenseConfig),
    Hybrid(HybridConfig),
}

fn config(a: &Args) -> anyhow::Result<Config> {
    match a
        .flags
        .get("config")
        .map(String::as_str)
        .unwrap_or("lexical-baseline-v1")
    {
        "lexical-baseline-v1" => Ok(Config::Lexical(EvalConfig::lexical_baseline_v1())),
        "dense-baseline-v1" => Ok(Config::Dense(DenseConfig::dense_baseline_v1())),
        "hybrid-baseline-v1" => Ok(Config::Hybrid(HybridConfig::hybrid_baseline_v1())),
        other => {
            bail!(
                "unknown configuration `{other}`; known: lexical-baseline-v1, dense-baseline-v1, hybrid-baseline-v1"
            )
        }
    }
}

fn model_dir(a: &Args) -> PathBuf {
    a.flags
        .get("model-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("reference/models/all-MiniLM-L6-v2"))
}

fn dense_cache_dir(a: &Args) -> PathBuf {
    a.flags
        .get("cache-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("target/xt-dense-cache"))
}

fn load_path(a: &Args) -> anyhow::Result<LoadPath> {
    match a
        .flags
        .get("load-path")
        .map(String::as_str)
        .unwrap_or("buffered")
    {
        "buffered" => Ok(LoadPath::Buffered),
        "mmap" => Ok(LoadPath::Mmap),
        other => bail!("unknown --load-path `{other}`; known: buffered, mmap"),
    }
}

fn load_path_name(p: LoadPath) -> &'static str {
    match p {
        LoadPath::Buffered => "buffered",
        LoadPath::Mmap => "mmap",
    }
}

fn load_dataset(dataset: &str, a: &Args) -> anyhow::Result<Dataset> {
    let m = manifest()?;
    let ds =
        Dataset::load(&m, dataset, &cache_dir(a)).with_context(|| format!("loading {dataset}"))?;
    if !ds.dangling.queries.is_empty() || !ds.dangling.documents.is_empty() {
        eprintln!(
            "note: {dataset}: {} judged queries and {} judged documents are dangling",
            ds.dangling.queries.len(),
            ds.dangling.documents.len()
        );
    }
    Ok(ds)
}

fn report_line(dataset: &str, config: &str, report: &EvalReport) {
    eprintln!(
        "{dataset} {config}: nDCG@10={:.6} Recall@100={:.6} (BEIR-rounded {} / {}) scored={} dropped_self_ids={}",
        report.mean_ndcg_10,
        report.mean_recall_100,
        report.beir_rounded.ndcg_10,
        report.beir_rounded.recall_100,
        report.scored_queries,
        report.dropped_identical
    );
}

/// The corpus embeddings for `dataset`: the cached `FlatIndex` when `cache.json` matches, else a
/// fresh embed-and-commit into the cache directory (spec FR-020; research D10).
fn cached_index(
    dataset: &Dataset,
    passages: &[String],
    cfg: &DenseConfig,
    embedder: &MiniLmEmbedder,
    a: &Args,
) -> anyhow::Result<FlatIndex> {
    let dir = dense_cache_dir(a).join(&dataset.name);
    let corpus_sha256 = dataset
        .hashes
        .get("corpus.jsonl")
        .cloned()
        .context("dataset hashes lack corpus.jsonl")?;
    let key = EmbeddingCacheKey {
        format_version: 1,
        config: cfg.name.clone(),
        dataset: dataset.name.clone(),
        embedder_fingerprint: embedder.fingerprint().to_owned(),
        corpus_sha256,
        documents: passages.len() as u64,
    };
    let open = |dir: &Path| -> anyhow::Result<FlatIndex> {
        Ok(match embedder.load_path() {
            LoadPath::Buffered => FlatIndex::open_for(dir, embedder)?,
            LoadPath::Mmap => FlatIndex::open_mapped_for(dir, embedder)?,
        })
    };
    if key.matches(&dir)
        && let Ok(index) = open(&dir)
        && index.len() == passages.len() as u64
    {
        eprintln!("embedded 0 passages (cache hit: {})", dir.display());
        return Ok(index);
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let mut index = FlatIndex::create(
        &dir,
        embedder.dim(),
        embedder.metric(),
        embedder.fingerprint(),
    )?;
    let started = Instant::now();
    for (i, passage) in passages.iter().enumerate() {
        let vector = embedder
            .embed(&[passage.as_str()], TextKind::Passage)?
            .pop()
            .context("embedder returned no vector")?;
        let id = u32::try_from(i).context("corpus exceeds u32 ids")?;
        index.add(DocId(id), &vector)?;
        if (i + 1) % 1000 == 0 {
            eprintln!(
                "  embedded {}/{} ({:.0} s)",
                i + 1,
                passages.len(),
                started.elapsed().as_secs_f64()
            );
        }
    }
    index.commit()?;
    key.write(&dir)?;
    eprintln!(
        "embedded {} passages in {:.1} s ({} threads, {} load path)",
        passages.len(),
        started.elapsed().as_secs_f64(),
        MiniLmEmbedder::thread_count(),
        load_path_name(embedder.load_path())
    );
    Ok(index)
}

fn evaluate_dense(dataset: &str, cfg: &DenseConfig, a: &Args) -> anyhow::Result<EvalReport> {
    let ds = load_dataset(dataset, a)?;
    let load_path = load_path(a)?;
    let embedder = MiniLmEmbedder::load(&model_dir(a), load_path).context("loading the model")?;
    let (passages, ids) = build_passages(&ds, cfg)?;
    let index = cached_index(&ds, &passages, cfg, &embedder, a)?;
    let started = Instant::now();
    let run = execute_dense(&embedder, &index, &ids, &ds, cfg)?;
    eprintln!(
        "searched {} queries in {:.1} s",
        run.results.len(),
        started.elapsed().as_secs_f64()
    );
    if let Some(path) = a.flags.get("export-run") {
        run.export_jsonl(Path::new(path))?;
        eprintln!("exported run to {path}");
    }
    let mut report = score(&run, &ds, &git_head())?;
    report.stage = Some(StageInfo {
        kind: "dense".into(),
        embedder_fingerprint: embedder.fingerprint().to_owned(),
        load_path: load_path_name(load_path).into(),
        thread_count: MiniLmEmbedder::thread_count(),
        baseline: "absolute".into(),
    });
    report_line(dataset, &cfg.name, &report);
    Ok(report)
}

/// Load the model, embed one sentence, exit — run under `/usr/bin/time -l` per load path so each
/// path's peak RSS is measured from cold in its own process (research D12).
fn model_memory(a: &Args) -> anyhow::Result<()> {
    let load_path = load_path(a)?;
    let started = Instant::now();
    let embedder = MiniLmEmbedder::load(&model_dir(a), load_path)?;
    let loaded = started.elapsed();
    let v = embedder.embed(&["memory probe"], TextKind::Passage)?;
    println!(
        "model-memory: load_path={} fingerprint={} dim={} threads={} load={:.0} ms",
        load_path_name(load_path),
        embedder.fingerprint(),
        v[0].len(),
        MiniLmEmbedder::thread_count(),
        loaded.as_secs_f64() * 1000.0
    );
    Ok(())
}

/// `{"doc_id","text","vector"}` per line for N evenly spaced cached rows, for
/// `gen_004_fixtures.py --verify-embed` (research D14).
fn export_vectors(a: &Args) -> anyhow::Result<()> {
    let dataset = a.flags.get("dataset").context("--dataset is required")?;
    let sample: usize = a
        .flags
        .get("sample")
        .context("--sample is required")?
        .parse()?;
    let out = a.flags.get("out").context("--out is required")?;
    let ds = load_dataset(dataset, a)?;
    let cfg = DenseConfig::dense_baseline_v1();
    let (passages, ids) = build_passages(&ds, &cfg)?;
    let dir = dense_cache_dir(a).join(dataset);
    let index =
        FlatIndex::open(&dir).with_context(|| format!("opening the cache at {}", dir.display()))?;
    if index.len() != passages.len() as u64 {
        bail!(
            "cache holds {} rows but the corpus has {}",
            index.len(),
            passages.len()
        );
    }
    let step = (passages.len() / sample.max(1)).max(1);
    let mut file = std::io::BufWriter::new(std::fs::File::create(out)?);
    let mut written = 0;
    for i in (0..passages.len()).step_by(step).take(sample) {
        let id = DocId(u32::try_from(i)?);
        let vector = index
            .vector(id)
            .with_context(|| format!("row {id} missing from the cache"))?;
        writeln!(
            file,
            "{}",
            serde_json::json!({ "doc_id": ids.external(id), "text": passages[i], "vector": vector })
        )?;
        written += 1;
    }
    file.flush()?;
    eprintln!("exported {written} vectors to {out}");
    Ok(())
}

/// The hybrid baseline: a `HybridIndex` fed from the 004 embedding cache by value (0 documents
/// embedded), every judged query searched with explanation, scored in the 003 format
/// (spec 005 FR-022; research D10).
fn evaluate_hybrid(dataset: &str, cfg: &HybridConfig, a: &Args) -> anyhow::Result<EvalReport> {
    cfg.validate()?;
    let ds = load_dataset(dataset, a)?;
    let load_path = load_path(a)?;
    let embedder = MiniLmEmbedder::load(&model_dir(a), load_path).context("loading the model")?;
    let thread_count = MiniLmEmbedder::thread_count();

    // The 004 cache must match exactly; never re-embed silently (research R2).
    let cache_dir = dense_cache_dir(a).join(dataset);
    let corpus_sha256 = ds
        .hashes
        .get("corpus.jsonl")
        .cloned()
        .context("dataset hashes lack corpus.jsonl")?;
    let key = EmbeddingCacheKey {
        format_version: 1,
        config: cfg.dense.name.clone(),
        dataset: dataset.to_owned(),
        embedder_fingerprint: embedder.fingerprint().to_owned(),
        corpus_sha256,
        documents: ds.corpus.ids.len() as u64,
    };
    if !key.matches(&cache_dir) {
        bail!(
            "the Feature 004 embedding cache at {} does not match (config {}, fingerprint, corpus \
             hash or count differ); run `beir run --dataset {dataset} --config dense-baseline-v1` first",
            cache_dir.display(),
            cfg.dense.name
        );
    }
    let cache = FlatIndex::open_for(&cache_dir, &embedder)
        .with_context(|| format!("opening the cache at {}", cache_dir.display()))?;
    if cache.len() != ds.corpus.ids.len() as u64 {
        bail!(
            "cache holds {} rows but the corpus has {}",
            cache.len(),
            ds.corpus.ids.len()
        );
    }

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
            keep.path().join("hybrid")
        }
    };
    let (schema, _, _) = build(&ds, &cfg.lexical)?;
    let dense_fields = vec![
        xtriever_core::FieldName::from("title"),
        xtriever_core::FieldName::from("text"),
    ];
    let mut hybrid_cfg = xtriever_pipeline::HybridConfig::new(schema, dense_fields);
    hybrid_cfg.candidate_depth = cfg.candidate_depth;
    hybrid_cfg.rrf_k = cfg.rrf_k;
    let mut index = HybridIndex::create(&index_dir, hybrid_cfg, Box::new(embedder))?;

    let started = Instant::now();
    let docs = build_external(&ds, &cfg.lexical)?;
    let mut batch = Vec::with_capacity(1000);
    for (i, (external_id, fields)) in docs.into_iter().enumerate() {
        let id = DocId(u32::try_from(i)?);
        let vector = cache
            .vector(id)
            .with_context(|| format!("cache row {i} ({external_id}) missing"))?;
        batch.push((
            SourceDocument {
                external_id,
                fields,
                chunk: None,
            },
            vector,
        ));
        if batch.len() == 1000 {
            index.add_embedded(&batch)?;
            batch.clear();
        }
    }
    index.add_embedded(&batch)?;
    index.commit()?;
    eprintln!(
        "embedded 0 documents (004 cache); ingested {} documents in {:.1} s",
        index.len(),
        started.elapsed().as_secs_f64()
    );

    let mut explain_out = a
        .flags
        .get("export-explain")
        .map(|p| std::fs::File::create(p).map(std::io::BufWriter::new))
        .transpose()?;
    let mut lex_ms = 0.0f64;
    let mut queries = 0usize;
    let started = Instant::now();
    let opts = SearchOptions {
        explain: true,
        ..SearchOptions::default()
    };
    let query_ids: Vec<String> = ds.qrels.grades.keys().cloned().collect();
    let mut qi = 0usize;
    let mut retrieve = |text: &str| -> xtriever_core::Result<Vec<String>> {
        let t = Instant::now();
        let r = index.search(text, None, cfg.k, &opts)?;
        lex_ms += t.elapsed().as_secs_f64() * 1000.0;
        queries += 1;
        if r.stages.degraded.is_some() {
            eprintln!("warning: query degraded: {:?}", r.stages.degraded);
        }
        if let Some(out) = explain_out.as_mut() {
            // The stage lists must be complete for the oracle: a second search with k large
            // enough to hold the union of both candidate lists (2 × depth) exposes every
            // candidate's rank through its explanation.
            let full = index.search(text, None, 2 * cfg.candidate_depth, &opts)?;
            let mut lexical: Vec<(u32, &str)> = Vec::new();
            let mut dense: Vec<(u32, &str)> = Vec::new();
            for h in &full.hits {
                if let Some(e) = &h.explain {
                    if let Some(rk) = e.bm25_rank {
                        lexical.push((rk, &h.external_id));
                    }
                    if let Some(rk) = e.dense_rank {
                        dense.push((rk, &h.external_id));
                    }
                }
            }
            lexical.sort_unstable();
            dense.sort_unstable();
            let line = serde_json::json!({
                "query_id": query_ids.get(qi).cloned().unwrap_or_default(),
                "lexical": lexical.iter().map(|(rk, id)| serde_json::json!([rk, id])).collect::<Vec<_>>(),
                "dense": dense.iter().map(|(rk, id)| serde_json::json!([rk, id])).collect::<Vec<_>>(),
                "fused": r.hits.iter().map(|h| h.external_id.as_str()).collect::<Vec<_>>(),
            });
            writeln!(out, "{line}").map_err(xtriever_core::Error::Io)?;
        }
        qi += 1;
        Ok(r.hits.into_iter().map(|h| h.external_id).collect())
    };
    let run = execute_external(&ds, &cfg.name, cfg.k, &mut retrieve)?;
    if let Some(mut out) = explain_out {
        out.flush()?;
    }
    eprintln!(
        "searched {queries} queries in {:.1} s ({:.1} ms per query end to end, incl. query embedding)",
        started.elapsed().as_secs_f64(),
        lex_ms / queries.max(1) as f64
    );
    if let Some(path) = a.flags.get("export-run") {
        run.export_jsonl(Path::new(path))?;
        eprintln!("exported run to {path}");
    }
    let mut report = score(&run, &ds, &git_head())?;
    report.stage = Some(StageInfo {
        kind: "hybrid".into(),
        embedder_fingerprint: index.embedder().fingerprint().to_owned(),
        load_path: load_path_name(load_path).into(),
        thread_count,
        baseline: "guarded".into(),
    });
    report_line(dataset, &cfg.name, &report);
    Ok(report)
}

/// Index, retrieve and score one dataset. Returns the report.
fn evaluate(dataset: &str, a: &Args) -> anyhow::Result<EvalReport> {
    let cfg = match config(a)? {
        Config::Lexical(cfg) => cfg,
        Config::Dense(cfg) => return evaluate_dense(dataset, &cfg, a),
        Config::Hybrid(cfg) => return evaluate_hybrid(dataset, &cfg, a),
    };
    let ds = load_dataset(dataset, a)?;
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
    report_line(dataset, &cfg.name, &report);
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
        eprintln!("usage: beir <verify|run|delta|compare|smoke|model-memory|export-vectors> ...");
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
            print!("{}", delta(&before, &after)?.to_markdown());
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
        "compare" => {
            if a.positional.len() != 2 {
                bail!("usage: beir compare a.json b.json");
            }
            let x = read_report(&a.positional[0])?;
            let y = read_report(&a.positional[1])?;
            print!(
                "{}",
                compare(std::slice::from_ref(&x), std::slice::from_ref(&y)).to_markdown()
            );
            Ok(ExitCode::SUCCESS)
        }
        "model-memory" => {
            model_memory(&a)?;
            Ok(ExitCode::SUCCESS)
        }
        "export-vectors" => {
            export_vectors(&a)?;
            Ok(ExitCode::SUCCESS)
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
