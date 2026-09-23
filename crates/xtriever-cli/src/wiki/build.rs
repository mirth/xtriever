//! `wiki build` — the whole build in one resumable command (contracts/cli.md; research D9).
//!
//! Streams the snapshot: verify → read + exclude → chunk → per shard of 4,096 passages: cache
//! hit or embed one passage at a time → `add_embedded` → … → `commit` → `merge` → verify →
//! records → rename `<out>.partial` to `<out>`. Nothing openable exists at `<out>` before the
//! last step; the snapshot and the embedding cache survive every failure.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, bail};
use xtriever_core::{Embedder, TextKind};
use xtriever_dense::{LoadPath, MiniLmEmbedder};
use xtriever_pipeline::{HybridIndex, SourceDocument};

use super::BuildArgs;
use super::cache::{cache_dir_for, read_shard, shard_key, write_shard};
use super::chunking::{Article, documents_for, wiki_config};
use super::manifest::{Manifest, verify_file};
use super::record::{
    BuildRecord, CacheStats, CorpusIdentity, Host, Models, artefact_bytes, attribution, now_rfc3339,
};
use super::rules::excluded_by;
use super::verify::verify_index;

/// Passages per cache shard / ingest batch.
pub const SHARD: usize = 4_096;

/// The pinned re-ranker the demo attaches (recorded, not part of the identity).
const RERANKER_FOR_DEMO: &str = xtriever_rerank::model::MODEL_ID;

/// One embedder shared by the index (which only needs its identity for `add_embedded`) and
/// the build (which counts and embeds with it).
#[derive(Clone)]
pub struct SharedEmbedder(pub Arc<MiniLmEmbedder>);

impl Embedder for SharedEmbedder {
    fn fingerprint(&self) -> &str {
        self.0.fingerprint()
    }
    fn dim(&self) -> usize {
        self.0.dim()
    }
    fn metric(&self) -> xtriever_core::Metric {
        self.0.metric()
    }
    fn max_input_tokens(&self) -> Option<usize> {
        self.0.max_input_tokens()
    }
    fn embed(&self, texts: &[&str], kind: TextKind) -> xtriever_core::Result<Vec<Vec<f32>>> {
        self.0.embed(texts, kind)
    }
}

/// Parse the load path flag.
///
/// # Errors
///
/// Any value but `buffered` / `mmap`.
/// The sparse option the flags ask for (Feature 027): `None` without `--sparse-encoder`; with
/// it, the given scale and boost or the engine's defaults (`SparseOption::default()`). A scale
/// or boost without an encoder is refused rather than ignored.
///
/// RED-CHECKPOINT STUB: not called by `run` until T037, so a build still works meanwhile.
///
/// # Errors
///
/// A scale or boost given without `--sparse-encoder`.
#[allow(dead_code)]
pub fn sparse_option(_args: &BuildArgs) -> anyhow::Result<Option<xtriever_pipeline::SparseOption>> {
    bail!("not implemented")
}

pub fn load_path(flag: &str) -> anyhow::Result<LoadPath> {
    match flag {
        "buffered" => Ok(LoadPath::Buffered),
        "mmap" => Ok(LoadPath::Mmap),
        other => bail!("--load-path {other}: expected buffered or mmap"),
    }
}

