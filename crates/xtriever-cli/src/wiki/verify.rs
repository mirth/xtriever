//! `wiki verify` — every passage inside the embedder's window, every article URL derivable,
//! the sidecar's counts equal to the index's (contracts/cli.md; spec FR-006, D6).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, bail};
use xtriever_core::DocId;
use xtriever_dense::MiniLmEmbedder;
use xtriever_pipeline::{HybridIndex, OpenOptions};

use super::VerifyArgs;
use super::build::{SharedEmbedder, load_path};
use super::chunking::WINDOW;
use super::record::{CorpusIdentity, Counts, Verify};
use super::url::derive_url;

/// The verify pass over a built index; fills `counts.passages_over_window` and
/// `counts.url_mismatches` and returns the verdict. `require_sidecar` checks `corpus.json`.
///
/// # Errors
///
/// A passage over the window, a URL mismatch, a missing or disagreeing sidecar, I/O.
pub fn verify_index(
    index_dir: &Path,
    embedder: &std::sync::Arc<MiniLmEmbedder>,
    snapshot_jsonl: Option<&Path>,
    require_sidecar: bool,
    counts: &mut Counts,
) -> anyhow::Result<Verify> {
    let index = HybridIndex::open_with(
        index_dir,
        Box::new(SharedEmbedder(std::sync::Arc::clone(embedder))),
        OpenOptions {
            mapped: true,
            read_only: true,
        },
    )
    .with_context(|| format!("opening {}", index_dir.display()))?;
    let urls = match snapshot_jsonl {
        Some(path) => Some(snapshot_urls(path)?),
        None => None,
    };
    let mut max_seen = 0u64;
    let mut over = 0u64;
    let mut url_mismatches = 0u64;
    let mut first_over: Option<String> = None;
    let mut first_url: Option<String> = None;
    let total = index.len();
    for i in 0..total {
        let id = DocId(u32::try_from(i).context("index exceeds u32 ids")?);
        let text = index.passage_text(id)?;
        let n = embedder.token_count(&text)? as u64;
        max_seen = max_seen.max(n);
        if n > WINDOW as u64 {
            over += 1;
            first_over.get_or_insert_with(|| {
                format!("{} ({n} positions)", index.external_id(id).unwrap_or("?"))
            });
        }
        if let Some(urls) = &urls {
            let external = index.external_id(id).unwrap_or("");
            let page = external.split('#').next().unwrap_or("");
            let title = text.split("\n\n").next().unwrap_or("");
            match urls.get(page) {
                Some(url) if derive_url(title) == *url => {}
                Some(url) => {
                    url_mismatches += 1;
                    first_url.get_or_insert_with(|| {
                        format!("{external}: {title:?} → {} ≠ {url}", derive_url(title))
                    });
                }
                None => {
                    url_mismatches += 1;
                    first_url.get_or_insert_with(|| {
                        format!("{external}: page {page} not in the snapshot")
                    });
                }
            }
        }
        if (i + 1) % 50_000 == 0 {
            eprintln!(
                "  verified {}/{total} passages (max {max_seen} positions)",
                i + 1
            );
        }
    }
    counts.passages_over_window = over;
    counts.url_mismatches = url_mismatches;
    let verdict = if over == 0 && url_mismatches == 0 {
        "PASS"
    } else {
        "FAIL"
    };
    let result = Verify {
        passages_checked: total,
        max_tokens_seen: max_seen,
        window: WINDOW as u64,
        verdict: verdict.to_owned(),
    };
    if let Some(p) = first_over {
        bail!("{over} passages exceed the {WINDOW}-position window; first: {p}");
    }
    if let Some(u) = first_url {
        bail!("{url_mismatches} article URLs do not derive from their titles; first: {u}");
    }
    if require_sidecar {
        let sidecar = index_dir.join("corpus.json");
        let text = std::fs::read_to_string(&sidecar).with_context(|| {
            format!(
                "reading {} (the corpus identity sidecar)",
                sidecar.display()
            )
        })?;
        let identity: CorpusIdentity = serde_json::from_str(&text)?;
        if identity.counts.passages != total {
            bail!(
                "{}: counts.passages {} ≠ index length {total}",
                sidecar.display(),
                identity.counts.passages
            );
        }
        counts.articles = identity.counts.articles;
        counts.excluded = identity.counts.excluded;
        counts.selected = identity.counts.selected;
        counts.passages = identity.counts.passages;
    }
    Ok(result)
}

/// `id → url` for every article in the snapshot.
fn snapshot_urls(path: &Path) -> anyhow::Result<HashMap<String, String>> {
    #[derive(serde::Deserialize)]
    struct IdUrl {
        id: String,
        url: String,
    }
    let reader = BufReader::with_capacity(
        1 << 20,
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?,
    );
    let mut out = HashMap::new();
    for line in reader.lines() {
        let row: IdUrl = serde_json::from_str(&line?)?;
        out.insert(row.id, row.url);
    }
    Ok(out)
}

/// Run the command.
///
/// # Errors
///
/// As [`verify_index`].
pub fn run(args: &VerifyArgs) -> anyhow::Result<()> {
    let embedder = std::sync::Arc::new(
        MiniLmEmbedder::load(&args.embedder_dir, load_path("mmap")?)
            .context("loading the embedder")?,
    );
    // The URL check is part of the contract, not optional: without the snapshot there is no
    // truth to check against, and a PASS that skipped it would be a lie (contracts/cli.md).
    let jsonl = args.snapshot_dir.join("simple.jsonl");
    if !jsonl.exists() {
        bail!(
            "no snapshot at {} — run scripts/fetch-wiki.sh; the URL check needs simple.jsonl",
            jsonl.display()
        );
    }
    let mut counts = Counts::default();
    let verify = verify_index(&args.index, &embedder, Some(&jsonl), true, &mut counts)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "counts": counts, "verify": verify }))?
    );
    Ok(())
}
