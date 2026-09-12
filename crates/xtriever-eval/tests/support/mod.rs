//! Shared helpers: golden loading and a synthetic BEIR-shaped dataset with a matching manifest.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use xtriever_eval::dataset::{Counts, DatasetEntry, FileEntry, Manifest};

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/fixtures/003")
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[derive(Deserialize, Clone)]
pub struct Expected {
    pub per_query: BTreeMap<String, PerQuery>,
    pub scored_queries: u32,
    pub dropped_identical: u32,
    pub mean_ndcg_10: f64,
    pub mean_recall_100: f64,
    pub beir_rounded: BTreeMap<String, f64>,
}

#[derive(Deserialize, Clone, Copy)]
pub struct PerQuery {
    pub ndcg_10: f64,
    pub recall_100: f64,
}

#[derive(Deserialize, Clone)]
pub struct Case {
    pub name: String,
    pub note: String,
    pub qrels: BTreeMap<String, BTreeMap<String, u32>>,
    pub run: BTreeMap<String, Vec<String>>,
    pub expected: Expected,
}

#[derive(Deserialize)]
pub struct Goldens {
    pub tolerance: f64,
    pub k: usize,
    pub cases: Vec<Case>,
}

pub fn goldens() -> Goldens {
    load_json(&fixtures_dir().join("metrics.json"))
}

pub fn case(name: &str) -> Case {
    goldens()
        .cases
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no golden case {name}"))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A tiny BEIR-shaped dataset named `mini` written under `cache_dir/mini/`, with a manifest whose
/// hashes and counts are correct for it. Deliberately CRLF with a header in the qrels (research D2).
/// Returns the manifest and the counts it asserts.
pub fn synthetic_dataset(cache_dir: &Path) -> (Manifest, Counts) {
    let dir = cache_dir.join("mini");
    std::fs::create_dir_all(dir.join("qrels")).unwrap();
    let corpus = concat!(
        r#"{"_id": "d1", "title": "Quantum lattice", "text": "quantum lattice photon", "metadata": {}}"#,
        "\n",
        r#"{"_id": "d2", "title": "", "text": "river valley harbor", "metadata": {}}"#,
        "\n",
        r#"{"_id": "d3", "title": "Piano", "text": "piano harp quantum", "metadata": {}}"#,
        "\n",
        r#"{"_id": "q2", "title": "Self", "text": "a document whose id equals a query id", "metadata": {}}"#,
        "\n",
    );
    let queries = concat!(
        r#"{"_id": "q1", "text": "quantum lattice", "metadata": {}}"#,
        "\n",
        r#"{"_id": "q2", "text": "river", "metadata": {}}"#,
        "\n",
        r#"{"_id": "q3", "text": "unjudged query", "metadata": {}}"#,
        "\n",
    );
    let qrels = "query-id\tcorpus-id\tscore\r\nq1\td1\t2\r\nq1\td3\t1\r\nq2\td2\t1\r\nq2\tq2\t1\r\nq2\tdX\t0\r\n";
    let files = [
        ("corpus.jsonl", corpus),
        ("queries.jsonl", queries),
        ("qrels/test.tsv", qrels),
    ];
    let mut entries = BTreeMap::new();
    for (rel, body) in files {
        std::fs::write(dir.join(rel), body).unwrap();
        entries.insert(
            rel.to_owned(),
            FileEntry {
                bytes: body.len() as u64,
                sha256: sha256_hex(body.as_bytes()),
                lines: body.lines().count() as u64,
            },
        );
    }
    let counts = Counts {
        documents: 4,
        queries: 3,
        judged_queries: 2,
        judgement_pairs: 5,
    };
    let entry = DatasetEntry {
        url: "file://synthetic".into(),
        archive: FileEntry {
            bytes: 0,
            sha256: String::new(),
            lines: 0,
        },
        files: entries,
        counts,
    };
    let manifest = Manifest {
        note: "synthetic".into(),
        datasets: [("mini".to_owned(), entry)].into_iter().collect(),
    };
    (manifest, counts)
}

/// Append one byte to a file so its size and hash both change.
pub fn tamper(path: &Path) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(b"x").unwrap();
}