struct Phases(Vec<(&'static str, u64)>);

impl Phases {
    fn add(&mut self, name: &'static str, ms: u64) {
        match self.0.iter_mut().find(|(n, _)| *n == name) {
            Some((_, total)) => *total += ms,
            None => self.0.push((name, ms)),
        }
    }
}

fn ms(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Run the command.
///
/// # Errors
///
/// Every failure named in contracts/cli.md; `<out>` is never written on failure.
pub fn run(args: &BuildArgs) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut phases = Phases(Vec::new());

    // --- fetch/verify: the snapshot must match the manifest before a line is read.
    let t = Instant::now();
    let manifest = Manifest::load(&args.manifest)?;
    let parquet_name = manifest
        .parquet
        .url
        .rsplit('/')
        .next()
        .context("parquet url has no file name")?;
    verify_file(
        &args.snapshot_dir.join(parquet_name),
        manifest.parquet.bytes,
        &manifest.parquet.sha256,
    )?;
    let jsonl_path = args.snapshot_dir.join(&manifest.jsonl.file);
    if manifest.jsonl.sha256.is_empty() {
        bail!("the manifest does not pin the JSONL; run scripts/fetch-wiki.sh and pin its output");
    }
    verify_file(&jsonl_path, manifest.jsonl.bytes, &manifest.jsonl.sha256)?;
    phases.add("fetch_verify", ms(t.elapsed()));
    eprintln!(
        "wiki build: snapshot verified ({} lines pinned)",
        manifest.jsonl.lines
    );

    // --- models and output layout.
    let embedder = Arc::new(
        MiniLmEmbedder::load(&args.embedder_dir, load_path(&args.load_path)?)
            .context("loading the embedder")?,
    );
    let shared = SharedEmbedder(Arc::clone(&embedder));
    let fingerprint = embedder.fingerprint().to_owned();
    let partial = args.limit.map(|n| n as u64);
    let mut identity = CorpusIdentity::new(&manifest, &fingerprint, partial);

    let out = &args.out;
    let staging: PathBuf = out.with_file_name(format!(
        "{}.partial",
        out.file_name()
            .and_then(|n| n.to_str())
            .context("--out needs a file name")?
    ));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .with_context(|| format!("removing stale {}", staging.display()))?;
    }
    if out.exists() {
        bail!("{} exists; remove it to rebuild", out.display());
    }
    std::fs::create_dir_all(&staging)?;
    let index_dir = staging.join("index");
    let mut index = HybridIndex::create(&index_dir, wiki_config(), Box::new(shared.clone()))
        .context("creating the index")?;
    let cache_dir = cache_dir_for(&args.cache_dir, &fingerprint);
    let mut cache = CacheStats {
        dir: cache_dir.display().to_string(),
        ..CacheStats::default()
    };
    let dim = embedder.dim();

    // --- read + exclude + chunk, streaming into shards.
    let file = std::fs::File::open(&jsonl_path)
        .with_context(|| format!("opening {}", jsonl_path.display()))?;
    let reader = BufReader::with_capacity(1 << 20, file);
    let rules = &manifest.exclusions;
    let rule_names: Vec<String> = rules.iter().map(rule_name).collect();
    for name in &rule_names {
        identity.counts.excluded.insert(name.clone(), 0);
    }
    let mut shard_docs: Vec<SourceDocument> = Vec::with_capacity(SHARD);
    let mut shard_no = 0usize;
    let mut passages_total = 0u64;
    let mut chunk_ms = 0u64;
    let mut embed_ms = 0u64;
    let mut ingest_ms = 0u64;
    let t_read = Instant::now();
    for line in reader.lines() {
        let line = line.context("reading the snapshot")?;
        if let Some(limit) = args.limit
            && identity.counts.articles as usize >= limit
        {
            break;
        }
        identity.counts.articles += 1;
        let article: Article = serde_json::from_str(&line)
            .with_context(|| format!("snapshot line {}", identity.counts.articles))?;
        if let Some(rule) = excluded_by(rules, &article.title, &article.text) {
            *identity
                .counts
                .excluded
                .entry(rule_names[rule].clone())
                .or_default() += 1;
            continue;
        }
        if super::url::derive_url(&article.title) != article.url {
            identity.counts.url_mismatches += 1;
            bail!(
                "article {} ({:?}): derived URL differs from the snapshot's {} (contracts/artefact.md)",
                article.id,
                article.title,
                article.url
            );
        }
        let t = Instant::now();
        let docs = documents_for(&article, &embedder)?;
        chunk_ms += ms(t.elapsed());
        if docs.is_empty() {
            continue;
        }
        identity.counts.selected += 1;
        passages_total += docs.len() as u64;
        for doc in docs {
            shard_docs.push(doc);
            if shard_docs.len() == SHARD {
                flush_shard(
                    &mut index,
                    &embedder,
                    &fingerprint,
                    dim,
                    &cache_dir,
                    shard_no,
                    &mut shard_docs,
                    &mut cache,
                    &mut embed_ms,
                    &mut ingest_ms,
                )?;
                shard_no += 1;
            }
        }
    }
    if !shard_docs.is_empty() {
        flush_shard(
            &mut index,
            &embedder,
            &fingerprint,
            dim,
            &cache_dir,
            shard_no,
            &mut shard_docs,
            &mut cache,
            &mut embed_ms,
            &mut ingest_ms,
        )?;
        shard_no += 1;
    }
    let read_total = ms(t_read.elapsed());
    phases.add(
        "read_exclude",
        read_total.saturating_sub(chunk_ms + embed_ms + ingest_ms),
    );
    phases.add("chunk", chunk_ms);
    phases.add("embed", embed_ms);
    phases.add("ingest", ingest_ms);
    cache.shards = shard_no as u64;
    identity.counts.passages = passages_total;
    eprintln!(
        "wiki build: {} articles read, {} selected, {} passages in {} shards ({} cache hits)",
        identity.counts.articles,
        identity.counts.selected,
        identity.counts.passages,
        shard_no,
        cache.hits
    );

    // --- commit, merge.
    let t = Instant::now();
    index.commit().context("committing")?;
    phases.add("commit", ms(t.elapsed()));
    let t = Instant::now();
    index.merge().context("merging")?;
    phases.add("merge", ms(t.elapsed()));
    eprintln!(
        "wiki build: committed and merged ({} passages)",
        index.len()
    );
    drop(index);

    // --- verify (the same pass `wiki verify` runs), then the records.
    let t = Instant::now();
    let verify = verify_index(
        &index_dir,
        &embedder,
        Some(&jsonl_path),
        false,
        &mut identity.counts,
    )?;
    phases.add("verify", ms(t.elapsed()));
    if verify.verdict != "PASS" {
        bail!("verify failed: {verify:?}");
    }
    let recorded_at = now_rfc3339();
    write_json(&index_dir.join("corpus.json"), &identity)?;
    phases.add("total", ms(started.elapsed()));
    let record = BuildRecord {
        schema_version: 1,
        feature: "008-wiki-corpus".to_owned(),
        recorded_at: recorded_at.clone(),
        corpus_identity: identity.corpus_identity.clone(),
        host: Host {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            threads: MiniLmEmbedder::thread_count(),
        },
        models: Models {
            embedder: fingerprint,
            reranker_for_demo: RERANKER_FOR_DEMO.to_owned(),
        },
        counts: identity.counts.clone(),
        phases_ms: phases
            .0
            .iter()
            .map(|(n, v)| ((*n).to_owned(), *v))
            .collect(),
        embedding_cache: cache,
        artefact_bytes: artefact_bytes(&index_dir)?,
        verify,
    };
    write_json(&staging.join("wiki-build.json"), &record)?;
    std::fs::write(
        staging.join("ATTRIBUTION.txt"),
        attribution(&manifest, &identity.corpus_identity, &recorded_at),
    )?;
    std::fs::rename(&staging, out)
        .with_context(|| format!("renaming {} → {}", staging.display(), out.display()))?;
    eprintln!(
        "wiki build: PASS — {} ({} passages, {} B, {:.1} min)",
        out.display(),
        record.counts.passages,
        record.artefact_bytes.get("total").copied().unwrap_or(0),
        started.elapsed().as_secs_f64() / 60.0
    );
    Ok(())
}

fn rule_name(rule: &super::rules::Rule) -> String {
    match rule {
        super::rules::Rule::TitleSuffix { value } => format!("title_suffix:{value}"),
        super::rules::Rule::LeadContains {
            value,
            within_chars,
        } => {
            format!("lead_contains:{value}:{within_chars}")
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn flush_shard(
    index: &mut HybridIndex,
    embedder: &MiniLmEmbedder,
    fingerprint: &str,
    dim: usize,
    cache_dir: &Path,
    shard_no: usize,
    docs: &mut Vec<SourceDocument>,
    cache: &mut CacheStats,
    embed_ms: &mut u64,
    ingest_ms: &mut u64,
) -> anyhow::Result<()> {
    let texts: Vec<&str> = docs
        .iter()
        .map(
            |d| match d.fields.get(&xtriever_core::FieldName::from("text")) {
                Some(xtriever_core::Value::Text(t)) => t.as_str(),
                _ => "",
            },
        )
        .collect();
    let key = shard_key(&texts);
    let t = Instant::now();
    let vectors = match read_shard(cache_dir, shard_no, &key, texts.len(), dim, fingerprint) {
        Some(v) => {
            cache.hits += 1;
            eprintln!("  shard {shard_no} hit ({} passages)", texts.len());
            v
        }
        None => {
            let mut flat = Vec::with_capacity(texts.len() * dim);
            for text in &texts {
                let mut v = embedder.embed(&[text], TextKind::Passage)?;
                let v = v.pop().context("embedder returned no vector")?;
                flat.extend_from_slice(&v);
            }
            write_shard(
                cache_dir,
                shard_no,
                &key,
                texts.len(),
                dim,
                fingerprint,
                &flat,
            )?;
            cache.misses += 1;
            eprintln!(
                "  shard {shard_no} embedded {} passages in {:.1} s",
                texts.len(),
                t.elapsed().as_secs_f64()
            );
            flat
        }
    };
    *embed_ms += ms(t.elapsed());
    let t = Instant::now();
    let batch: Vec<(SourceDocument, Vec<f32>)> = docs
        .drain(..)
        .zip(vectors.chunks_exact(dim))
        .map(|(d, v)| (d, v.to_vec()))
        .collect();
    index
        .add_embedded(&batch)
        .with_context(|| format!("ingesting shard {shard_no}"))?;
    *ingest_ms += ms(t.elapsed());
    std::io::stderr().flush().ok();
    Ok(())
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(value)? + "\n";
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod sparse_tests {
    use clap::Parser;
    use xtriever_pipeline::SparseOption;

    use super::sparse_option;
    use crate::wiki::{BuildArgs, WikiCommand};

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        wiki: WikiCommand,
    }

    fn args(extra: &[&str]) -> BuildArgs {
        let mut argv = vec!["xtriever", "build", "--out", "target/x"];
        argv.extend_from_slice(extra);
        match Cli::try_parse_from(argv).unwrap().wiki {
            WikiCommand::Build(a) => a,
            _ => panic!("not a build"),
        }
    }

    /// Feature 027 (T037): no `--sparse-encoder`, no option — the build is what it was.
    #[test]
    fn without_the_encoder_flag_there_is_no_option() {
        assert_eq!(sparse_option(&args(&[])).unwrap(), None);
    }

    /// The encoder alone takes the engine's defaults, never restated here.
    #[test]
    fn the_encoder_alone_takes_the_engines_defaults() {
        assert_eq!(
            sparse_option(&args(&["--sparse-encoder", "enc"])).unwrap(),
            Some(SparseOption::default())
        );
    }

    #[test]
    fn scale_and_boost_are_taken_as_given() {
        let a = args(&[
            "--sparse-encoder",
            "enc",
            "--sparse-scale",
            "20",
            "--sparse-boost",
            "0.5",
        ]);
        assert_eq!(
            sparse_option(&a).unwrap(),
            Some(SparseOption {
                scale: 20,
                boost: 0.5
            })
        );
    }

    /// A scale or boost without an encoder would be silently ignored; it is refused instead.
    #[test]
    fn a_scale_or_boost_without_the_encoder_is_refused() {
        for extra in [["--sparse-scale", "20"], ["--sparse-boost", "0.5"]] {
            let e = sparse_option(&args(&extra)).unwrap_err().to_string();
            assert!(e.contains("--sparse-encoder"), "{e}");
        }
    }
}
