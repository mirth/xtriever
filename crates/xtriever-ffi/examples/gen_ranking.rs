//! Mint `reference/fixtures/001/ranking.json` — the host↔device determinism oracle (FR-014).
//!
//! This cannot come from Python: `ExpectedRanking` must be what *tantivy itself* produced, compared
//! bit-exact, which is a different question from `bm25_reference.json`'s cross-implementation
//! parity check (report.md D-001). It is invoked and cross-checked by
//! `reference/gen_001_fixtures.py --emit-ranking`, which refuses to write the file unless this
//! output agrees with its independent Python BM25.
//!
//! Emits JSON on stdout; writes nothing itself.
//!
//! ```sh
//! cargo run -p xtriever-ffi --features spike --example gen_ranking -- <corpus.json> <k>
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use xtriever_ffi::ffi::{SpikeDocument, spike_index, spike_query};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let corpus_path = PathBuf::from(
        args.next()
            .context("usage: gen_ranking <corpus.json> <k>")?,
    );
    let k: u32 = args
        .next()
        .context("missing <k>")?
        .parse()
        .context("k must be a number")?;

    let corpus: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&corpus_path)?)
            .with_context(|| format!("parsing {}", corpus_path.display()))?;
    let query = corpus["query"].as_str().context("corpus.query")?.to_owned();

    let documents: Vec<SpikeDocument> = corpus["documents"]
        .as_array()
        .context("corpus.documents")?
        .iter()
        .map(|d| -> Result<SpikeDocument> {
            Ok(SpikeDocument {
                external_id: d["external_id"].as_str().context("external_id")?.to_owned(),
                text: d["text"].as_str().context("text")?.to_owned(),
            })
        })
        .collect::<Result<_>>()?;

    let dir = tempfile::tempdir()?;
    let path = dir.path().to_string_lossy().into_owned();

    let outcome = spike_index(path.clone(), documents).map_err(|e| anyhow::anyhow!("{e}"))?;
    if outcome.segment_count != 1 {
        // More than one segment means the collector's tie-break runs across segments and the
        // golden stops being comparable at all. Refuse to mint rather than emit a fixture that
        // would produce uninterpretable failures later.
        bail!(
            "index produced {} segments, expected 1 — the golden ranking would not be comparable",
            outcome.segment_count
        );
    }

    let hits = spike_query(path, query.clone(), k).map_err(|e| anyhow::anyhow!("{e}"))?;

    let payload = serde_json::json!({
        "schema_version": 1,
        "status": "minted",
        "provenance": "host-tantivy-run",
        "k": k,
        "query": query,
        "writer_threads": 1,
        "writer_memory_budget_bytes": 15_000_000,
        "documents_indexed": outcome.documents_indexed,
        "segment_count": outcome.segment_count,
        "hits": hits.iter().map(|h| serde_json::json!({
            "external_id": h.external_id,
            "score": h.score,
            // The bit pattern is what the oracle actually compares. A decimal round-trip through
            // JSON can lose a ULP, and that lost ULP would later be written up as an iOS
            // determinism finding rather than the serialization artifact it is.
            "score_bits": h.score.to_bits(),
            "segment_ord": h.segment_ord,
            "doc_id": h.doc_id,
        })).collect::<Vec<_>>(),
    });

    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}
