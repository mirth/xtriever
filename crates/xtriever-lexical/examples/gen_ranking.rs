//! Mint the `expected` rankings in `reference/fixtures/002/queries.json` from a real index built in
//! one batch, then let `gen_002_fixtures.py --verify-ranking` cross-check them against the
//! independent Python oracle (research D17). Refuses to overwrite a minted entry without `--force`.
//!
//! Usage: `cargo run -p xtriever-lexical --example gen_ranking -- --fixtures reference/fixtures/002 [--force]`

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use xtriever_core::{Document, Filter, Hit, LexicalIndex, LexicalQuery, Schema};
use xtriever_lexical::TantivyIndex;

#[derive(Deserialize)]
struct Corpus {
    documents: Vec<Document>,
}

#[derive(Deserialize, Serialize)]
struct Entry {
    name: String,
    query: LexicalQuery,
    filter: Option<Filter>,
    k: usize,
    oracle: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    tie_ok: bool,
    expected: Option<Vec<Hit>>,
}

#[derive(Deserialize, Serialize)]
struct Queries {
    score_rel_tol: f64,
    queries: Vec<Entry>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let fixtures = args
        .iter()
        .position(|a| a == "--fixtures")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .ok_or("usage: gen_ranking --fixtures <dir> [--force]")?;
    let force = args.iter().any(|a| a == "--force");

    let schema: Schema =
        serde_json::from_str(&std::fs::read_to_string(fixtures.join("schema.json"))?)?;
    let corpus: Corpus =
        serde_json::from_str(&std::fs::read_to_string(fixtures.join("corpus.json"))?)?;
    let queries_path = fixtures.join("queries.json");
    let mut queries: Queries = serde_json::from_str(&std::fs::read_to_string(&queries_path)?)?;

    let dir = tempfile::tempdir()?;
    let mut index = TantivyIndex::create(&dir.path().join("idx"), schema)?;
    index.add(&corpus.documents)?;
    index.commit()?;

    let mut minted = 0;
    for entry in &mut queries.queries {
        if entry.expected.is_some() && !force {
            println!(
                "  {}: already minted (use --force to overwrite)",
                entry.name
            );
            continue;
        }
        let hits = index.search(&entry.query, entry.filter.as_ref(), entry.k)?;
        println!("  {}: {} hits", entry.name, hits.len());
        entry.expected = Some(hits);
        minted += 1;
    }
    // Pretty, stable output; `Json` round-trip keeps key order as declared on the structs.
    let json: Json = serde_json::to_value(&queries)?;
    std::fs::write(&queries_path, serde_json::to_string_pretty(&json)? + "\n")?;
    println!(
        "gen_ranking: minted {minted} entries into {}",
        queries_path.display()
    );
    Ok(())
}
